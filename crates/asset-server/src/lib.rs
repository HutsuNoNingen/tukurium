//! Local-first account, session and immutable item catalogue. The relay never owns this data.
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get as http_get, post, put},
};
use base64::Engine as _;
use futures_util::StreamExt;
use kokoverse_protocol::{ItemPermissions, ObjectView, Snapshot};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
mod workshop;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub struct Error(pub StatusCode, pub String);
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"ok":false,"message":self.1}))).into_response()
    }
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        eprintln!("Database: {e}");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database operation failed".into(),
        )
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        eprintln!("Storage: {e}");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Storage operation failed".into(),
        )
    }
}
fn bad(s: &str) -> Error {
    Error(StatusCode::BAD_REQUEST, s.into())
}
fn denied() -> Error {
    Error(
        StatusCode::FORBIDDEN,
        "This operation is not permitted".into(),
    )
}
fn missing() -> Error {
    Error(StatusCode::NOT_FOUND, "Not found or not accessible".into())
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad(&format!("Missing {key}")))
}
fn label(v: &Value, key: &str, max: usize) -> Result<String> {
    let s = text(v, key)?.trim();
    if s.is_empty() || s.chars().count() > max || s.chars().any(char::is_control) {
        return Err(bad("Invalid name"));
    }
    Ok(s.into())
}
fn get(db: &Connection, kind: &str, id: &str) -> Result<Option<Value>> {
    let value: Option<String> = db
        .query_row(
            "SELECT value FROM records WHERE kind=? AND id=?",
            params![kind, id],
            |r| r.get(0),
        )
        .optional()?;
    value
        .map(|s| serde_json::from_str(&s).map_err(|_| bad("Invalid stored record")))
        .transpose()
}
fn set(db: &Connection, kind: &str, id: &str, value: &Value) -> Result<()> {
    db.execute("INSERT INTO records(kind,id,value) VALUES(?,?,?) ON CONFLICT(kind,id) DO UPDATE SET value=excluded.value",params![kind,id,value.to_string()])?;
    Ok(())
}
fn all(db: &Connection, kind: &str) -> Result<Vec<Value>> {
    let mut s = db.prepare("SELECT value FROM records WHERE kind=? ORDER BY id")?;
    let rows = s.query_map([kind], |r| r.get::<_, String>(0))?;
    rows.map(|r| serde_json::from_str(&r?).map_err(|_| bad("Invalid record")))
        .collect()
}
fn user(db: &Connection, headers: &HeaderMap) -> Result<Value> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .filter(|s| s.len() == 64)
        .ok_or_else(|| {
            Error(
                StatusCode::UNAUTHORIZED,
                "Enter your name to continue".into(),
            )
        })?;
    let u = get(db, "users", &hash(token.as_bytes()))?
        .ok_or_else(|| Error(StatusCode::UNAUTHORIZED, "Enter your HN to continue".into()))?;
    Ok(get(
        db,
        "handles",
        &hash(u["name"].as_str().unwrap_or("").as_bytes()),
    )?
    .unwrap_or(u))
}
fn same_user(db: &Connection, owner: &str, uid: &str) -> Result<bool> {
    Ok(owner == uid
        || all(db, "handles")?.iter().any(|u| {
            u["id"] == uid
                && u["aliases"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(owner)))
        }))
}

// Preserve old saved versions and keys; combine access under the HN's primary identity.
fn handle_user(db: &Connection, name: &str) -> Result<Value> {
    let key = hash(name.as_bytes());
    if let Some(u) = get(db, "handles", &key)? {
        return Ok(u);
    }
    let mut members: Vec<_> = all(db, "users")?
        .into_iter()
        .filter(|u| u["name"].as_str().is_some_and(|n| n.trim() == name))
        .collect();
    members.sort_by_key(|u| {
        (
            u["created_at"].as_u64().unwrap_or(0),
            u["id"].as_str().unwrap_or("").to_string(),
        )
    });
    let mut u = members
        .first()
        .cloned()
        .unwrap_or_else(|| json!({"id":Uuid::new_v4().to_string(),"name":name,"created_at":now()}));
    u["name"] = json!(name);
    let uid = u["id"].as_str().unwrap().to_string();
    let aliases: std::collections::BTreeSet<_> = members
        .iter()
        .filter_map(|m| m["id"].as_str())
        .filter(|id| *id != uid)
        .map(str::to_owned)
        .collect();
    u["aliases"] = json!(aliases);
    for g in all(db, "grants")?
        .into_iter()
        .filter(|g| g["user_id"].as_str().is_some_and(|id| aliases.contains(id)))
    {
        let id = g["version_id"].as_str().unwrap();
        let target = format!("{uid}:{id}");
        let mut combined = g.clone();
        combined["user_id"] = json!(uid);
        if let Some(prior) = get(db, "grants", &target)? {
            for field in ["modify", "redistribute"] {
                combined["permissions"][field] =
                    json!(g["permissions"][field] == true || prior["permissions"][field] == true);
            }
        }
        set(db, "grants", &target, &combined)?;
    }
    for kind in ["entries", "asset_owners"] {
        let records: Vec<(String, Value)> = {
            let mut stmt = db.prepare("SELECT id,value FROM records WHERE kind=?")?;
            stmt.query_map([kind], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .map(|r| {
                let (id, v) = r?;
                Ok((
                    id,
                    serde_json::from_str(&v).map_err(|_| bad("Invalid record"))?,
                ))
            })
            .collect::<Result<_>>()?
        };
        for (id, mut record) in records {
            if !record["user_id"]
                .as_str()
                .is_some_and(|id| aliases.contains(id))
            {
                continue;
            }
            let suffix = id
                .split_once(':')
                .ok_or_else(|| bad("Invalid identity reference"))?
                .1;
            let target = format!("{uid}:{suffix}");
            if get(db, kind, &target)?.is_none() {
                record["user_id"] = json!(uid);
                if kind == "entries" {
                    record["id"] = json!(target);
                }
                set(db, kind, &target, &record)?;
            }
        }
    }
    set(db, "handles", &key, &u)?;
    Ok(u)
}
fn grant(db: &Connection, version: &str, uid: &str) -> Result<Value> {
    get(db, "grants", &format!("{uid}:{version}"))?.ok_or_else(missing)
}
// Licensing is deferred. Stored historical flags are metadata, not access rules.
fn prototype_permissions() -> ItemPermissions {
    ItemPermissions { modify: true, redistribute: true }
}
fn session(db: &Connection, id: &str) -> Result<Value> {
    get(db, "sessions", id)?.ok_or_else(missing)
}
fn live(s: &Value) -> bool {
    s["state"] == "ready" && s["heartbeat"].as_u64().unwrap_or(0) + 30 > now()
}

#[derive(Clone)]
pub struct Server {
    db: Arc<Mutex<Connection>>,
    pub root: PathBuf,
    relay: String,
    quota: u64,
    transfers: Arc<tokio::sync::Semaphore>,
}
impl Server {
    pub fn open(root: PathBuf, relay: String, quota: u64) -> Result<Self> {
        std::fs::create_dir_all(root.join("assets"))?;
        let db = Connection::open(root.join("catalog.sqlite"))?;
        let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > 1 {
            return Err(bad("This database was created by a newer server"));
        }
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,value TEXT NOT NULL,PRIMARY KEY(kind,id)); PRAGMA user_version=1;")?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            root,
            relay,
            quota,
            transfers: Arc::new(tokio::sync::Semaphore::new(2)),
        })
    }
    pub fn router(self) -> Router {
        Router::new()
            .route(
                "/health",
                http_get(|| async { Json(json!({"ok":true,"service":"kokoverse"})) }),
            )
            .route("/v1/users", post(register))
            .route("/v1/rpc", post(rpc))
            .route(
                "/v1/assets/{id}",
                put(upload).get(download).layer(DefaultBodyLimit::disable()),
            )
            .layer(DefaultBodyLimit::max(1024 * 1024))
            .with_state(self)
    }
}

async fn register(
    State(s): State<Server>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    let name = label(&v, "name", 40)?;
    let supplied = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()));
    let mut db = s.db.lock().unwrap();
    let tx = db.transaction()?;
    let mut result = handle_user(&tx, &name)?;
    let token = if let Some(token) = supplied {
        // Switching HNs never rebinds another window's existing session token.
        if get(&tx, "users", &hash(token.as_bytes()))?.is_some_and(|u| u["name"] != name) {
            secret()
        } else {
            token.to_owned()
        }
    } else {
        secret()
    };
    let key = hash(token.as_bytes());
    set(&tx, "users", &key, &result)?;
    tx.commit()?;
    result["token"] = json!(token);
    Ok(Json(result))
}

async fn rpc(
    State(s): State<Server>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    // All metadata mutations are serialized inside a SQLite transaction, including retries.
    let mut db = s.db.lock().unwrap();
    let u = user(&db, &headers)?;
    let tx = db.transaction()?;
    let uid = u["id"].as_str().unwrap();
    let action = text(&v, "action")?;
    let cache_key = v["request_id"]
        .as_str()
        .filter(|x| {
            x.len() <= 128
                && matches!(
                    action,
                    "create_session"
                        | "save_item"
                        | "share_item"
                        | "rename_entry"
                        | "remove_entry"
                        | "rename_user"
                        | "place_item"
                        | "publish_item"
                        | "unpublish_item"
                        | "acquire_item"
                )
        })
        .map(|r| format!("{uid}:{action}:{r}"));
    let fingerprint = hash(v.to_string().as_bytes());
    if let Some(key) = &cache_key
        && let Some(old) = get(&tx, "requests", key)?
    {
        if old["fingerprint"] != fingerprint {
            return Err(Error(
                StatusCode::CONFLICT,
                "Request ID was reused with different data".into(),
            ));
        }
        return Ok(Json(old["result"].clone()));
    }
    let result = match action {
        "me" => u.clone(),
        "rename_user" => {
            return Err(bad("Enter another HN to switch users"));
        }
        "sessions" => json!(
            all(&tx, "sessions")?
                .into_iter()
                .filter(live)
                .collect::<Vec<_>>()
        ),
        "create_session" => {
            let active = all(&tx, "sessions")?
                .into_iter()
                .filter(|x| {
                    x["owner_id"] == uid
                        && x["state"] != "ended"
                        && x["heartbeat"].as_u64().unwrap_or(0) + 30 > now()
                })
                .count();
            if active >= 3 {
                return Err(bad("At most three active sessions per user"));
            }
            let id = Uuid::new_v4().to_string();
            let host_peer = format!("host-{}", Uuid::new_v4().simple());
            let room = json!({"id":id,"name":label(&v,"name",80)?,"owner_id":uid,"owner_name":u["name"],"host_peer":host_peer,"relay":s.relay,"state":"preparing","heartbeat":now(),"created_at":now()});
            set(&tx, "sessions", &id, &room)?;
            issue_ticket(&tx, &room, &u, true)?
        }
        "join_session" => {
            let room = session(&tx, text(&v, "session_id")?)?;
            if !live(&room) {
                return Err(Error(StatusCode::CONFLICT, "Session is not ready".into()));
            }
            issue_ticket(&tx, &room, &u, false)?
        }
        "verify_ticket" => {
            let room = session(&tx, text(&v, "session_id")?)?;
            if room["owner_id"] != uid {
                return Err(denied());
            }
            let t =
                get(&tx, "tickets", &hash(text(&v, "ticket")?.as_bytes()))?.ok_or_else(missing)?;
            if t["session_id"] != room["id"]
                || t["expires_at"].as_u64().unwrap_or(0) < now()
                || t["peer"] != v["peer"]
            {
                return Err(denied());
            }
            t
        }
        "heartbeat" | "end_session" => {
            let id = text(&v, "session_id")?;
            let mut room = session(&tx, id)?;
            if room["owner_id"] != uid {
                return Err(denied());
            }
            if room["state"] == "ended" || room["heartbeat"].as_u64().unwrap_or(0) + 30 < now() {
                return Err(Error(
                    StatusCode::CONFLICT,
                    "Session has ended; create a new session".into(),
                ));
            }
            room["state"] = if action == "end_session" {
                json!("ended")
            } else if v["ready"] == true {
                json!("ready")
            } else {
                json!("reconnecting")
            };
            room["heartbeat"] = json!(now());
            set(&tx, "sessions", id, &room)?;
            room
        }
        "library" => {
            let query = v["query"].as_str().unwrap_or("").to_lowercase();
            let offset = v["offset"].as_u64().unwrap_or(0).min(100000) as usize;
            let rows: Vec<_> = all(&tx, "entries")?
                .into_iter()
                .filter(|e| {
                    e["user_id"] == uid
                        && e["name"]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&query)
                })
                .map(|entry| workshop::library_entry(&tx, entry, uid))
                .collect::<Result<_>>()?;
            json!({"total":rows.len(),"entries":rows.into_iter().skip(offset).take(10).collect::<Vec<_>>()})
        }
        "save_item" => save_item(&tx, &v, uid)?,
        "version" => {
            let id = text(&v, "version_id")?;
            let mut version = get(&tx, "versions", id)?.ok_or_else(missing)?;
            version["permissions"] = json!(prototype_permissions());
            version["creator_name"] = workshop::user_name(
                &tx,
                version["object"]["item_identity"]["creator_id"]
                    .as_str()
                    .unwrap_or(""),
            )?;
            version["saved_by_name"] =
                workshop::user_name(&tx, version["saved_by"].as_str().unwrap_or(""))?;
            version["publication"] =
                workshop::active_publication(&tx, uid, id)?.unwrap_or(Value::Null);
            version
        }
        "workshop" => workshop::list(&tx, &v, uid)?,
        "workshop_detail" => workshop::detail(&tx, text(&v, "publication_id")?, uid)?,
        "publish_item" => workshop::publish(&tx, &v, &u)?,
        "unpublish_item" => workshop::unpublish(&tx, &v, uid)?,
        "acquire_item" => workshop::acquire(&tx, &v, uid)?,
        "share_item" => {
            let id = text(&v, "version_id")?;
            let target = text(&v, "user_id")?;
            if !all(&tx, "users")?.iter().any(|u| u["id"] == target) {
                return Err(missing());
            }
            let version = get(&tx, "versions", id)?.ok_or_else(missing)?;
            add_grant(&tx, target, id, &version["name"], &prototype_permissions())?;
            json!({"shared":true})
        }
        "rename_entry" | "remove_entry" => {
            let id = text(&v, "entry_id")?;
            let mut entry = get(&tx, "entries", id)?.ok_or_else(missing)?;
            if entry["user_id"] != uid {
                return Err(missing());
            }
            if action == "remove_entry" {
                tx.execute("DELETE FROM records WHERE kind='entries' AND id=?", [id])?;
                json!({"removed":true})
            } else {
                entry["name"] = json!(label(&v, "name", 80)?);
                set(&tx, "entries", id, &entry)?;
                entry
            }
        }
        "place_item" => {
            let id = text(&v, "version_id")?;
            get(&tx, "versions", id)?.ok_or_else(missing)?;
            let room = session(&tx, text(&v, "session_id")?)?;
            if !live(&room) {
                return Err(bad("Session is not ready"));
            }
            let ticket =
                get(&tx, "tickets", &hash(text(&v, "ticket")?.as_bytes()))?.ok_or_else(missing)?;
            if ticket["user_id"] != uid
                || ticket["session_id"] != room["id"]
                || ticket["expires_at"].as_u64().unwrap_or(0) < now()
            {
                return Err(denied());
            }
            let token = secret();
            let p = json!({"version_id":id,"session_id":room["id"],"user_id":uid,"position":v["position"],"expires_at":now()+60});
            set(&tx, "placements", &hash(token.as_bytes()), &p)?;
            json!({"placement":token})
        }
        "resolve_placement" => {
            let key = hash(text(&v, "placement")?.as_bytes());
            let p = get(&tx, "placements", &key)?.ok_or_else(missing)?;
            let room = session(&tx, p["session_id"].as_str().unwrap())?;
            if room["owner_id"] != uid || p["expires_at"].as_u64().unwrap_or(0) < now() {
                return Err(denied());
            }
            let id = p["version_id"].as_str().unwrap();
            let version = get(&tx, "versions", id)?.ok_or_else(missing)?;
            let mut result = p;
            result["object"] = version["object"].clone();
            result["permissions"] = json!(prototype_permissions());
            result
        }
        _ => return Err(bad("Unknown action")),
    };
    if let Some(key) = cache_key {
        set(
            &tx,
            "requests",
            &key,
            &json!({"fingerprint":fingerprint,"result":result}),
        )?;
    }
    tx.commit()?;
    Ok(Json(result))
}

fn issue_ticket(db: &Connection, room: &Value, u: &Value, host: bool) -> Result<Value> {
    let token = secret();
    let peer = if host {
        room["host_peer"].as_str().unwrap().to_string()
    } else {
        format!("guest-{}", Uuid::new_v4().simple())
    };
    let t = json!({"user_id":u["id"],"name":u["name"],"aliases":u["aliases"],"session_id":room["id"],"peer":peer,"host":host,"expires_at":now()+86400});
    set(db, "tickets", &hash(token.as_bytes()), &t)?;
    Ok(json!({"session":room,"peer":peer,"ticket":token,"host":host}))
}
fn add_grant(
    db: &Connection,
    uid: &str,
    id: &str,
    name: &Value,
    p: &ItemPermissions,
) -> Result<()> {
    let key = format!("{uid}:{id}");
    // Existing grants are immutable; resending a restricted copy cannot revoke earlier rights.
    if get(db, "grants", &key)?.is_none() {
        set(
            db,
            "grants",
            &key,
            &json!({"user_id":uid,"version_id":id,"permissions":p,"created_at":now()}),
        )?;
    }
    if get(db, "entries", &key)?.is_none() {
        let thumbnail = get(db, "versions", id)?
            .map(|v| v["thumbnail"].clone())
            .unwrap_or(Value::Null);
        set(
            db,
            "entries",
            &key,
            &json!({"id":key,"user_id":uid,"version_id":id,"name":name,"thumbnail":thumbnail,"created_at":now()}),
        )?;
    }
    Ok(())
}
fn save_item(db: &Connection, v: &Value, uid: &str) -> Result<Value> {
    let mut object: ObjectView =
        serde_json::from_value(v["object"].clone()).map_err(|_| bad("Invalid item"))?;
    if object.panel.is_some() || object.inspector.is_some() {
        return Err(bad(
            "Save ordinary items; world inspectors reference their current world",
        ));
    }
    let mut model = kokoverse_core::WorldModel::new();
    model
        .restore(Snapshot {
            world_revision: object.object_revision.clone(),
            objects: vec![object.clone()],
        })
        .map_err(|e| bad(&e))?;
    let mut deps = vec![];
    match &object.visual {
        kokoverse_protocol::Visual::Model { asset_id, .. }
        | kokoverse_protocol::Visual::Video { asset_id, .. } => deps.push(asset_id.clone()),
        _ => {}
    }
    for id in &deps {
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE kind='asset_owners' AND id LIKE ?)",
            [format!("%:{id}")], |row| row.get(0),
        )?;
        if !exists {
            return Err(missing());
        }
    }
    for script in &mut object.scripts {
        script.state.clear();
    }
    let source = object
        .item_identity
        .as_ref()
        .and_then(|i| i.source_version.clone());
    let permissions = prototype_permissions();
    let creator = if let Some(source) = &source {
        get(db, "versions", source)?.ok_or_else(missing)?["object"]["item_identity"]["creator_id"]
            .as_str()
            .unwrap_or(uid)
            .to_string()
    } else {
        object.item_identity.as_ref().map(|i| i.creator_id.clone()).unwrap_or_else(|| uid.to_string())
    };
    let editors = object
        .item_identity
        .as_ref()
        .map(|i| i.editors.clone())
        .unwrap_or_default();
    object.item_identity = Some(kokoverse_protocol::ItemIdentity {
        creator_id: creator,
        owner_id: uid.into(),
        source_version: source.clone(),
        permissions: permissions.clone(),
        editors,
    });
    let thumbnail = if let Some(text) = v["thumbnail"].as_str() {
        let data = text
            .strip_prefix("data:image/png;base64,")
            .filter(|x| x.len() <= 65536)
            .ok_or_else(|| bad("Invalid thumbnail"))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| bad("Invalid thumbnail"))?;
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|_| bad("Invalid thumbnail"))?;
        if image.width() > 256 || image.height() > 256 {
            return Err(bad("Thumbnail exceeds 256 pixels"));
        }
        json!(text)
    } else {
        Value::Null
    };
    let id = Uuid::new_v4().to_string();
    let result = json!({"id":id,"name":label(v,"name",80)?,"object":object,"assets":deps,"source_version":source,"thumbnail":thumbnail,"saved_by":uid,"saved_at":now(),"content_hash":hash(serde_json::to_vec(&object).unwrap().as_slice()),"permissions":permissions});
    set(db, "versions", &id, &result)?;
    add_grant(db, uid, &id, &result["name"], &permissions)?;
    Ok(result)
}

struct TemporaryAsset(PathBuf);
impl Drop for TemporaryAsset {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
async fn upload(
    State(s): State<Server>,
    Path(id): Path<String>,
    request: Request,
) -> Result<Json<Value>> {
    if !kokoverse_protocol::valid_asset_id(&id) {
        return Err(bad("Invalid asset ID"));
    }
    let uid = {
        let db = s.db.lock().unwrap();
        user(&db, request.headers())?["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let _permit = s.transfers.clone().try_acquire_owned().map_err(|_| {
        Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Two asset transfers are already active; retry shortly".into(),
        )
    })?;
    let temporary = TemporaryAsset(
        s.root
            .join("assets")
            .join(format!("{}.tmp", Uuid::new_v4())),
    );
    let mut file = tokio::fs::File::create(&temporary.0).await?;
    let mut body = request.into_body().into_data_stream();
    let mut size = 0usize;
    let mut digest = Sha256::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| bad("Upload was interrupted"))?;
        size = size
            .checked_add(chunk.len())
            .ok_or_else(|| bad("Upload too large"))?;
        if size > 128 * 1024 * 1024 {
            return Err(Error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Asset exceeds 128 MiB".into(),
            ));
        }
        digest.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.sync_all().await?;
    drop(file);
    if format!("{:x}", digest.finalize()) != id {
        return Err(bad("Asset checksum mismatch"));
    }
    let bytes = tokio::fs::read(&temporary.0).await?;
    tokio::task::spawn_blocking(move || validate_asset(&bytes))
        .await
        .map_err(|_| bad("Asset validation failed"))??;
    let mut db = s.db.lock().unwrap();
    let tx = db.transaction()?;
    let total: u64 = all(&tx, "asset_owners")?
        .iter()
        .filter(|r| r["user_id"] == uid)
        .map(|r| r["size"].as_u64().unwrap_or(0))
        .sum();
    let key = format!("{uid}:{id}");
    if get(&tx, "asset_owners", &key)?.is_none() && total + size as u64 > s.quota {
        return Err(Error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Storage quota exceeded".into(),
        ));
    }
    let destination = s.root.join("assets").join(&id);
    if !destination.exists() {
        std::fs::rename(&temporary.0, destination)?;
    }
    set(&tx, "assets", &id, &json!({"id":id,"size":size}))?;
    set(
        &tx,
        "asset_owners",
        &key,
        &json!({"user_id":uid,"size":size}),
    )?;
    tx.commit()?;
    Ok(Json(json!({"id":id,"size":size})))
}
async fn download(
    State(s): State<Server>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse> {
    if !kokoverse_protocol::valid_asset_id(&id) {
        return Err(missing());
    }
    {
        let db = s.db.lock().unwrap();
        user(&db, &headers)?;
    }
    let file = tokio::fs::File::open(s.root.join("assets").join(id))
        .await
        .map_err(|_| missing())?;
    Ok((
        [("content-type", "application/octet-stream")],
        Body::from_stream(tokio_util::io::ReaderStream::new(file)),
    ))
}
fn validate_asset(bytes: &[u8]) -> Result<()> {
    if bytes.len() > 128 * 1024 * 1024 || bytes.len() < 20 {
        return Err(bad("Invalid asset size"));
    }
    if bytes.starts_with(b"glTF") {
        kokoverse_core::asset_validation::validate_glb(bytes).map_err(|e| bad(&e))?;
        return Ok(());
    }
    if bytes.starts_with(b"OggS")
        && bytes[..bytes.len().min(65536)]
            .windows(7)
            .any(|w| w == b"\x80theora")
    {
        return Ok(());
    }
    let img = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| bad("Invalid image"))?;
    let (w, h) = img
        .into_dimensions()
        .map_err(|_| bad("Unsupported asset format"))?;
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err(bad("Image dimensions exceed 4096"));
    }
    image::load_from_memory(bytes).map_err(|_| bad("Corrupt image data"))?;
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub server: String,
    pub token: String,
    pub user: Value,
}
#[derive(Clone)]
pub struct Client {
    pub server: String,
    pub token: String,
    http: reqwest::Client,
}
impl Client {
    pub fn new(server: String, token: String) -> std::result::Result<Self, String> {
        let url = reqwest::Url::parse(&server).map_err(|e| e.to_string())?;
        if !matches!(url.scheme(), "http" | "https")
            || (url.scheme() == "http"
                && !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
        {
            return Err("HTTP is only allowed on localhost".into());
        }
        Ok(Self {
            server: server.trim_end_matches('/').into(),
            token,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|e| e.to_string())?,
        })
    }
    async fn json(&self, request: reqwest::RequestBuilder) -> std::result::Result<Value, String> {
        let r = request
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| format!("Local server is unavailable: {e}"))?;
        let status = r.status();
        let v: Value = r
            .json()
            .await
            .map_err(|_| "Invalid server response".to_string())?;
        if !status.is_success() {
            return Err(v["message"]
                .as_str()
                .unwrap_or("Server request failed")
                .into());
        }
        Ok(v)
    }
    pub async fn register(&self, name: &str) -> std::result::Result<Value, String> {
        self.json(
            self.http
                .post(format!("{}/v1/users", self.server))
                .json(&json!({"name":name})),
        )
        .await
    }
    pub async fn rpc(&self, v: Value) -> std::result::Result<Value, String> {
        self.json(self.http.post(format!("{}/v1/rpc", self.server)).json(&v))
            .await
    }
    pub async fn upload(&self, id: &str, bytes: Vec<u8>) -> std::result::Result<(), String> {
        self.json(
            self.http
                .put(format!("{}/v1/assets/{id}", self.server))
                .body(bytes),
        )
        .await
        .map(|_| ())
    }
    pub async fn download(&self, id: &str) -> std::result::Result<Vec<u8>, String> {
        let r = self
            .http
            .get(format!("{}/v1/assets/{id}", self.server))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !r.status().is_success() {
            return Err("Asset is not accessible".into());
        }
        let b = r.bytes().await.map_err(|e| e.to_string())?;
        if b.len() > 128 * 1024 * 1024 || hash(&b) != id {
            return Err("Downloaded asset checksum mismatch".into());
        }
        Ok(b.to_vec())
    }
}

#[cfg(test)]
mod hn_tests {
    use super::*;

    fn server() -> Server {
        Server::open(
            std::env::temp_dir().join(format!("kokoverse-hn-{}", Uuid::new_v4())),
            "127.0.0.1:48001".into(),
            1024,
        )
        .unwrap()
    }
    fn auth(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
        headers
    }
    #[tokio::test]
    async fn hn_login_shares_accounts_and_switching_does_not_rebind_old_sessions() {
        let s = server();
        let first = register(
            State(s.clone()),
            HeaderMap::new(),
            Json(json!({"name":" Alice "})),
        )
        .await
        .unwrap()
        .0;
        let again = register(
            State(s.clone()),
            HeaderMap::new(),
            Json(json!({"name":"Alice"})),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(first["id"], again["id"]);
        assert_eq!(first["name"], "Alice");
        let token = first["token"].as_str().unwrap();
        let switched = register(State(s.clone()), auth(token), Json(json!({"name":"Bob"})))
            .await
            .unwrap()
            .0;
        assert_ne!(switched["id"], first["id"]);
        assert_ne!(switched["token"], first["token"]);
        assert_eq!(
            user(&s.db.lock().unwrap(), &auth(token)).unwrap()["id"],
            first["id"]
        );
        assert!(
            register(
                State(s.clone()),
                HeaderMap::new(),
                Json(json!({"name":"  "}))
            )
            .await
            .is_err()
        );
        let root = s.root.clone();
        drop(s);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn legacy_duplicate_hns_preserve_all_entries_assets_and_original_versions() {
        let s = server();
        let original =
            json!({"id":"v1","name":"Saved item","thumbnail":null,"saved_by":"legacy-b"});
        {
            let db = s.db.lock().unwrap();
            set(
                &db,
                "users",
                "old-a",
                &json!({"id":"legacy-a","name":"Shared HN","created_at":1}),
            )
            .unwrap();
            set(
                &db,
                "users",
                "old-b",
                &json!({"id":"legacy-b","name":"Shared HN","created_at":2}),
            )
            .unwrap();
            set(&db, "versions", "v1", &original).unwrap();
            add_grant(
                &db,
                "legacy-a",
                "v1",
                &json!("Original name"),
                &ItemPermissions {
                    modify: false,
                    redistribute: false,
                },
            )
            .unwrap();
            add_grant(
                &db,
                "legacy-b",
                "v1",
                &json!("Other name"),
                &ItemPermissions {
                    modify: true,
                    redistribute: true,
                },
            )
            .unwrap();
            set(&db,"entries","legacy-b:v2",&json!({"id":"legacy-b:v2","user_id":"legacy-b","version_id":"v2","name":"Renamed copy"})).unwrap();
            set(
                &db,
                "asset_owners",
                "legacy-b:asset",
                &json!({"user_id":"legacy-b","size":42}),
            )
            .unwrap();
        }
        let login = register(
            State(s.clone()),
            HeaderMap::new(),
            Json(json!({"name":"Shared HN"})),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(login["id"], "legacy-a");
        {
            let db = s.db.lock().unwrap();
            assert_eq!(
                grant(&db, "v1", "legacy-a").unwrap()["permissions"]["modify"],
                true
            );
            assert_eq!(
                get(&db, "entries", "legacy-a:v2").unwrap().unwrap()["name"],
                "Renamed copy"
            );
            assert_eq!(
                get(&db, "asset_owners", "legacy-a:asset").unwrap().unwrap()["size"],
                42
            );
            assert_eq!(get(&db, "versions", "v1").unwrap().unwrap(), original);
            assert!(same_user(&db, "legacy-b", "legacy-a").unwrap());
            assert!(!same_user(&db, "stranger", "legacy-a").unwrap());
        }
        let root = s.root.clone();
        drop(s);
        std::fs::remove_dir_all(root).unwrap();
    }
}

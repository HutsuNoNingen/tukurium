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
/// Records of `kind` whose top-level string field `field` equals
/// `value`, filtered by SQLite instead of loading and parsing every
/// record of the kind. `field` is interpolated into the JSON path and
/// must be a trusted internal literal, never request input. Row order
/// matches `all` (ORDER BY id), and the match semantics match the
/// in-memory `record[field] == value` comparison for string fields:
/// missing or non-string fields never match.
fn find_by_field(db: &Connection, kind: &str, field: &str, value: &str) -> Result<Vec<Value>> {
    let sql = format!(
        "SELECT value FROM records WHERE kind=? AND json_extract(value,'$.{field}')=? ORDER BY id"
    );
    let mut s = db.prepare(&sql)?;
    let rows = s.query_map(params![kind, value], |r| r.get::<_, String>(0))?;
    rows.map(|r| serde_json::from_str(&r?).map_err(|_| bad("Invalid record")))
        .collect()
}

/// Total stored asset bytes for one user, aggregated by SQLite.
/// Matches summing `size` (missing/non-numeric sizes count as 0) over
/// the user's `asset_owners` records.
fn stored_asset_bytes(db: &Connection, uid: &str) -> Result<u64> {
    let total: i64 = db.query_row(
        "SELECT COALESCE(SUM(CAST(json_extract(value,'$.size') AS INTEGER)),0) \
         FROM records WHERE kind='asset_owners' AND json_extract(value,'$.user_id')=?",
        params![uid],
        |r| r.get(0),
    )?;
    Ok(total.max(0) as u64)
}
fn all(db: &Connection, kind: &str) -> Result<Vec<Value>> {
    let mut s = db.prepare("SELECT value FROM records WHERE kind=? ORDER BY id")?;
    let rows = s.query_map([kind], |r| r.get::<_, String>(0))?;
    rows.map(|r| serde_json::from_str(&r?).map_err(|_| bad("Invalid record")))
        .collect()
}
fn all_with_ids(db: &Connection, kind: &str) -> Result<Vec<(String, Value)>> {
    let mut s = db.prepare("SELECT id,value FROM records WHERE kind=? ORDER BY id")?;
    let rows = s.query_map([kind], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    rows.map(|r| {
        let (id, v) = r?;
        Ok((
            id,
            serde_json::from_str(&v).map_err(|_| bad("Invalid record"))?,
        ))
    })
    .collect()
}

const DAY: u64 = 86400;

/// Remove records whose usefulness has ended so the catalogue does not grow
/// forever: expired tickets and placements, ended or long-stale sessions,
/// idempotency results older than a day, and token rows unused for 30 days.
/// Rows written before timestamps existed are stamped on their first sweep
/// and only become eligible one full grace period later.
fn sweep_expired_at(db: &Connection, now: u64) -> Result<usize> {
    let mut removed = 0usize;
    let mut delete = |kind: &str, id: &str| -> Result<()> {
        db.execute(
            "DELETE FROM records WHERE kind=? AND id=?",
            params![kind, id],
        )?;
        removed += 1;
        Ok(())
    };
    for kind in ["tickets", "placements"] {
        for (id, v) in all_with_ids(db, kind)? {
            if v["expires_at"].as_u64().unwrap_or(0) <= now {
                delete(kind, &id)?;
            }
        }
    }
    for (id, v) in all_with_ids(db, "sessions")? {
        let stale = v["heartbeat"].as_u64().unwrap_or(0).saturating_add(DAY) <= now;
        if v["state"] == "ended" || stale {
            delete("sessions", &id)?;
        }
    }
    for (id, mut v) in all_with_ids(db, "requests")? {
        match v["created_at"].as_u64() {
            Some(t) if t.saturating_add(DAY) <= now => delete("requests", &id)?,
            Some(_) => {}
            None => {
                v["created_at"] = json!(now);
                set(db, "requests", &id, &v)?;
            }
        }
    }
    for (id, mut v) in all_with_ids(db, "users")? {
        match v["last_seen"].as_u64() {
            Some(t) if t.saturating_add(30 * DAY) <= now => delete("users", &id)?,
            Some(_) => {}
            None => {
                v["last_seen"] = json!(now);
                set(db, "users", &id, &v)?;
            }
        }
    }
    Ok(removed)
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
        let server = Self {
            db: Arc::new(Mutex::new(db)),
            root,
            relay,
            quota,
            transfers: Arc::new(tokio::sync::Semaphore::new(2)),
        };
        match server.sweep_expired() {
            Ok(n) if n > 0 => eprintln!("Catalog sweep removed {n} expired records"),
            Ok(_) => {}
            Err(e) => eprintln!("Catalog sweep at startup failed: {}", e.1),
        }
        Ok(server)
    }
    pub fn sweep_expired(&self) -> Result<usize> {
        let db = self.db.lock().unwrap();
        sweep_expired_at(&db, now())
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
    let mut stored = result.clone();
    stored["last_seen"] = json!(now());
    set(&tx, "users", &key, &stored)?;
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
            let active = find_by_field(&tx, "sessions", "owner_id", uid)?
                .into_iter()
                .filter(|x| {
                    x["state"] != "ended" && x["heartbeat"].as_u64().unwrap_or(0) + 30 > now()
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
            let mut cache = workshop::RequestCache::default();
            let rows: Vec<_> = find_by_field(&tx, "entries", "user_id", uid)?
                .into_iter()
                .filter(|e| {
                    e["name"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&query)
                })
                .map(|entry| workshop::library_entry(&tx, &mut cache, entry, uid))
                .collect::<Result<_>>()?;
            json!({"total":rows.len(),"entries":rows.into_iter().skip(offset).take(10).collect::<Vec<_>>()})
        }
        "save_item" => save_item(&tx, &v, uid)?,
        "version" => {
            let id = text(&v, "version_id")?;
            let mut version = get(&tx, "versions", id)?.ok_or_else(missing)?;
            version["permissions"] = json!(prototype_permissions());
            let mut cache = workshop::RequestCache::default();
            version["creator_name"] = workshop::user_name(
                &tx,
                &mut cache,
                version["object"]["item_identity"]["creator_id"]
                    .as_str()
                    .unwrap_or(""),
            )?;
            version["saved_by_name"] = workshop::user_name(
                &tx,
                &mut cache,
                version["saved_by"].as_str().unwrap_or(""),
            )?;
            version["publication"] =
                workshop::active_publication(&tx, &mut cache, uid, id)?.unwrap_or(Value::Null);
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
            &json!({"fingerprint":fingerprint,"result":result,"created_at":now()}),
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
    let validation_path = temporary.0.clone();
    tokio::task::spawn_blocking(move || validate_asset(&validation_path))
        .await
        .map_err(|_| bad("Asset validation failed"))??;
    let mut db = s.db.lock().unwrap();
    let tx = db.transaction()?;
    let total: u64 = stored_asset_bytes(&tx, &uid)?;
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
/// Validate an uploaded asset in place, header-first, instead of
/// reading the whole file into memory up front: format detection
/// and the image dimension check only need the leading bytes /
/// headers, oversized images are rejected before any decode, and
/// the image decode itself runs under an explicit allocation
/// limit (a 4096x4096 RGBA decode is ~64 MiB; the limit leaves
/// headroom without letting a crafted file exceed it). Validation
/// strength is unchanged: GLB structure, the theora marker, image
/// dimensions, and full image decodability are all still checked.
fn validate_asset(path: &std::path::Path) -> Result<()> {
    use std::io::Read as _;
    let size = std::fs::metadata(path).map_err(|_| bad("Invalid asset size"))?.len() as usize;
    if size > 128 * 1024 * 1024 || size < 20 {
        return Err(bad("Invalid asset size"));
    }
    let mut head = vec![0u8; size.min(65536)];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut head))
        .map_err(|_| bad("Invalid asset size"))?;
    if head.starts_with(b"glTF") {
        // GLB validation walks chunk headers over the whole file;
        // it does not copy or decode, so reading it here (only in
        // this branch) stays proportional to the file itself.
        let bytes = std::fs::read(path).map_err(|_| bad("Invalid asset size"))?;
        kokoverse_core::asset_validation::validate_glb(&bytes).map_err(|e| bad(&e))?;
        return Ok(());
    }
    if head.starts_with(b"OggS") && head.windows(7).any(|w| w == b"\x80theora") {
        return Ok(());
    }
    let dimensions = image::ImageReader::open(path)
        .map_err(|_| bad("Invalid image"))?
        .with_guessed_format()
        .map_err(|_| bad("Invalid image"))?
        .into_dimensions()
        .map_err(|_| bad("Unsupported asset format"))?;
    let (w, h) = dimensions;
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err(bad("Image dimensions exceed 4096"));
    }
    let mut reader = image::ImageReader::open(path)
        .map_err(|_| bad("Invalid image"))?
        .with_guessed_format()
        .map_err(|_| bad("Invalid image"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| bad("Corrupt image data"))?;
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
    async fn library_rpc_matches_full_scan_semantics_and_asset_bytes_match_memory_sum() {
        let s = server();
        let alice = register(
            State(s.clone()),
            HeaderMap::new(),
            Json(json!({"name":"Alice"})),
        )
        .await
        .unwrap()
        .0;
        let bob = register(State(s.clone()), HeaderMap::new(), Json(json!({"name":"Bob"})))
            .await
            .unwrap()
            .0;
        let (alice_id, bob_id) = (
            alice["id"].as_str().unwrap().to_owned(),
            bob["id"].as_str().unwrap().to_owned(),
        );
        {
            let db = s.db.lock().unwrap();
            // Versions whose creator is Bob; v3's creator id does not exist.
            for (v, creator) in [("v1", bob_id.as_str()), ("v2", bob_id.as_str()), ("v3", "ghost")] {
                set(&db,"versions",v,&json!({"id":v,"object":{"visual":{"kind":"image"},"scripts":[1],"item_identity":{"creator_id":creator}},"saved_by":alice_id})).unwrap();
            }
            // Alice's entries (ids chosen so ORDER BY id interleaves users),
            // plus Bob's entry that must not appear in Alice's library.
            for (id, name, version) in [
                ("a1", "Cat", "v1"),
                ("a2", "dog", "v2"),
                ("a3", "Catalog", "v3"),
                ("a4", "bird", "missing-version"),
            ] {
                set(&db,"entries",&format!("{alice_id}:{id}"),&json!({"id":format!("{alice_id}:{id}"),"user_id":alice_id,"version_id":version,"name":name})).unwrap();
            }
            set(&db,"entries",&format!("{bob_id}:b1"),&json!({"id":format!("{bob_id}:b1"),"user_id":bob_id,"version_id":"v1","name":"Cat"})).unwrap();
            // Publications: Alice published v1; a removed one for v2; Bob published v1 too.
            set(&db,"publications","p1",&json!({"id":"p1","publisher_id":alice_id,"version_id":"v1","state":"published"})).unwrap();
            set(&db,"publications","p2",&json!({"id":"p2","publisher_id":alice_id,"version_id":"v2","state":"removed"})).unwrap();
            set(&db,"publications","p3",&json!({"id":"p3","publisher_id":bob_id,"version_id":"v1","state":"published"})).unwrap();
            // Asset ownership rows for the quota aggregate.
            set(&db,"asset_owners",&format!("{alice_id}:x"),&json!({"user_id":alice_id,"size":100})).unwrap();
            set(&db,"asset_owners",&format!("{alice_id}:y"),&json!({"user_id":alice_id,"size":23})).unwrap();
            set(&db,"asset_owners",&format!("{bob_id}:x"),&json!({"user_id":bob_id,"size":999})).unwrap();
            set(&db,"asset_owners",&format!("{alice_id}:z"),&json!({"user_id":alice_id})).unwrap();
        }
        let token = alice["token"].as_str().unwrap();
        let call = |action: Value| {
            let s = s.clone();
            let headers = auth(token);
            async move { rpc(State(s), headers, Json(action)).await.unwrap().0 }
        };
        // Expected output, computed with the pre-change algorithm:
        // full scan in id order, in-memory filter, and a fresh lookup
        // context for every entry.
        let expected = |query: &str, offset: usize| {
            let db = s.db.lock().unwrap();
            let rows: Vec<_> = all(&db, "entries")
                .unwrap()
                .into_iter()
                .filter(|e| {
                    e["user_id"] == alice_id
                        && e["name"]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&query.to_lowercase())
                })
                .map(|entry| {
                    workshop::library_entry(
                        &db,
                        &mut workshop::RequestCache::default(),
                        entry,
                        &alice_id,
                    )
                })
                .collect::<Result<Vec<_>>>()
                .unwrap();
            json!({"total":rows.len(),"entries":rows.into_iter().skip(offset).take(10).collect::<Vec<_>>()})
        };
        for (query, offset) in [("", 0), ("cat", 0), ("CAT", 0), ("", 2), ("zzz", 0)] {
            let actual = call(json!({"action":"library","query":query,"offset":offset})).await;
            assert_eq!(actual, expected(query, offset), "query={query} offset={offset}");
        }
        // The version action resolves both names and the publication
        // through the same cached lookups.
        let version = call(json!({"action":"version","version_id":"v1"})).await;
        assert_eq!(version["creator_name"], json!("Bob"));
        assert_eq!(version["saved_by_name"], json!("Alice"));
        assert_eq!(version["publication"]["id"], json!("p1"));
        // SQL aggregate equals the in-memory sum (missing size = 0).
        {
            let db = s.db.lock().unwrap();
            let manual: u64 = all(&db, "asset_owners")
                .unwrap()
                .iter()
                .filter(|r| r["user_id"] == alice_id)
                .map(|r| r["size"].as_u64().unwrap_or(0))
                .sum();
            assert_eq!(stored_asset_bytes(&db, &alice_id).unwrap(), manual);
            assert_eq!(manual, 123);
        }
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

    #[test]
    fn asset_validation_is_header_first_but_keeps_its_strength() {
        let dir = std::env::temp_dir().join(format!("kokoverse-validate-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        };
        // A valid small PNG passes, as before.
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(8, 8)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        assert!(validate_asset(&write("ok.png", &png)).is_ok());
        // Over-limit dimensions are rejected from the header.
        let mut wide = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(4097, 1)
            .write_to(&mut wide, image::ImageFormat::Png)
            .unwrap();
        let wide = write("wide.png", &wide.into_inner());
        let error = validate_asset(&wide).unwrap_err();
        assert!(error.1.contains("4096"), "{}", error.1);
        // A truncated PNG still fails the decode check.
        assert!(validate_asset(&write("cut.png", &png[..png.len() / 2])).is_err());
        // Tiny files are rejected on size alone.
        assert!(validate_asset(&write("tiny.bin", b"abc")).is_err());
        // A minimal valid GLB passes; a truncated one does not.
        let mut document = serde_json::to_vec(&json!({"asset":{"version":"2.0"},"meshes":[{}]})).unwrap();
        while !document.len().is_multiple_of(4) {
            document.push(b' ');
        }
        let mut glb = b"glTF".to_vec();
        glb.extend(2u32.to_le_bytes());
        glb.extend((20u32 + document.len() as u32).to_le_bytes());
        glb.extend((document.len() as u32).to_le_bytes());
        glb.extend(b"JSON");
        glb.extend(&document);
        assert!(validate_asset(&write("ok.glb", &glb)).is_ok());
        assert!(validate_asset(&write("cut.glb", &glb[..glb.len() - 4])).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod sweep_tests {
    use super::*;

    fn server() -> Server {
        Server::open(
            std::env::temp_dir().join(format!("kokoverse-sweep-{}", Uuid::new_v4())),
            "127.0.0.1:48001".into(),
            1024,
        )
        .unwrap()
    }

    #[test]
    fn sweep_removes_only_expired_records_and_stamps_legacy_rows() {
        let s = server();
        let now = 100_000_000u64;
        {
            let db = s.db.lock().unwrap();
            set(&db, "tickets", "t-old", &json!({"expires_at":now-1})).unwrap();
            set(&db, "tickets", "t-new", &json!({"expires_at":now+100})).unwrap();
            set(&db, "placements", "p-old", &json!({"expires_at":now-1})).unwrap();
            set(&db, "placements", "p-new", &json!({"expires_at":now+60})).unwrap();
            set(
                &db,
                "sessions",
                "s-ended",
                &json!({"state":"ended","heartbeat":now}),
            )
            .unwrap();
            set(
                &db,
                "sessions",
                "s-stale",
                &json!({"state":"ready","heartbeat":now-DAY-1}),
            )
            .unwrap();
            set(
                &db,
                "sessions",
                "s-live",
                &json!({"state":"ready","heartbeat":now}),
            )
            .unwrap();
            set(
                &db,
                "requests",
                "r-old",
                &json!({"fingerprint":"f","result":{},"created_at":now-DAY-1}),
            )
            .unwrap();
            set(&db, "requests", "r-legacy", &json!({"fingerprint":"f","result":{}})).unwrap();
            set(&db, "users", "u-old", &json!({"last_seen":now-30*DAY-1})).unwrap();
            set(&db, "users", "u-legacy", &json!({"id":"x","name":"Legacy"})).unwrap();

            assert_eq!(sweep_expired_at(&db, now).unwrap(), 6);
            for (kind, id) in [
                ("tickets", "t-new"),
                ("placements", "p-new"),
                ("sessions", "s-live"),
                ("requests", "r-legacy"),
                ("users", "u-legacy"),
            ] {
                assert!(get(&db, kind, id).unwrap().is_some(), "{kind}/{id} kept");
            }
            for (kind, id) in [
                ("tickets", "t-old"),
                ("placements", "p-old"),
                ("sessions", "s-ended"),
                ("sessions", "s-stale"),
                ("requests", "r-old"),
                ("users", "u-old"),
            ] {
                assert!(get(&db, kind, id).unwrap().is_none(), "{kind}/{id} removed");
            }
            // Legacy rows are stamped, then age out one grace period later.
            assert_eq!(
                get(&db, "requests", "r-legacy").unwrap().unwrap()["created_at"],
                json!(now)
            );
            assert_eq!(
                get(&db, "users", "u-legacy").unwrap().unwrap()["last_seen"],
                json!(now)
            );
            // One day later the short-lived rows kept above have expired too;
            // only the stamped user row (30-day grace) survives.
            assert_eq!(sweep_expired_at(&db, now + DAY + 1).unwrap(), 4);
            assert!(get(&db, "requests", "r-legacy").unwrap().is_none());
            assert!(get(&db, "users", "u-legacy").unwrap().is_some());
        }
        let root = s.root.clone();
        drop(s);
        std::fs::remove_dir_all(root).unwrap();
    }
}

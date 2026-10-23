use super::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct License {
    save: bool,
    modify: bool,
    redistribute: bool,
}
impl Default for License {
    fn default() -> Self { Self { save: true, modify: true, redistribute: true } }
}

pub(super) fn user_name(db: &Connection, uid: &str) -> Result<Value> {
    Ok(all(db, "handles")?
        .into_iter()
        .chain(all(db, "users")?)
        .find(|u| {
            u["id"] == uid
                || u["aliases"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(uid)))
        })
        .map(|u| u["name"].clone())
        .unwrap_or(json!("Unknown creator")))
}

pub(super) fn active_publication(
    db: &Connection,
    uid: &str,
    version: &str,
) -> Result<Option<Value>> {
    Ok(all(db, "publications")?.into_iter().find(|p| {
        p["publisher_id"] == uid && p["version_id"] == version && p["state"] == "published"
    }))
}

pub(super) fn library_entry(db: &Connection, mut entry: Value, uid: &str) -> Result<Value> {
    let id = entry["version_id"].as_str().unwrap_or("");
    if let Some(version) = get(db, "versions", id)? {
        let id = id.to_owned();
        entry["permissions"] = json!(prototype_permissions());
        entry["kind"] = version["object"]["visual"]["kind"].clone();
        entry["script_count"] = json!(version["object"]["scripts"].as_array().map_or(0, Vec::len));
        entry["creator_name"] = user_name(
            db,
            version["object"]["item_identity"]["creator_id"]
                .as_str()
                .unwrap_or(""),
        )?;
        entry["publication"] = active_publication(db, uid, &id)?
            .map(|p| p["id"].clone())
            .unwrap_or(Value::Null);
    }
    Ok(entry)
}

fn view(db: &Connection, mut p: Value, uid: &str) -> Result<Value> {
    let g = get(
        db,
        "grants",
        &format!("{uid}:{}", p["version_id"].as_str().unwrap_or("")),
    )?;
    p["owned"] = json!(g.is_some());
    p["in_library"] = json!(
        get(
            db,
            "entries",
            &format!("{uid}:{}", p["version_id"].as_str().unwrap_or(""))
        )?
        .is_some()
    );
    p["my_permissions"] = json!(prototype_permissions());
    p["license"] = json!(License::default());
    p["can_manage"] = json!(same_user(
        db,
        p["publisher_id"].as_str().unwrap_or(""),
        uid
    )?);
    // Only publication metadata is exposed before acquisition: no object, source code or asset IDs.
    Ok(p)
}

pub(super) fn list(db: &Connection, v: &Value, uid: &str) -> Result<Value> {
    let query = v["query"].as_str().unwrap_or("").trim().to_lowercase();
    let mine = v["mine"] == true;
    let mut rows: Vec<_> = all(db, "publications")?
        .into_iter()
        .filter(|p| {
            p["state"] == "published"
                && (!mine || p["publisher_id"] == uid)
                && ["title", "description", "publisher_name", "creator_name"]
                    .iter()
                    .any(|key| {
                        p[key]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&query)
                    })
        })
        .collect();
    rows.sort_by(|a, b| {
        b["published_at"]
            .as_u64()
            .cmp(&a["published_at"].as_u64())
            .then_with(|| b["id"].as_str().cmp(&a["id"].as_str()))
    });
    let total = rows.len();
    let offset = v["offset"].as_u64().unwrap_or(0).min(100000) as usize;
    let entries = rows
        .into_iter()
        .skip(offset)
        .take(12)
        .map(|p| view(db, p, uid))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({"entries":entries,"total":total}))
}

pub(super) fn detail(db: &Connection, id: &str, uid: &str) -> Result<Value> {
    let p = get(db, "publications", id)?.ok_or_else(missing)?;
    if p["state"] != "published" && !same_user(db, p["publisher_id"].as_str().unwrap_or(""), uid)? {
        return Err(missing());
    }
    view(db, p, uid)
}

pub(super) fn publish(db: &Connection, v: &Value, u: &Value) -> Result<Value> {
    let uid = u["id"].as_str().unwrap();
    let id = text(v, "version_id")?;
    let license = License::default();
    if active_publication(db, uid, id)?.is_some() {
        return Err(Error(
            StatusCode::CONFLICT,
            "This saved version is already published."
                .into(),
        ));
    }
    if all(db, "publications")?
        .iter()
        .filter(|p| p["publisher_id"] == uid && p["state"] == "published")
        .count()
        >= 100
    {
        return Err(bad("At most 100 active publications per HN"));
    }
    let description = v["description"].as_str().unwrap_or("").trim();
    if description.chars().count() > 2000
        || description
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(bad("Description must be at most 2000 characters"));
    }
    let version = get(db, "versions", id)?.ok_or_else(missing)?;
    let creator = version["object"]["item_identity"]["creator_id"]
        .as_str()
        .unwrap_or(uid);
    let publication_id = Uuid::new_v4().to_string();
    let p = json!({"id":publication_id,"version_id":id,"publisher_id":uid,"publisher_name":u["name"],"creator_name":user_name(db,creator)?,"title":label(v,"title",80)?,"description":description,"license":license,"thumbnail":version["thumbnail"],"kind":version["object"]["visual"]["kind"],"script_count":version["object"]["scripts"].as_array().map_or(0,Vec::len),"saved_at":version["saved_at"],"published_at":now(),"state":"published","acquisitions":0});
    set(db, "publications", &publication_id, &p)?;
    view(db, p, uid)
}

pub(super) fn unpublish(db: &Connection, v: &Value, uid: &str) -> Result<Value> {
    let id = text(v, "publication_id")?;
    let mut p = get(db, "publications", id)?.ok_or_else(missing)?;
    if !same_user(db, p["publisher_id"].as_str().unwrap_or(""), uid)? {
        return Err(denied());
    }
    p["state"] = json!("unpublished");
    p["unpublished_at"] = json!(now());
    set(db, "publications", id, &p)?;
    view(db, p, uid)
}

pub(super) fn acquire(db: &Connection, v: &Value, uid: &str) -> Result<Value> {
    let id = text(v, "publication_id")?;
    let mut p = get(db, "publications", id)?.ok_or_else(missing)?;
    if p["state"] != "published" {
        return Err(missing());
    }
    let version = p["version_id"].as_str().unwrap().to_owned();
    let permissions = prototype_permissions();
    add_grant(db, uid, &version, &p["title"], &permissions)?;
    // Keep acquisitions as catalog history while licensing is deferred.
    let mut g = grant(db, &version, uid)?;
    g["permissions"] = json!(permissions);
    set(db, "grants", &format!("{uid}:{version}"), &g)?;
    let key = format!("{uid}:{id}");
    if get(db, "acquisitions", &key)?.is_none() {
        set(
            db,
            "acquisitions",
            &key,
            &json!({"user_id":uid,"publication_id":id,"acquired_at":now(),"permissions":permissions}),
        )?;
        p["acquisitions"] = json!(p["acquisitions"].as_u64().unwrap_or(0) + 1);
        set(db, "publications", id, &p)?;
    }
    Ok(
        json!({"entry":library_entry(db,get(db,"entries",&format!("{uid}:{version}"))?.ok_or_else(missing)?,uid)?,"publication":view(db,p,uid)?}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE records(kind TEXT,id TEXT,value TEXT,PRIMARY KEY(kind,id));",
        )
        .unwrap();
        db
    }
    fn saved(db: &Connection) -> Value {
        let object = kokoverse_core::WorldModel::new()
            .snapshot()
            .objects
            .remove(0);
        save_item(db, &json!({"name":"Original","object":object}), "author").unwrap()
    }
    fn request(version: &Value, license: Value) -> Value {
        json!({"version_id":version["id"],"title":"Reusable cube","description":"A public preview","license":license})
    }
    #[test]
    fn acquisition_is_idempotent_and_other_users_can_republish() {
        let db = database();
        let author = json!({"id":"author","name":"Creator"});
        let version = saved(&db);
        let limited = request(
            &version,
            json!({"save":true,"modify":false,"redistribute":false}),
        );
        let first = publish(&db, &limited, &author).unwrap();
        assert!(publish(&db, &limited, &author).is_err());
        let before = detail(&db, first["id"].as_str().unwrap(), "reader").unwrap();
        assert_eq!(before["owned"], false);
        assert!(before.get("object").is_none());
        assert!(before.get("assets").is_none());
        let get_first = json!({"publication_id":first["id"]});
        acquire(&db, &get_first, "reader").unwrap();
        acquire(&db, &get_first, "reader").unwrap();
        assert_eq!(
            detail(&db, first["id"].as_str().unwrap(), "author").unwrap()["acquisitions"],
            1
        );
        assert_eq!(
            grant(&db, version["id"].as_str().unwrap(), "reader").unwrap()["permissions"]["modify"],
            true
        );
        assert!(publish(&db, &limited, &json!({"id":"reader","name":"Reader"})).is_ok());
        assert!(unpublish(&db, &get_first, "reader").is_err());
        unpublish(&db, &get_first, "author").unwrap();
        assert!(acquire(&db, &get_first, "reader").is_err());
        assert!(grant(&db, version["id"].as_str().unwrap(), "reader").is_ok());
        let open = publish(
            &db,
            &request(
                &version,
                json!({"save":true,"modify":true,"redistribute":true}),
            ),
            &author,
        )
        .unwrap();
        acquire(&db, &json!({"publication_id":open["id"]}), "reader").unwrap();
        let g = grant(&db, version["id"].as_str().unwrap(), "reader").unwrap();
        assert_eq!(g["permissions"]["modify"], true);
        assert_eq!(g["permissions"]["redistribute"], true);
        let entry_id = format!("reader:{}", version["id"].as_str().unwrap());
        db.execute(
            "DELETE FROM records WHERE kind='entries' AND id=?",
            [entry_id],
        )
        .unwrap();
        assert_eq!(
            detail(&db, open["id"].as_str().unwrap(), "reader").unwrap()["in_library"],
            false
        );
        acquire(&db, &json!({"publication_id":open["id"]}), "reader").unwrap();
        assert_eq!(
            detail(&db, open["id"].as_str().unwrap(), "reader").unwrap()["in_library"],
            true
        );
        assert_eq!(
            get(&db, "versions", version["id"].as_str().unwrap())
                .unwrap()
                .unwrap(),
            version
        );
    }
    #[test]
    fn historical_license_flags_do_not_block_acquisition_or_derived_saves() {
        let db = database();
        let author_id = Uuid::new_v4().to_string();
        let reader_id = Uuid::new_v4().to_string();
        let author = json!({"id":author_id,"name":"Creator"});
        let object = kokoverse_core::WorldModel::new().snapshot().objects.remove(0);
        let mut version = save_item(&db, &json!({"name":"Original","object":object}), &author_id).unwrap();
        let id = version["id"].as_str().unwrap().to_string();
        version["permissions"] = json!({"modify":false,"redistribute":false});
        version["object"]["item_identity"]["permissions"] = version["permissions"].clone();
        set(&db,"versions",&id,&version).unwrap();
        add_grant(&db,&reader_id,&id,&json!("Existing restricted copy"),&ItemPermissions {modify:false,redistribute:false}).unwrap();

        let mut publication = publish(&db,&request(&version,json!({"save":false,"modify":false,"redistribute":false})),&author).unwrap();
        let publication_id = publication["id"].as_str().unwrap().to_string();
        // Seed an existing display-only publication, without migrating or rewriting it.
        publication["license"] = json!({"save":false,"modify":false,"redistribute":false});
        set(&db,"publications",&publication_id,&publication).unwrap();
        assert_eq!(detail(&db,&publication_id,&reader_id).unwrap()["license"]["save"],true);
        acquire(&db,&json!({"publication_id":publication_id}),&reader_id).unwrap();
        let mut copied = version["object"].clone();
        copied["item_identity"]["source_version"] = json!(id);
        let derived = save_item(&db,&json!({"name":"Reader edit","object":copied}),&reader_id).unwrap();
        assert_eq!(derived["object"]["item_identity"]["creator_id"],author_id);
        assert_eq!(derived["object"]["item_identity"]["owner_id"],reader_id);
        assert_eq!(derived["source_version"],id);
        assert_eq!(derived["permissions"]["redistribute"],true);
        assert!(publish(&db,&request(&version,Value::Null),&json!({"id":reader_id,"name":"Reader"})).is_ok());
        assert_eq!(get(&db,"versions",&id).unwrap().unwrap(),version);
        assert_eq!(get(&db,"publications",&publication_id).unwrap().unwrap()["license"]["save"],false);
    }
    #[test]
    fn catalog_is_paginated_searchable_and_excludes_unpublished_entries() {
        let db = database();
        let author = json!({"id":"author","name":"Creator"});
        for _ in 0..13 {
            let version = saved(&db);
            publish(
                &db,
                &request(
                    &version,
                    json!({"save":true,"modify":false,"redistribute":false}),
                ),
                &author,
            )
            .unwrap();
        }
        let first = list(&db, &json!({"query":"CREATOR"}), "reader").unwrap();
        assert_eq!(first["total"], 13);
        assert_eq!(first["entries"].as_array().unwrap().len(), 12);
        assert_eq!(
            list(&db, &json!({"offset":12}), "reader").unwrap()["entries"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            list(&db, &json!({"mine":true}), "reader").unwrap()["total"],
            0
        );
        unpublish(
            &db,
            &json!({"publication_id":first["entries"][0]["id"]}),
            "author",
        )
        .unwrap();
        assert_eq!(list(&db, &json!({}), "reader").unwrap()["total"], 12);
    }
}

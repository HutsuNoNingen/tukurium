//! Shared source drafts use stable character identities. Applying a draft is a separate script command.
use kokoverse_protocol::{Address, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

#[derive(Clone, Deserialize)]
pub struct Request {
    pub request_id: String,
    pub client_id: String,
    pub target: Address,
    pub script_id: String,
    pub action: String,
    #[serde(default)]
    pub generation: String,
    #[serde(default)]
    pub after: String,
    #[serde(default)]
    pub delete: Vec<String>,
    #[serde(default)]
    pub insert: String,
    #[serde(default)]
    pub anchor: String,
    #[serde(default)]
    pub head: String,
    #[serde(default)]
    pub name: String,
}
#[derive(Clone, Serialize)]
pub struct Cursor {
    pub client_id: String,
    pub name: String,
    pub anchor: usize,
    pub head: usize,
}
#[derive(Clone, Serialize)]
pub struct View {
    pub target: Address,
    pub script_id: String,
    pub generation: String,
    pub revision: u64,
    pub text: String,
    pub ids: Vec<String>,
    pub cursors: Vec<Cursor>,
}
#[derive(Serialize)]
pub struct Reply {
    pub view: View,
    pub inserted_ids: Vec<String>,
}
#[derive(Clone)]
struct Atom {
    after: u64,
    value: char,
    deleted: bool,
}
struct Presence {
    owner: String,
    name: String,
    anchor: u64,
    head: u64,
    updated: Instant,
}
struct Document {
    target: Address,
    script_id: String,
    generation: String,
    revision: u64,
    atoms: BTreeMap<u64, Atom>,
    next: u64,
    applied_source: String,
    cursors: BTreeMap<String, Presence>,
    seen: HashMap<String, Vec<String>>,
}
impl Document {
    fn new(target: Address, script_id: String, source: &str) -> Self {
        let mut doc = Self {
            target,
            script_id,
            generation: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            atoms: BTreeMap::new(),
            next: 1,
            applied_source: source.into(),
            cursors: BTreeMap::new(),
            seen: HashMap::new(),
        };
        let mut after = 0;
        for value in source.chars() {
            doc.atoms.insert(
                doc.next,
                Atom {
                    after,
                    value,
                    deleted: false,
                },
            );
            after = doc.next;
            doc.next += 1;
        }
        doc
    }
    fn ordered(&self) -> Vec<u64> {
        let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
        for (&id, atom) in &self.atoms {
            children.entry(atom.after).or_default().push(id);
        }
        let mut stack = children.remove(&0).unwrap_or_default();
        let mut ids = Vec::with_capacity(self.atoms.len());
        while let Some(id) = stack.pop() {
            ids.push(id);
            if let Some(next) = children.remove(&id) {
                stack.extend(next);
            }
        }
        ids
    }
    fn view(&self) -> View {
        let mut text = String::new();
        let mut ids = Vec::new();
        let mut positions = HashMap::from([(0, 0)]);
        for id in self.ordered() {
            let atom = &self.atoms[&id];
            if !atom.deleted {
                text.push(atom.value);
                ids.push(id.to_string());
            }
            positions.insert(id, ids.len());
        }
        View {
            target: self.target.clone(),
            script_id: self.script_id.clone(),
            generation: self.generation.clone(),
            revision: self.revision,
            text,
            ids,
            cursors: self
                .cursors
                .iter()
                .map(|(id, c)| Cursor {
                    client_id: id.clone(),
                    name: c.name.clone(),
                    anchor: *positions.get(&c.anchor).unwrap_or(&0),
                    head: *positions.get(&c.head).unwrap_or(&0),
                })
                .collect(),
        }
    }
}

#[derive(Default)]
pub struct ScriptDocuments {
    documents: HashMap<(Address, String), Document>,
}
impl ScriptDocuments {
    pub fn handle(
        &mut self,
        owner: &str,
        input: Request,
        world: &Snapshot,
        now: Instant,
    ) -> Result<Reply, String> {
        if input.request_id.is_empty()
            || input.request_id.len() > 80
            || input.client_id.is_empty()
            || input.client_id.len() > 80
            || input.name.chars().count() > 80
            || input.delete.len() > 8192
            || input.insert.len() > 8192
        {
            return Err("Invalid document request".into());
        }
        self.prune(world, now);
        let script = world
            .objects
            .iter()
            .find(|o| o.address == input.target)
            .and_then(|o| o.scripts.iter().find(|s| s.id == input.script_id))
            .ok_or("Script no longer exists")?;
        let key = (input.target.clone(), input.script_id.clone());
        if !self.documents.contains_key(&key) && input.action != "open" {
            return Err("Open the script document first".into());
        }
        let doc = self.documents.entry(key).or_insert_with(|| {
            Document::new(
                input.target.clone(),
                input.script_id.clone(),
                &script.source,
            )
        });
        if doc.applied_source != script.source {
            if doc.view().text == doc.applied_source {
                *doc = Document::new(
                    input.target.clone(),
                    input.script_id.clone(),
                    &script.source,
                );
            }
            doc.applied_source = script.source.clone();
        }
        if input.action == "open"
            && doc.cursors.is_empty()
            && doc.view().text == script.source
            && !doc.seen.is_empty()
        {
            *doc = Document::new(
                input.target.clone(),
                input.script_id.clone(),
                &script.source,
            );
        }
        if input.action != "open" && input.generation != doc.generation {
            return Err("Document changed; reopen it".into());
        }
        let peer = format!("{owner}/{}", input.client_id);
        if doc.cursors.len() >= 64 && !doc.cursors.contains_key(&peer) {
            return Err("Too many document editors".into());
        }
        let mut inserted_ids = Vec::new();
        match input.action.as_str() {
            "open" => {}
            "edit" => {
                let command = format!("{owner}/{}", input.request_id);
                if let Some(ids) = doc.seen.get(&command) {
                    inserted_ids = ids.clone();
                } else {
                    if doc.seen.len() >= 4096
                        || doc.atoms.len() + input.insert.chars().count() > 65536
                    {
                        return Err("Draft history is full; apply it and reopen after everyone leaves the editor".into());
                    }
                    let after = input
                        .after
                        .parse::<u64>()
                        .map_err(|_| "Invalid insertion anchor")?;
                    if after != 0 && !doc.atoms.contains_key(&after) {
                        return Err("Unknown insertion anchor".into());
                    }
                    let delete: HashSet<u64> = input
                        .delete
                        .iter()
                        .map(|id| id.parse::<u64>().map_err(|_| "Invalid deletion"))
                        .collect::<Result<_, _>>()?;
                    if delete.iter().any(|id| !doc.atoms.contains_key(id)) {
                        return Err("Unknown deleted character".into());
                    }
                    let visible = doc.view().text.len();
                    let removed: usize = delete
                        .iter()
                        .filter_map(|id| doc.atoms.get(id))
                        .filter(|a| !a.deleted)
                        .map(|a| a.value.len_utf8())
                        .sum();
                    if visible - removed + input.insert.len() > 8192 {
                        return Err("Source exceeds 8192 bytes".into());
                    }
                    for id in delete {
                        doc.atoms.get_mut(&id).unwrap().deleted = true;
                    }
                    let mut anchor = after;
                    for value in input.insert.chars() {
                        let id = doc.next;
                        doc.next += 1;
                        doc.atoms.insert(
                            id,
                            Atom {
                                after: anchor,
                                value,
                                deleted: false,
                            },
                        );
                        inserted_ids.push(id.to_string());
                        anchor = id;
                    }
                    doc.seen.insert(command, inserted_ids.clone());
                    doc.revision += 1;
                }
            }
            "cursor" => {}
            "leave" => {
                doc.cursors.remove(&peer);
            }
            _ => return Err("Unknown document action".into()),
        }
        if input.action != "leave" {
            let valid = |id: &str| {
                id.parse::<u64>()
                    .ok()
                    .filter(|id| *id == 0 || doc.atoms.contains_key(id))
                    .unwrap_or(0)
            };
            let old = doc.cursors.get(&peer);
            let anchor = if input.anchor.is_empty() {
                old.map_or(0, |c| c.anchor)
            } else {
                valid(&input.anchor)
            };
            let head = if input.head.is_empty() {
                old.map_or(0, |c| c.head)
            } else {
                valid(&input.head)
            };
            doc.cursors.insert(
                peer,
                Presence {
                    owner: owner.into(),
                    name: input.name,
                    anchor,
                    head,
                    updated: now,
                },
            );
        }
        Ok(Reply {
            view: doc.view(),
            inserted_ids,
        })
    }
    pub fn prune(&mut self, world: &Snapshot, now: Instant) {
        self.documents.retain(|(target, id), doc| {
            doc.cursors
                .retain(|_, c| now.duration_since(c.updated) < Duration::from_secs(6));
            world
                .objects
                .iter()
                .any(|o| &o.address == target && o.scripts.iter().any(|s| &s.id == id))
        });
    }
    pub fn forget(&mut self, owner: &str) {
        for doc in self.documents.values_mut() {
            doc.cursors.retain(|_, c| c.owner != owner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (ScriptDocuments, Snapshot, Request) {
        let mut world = crate::WorldModel::new();
        let target = world.snapshot().objects[0].address.clone();
        world
            .enqueue(kokoverse_protocol::WorldCommand::Script {
                command: kokoverse_protocol::ScriptCommand {
                    command_id: "a".into(),
                    target: target.clone(),
                    action: kokoverse_protocol::ScriptAction::Attach {
                        source: crate::scripting::ROTATOR.into(),
                    },
                },
            })
            .unwrap();
        world.tick();
        let snapshot = world.snapshot();
        let id = snapshot.objects[0].scripts[0].id.clone();
        let req = Request {
            request_id: "open".into(),
            client_id: "panel".into(),
            target,
            script_id: id,
            action: "open".into(),
            generation: String::new(),
            after: "0".into(),
            delete: vec![],
            insert: String::new(),
            anchor: String::new(),
            head: String::new(),
            name: "Alice".into(),
        };
        (ScriptDocuments::default(), snapshot, req)
    }
    #[test]
    fn concurrent_inserts_and_duplicate_delivery_preserve_both_edits() {
        let (mut docs, world, mut request) = setup();
        let now = Instant::now();
        let initial = docs.handle("alice", request.clone(), &world, now).unwrap();
        request.generation = initial.view.generation;
        request.action = "edit".into();
        request.insert = "A".into();
        request.request_id = "a".into();
        docs.handle("alice", request.clone(), &world, now).unwrap();
        request.insert = "日本".into();
        request.request_id = "b".into();
        let b = docs.handle("bob", request.clone(), &world, now).unwrap();
        assert!(b.view.text.starts_with("日本Areturn"));
        assert_eq!(
            docs.handle("bob", request, &world, now).unwrap().view.text,
            b.view.text
        );
        assert_eq!(
            world.objects[0].scripts[0].source,
            crate::scripting::ROTATOR
        );
    }
    #[test]
    fn deleting_original_characters_does_not_delete_concurrent_insertions() {
        let (mut docs, world, mut request) = setup();
        let now = Instant::now();
        let initial = docs.handle("a", request.clone(), &world, now).unwrap().view;
        request.generation = initial.generation;
        request.action = "edit".into();
        request.request_id = "insert".into();
        request.after = initial.ids[0].clone();
        request.insert = "X".into();
        docs.handle("b", request.clone(), &world, now).unwrap();
        request.request_id = "delete".into();
        request.insert = String::new();
        request.delete = initial.ids[..6].to_vec();
        let result = docs.handle("a", request, &world, now).unwrap();
        assert!(result.view.text.starts_with("X {"));
    }
    #[test]
    fn cursor_anchors_follow_insertions_and_expire_without_losing_the_draft() {
        let (mut docs, world, mut request) = setup();
        let now = Instant::now();
        let initial = docs
            .handle("alice", request.clone(), &world, now)
            .unwrap()
            .view;
        request.action = "cursor".into();
        request.generation = initial.generation;
        request.head = initial.ids[5].clone();
        request.anchor = request.head.clone();
        docs.handle("alice", request.clone(), &world, now).unwrap();
        request.request_id = "bob".into();
        request.name = "Bob".into();
        request.action = "edit".into();
        request.after = "0".into();
        request.insert = "日本".into();
        request.head = "0".into();
        request.anchor = "0".into();
        let view = docs
            .handle("bob", request.clone(), &world, now)
            .unwrap()
            .view;
        assert_eq!(
            view.cursors
                .iter()
                .find(|c| c.name == "Alice")
                .unwrap()
                .head,
            8
        );
        request.action = "open".into();
        request.request_id = "later".into();
        let later = docs
            .handle("bob", request, &world, now + Duration::from_secs(7))
            .unwrap()
            .view;
        assert_eq!(later.cursors.len(), 1);
        assert!(later.text.starts_with("日本return"));
    }

    #[test]
    fn oversized_edits_are_rejected_before_deleting_any_characters() {
        let (mut docs, world, mut request) = setup();
        let now = Instant::now();
        let initial = docs.handle("a", request.clone(), &world, now).unwrap().view;
        request.action = "edit".into();
        request.generation = initial.generation;
        request.delete = initial.ids[..3].to_vec();
        request.insert = "x".repeat(8192);
        request.request_id = "big".into();
        assert!(docs.handle("a", request.clone(), &world, now).is_err());
        request.action = "open".into();
        request.insert.clear();
        request.delete.clear();
        assert_eq!(
            docs.handle("a", request, &world, now).unwrap().view.text,
            initial.text
        );
    }
}

//! Session-only collaborative graph drafts. The host orders small graph operations;
//! applying the draft remains a separate, validated Script command.
use kokoverse_protocol::{Address, Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const PREFIX: &str = "@tukurium-graph-v1\n";

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
    pub ops: Vec<Value>,
}

#[derive(Clone, Serialize)]
pub struct View {
    pub target: Address,
    pub script_id: String,
    pub generation: String,
    pub revision: u64,
    pub graph: Value,
    pub dirty: bool,
    pub editors: usize,
}

struct Document {
    target: Address,
    script_id: String,
    generation: String,
    revision: u64,
    graph: Value,
    applied_source: String,
    dirty: bool,
    editors: HashMap<String, Instant>,
    seen: HashSet<String>,
}

impl Document {
    fn new(target: Address, script_id: String, source: &str) -> Result<Self, String> {
        let graph = serde_json::from_str(source.strip_prefix(PREFIX).ok_or("Not a node graph")?)
            .map_err(|_| "Invalid graph source")?;
        validate_graph(&graph)?;
        Ok(Self {
            target,
            script_id,
            generation: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            graph,
            applied_source: source.into(),
            dirty: false,
            editors: HashMap::new(),
            seen: HashSet::new(),
        })
    }
    fn view(&self) -> View {
        View {
            target: self.target.clone(),
            script_id: self.script_id.clone(),
            generation: self.generation.clone(),
            revision: self.revision,
            graph: self.graph.clone(),
            dirty: self.dirty,
            editors: self.editors.len(),
        }
    }
    fn source(&self) -> String {
        format!("{PREFIX}{}", self.graph)
    }
}

#[derive(Default)]
pub struct GraphDocuments {
    documents: HashMap<(Address, String), Document>,
}

impl GraphDocuments {
    pub fn handle(
        &mut self,
        owner: &str,
        input: Request,
        world: &Snapshot,
        now: Instant,
    ) -> Result<View, String> {
        if input.request_id.is_empty()
            || input.request_id.len() > 80
            || input.client_id.is_empty()
            || input.client_id.len() > 80
            || input.ops.len() > 32
        {
            return Err("Invalid graph request".into());
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
            return Err("Open the graph draft first".into());
        }
        if !self.documents.contains_key(&key) {
            self.documents.insert(
                key.clone(),
                Document::new(
                    input.target.clone(),
                    input.script_id.clone(),
                    &script.source,
                )?,
            );
        }
        let doc = self.documents.get_mut(&key).unwrap();
        if doc.applied_source != script.source {
            if !doc.dirty {
                *doc = Document::new(
                    input.target.clone(),
                    input.script_id.clone(),
                    &script.source,
                )?;
            } else if doc.source() == script.source {
                doc.dirty = false;
            }
            doc.applied_source = script.source.clone();
        }
        if input.action == "open" && doc.editors.is_empty() && !doc.dirty && !doc.seen.is_empty() {
            *doc = Document::new(
                input.target.clone(),
                input.script_id.clone(),
                &script.source,
            )?;
        }
        if input.action != "open" && input.generation != doc.generation {
            return Err("Graph draft changed; reopen it".into());
        }
        let peer = format!("{owner}/{}", input.client_id);
        match input.action.as_str() {
            "open" => {
                if doc.editors.len() >= 64 && !doc.editors.contains_key(&peer) {
                    return Err("Too many graph editors".into());
                }
                doc.editors.insert(peer, now);
            }
            "edit" => {
                if !doc.editors.contains_key(&peer) {
                    return Err("Open the graph draft first".into());
                }
                let command = format!("{owner}/{}", input.request_id);
                if !doc.seen.contains(&command) {
                    if input.ops.is_empty() || doc.seen.len() >= 4096 {
                        return Err("Graph draft history is full or edit is empty".into());
                    }
                    let mut graph = doc.graph.clone();
                    for op in &input.ops {
                        apply_op(&mut graph, op)?;
                    }
                    validate_graph(&graph)?;
                    doc.graph = graph;
                    doc.revision += 1;
                    doc.dirty = doc.source() != script.source;
                    doc.seen.insert(command);
                }
                doc.editors.insert(peer, now);
            }
            "leave" => {
                doc.editors.remove(&peer);
            }
            _ => return Err("Unknown graph action".into()),
        }
        Ok(doc.view())
    }

    pub fn source(
        &self,
        target: &Address,
        script_id: &str,
        generation: &str,
        revision: u64,
    ) -> Result<String, String> {
        let doc = self
            .documents
            .get(&(target.clone(), script_id.into()))
            .ok_or("Open the graph draft first")?;
        if doc.generation != generation || doc.revision != revision {
            return Err("Graph changed while applying; try Apply again".into());
        }
        Ok(doc.source())
    }

    pub fn prune(&mut self, world: &Snapshot, now: Instant) {
        self.documents.retain(|(target, id), doc| {
            doc.editors
                .retain(|_, seen| now.saturating_duration_since(*seen) < Duration::from_secs(120));
            world
                .objects
                .iter()
                .any(|o| o.address == *target && o.scripts.iter().any(|s| s.id == *id))
        });
    }
}

fn ident(value: &Value) -> Result<&str, String> {
    let name = value.as_str().ok_or("Expected graph identifier")?;
    if name.is_empty()
        || name.len() > 48
        || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err("Invalid graph identifier".into());
    }
    Ok(name)
}

fn validate_graph(graph: &Value) -> Result<(), String> {
    let nodes = graph["nodes"]
        .as_array()
        .ok_or("Graph nodes must be an array")?;
    let links = graph["links"]
        .as_array()
        .ok_or("Graph links must be an array")?;
    if nodes.len() > 64
        || links.len() > 128
        || graph["name"]
            .as_str()
            .is_none_or(|n| n.len() > 40 || n.is_empty())
    {
        return Err("Graph exceeds limits".into());
    }
    let mut ids = HashSet::new();
    for node in nodes {
        let id = ident(&node["id"])?;
        if !ids.insert(id)
            || ident(&node["kind"]).is_err()
            || !node["x"].as_f64().is_some_and(f64::is_finite)
            || !node["y"].as_f64().is_some_and(f64::is_finite)
        {
            return Err("Invalid graph node".into());
        }
    }
    let mut inputs = HashSet::new();
    for link in links {
        let from = ident(&link["from"])?;
        let to = ident(&link["to"])?;
        let input = ident(&link["input"])?;
        ident(&link["out"])?;
        if !ids.contains(from) || !ids.contains(to) || !inputs.insert((to, input)) {
            return Err("Invalid graph link".into());
        }
    }
    if format!("{PREFIX}{graph}").len() > 8192 {
        return Err("Graph exceeds 8192 bytes".into());
    }
    Ok(())
}

fn apply_op(graph: &mut Value, op: &Value) -> Result<(), String> {
    let kind = op["kind"].as_str().ok_or("Missing graph operation")?;
    match kind {
        "name" => {
            let name = op["value"].as_str().ok_or("Invalid graph name")?;
            graph["name"] = json!(name);
        }
        "add_node" => {
            let node = &op["node"];
            let id = ident(&node["id"])?;
            if graph["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["id"] == id)
            {
                return Err("Node ID already exists".into());
            }
            graph["nodes"].as_array_mut().unwrap().push(node.clone());
        }
        "remove_node" => {
            let id = ident(&op["id"])?;
            graph["nodes"]
                .as_array_mut()
                .unwrap()
                .retain(|n| n["id"] != id);
            graph["links"]
                .as_array_mut()
                .unwrap()
                .retain(|l| l["from"] != id && l["to"] != id);
        }
        "position" | "value" => {
            let id = ident(&op["id"])?;
            let node = graph["nodes"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|n| n["id"] == id)
                .ok_or("Node was removed")?;
            if kind == "value" {
                node["value"] = op["value"].clone();
            } else {
                node["x"] = op["x"].clone();
                node["y"] = op["y"].clone();
            }
        }
        "connect" => {
            let link = &op["link"];
            ident(&link["from"])?;
            ident(&link["out"])?;
            ident(&link["to"])?;
            ident(&link["input"])?;
            graph["links"]
                .as_array_mut()
                .unwrap()
                .retain(|l| l["to"] != link["to"] || l["input"] != link["input"]);
            graph["links"].as_array_mut().unwrap().push(link.clone());
        }
        "disconnect" => {
            let link = &op["link"];
            graph["links"].as_array_mut().unwrap().retain(|l| l != link);
        }
        _ => return Err("Unknown graph operation".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_edits_merge_and_invalid_batch_is_atomic() {
        let mut graph = json!({"name":"G","nodes":[{"id":"a","kind":"number","x":0,"y":0,"value":1},{"id":"b","kind":"number","x":1,"y":1,"value":2}],"links":[]});
        apply_op(&mut graph, &json!({"kind":"value","id":"a","value":3})).unwrap();
        apply_op(&mut graph, &json!({"kind":"position","id":"b","x":5,"y":6})).unwrap();
        assert_eq!(graph["nodes"][0]["value"], 3);
        assert_eq!(graph["nodes"][1]["x"], 5);
        let mut candidate = graph.clone();
        apply_op(
            &mut candidate,
            &json!({"kind":"connect","link":{"from":"a","out":"value","to":"missing","input":"a"}}),
        )
        .unwrap();
        assert!(validate_graph(&candidate).is_err());
        assert!(validate_graph(&graph).is_ok());
    }
}

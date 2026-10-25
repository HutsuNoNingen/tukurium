use super::{Engine, Origin};
use kokoverse_core::graph_documents::{GraphDocuments, Request};
use kokoverse_protocol::{ScriptAction, ScriptCommand, WorldCommand};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Bridge {
    pub documents: GraphDocuments,
    pending: HashMap<String, (u64, Instant)>,
}

impl Engine {
    pub(super) fn graph_document_message(&mut self, origin: Origin, body: &Value) {
        if body["kind"] == "script_graph_state" {
            if matches!(origin, Origin::Peer(ref p) if p == &self.host_peer) {
                self.ui(body.clone());
            }
            return;
        }
        if body["kind"] == "script_graph_result" {
            if matches!(origin, Origin::Peer(ref p) if p == &self.host_peer)
                && let Some(id) = body["request_id"].as_str()
                && let Some((client, _)) = self.graph_documents.pending.remove(id)
            {
                let response = self.envelope(body.clone());
                self.send(client, response);
            }
            return;
        }
        let Ok(request) = serde_json::from_value::<Request>(body["request"].clone()) else {
            return;
        };
        if !self.host {
            if let Origin::Local(client) = origin {
                if request.action == "apply" {
                    if self.pending.len() >= 128 {
                        self.reply(Origin::Local(client), json!({"kind":"result","command_id":request.request_id,"ok":false,"message":"Too many pending edits"}));
                        return;
                    }
                    self.pending.insert(
                        request.request_id.clone(),
                        (client, Instant::now(), request.request_id.clone()),
                    );
                } else {
                    self.graph_documents
                        .pending
                        .retain(|_, (_, at)| at.elapsed() < Duration::from_secs(10));
                    if self.graph_documents.pending.len() >= 128 {
                        return;
                    }
                    self.graph_documents
                        .pending
                        .insert(request.request_id.clone(), (client, Instant::now()));
                }
                let host = self.host_peer.clone();
                self.network(Some(&host), body.clone());
            }
            return;
        }
        let owner = match &origin {
            Origin::Local(id) => format!("local-{id}"),
            Origin::Peer(peer) => peer.clone(),
        };
        let response = if request.action == "apply" {
            let expected = body["request"]["revision"].as_u64();
            let source = expected
                .ok_or_else(|| "Missing graph revision".to_string())
                .and_then(|revision| {
                    self.graph_documents.documents.source(
                        &request.target,
                        &request.script_id,
                        &request.generation,
                        revision,
                    )
                });
            match source {
                Ok(source) => {
                    let Some(script) = self
                        .model
                        .snapshot()
                        .objects
                        .into_iter()
                        .find(|o| o.address == request.target)
                        .and_then(|o| o.scripts.into_iter().find(|s| s.id == request.script_id))
                    else {
                        self.reply(origin, json!({"kind":"result","command_id":request.request_id,"ok":false,"message":"Script no longer exists"}));
                        return;
                    };
                    let mut parameters = script.parameters;
                    if let Some(extra) = body["request"]["parameters"].as_object() {
                        for (key, value) in extra {
                            parameters.insert(key.clone(), value.clone());
                        }
                    }
                    let nodes =
                        serde_json::from_str::<Value>(&source["@tukurium-graph-v1\n".len()..])
                            .unwrap_or_default();
                    parameters.retain(|key, _| {
                        nodes["nodes"].as_array().is_some_and(|items| {
                            items.iter().any(|node| {
                                let id = node["id"].as_str().unwrap_or("");
                                (key == &format!("ref_{id}")
                                    && ["read_field", "write_field", "drive_field"]
                                        .contains(&node["kind"].as_str().unwrap_or("")))
                                    || (key == &format!("p_{id}")
                                        && node["kind"] == "public_number")
                            })
                        })
                    });
                    let enabled = body["request"]["enabled"]
                        .as_bool()
                        .unwrap_or(script.enabled);
                    let expected_revision = body["request"]["expected_revision"]
                        .as_str()
                        .unwrap_or("")
                        .to_owned();
                    self.queue_command(
                        &owner,
                        WorldCommand::Script {
                            command: ScriptCommand {
                                command_id: request.request_id,
                                target: request.target,
                                action: ScriptAction::Update {
                                    script_id: request.script_id,
                                    expected_revision,
                                    source,
                                    enabled,
                                    parameters,
                                },
                            },
                        },
                        origin,
                    );
                    return;
                }
                Err(message) => {
                    json!({"kind":"result","command_id":request.request_id,"ok":false,"message":message})
                }
            }
        } else {
            match self.graph_documents.documents.handle(
                &owner,
                request.clone(),
                &self.model.snapshot(),
                Instant::now(),
            ) {
                Ok(view) => {
                    let update = json!({"kind":"script_graph_state","document":view});
                    self.ui(update.clone());
                    self.network(None, update);
                    json!({"kind":"script_graph_result","request_id":request.request_id,"ok":true,"document":view})
                }
                Err(message) => {
                    json!({"kind":"script_graph_result","request_id":request.request_id,"ok":false,"message":message})
                }
            }
        };
        match origin {
            Origin::Local(id) => {
                let response = self.envelope(response);
                self.send(id, response);
            }
            Origin::Peer(peer) => self.network(Some(&peer), response),
        }
    }
}

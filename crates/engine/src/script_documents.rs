use super::{Engine, Origin};
use kokoverse_core::script_documents::{Request, ScriptDocuments};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Bridge {
    documents: ScriptDocuments,
    pending: HashMap<String, (u64, Instant)>,
}
impl Engine {
    pub(super) fn script_document_message(&mut self, origin: Origin, body: &Value) {
        if body["kind"] == "script_document_state" {
            if matches!(origin, Origin::Peer(ref p) if p == &self.host_peer) {
                self.ui(body.clone());
            }
            return;
        }
        if body["kind"] == "script_document_result" {
            if matches!(origin, Origin::Peer(ref p) if p == &self.host_peer)
                && let Some(id) = body["request_id"].as_str()
                && let Some((client, _)) = self.documents.pending.remove(id)
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
                self.documents
                    .pending
                    .retain(|_, (_, sent)| sent.elapsed() < Duration::from_secs(10));
                if self.documents.pending.len() >= 128 {
                    return;
                }
                self.documents
                    .pending
                    .insert(request.request_id.clone(), (client, Instant::now()));
                let host = self.host_peer.clone();
                self.network(Some(&host), body.clone());
            }
            return;
        }
        let owner = match &origin {
            Origin::Local(id) => format!("local-{id}"),
            Origin::Peer(peer) => peer.clone(),
        };
        let response = match self.documents.documents.handle(
            &owner,
            request.clone(),
            &self.model.snapshot(),
            Instant::now(),
        ) {
            Ok(reply) => {
                let update = json!({"kind":"script_document_state","document":reply.view});
                self.ui(update.clone());
                self.network(None, update);
                json!({"kind":"script_document_result","request_id":request.request_id,"ok":true,"document":reply.view,"inserted_ids":reply.inserted_ids})
            }
            Err(message) => {
                json!({"kind":"script_document_result","request_id":request.request_id,"ok":false,"message":message})
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

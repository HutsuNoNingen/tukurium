//! Calibrate renderer clocks against the host's monotonic timeline.
use super::{Engine, Origin};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub(super) struct ClockSync {
    origin: Instant,
    epoch: f64,
    pending: HashMap<String, (u64, String, f64, Instant)>,
}
impl ClockSync {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
            epoch: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
            pending: HashMap::new(),
        }
    }
    pub fn now(&self) -> f64 {
        self.epoch + self.origin.elapsed().as_secs_f64()
    }
    pub fn expire(&mut self) {
        self.pending
            .retain(|_, (_, _, _, sent)| sent.elapsed() < Duration::from_secs(10));
    }
}
impl Engine {
    pub(super) fn clock_message(&mut self, origin: Origin, body: &Value) {
        let Some(request) = body["request_id"]
            .as_str()
            .filter(|r| !r.is_empty() && r.len() <= 80)
        else {
            return;
        };
        match body["kind"].as_str() {
            Some("clock_ping") => {
                let Some(client_time) = body["client_time"]
                    .as_f64()
                    .filter(|t| t.is_finite() && *t >= 0.0)
                else {
                    return;
                };
                if self.host {
                    let response = json!({"kind":"clock_pong","request_id":request,"client_time":client_time,"server_time":self.clock.now()});
                    match origin {
                        Origin::Local(id) => {
                            let response = self.envelope(response);
                            self.send(id, response);
                        }
                        Origin::Peer(peer) => self.network(Some(&peer), response),
                    }
                } else if let Origin::Local(id) = origin {
                    if self.clock.pending.len() >= 128 {
                        return;
                    }
                    let key = uuid::Uuid::new_v4().to_string();
                    self.clock.pending.insert(
                        key.clone(),
                        (id, request.into(), client_time, Instant::now()),
                    );
                    self.network(
                        Some(&self.host_peer.clone()),
                        json!({"kind":"clock_ping","request_id":key,"client_time":client_time}),
                    );
                }
            }
            Some("clock_pong") if matches!(origin,Origin::Peer(ref p) if p==&self.host_peer) => {
                let Some(server_time) = body["server_time"]
                    .as_f64()
                    .filter(|t| t.is_finite() && *t >= 0.0)
                else {
                    return;
                };
                if let Some((client, request, client_time, _)) = self.clock.pending.remove(request)
                {
                    let response=self.envelope(json!({"kind":"clock_pong","request_id":request,"client_time":client_time,"server_time":server_time}));
                    self.send(client, response);
                }
            }
            _ => {}
        }
    }
}

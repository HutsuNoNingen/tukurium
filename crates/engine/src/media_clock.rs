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
    offset: f64,
    calibrated: bool,
    best_rtt: f64,
    sample: Option<(String, f64, Instant)>,
    next_sample: Instant,
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
            offset: 0.0,
            calibrated: false,
            best_rtt: f64::INFINITY,
            sample: None,
            next_sample: Instant::now(),
        }
    }
    pub fn now(&self) -> f64 {
        self.epoch + self.origin.elapsed().as_secs_f64() + self.offset
    }
    pub fn ready(&self) -> bool {
        self.calibrated
    }
    fn probe(&mut self) -> Option<(String, f64)> {
        if Instant::now() < self.next_sample || self.sample.is_some() {
            return None;
        }
        let id = uuid::Uuid::new_v4().to_string();
        let sent = self.epoch + self.origin.elapsed().as_secs_f64();
        self.sample = Some((id.clone(), sent, Instant::now()));
        self.next_sample = Instant::now() + Duration::from_secs(2);
        Some((id, sent))
    }
    fn accept(&mut self, request: &str, server_time: f64) -> bool {
        let Some((id, sent, instant)) = self.sample.as_ref() else {
            return false;
        };
        if id != request {
            return false;
        }
        let rtt = instant.elapsed().as_secs_f64();
        if !self.calibrated || rtt <= self.best_rtt * 1.5 + 0.01 {
            self.offset = server_time - (*sent + rtt * 0.5);
            self.best_rtt = self.best_rtt.min(rtt);
            self.calibrated = true;
        }
        self.sample = None;
        true
    }
    pub fn expire(&mut self) {
        if self
            .sample
            .as_ref()
            .is_some_and(|(_, _, sent)| sent.elapsed() > Duration::from_secs(5))
        {
            self.sample = None;
        }
        self.pending
            .retain(|_, (_, _, _, sent)| sent.elapsed() < Duration::from_secs(10));
    }
}
impl Engine {
    pub(super) fn update_engine_clock(&mut self) {
        if !self.host
            && self.synced
            && let Some((request, sent)) = self.clock.probe()
        {
            let host = self.host_peer.clone();
            self.network(
                Some(&host),
                json!({"kind":"clock_ping", "request_id":request, "client_time":sent}),
            );
        }
    }
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
                if self.clock.accept(request, server_time) {
                    return;
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_drive_clock_corrects_machine_clock_skew_and_ignores_unmatched_responses() {
        let mut clock = ClockSync::new();
        let reference = clock.now();
        clock.epoch += 3600.0;
        let (id, _) = clock.probe().unwrap();
        assert!(!clock.accept("unmatched", reference));
        assert!(!clock.ready());
        assert!(clock.accept(&id, reference));
        assert!(clock.ready());
        assert!((clock.now() - reference).abs() < 0.1);
    }
}

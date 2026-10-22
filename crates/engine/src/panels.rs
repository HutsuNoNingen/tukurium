use super::*;
use kokoverse_protocol::PanelInput;

impl Engine {
    pub(super) fn panel_input(&mut self, id: u64, body: &Value) {
        let Ok(mut input) = serde_json::from_value::<PanelInput>(body["input"].clone()) else {
            return;
        };
        if input.command_id.is_empty() || input.command_id.len() > 80 {
            return;
        }
        if self.host {
            self.apply_panel_input(&format!("local-{id}"), input, Origin::Local(id));
        } else if self.pending_panels.len() < 128 {
            let original = input.command_id.clone();
            input.command_id = Uuid::new_v4().to_string();
            self.pending_panels.insert(
                input.command_id.clone(),
                (id, std::time::Instant::now(), original),
            );
            self.network(
                Some(&self.host_peer.clone()),
                json!({"kind":"panel_input","input":input}),
            );
        }
    }

    pub(super) fn apply_panel_input(&mut self, owner: &str, input: PanelInput, origin: Origin) {
        let id = input.command_id.clone();
        let result = self.panel_sessions.apply(
            owner,
            input,
            &self.model.snapshot(),
            std::time::Instant::now(),
        );
        self.publish_panel_sessions();
        self.reply(origin,json!({"kind":"panel_result","command_id":id,"ok":result.is_ok(),"owner":owner,"message":result.err()}));
    }

    pub(super) fn publish_panel_sessions(&mut self) {
        self.panel_sessions
            .prune(&self.model.snapshot(), std::time::Instant::now());
        let sessions = if self.host {
            self.panel_sessions.views()
        } else {
            self.remote_panel_sessions.clone()
        };
        let value = json!(sessions);
        if value != self.last_panel_sessions {
            self.last_panel_sessions = value.clone();
            self.ui(json!({"kind":"panel_sessions","sessions":value}));
            if self.host {
                self.network(None, json!({"kind":"panel_sessions","sessions":value}));
            }
        }
        let expired: Vec<_> = self
            .pending_panels
            .iter()
            .filter(|(_, (_, t, _))| t.elapsed() > Duration::from_secs(10))
            .map(|(k, _)| k.clone())
            .collect();
        for key in expired {
            if let Some((id, _, command)) = self.pending_panels.remove(&key) {
                self.reply(Origin::Local(id),json!({"kind":"panel_result","command_id":command,"ok":false,"message":"Panel request timed out; focus again"}));
            }
        }
    }

    pub(super) fn receive_panel_result(&mut self, mut body: Value) {
        let Some(command) = body["command_id"].as_str() else {
            return;
        };
        if let Some((id, _, original)) = self.pending_panels.remove(command) {
            body["command_id"] = json!(original);
            self.reply(Origin::Local(id), body);
        }
    }
}

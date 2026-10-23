use super::*;
use kokoverse_protocol::EditSessionRequest;

impl Engine {
    pub(super) fn edit_session(&mut self, id: u64, body: &Value) {
        let Ok(mut input) = serde_json::from_value::<EditSessionRequest>(body["request"].clone())
        else {
            return;
        };
        if input.command_id.is_empty() || input.command_id.len() > 80 {
            return;
        }
        if self.host {
            self.apply_edit_session(&format!("local-{id}"), input, Origin::Local(id));
        } else if self.pending.len() < 128 {
            let original = input.command_id.clone();
            input.command_id = Uuid::new_v4().to_string();
            self.pending.insert(
                input.command_id.clone(),
                (id, std::time::Instant::now(), original),
            );
            self.network(
                Some(&self.host_peer.clone()),
                json!({"kind":"edit_session","request":input}),
            );
        } else {
            self.reply(Origin::Local(id), json!({"kind":"result","command_id":input.command_id,"ok":false,"message":"Too many pending requests"}));
        }
    }

    pub(super) fn apply_edit_session(
        &mut self,
        owner: &str,
        input: EditSessionRequest,
        origin: Origin,
    ) {
        let id = input.command_id.clone();
        let result = self.edit_sessions.apply(
            owner,
            input,
            &self.model.snapshot(),
            std::time::Instant::now(),
        );
        self.publish_edit_sessions();
        self.reply(origin,json!({"kind":"result","command_id":id,"ok":result.is_ok(),"owner":owner,"message":result.err()}));
    }

    pub(super) fn publish_edit_sessions(&mut self) {
        self.edit_sessions
            .prune(&self.model.snapshot(), std::time::Instant::now());
        let sessions = if self.host {
            self.edit_sessions.views()
        } else {
            self.remote_edit_sessions.clone()
        };
        let value = json!(sessions);
        if value != self.last_edit_sessions {
            self.last_edit_sessions = value.clone();
            self.ui(json!({"kind":"edit_sessions","sessions":value}));
            if self.host {
                self.network(None, json!({"kind":"edit_sessions","sessions":value}));
            }
        }
    }
}

mod assets;
mod edit_sessions;
mod media_clock;
mod persistence;
mod previews;
use kokoverse_core::{WorldModel, replication::REGISTRY};
use kokoverse_protocol::{
    AvatarPose, GrabView, InteractionRay, ObjectPreview, ParticipantView, PlayerInput,
    TransformCommand,
};
use kokoverse_protocol::{
    Envelope, ReplicatedWorld, ReplicationCommit, VERSION, WorldCommand, read_frame, write_frame,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use uuid::Uuid;

enum Event {
    Connected(u64, bool, mpsc::Sender<Value>),
    Message(u64, Value),
    Closed(u64),
}

async fn connection(stream: TcpStream, id: u64, adapter: bool, events: mpsc::Sender<Event>) {
    let (mut reader, mut writer) = stream.into_split();
    let (out, mut incoming) = mpsc::channel::<Value>(64);
    if events
        .send(Event::Connected(id, adapter, out))
        .await
        .is_err()
    {
        return;
    }
    let task = tokio::spawn(async move {
        while let Some(value) = incoming.recv().await {
            match tokio::time::timeout(Duration::from_secs(5), write_frame(&mut writer, &value))
                .await
            {
                Ok(Ok(())) => {}
                _ => break,
            }
        }
    });
    loop {
        tokio::select! {
            result = read_frame(&mut reader) => {
                match result {
                    Ok(value) => if events.send(Event::Message(id, value)).await.is_err() { break; },
                    Err(_) => break,
                }
            },
            _ = events.closed() => break,
            _ = tokio::time::sleep(Duration::from_millis(100)) => if task.is_finished() { break; },
        }
    }
    task.abort();
    let _ = events.send(Event::Closed(id)).await;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Origin {
    Local(u64),
    Peer(String),
}

struct Engine {
    edit_sessions: kokoverse_core::edit_sessions::EditSessions,
    remote_edit_sessions: Vec<kokoverse_protocol::EditSession>,
    last_edit_sessions: Value,
    clock: media_clock::ClockSync,
    assets: assets::Assets,
    local_ray: Option<InteractionRay>,
    remote_rays: HashMap<String, InteractionRay>,
    rays_dirty: bool,
    grab_owner: Option<u64>,
    grab_command: Option<String>,
    last_grab: Option<GrabView>,
    previews: previews::Previews,
    model: WorldModel,
    host: bool,
    automatic_role: bool,
    host_peer: String,
    session: String,
    epoch: String,
    sequence: u64,
    synced: bool,
    adapter: Option<u64>,
    clients: HashMap<u64, mpsc::Sender<Value>>,
    pending: HashMap<String, (u64, std::time::Instant, String)>,
    deferred_results: HashMap<String, Value>,
    tickets: HashMap<u64, (String, String, Origin)>,
    inflight: HashSet<String>,
    peer: String,
    input_owner: Option<(u64, std::time::Instant)>,
    last_tick: std::time::Instant,
    pose_sequence: u64,
    remote_epochs: HashMap<String, String>,
    remote_seen: HashMap<String, std::time::Instant>,
    outcomes: HashMap<String, Value>,
    order: VecDeque<String>,
    last_script_status: Value,
    remote_script_status: Vec<kokoverse_protocol::ScriptStatus>,
}

impl Engine {
    fn sync_grab(&mut self) {
        let view = self.model.grab_view();
        if let Some(owner) = self.grab_owner {
            if let Some(grab) = &view {
                self.previews.set(
                    format!("local:{owner}"),
                    Some(grab.preview.clone()),
                    std::time::Instant::now(),
                );
            } else {
                self.previews.remove(&format!("local:{owner}"));
                self.grab_owner = None;
            }
        }
        if view != self.last_grab {
            self.last_grab = view.clone();
            self.ui(json!({"kind":"grab_state","grab":view}));
        }
        let ray = self.model.interaction_ray();
        if ray != self.local_ray {
            self.local_ray = ray;
            self.rays_dirty = true;
        }
        if self.rays_dirty {
            self.rays_dirty = false;
            let mut rays: Vec<Value> = self
                .remote_rays
                .iter()
                .map(|(owner, ray)| json!({"owner":owner,"ray":ray}))
                .collect();
            if let Some(ray) = &self.local_ray {
                rays.push(json!({"owner":"local","ray":ray}));
            }
            self.ui(json!({"kind":"interaction_rays","rays":rays}));
        }
    }
    fn cancel_grab(&mut self) {
        self.grab_command = None;
        self.model.cancel_grab();
        self.sync_grab();
    }
    fn dispatch_command(&mut self, id: u64, mut command: WorldCommand, grab: bool) {
        let requested_id = command.command_id().to_string();
        if !self.host {
            command.set_command_id(Uuid::new_v4().to_string());
        }
        if grab {
            self.grab_command = Some(requested_id.clone());
        }
        if self.host {
            self.queue_command(&format!("local-{id}"), command, Origin::Local(id));
        } else if self.pending.len() < 128 {
            self.pending.insert(
                command.command_id().to_string(),
                (id, std::time::Instant::now(), requested_id),
            );
            let peer = self.host_peer.clone();
            self.network(Some(&peer), serde_json::to_value(command).unwrap());
        } else {
            self.reply(Origin::Local(id),json!({"kind":"result","command_id":requested_id,"ok":false,"message":"Too many pending edits"}));
        }
    }
    fn handle_grab(&mut self, id: u64, body: &Value) {
        match body["action"].as_str().unwrap_or("") {
            "begin" => {
                let result = if self.grab_owner.is_some()
                    || self.previews.local().is_some()
                    || self.input_owner.is_some_and(|(owner, _)| owner != id)
                {
                    Err("Another local interaction is active".into())
                } else {
                    self.previews.prune(&self.model, std::time::Instant::now());
                    self.model.set_collision_previews(self.previews.positions());
                    self.model.begin_grab()
                };
                if let Err(message) = result {
                    let e = self.envelope(json!({"kind":"grab_error","message":message}));
                    self.send(id, e);
                } else {
                    self.grab_command = None;
                    self.grab_owner = Some(id);
                    self.sync_grab();
                }
            }
            "distance" if self.grab_owner == Some(id) => {
                if let Some(delta) = body["delta"].as_f64() {
                    self.model.adjust_grab_distance(delta);
                }
            }
            "rotate" if self.grab_owner == Some(id) => {
                if let Ok(delta) = serde_json::from_value::<[f64; 2]>(body["delta"].clone()) {
                    self.model
                        .rotate_grab(delta, body["vertical"].as_bool().unwrap_or(false));
                }
            }
            "scale" if self.grab_owner == Some(id) => {
                if let Some(factor) = body["factor"].as_f64() {
                    self.model.scale_grab(factor);
                }
            }
            "align" if self.grab_owner == Some(id) => self.model.align_grab(),
            "cancel" if self.grab_owner == Some(id) => self.cancel_grab(),
            "release" if self.grab_owner == Some(id) => {
                if let Some(p) = self.model.release_grab() {
                    self.sync_grab();
                    self.dispatch_command(
                        id,
                        WorldCommand::Transform {
                            command: TransformCommand {
                                command_id: body["command_id"]
                                    .as_str()
                                    .filter(|id| !id.is_empty() && id.len() <= 80)
                                    .map(str::to_string)
                                    .unwrap_or_else(|| Uuid::new_v4().to_string()),
                                target: p.target,
                                expected_position_revision: p.expected_revision,
                                expected_rotation_revision: p.expected_rotation_revision.unwrap(),
                                expected_scale_revision: p.expected_scale_revision.unwrap(),
                                position: p.position,
                                rotation: p.rotation.unwrap(),
                                scale: p.scale.unwrap(),
                            },
                        },
                        true,
                    );
                }
            }
            _ => {}
        }
    }
    fn envelope(&mut self, body: Value) -> Value {
        self.sequence += 1;
        json!(Envelope {
            protocol_version: VERSION,
            session_id: self.session.clone(),
            authority_epoch: self.epoch.clone(),
            sequence: self.sequence.to_string(),
            body
        })
    }
    fn send(&mut self, id: u64, value: Value) {
        if self
            .clients
            .get(&id)
            .is_some_and(|c| c.try_send(value).is_err())
        {
            self.clients.remove(&id); // Slow readers reconnect for a fresh snapshot.
        }
    }
    fn ui(&mut self, body: Value) {
        let value = self.envelope(body);
        let ids: Vec<_> = self
            .clients
            .keys()
            .copied()
            .filter(|id| Some(*id) != self.adapter)
            .collect();
        for id in ids {
            self.send(id, value.clone());
        }
    }
    fn network(&mut self, to: Option<&str>, body: Value) {
        let envelope = self.envelope(body);
        if let Some(id) = self.adapter {
            self.send(id, json!({"type":"send", "to":to, "body":envelope}));
        }
    }
    fn state(&mut self) -> Value {
        json!({"kind":"snapshot", "snapshot": self.model.snapshot(), "editable":self.synced,
            "role":if self.host {"host"} else {"guest"},
            "script_statuses":if self.host {self.model.script_statuses()} else {self.remote_script_status.clone()},
            "component_schemas":REGISTRY.schemas(),
            "script_templates":[{"name":"Rotator","source":kokoverse_core::scripting::ROTATOR},
                {"name":"ClickColor","source":kokoverse_core::scripting::CLICK_COLOR}]})
    }
    fn publish_state(&mut self) {
        self.previews.prune(&self.model, std::time::Instant::now());
        let state = self.state();
        self.ui(state);
    }
    fn publish_presence(&mut self) {
        self.ui(
            json!({"kind":"presence", "self_id":"local", "participants":self.model.participants(), "local_motion":self.model.local_motion()}),
        );
    }
    fn clear_remote_presence(&mut self) {
        self.remote_edit_sessions.clear();
        self.remote_rays.clear();
        self.rays_dirty = true;
        self.cancel_grab();
        self.previews.clear();
        for id in self.remote_seen.keys() {
            self.model.remove_participant(id);
        }
        self.remote_seen.clear();
        self.remote_epochs.clear();
    }
    fn send_pose(&mut self) {
        if self.adapter.is_none() {
            return;
        }
        self.pose_sequence += 1;
        let Some(local) = self.model.local_player() else {
            return;
        };
        let body =
            self.envelope(json!({"kind":"pose", "name":local.name, "pose":local.pose, "avatar_texture":local.avatar_texture,"preview":self.previews.local(),"ray":self.local_ray}));
        if let Some(id) = self.adapter {
            self.send(
                id,
                json!({"type":"latest", "sequence":self.pose_sequence.to_string(), "body":body}),
            );
        }
    }
    fn status(&mut self, message: &str) {
        if !self.synced {
            self.cancel_grab();
        }
        self.ui(json!({"kind":"status", "message":message, "editable":self.synced}));
    }
    fn request_snapshot(&mut self) {
        let peer = self.host_peer.clone();
        self.network(Some(&peer), json!({"kind":"snapshot_request"}));
    }
    fn reply(&mut self, origin: Origin, mut result: Value) {
        if matches!(origin, Origin::Local(_)) && result.get("snapshot").is_some() {
            result["snapshot"] = json!(self.model.snapshot());
        }
        if let Origin::Local(id) = &origin
            && self.grab_owner == Some(*id)
            && result["command_id"].as_str() == self.grab_command.as_deref()
        {
            self.grab_command = None;
            self.cancel_grab();
        }
        match origin {
            Origin::Local(id) => {
                let e = self.envelope(result);
                self.send(id, e);
            }
            Origin::Peer(peer) => self.network(Some(&peer), result),
        }
    }
    fn queue_command(&mut self, requester: &str, mut command: WorldCommand, origin: Origin) {
        let command_id = command.command_id().to_string();
        let key = format!("{requester}:{command_id}");
        if let Some(mut result) = self.outcomes.get(&key).cloned() {
            if result["ok"] == false {
                result["snapshot"] = json!(REGISTRY.encode_world(&self.model.snapshot()));
            }
            self.reply(origin, result);
            return;
        }
        if self.inflight.contains(&key) {
            return;
        }
        self.edit_sessions
            .prune(&self.model.snapshot(), std::time::Instant::now());
        if let Err(message) = self.edit_sessions.check_command(requester, &command) {
            self.reply(
                origin,
                json!({"kind":"result","command_id":command_id,"ok":false,"message":message}),
            );
            return;
        }
        if let WorldCommand::Edit { edit } = &mut command
            && edit.field_id == "panel_pin"
            && edit.value["user_id"] == "$self"
        {
            edit.value["user_id"] = json!(if matches!(origin, Origin::Local(_)) {
                "local".to_string()
            } else {
                requester.to_string()
            });
        }
        // Asset references can be committed before their bytes arrive.
        // Missing bytes are resolved by the bounded asset waiters, not world edits.
        match self.model.enqueue(command) {
            Ok(ticket) => {
                self.inflight.insert(key.clone());
                self.tickets.insert(ticket, (key, command_id, origin));
            }
            Err(message) => self.reply(
                origin,
                json!({"kind":"result", "command_id":command_id, "ok":false, "message":message}),
            ),
        }
    }
    fn tick(&mut self) {
        self.model.set_script_authority(self.host);
        self.model.set_media_time(self.clock.now());
        let presence_revision = self.model.presence_revision();
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_tick);
        self.last_tick = now;
        if self
            .input_owner
            .is_some_and(|(_, last)| last.elapsed() > Duration::from_millis(250))
        {
            if self.grab_command.is_none() {
                self.cancel_grab();
            }
            self.input_owner = None;
            self.model.stop_player_input();
        }
        self.previews.prune(&self.model, now);
        self.model.set_collision_previews(self.previews.positions());
        let outcomes = self.model.advance(elapsed);
        self.sync_grab();
        let mut replies = Vec::new();
        let mut changed = false;
        for outcome in outcomes {
            if outcome.ticket == 0 {
                if let Ok(commit) = outcome.result {
                    changed = true;
                    self.network(None, json!({"kind":"component_commit", "commit":REGISTRY.encode_commit(&commit)}));
                }
                continue;
            }
            let Some((key, command_id, origin)) = self.tickets.remove(&outcome.ticket) else {
                continue;
            };
            self.inflight.remove(&key);
            let result = match outcome.result {
                Ok(commit) => {
                    changed = true;
                    let result = json!({"kind":"result", "command_id":command_id, "ok":true, "world_revision":commit.world_revision, "selected":outcome.selected});
                    self.network(None, json!({"kind":"component_commit", "commit":REGISTRY.encode_commit(&commit)}));
                    result
                }
                Err(message) => {
                    json!({"kind":"result", "command_id":command_id, "ok":false, "message":message, "snapshot":REGISTRY.encode_world(&self.model.snapshot())})
                }
            };
            if self.order.len() >= 4096
                && let Some(old) = self.order.pop_front()
            {
                self.outcomes.remove(&old);
            }
            self.order.push_back(key.clone());
            let mut cached = result.clone();
            if let Some(object) = cached.as_object_mut() {
                object.remove("snapshot");
            }
            self.outcomes.insert(key, cached);
            replies.push((origin, result));
        }
        if changed {
            self.publish_state();
        }
        let statuses = json!(if self.host {
            self.model.script_statuses()
        } else {
            self.remote_script_status.clone()
        });
        if statuses != self.last_script_status {
            self.last_script_status = statuses.clone();
            self.ui(json!({"kind":"script_statuses","statuses":statuses}));
            if self.host {
                self.network(None, json!({"kind":"script_statuses","statuses":statuses}));
            }
        }
        for (origin, result) in replies {
            self.reply(origin, result);
        }
        if self.model.presence_revision() != presence_revision {
            self.publish_presence();
        }
        self.previews.prune(&self.model, std::time::Instant::now());
        if self.previews.dirty {
            self.previews.dirty = false;
            self.ui(self.previews.body());
        }
        self.publish_edit_sessions();
    }
    fn flush_results(&mut self) {
        let revision = self.model.snapshot().world_revision.parse::<u64>().unwrap();
        let ready: Vec<_> = self
            .deferred_results
            .iter()
            .filter(|(_, result)| {
                result["world_revision"]
                    .as_str()
                    .and_then(|r| r.parse::<u64>().ok())
                    .is_none_or(|r| r <= revision)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for command in ready {
            let mut result = self.deferred_results.remove(&command).unwrap();
            if let Some((id, _, requested_id)) = self.pending.remove(&command) {
                result["command_id"] = json!(requested_id);
                if result.get("snapshot").is_some() {
                    result["snapshot"] = json!(self.model.snapshot());
                }
                self.reply(Origin::Local(id), result);
            }
        }
    }
    fn handle_ui(&mut self, id: u64, value: Value) {
        let Ok(envelope) = serde_json::from_value::<Envelope>(value) else {
            return;
        };
        if envelope.protocol_version != VERSION || envelope.session_id != self.session {
            return;
        }
        if envelope.body["kind"] == "hello" {
            let state = self.state();
            let e = self.envelope(state);
            self.send(id, e);
            self.publish_presence();
            self.ui(self.previews.body());
            let view = self.model.grab_view();
            let e = self.envelope(json!({"kind":"grab_state","grab":view}));
            self.send(id, e);
            let body = json!({"kind":"edit_sessions","sessions":if self.host {self.edit_sessions.views()}else{self.remote_edit_sessions.clone()}});
            let envelope = self.envelope(body);
            self.send(id, envelope);
            return;
        }
        if envelope.body["kind"] == "edit_session" {
            if self.synced && envelope.authority_epoch == self.epoch {
                self.edit_session(id, &envelope.body);
            }
            return;
        }
        if envelope.body["kind"] == "clock_ping" {
            if self.synced && envelope.authority_epoch == self.epoch {
                self.clock_message(Origin::Local(id), &envelope.body);
            }
            return;
        }
        if envelope.body["kind"]
            .as_str()
            .is_some_and(|k| k.starts_with("asset_"))
        {
            if self.synced && envelope.authority_epoch == self.epoch {
                self.asset_message(Origin::Local(id), &envelope.body);
            }
            return;
        }
        if envelope.body["kind"] == "grab" {
            if self.synced && envelope.authority_epoch == self.epoch {
                self.handle_grab(id, &envelope.body);
            }
            return;
        }
        if envelope.body["kind"] == "drag_preview" {
            if self.grab_owner.is_some() {
                return;
            }
            let owner = format!("local:{id}");
            if self
                .previews
                .local_owner()
                .is_some_and(|current| current != owner)
            {
                return;
            }
            let preview = serde_json::from_value::<ObjectPreview>(envelope.body["preview"].clone())
                .ok()
                .filter(|preview| {
                    self.synced
                        && envelope.authority_epoch == self.epoch
                        && self.model.accepts_preview(preview)
                });
            self.previews.set(owner, preview, std::time::Instant::now());
            return;
        }
        if envelope.body["kind"] == "player_input" {
            let Ok(input) = serde_json::from_value::<PlayerInput>(envelope.body["input"].clone())
            else {
                return;
            };
            if !kokoverse_core::locomotion::valid_input(&input) {
                return;
            }
            if self.input_owner.is_some_and(|(owner, last)| {
                owner != id && last.elapsed() <= Duration::from_millis(250)
            }) {
                return;
            }
            if self.model.set_player_input(input.clone()).is_ok() {
                self.input_owner = input.active.then_some((id, std::time::Instant::now()));
            }
            return;
        }
        if envelope.body["kind"] == "profile" {
            if let Some(name) = envelope.body["name"].as_str() {
                let name = name.trim();
                if !name.is_empty()
                    && name.chars().count() <= 40
                    && let Some(mut local) = self.model.local_player()
                {
                    local.name = name.into();
                    if let Some(texture) = envelope.body.get("avatar_texture") {
                        match texture.as_str() {
                            Some(id) if self.assets.is_avatar_texture(id) => {
                                local.avatar_texture = Some(id.into())
                            }
                            None if texture.is_null() => local.avatar_texture = None,
                            _ => return,
                        }
                    }
                    let _ = self.model.update_participant(local);
                }
            }
            return;
        }
        if envelope.body["kind"] != "edit"
            && envelope.body["kind"] != "structure"
            && envelope.body["kind"] != "transform"
            && envelope.body["kind"] != "playback"
            && envelope.body["kind"] != "script"
        {
            return;
        }
        let requested_id = envelope.body["edit"]["command_id"]
            .as_str()
            .or_else(|| envelope.body["command"]["command_id"].as_str())
            .unwrap_or("")
            .to_owned();
        if !self.synced || envelope.authority_epoch != self.epoch {
            let e = self.envelope(json!({"kind":"result", "command_id":requested_id, "ok":false, "message":"Reconnect to the current host before editing"}));
            self.send(id, e);
            return;
        }
        let Ok(command) = serde_json::from_value::<WorldCommand>(envelope.body) else {
            let snapshot = self.model.snapshot();
            let error = self.envelope(
                json!({"kind":"result", "command_id":requested_id, "ok":false,
                "message":"Invalid edit payload", "snapshot":snapshot}),
            );
            self.send(id, error);
            return;
        };
        if command.command_id().len() > 80 || command.command_id().is_empty() {
            return;
        }
        self.dispatch_command(id, command, false);
    }
    fn handle_network(&mut self, value: Value) {
        match value["type"].as_str().unwrap_or("") {
            "ready" => {
                if self.automatic_role
                    && let Some(peer) = value["self_peer"].as_str()
                {
                    self.host = peer == self.host_peer;
                    self.peer = peer.into();
                    if self.host && self.epoch.is_empty() {
                        self.epoch = Uuid::new_v4().to_string();
                        self.synced = true;
                    }
                }
                self.clear_remote_presence();
                if let Some(peers) = value["peers"].as_array() {
                    for peer in peers {
                        if let (Some(id), Some(epoch)) =
                            (peer["peer"].as_str(), peer["publisherEpoch"].as_str())
                            && id != self.peer
                        {
                            self.remote_epochs.insert(id.into(), epoch.into());
                        }
                    }
                }
                self.status("KokoSync connected");
                self.publish_state();
                if !self.host {
                    self.request_snapshot();
                }
            }
            "error" => {
                self.clear_remote_presence();
                if !self.host {
                    self.synced = false;
                    self.pending.clear();
                }
                self.status(
                    value["message"]
                        .as_str()
                        .unwrap_or("Transport failed; reconnect"),
                );
            }
            "peer_left" => {
                if let (Some(peer), Some(epoch)) =
                    (value["peer"].as_str(), value["publisher_epoch"].as_str())
                    && self
                        .remote_epochs
                        .get(peer)
                        .is_some_and(|current| current == epoch)
                {
                    self.remote_epochs.remove(peer);
                    let id = format!("{peer}:{epoch}");
                    self.previews.forget(&id);
                    self.remote_rays.remove(&id);
                    self.rays_dirty = true;
                    self.remote_seen.remove(&id);
                    self.model.remove_participant(&id);
                    self.edit_sessions.forget(&id);
                    if peer == self.host_peer && !self.host {
                        self.synced = false;
                        self.pending.clear();
                        self.deferred_results.clear();
                        self.status("Host disconnected. Editing paused.");
                    }
                }
            }
            "peer_joined" => {
                if let (Some(peer), Some(epoch)) =
                    (value["peer"].as_str(), value["publisher_epoch"].as_str())
                {
                    if let Some(old) = self.remote_epochs.insert(peer.into(), epoch.into()) {
                        self.remote_rays.remove(&format!("{peer}:{old}"));
                        self.rays_dirty = true;
                        self.previews.forget(&format!("{peer}:{old}"));
                        self.model.remove_participant(&format!("{peer}:{old}"));
                        self.remote_seen.remove(&format!("{peer}:{old}"));
                    }
                    if peer == self.host_peer && !self.host {
                        self.synced = false;
                        self.epoch.clear();
                        self.request_snapshot();
                    }
                }
            }
            "pose" => {
                let (Some(peer), Some(epoch)) =
                    (value["peer"].as_str(), value["publisher_epoch"].as_str())
                else {
                    return;
                };
                if !self
                    .remote_epochs
                    .get(peer)
                    .is_some_and(|current| current == epoch)
                {
                    return;
                }
                let Ok(e) = serde_json::from_value::<Envelope>(value["body"].clone()) else {
                    return;
                };
                if e.protocol_version != VERSION
                    || e.session_id != self.session
                    || e.body["kind"] != "pose"
                {
                    return;
                }
                let (Some(name), Ok(pose)) = (
                    e.body["name"].as_str(),
                    serde_json::from_value::<AvatarPose>(e.body["pose"].clone()),
                ) else {
                    return;
                };
                let id = format!("{peer}:{epoch}");
                let ray = serde_json::from_value::<InteractionRay>(e.body["ray"].clone())
                    .ok()
                    .filter(|r| {
                        r.origin
                            .iter()
                            .chain(&r.point)
                            .all(|v| v.is_finite() && v.abs() < 1010.0)
                            && (0..3)
                                .map(|i| (r.point[i] - r.origin[i]).powi(2))
                                .sum::<f64>()
                                <= 36.01
                            && (0..3)
                                .map(|i| (r.origin[i] - pose.position[i]).powi(2))
                                .sum::<f64>()
                                < 4.0
                    });
                match ray {
                    Some(ray) => {
                        if self.remote_rays.get(&id) != Some(&ray) {
                            self.remote_rays.insert(id.clone(), ray);
                            self.rays_dirty = true;
                        }
                    }
                    None => {
                        self.rays_dirty |= self.remote_rays.remove(&id).is_some();
                    }
                }
                if let Ok(sequence) = e.sequence.parse::<u64>() {
                    let preview =
                        serde_json::from_value::<ObjectPreview>(e.body["preview"].clone())
                            .ok()
                            .filter(|preview| {
                                self.synced
                                    && e.authority_epoch == self.epoch
                                    && self.model.accepts_preview(preview)
                            });
                    self.previews
                        .receive(id.clone(), sequence, preview, std::time::Instant::now());
                }
                if self
                    .model
                    .update_participant(ParticipantView {
                        id: id.clone(),
                        name: name.into(),
                        pose,
                        avatar_texture: e.body["avatar_texture"].as_str().map(str::to_owned),
                    })
                    .is_ok()
                {
                    self.remote_seen.insert(id, std::time::Instant::now());
                }
            }
            "message" => {
                let Some(peer) = value["peer"].as_str() else {
                    return;
                };
                let Ok(e) = serde_json::from_value::<Envelope>(value["body"].clone()) else {
                    return;
                };
                if e.protocol_version != VERSION || e.session_id != self.session {
                    return;
                }
                if e.body["kind"] == "clock_ping" || e.body["kind"] == "clock_pong" {
                    if (self.host || peer == self.host_peer) && e.authority_epoch == self.epoch {
                        self.clock_message(Origin::Peer(peer.into()), &e.body);
                    }
                    return;
                }
                if e.body["kind"]
                    .as_str()
                    .is_some_and(|k| k.starts_with("asset_"))
                {
                    if (self.host || peer == self.host_peer) && e.authority_epoch == self.epoch {
                        self.asset_message(Origin::Peer(peer.into()), &e.body);
                    }
                    return;
                }
                if self.host {
                    if e.body["kind"] == "snapshot_request" {
                        let snapshot = self.model.snapshot();
                        self.network(Some(peer), json!({"kind":"component_snapshot", "snapshot":REGISTRY.encode_world(&snapshot),"script_statuses":self.model.script_statuses()}));
                        self.network(
                            Some(peer),
                            json!({"kind":"edit_sessions","sessions":self.edit_sessions.views()}),
                        );
                    } else if e.body["kind"] == "edit_session" && e.authority_epoch == self.epoch {
                        if let Ok(input) = serde_json::from_value(e.body["request"].clone()) {
                            let owner = format!(
                                "{}:{}",
                                peer,
                                value["publisher_epoch"].as_str().unwrap_or("")
                            );
                            self.apply_edit_session(&owner, input, Origin::Peer(peer.into()));
                        }
                    } else if (e.body["kind"] == "edit"
                        || e.body["kind"] == "structure"
                        || e.body["kind"] == "transform"
                        || e.body["kind"] == "playback"
                        || e.body["kind"] == "script")
                        && e.authority_epoch == self.epoch
                        && let Ok(command) = serde_json::from_value::<WorldCommand>(e.body)
                    {
                        if command.command_id().len() > 80 || command.command_id().is_empty() {
                            return;
                        }
                        let requester = format!(
                            "{}:{}",
                            peer,
                            value["publisher_epoch"].as_str().unwrap_or("")
                        );
                        self.queue_command(&requester, command, Origin::Peer(peer.into()));
                    }
                } else if peer == self.host_peer {
                    let kind = e.body["kind"].as_str().unwrap_or("");
                    if kind == "edit_sessions" && e.authority_epoch == self.epoch {
                        if let Ok(sessions) = serde_json::from_value(e.body["sessions"].clone()) {
                            self.remote_edit_sessions = sessions;
                        }
                        return;
                    }
                    if kind == "component_snapshot" {
                        if !self.epoch.is_empty() && e.authority_epoch != self.epoch {
                            return;
                        }
                        let Ok(snapshot) =
                            serde_json::from_value::<ReplicatedWorld>(e.body["snapshot"].clone())
                                .map_err(|e| e.to_string())
                                .and_then(|s| REGISTRY.decode_world(s))
                        else {
                            return;
                        };
                        let Ok(revision) = snapshot.world_revision.parse::<u64>() else {
                            return;
                        };
                        let current = self.model.snapshot().world_revision.parse::<u64>().unwrap();
                        if self.synced && revision < current {
                            return;
                        }
                        if self.model.restore(snapshot).is_ok() {
                            self.remote_script_status =
                                serde_json::from_value(e.body["script_statuses"].clone())
                                    .unwrap_or_default();
                            self.epoch = e.authority_epoch;
                            self.synced = true;
                            self.publish_state();
                            self.flush_results();
                        }
                    } else if kind == "script_statuses"
                        && self.synced
                        && e.authority_epoch == self.epoch
                    {
                        if let Ok(statuses) = serde_json::from_value(e.body["statuses"].clone()) {
                            self.remote_script_status = statuses;
                        }
                    } else if kind == "component_commit"
                        && self.synced
                        && e.authority_epoch == self.epoch
                    {
                        let Ok(commit) =
                            serde_json::from_value::<ReplicationCommit>(e.body["commit"].clone())
                                .map_err(|e| e.to_string())
                                .and_then(|c| REGISTRY.decode_commit(c))
                        else {
                            self.synced = false;
                            self.status("Invalid component update. Resynchronizing…");
                            self.request_snapshot();
                            return;
                        };
                        let current = self.model.snapshot().world_revision.parse::<u64>().unwrap();
                        if commit
                            .world_revision
                            .parse::<u64>()
                            .is_ok_and(|r| r <= current)
                        {
                            return;
                        }
                        if self.model.apply_commit(commit).is_err() {
                            self.synced = false;
                            self.status("Missing or invalid update. Resynchronizing…");
                            self.request_snapshot();
                        } else {
                            self.publish_state();
                            self.flush_results();
                        }
                    } else if kind == "result"
                        && e.authority_epoch == self.epoch
                        && let Some(command) = e.body["command_id"].as_str()
                        && self.pending.contains_key(command)
                    {
                        if let Ok(snapshot) =
                            serde_json::from_value::<ReplicatedWorld>(e.body["snapshot"].clone())
                                .map_err(|e| e.to_string())
                                .and_then(|s| REGISTRY.decode_world(s))
                        {
                            let current =
                                self.model.snapshot().world_revision.parse::<u64>().unwrap();
                            if snapshot
                                .world_revision
                                .parse::<u64>()
                                .is_ok_and(|r| r > current)
                                && self.model.restore(snapshot).is_ok()
                            {
                                self.publish_state();
                            }
                        }
                        self.deferred_results.insert(command.to_string(), e.body);
                        self.flush_results();
                    }
                }
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let arg = |key: &str, default: &str| {
        args.windows(2)
            .find(|w| w[0] == key)
            .map(|w| w[1].clone())
            .unwrap_or(default.into())
    };
    let role = arg("--role", "host");
    let host = role == "host";
    let ui_port = arg("--port", "47000");
    let transport_port = arg("--transport-port", "47001");
    let ui = TcpListener::bind(format!("127.0.0.1:{ui_port}")).await?;
    let transport = TcpListener::bind(format!("127.0.0.1:{transport_port}")).await?;
    let clock = media_clock::ClockSync::new();
    let mut model = WorldModel::new();
    model.set_media_time(clock.now());
    let load = arg("--load", "");
    let save = arg("--save", "");
    let save_path = (!save.is_empty()).then(|| std::path::PathBuf::from(&save));
    if !load.is_empty() {
        model.restore(persistence::read_snapshot(std::path::Path::new(&load))?)?;
    } else if host
        && let Some(path) = &save_path
        && (path.exists() || persistence::backup_path(path).exists())
    {
        model.restore(persistence::load_checkpoint(path)?)?;
        eprintln!("Restored world from {}", path.display());
    }
    // Auto-role clients must wait for the relay's decision: a guest can join
    // even if an unrelated checkpoint in its own installation is unreadable.
    let mut auto_checkpoint = if role == "auto" && load.is_empty() {
        save_path.clone()
    } else {
        None
    };
    let mut persistence = persistence::Persistence::new(save_path);
    let mut engine = Engine {
        clock,
        assets: assets::Assets::new(std::path::PathBuf::from(arg(
            "--asset-dir",
            &format!(".run/assets/{ui_port}"),
        )))?,
        local_ray: None,
        remote_rays: HashMap::new(),
        rays_dirty: false,
        grab_owner: None,
        grab_command: None,
        last_grab: None,
        previews: previews::Previews::default(),
        model,
        edit_sessions: Default::default(),
        remote_edit_sessions: Vec::new(),
        last_edit_sessions: Value::Null,
        host,
        automatic_role: role == "auto",
        host_peer: arg("--host-peer", "host"),
        session: arg("--session", "demo"),
        epoch: if host {
            Uuid::new_v4().to_string()
        } else {
            String::new()
        },
        sequence: 0,
        synced: host,
        adapter: None,
        clients: HashMap::new(),
        pending: HashMap::new(),
        deferred_results: HashMap::new(),
        tickets: HashMap::new(),
        inflight: HashSet::new(),
        peer: arg("--peer", if host { "host" } else { "guest" }),
        input_owner: None,
        last_tick: std::time::Instant::now(),
        pose_sequence: 0,
        remote_epochs: HashMap::new(),
        remote_seen: HashMap::new(),
        outcomes: HashMap::new(),
        order: VecDeque::new(),
        last_script_status: Value::Null,
        remote_script_status: Vec::new(),
    };
    engine.model.update_participant(ParticipantView {
        avatar_texture: None,
        id: "local".into(),
        name: arg("--name", if host { "Host" } else { "Guest" }),
        pose: AvatarPose {
            position: if host {
                [-1.8, 0.0, 3.0]
            } else {
                [1.8, 0.0, 3.0]
            },
            yaw: 0.0,
            pitch: 0.0,
        },
    })?;
    engine.model.tick();
    engine.model.enable_local_player("local")?;
    engine.tick();
    let (events, mut rx) = mpsc::channel(256);
    let mut next_id = 0;
    eprintln!(
        "Kokoverse {} | UI 127.0.0.1:{} | adapter 127.0.0.1:{}",
        if host { "host" } else { "guest" },
        ui_port,
        transport_port
    );
    let mut retry = tokio::time::interval(Duration::from_secs(2));
    let mut simulation = tokio::time::interval(Duration::from_millis(16));
    let mut poses = tokio::time::interval(Duration::from_millis(50));
    let mut autosave = tokio::time::interval(Duration::from_secs(2));
    autosave.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    poses.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    simulation.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = autosave.tick() => {
                if let Err(error) = persistence.poll().await {
                    eprintln!("World autosave failed: {error}");
                    engine.status(&format!("World autosave failed: {error}"));
                }
                if engine.host { persistence.schedule(engine.model.snapshot()); }
            },
            _ = simulation.tick() => engine.tick(),
            _ = poses.tick() => engine.send_pose(),
            accepted = ui.accept() => { let (stream, _) = accepted?; if engine.clients.len() < 32 { next_id += 1; tokio::spawn(connection(stream, next_id, false, events.clone())); } },
            accepted = transport.accept() => { let (stream, _) = accepted?; if engine.adapter.is_none() { next_id += 1; tokio::spawn(connection(stream, next_id, true, events.clone())); } },
            _ = retry.tick() => {
                engine.expire_assets();
                engine.clock.expire();
                if engine.pending.values().any(|(_, time, _)| time.elapsed() > Duration::from_secs(10)) {
                    engine.pending.clear();
                    engine.deferred_results.clear();
                    engine.synced = false;
                    engine.status("Edit result unknown. Resynchronizing; check current values before retrying.");
                }
                if !engine.host && !engine.synced && engine.adapter.is_some() {engine.request_snapshot();}
                let stale:Vec<_> = engine.remote_seen.iter().filter(|(_,time)| time.elapsed()>Duration::from_secs(5)).map(|(id,_)| id.clone()).collect();
                for id in stale { engine.remote_seen.remove(&id); engine.model.remove_participant(&id); engine.remote_rays.remove(&id); engine.rays_dirty=true; }
            },
            event = rx.recv() => match event {
                Some(Event::Connected(id, adapter, out)) => {
                    if adapter && engine.adapter.is_some() { continue; }
                    engine.clients.insert(id, out);
                    if adapter { engine.adapter = Some(id); } else {
                        let state = engine.state(); let e = engine.envelope(state); engine.send(id, e);
                    }
                },
                Some(Event::Message(id, value)) => {
                    if !engine.clients.contains_key(&id) { continue; }
                    if Some(id) == engine.adapter {
                        if value["type"] == "ready"
                            && let Some(peer) = value["self_peer"].as_str()
                            && let Some(path) = auto_checkpoint.take()
                            && peer == engine.host_peer
                            && (path.exists() || persistence::backup_path(&path).exists())
                        {
                            engine.model.restore(persistence::load_checkpoint(&path)?)?;
                            eprintln!("Restored world from {}", path.display());
                        }
                        engine.handle_network(value);
                    } else {
                        let save_request = serde_json::from_value::<Envelope>(value.clone()).ok()
                            .filter(|e| e.protocol_version == VERSION && e.session_id == engine.session && e.body["kind"] == "save_world");
                        if let Some(request) = save_request {
                            engine.tick();
                            let result = if engine.host {
                                persistence.flush(engine.model.snapshot()).await
                            } else { Ok(false) };
                            let response = match result {
                                Ok(saved) => json!({"kind":"save_result","request_id":request.body["request_id"],"ok":true,"saved":saved,"world_revision":engine.model.snapshot().world_revision}),
                                Err(error) => json!({"kind":"save_result","request_id":request.body["request_id"],"ok":false,"message":error}),
                            };
                            let envelope = engine.envelope(response);
                            engine.send(id, envelope);
                        } else { engine.handle_ui(id, value); }
                    }
                },
                Some(Event::Closed(id)) => {
                    engine.edit_sessions.forget(&format!("local-{id}"));
                    engine.close_asset_client(id);
                    if engine.grab_owner == Some(id) { engine.cancel_grab(); }
                    if engine.input_owner.is_some_and(|(owner, _)| owner == id) {
                        engine.input_owner = None;
                        engine.model.stop_player_input();
                    }
                    engine.previews.remove(&format!("local:{id}"));
                    engine.clients.remove(&id); engine.pending.retain(|_, (client, _, _)| *client != id);
                    if engine.adapter == Some(id) {
                        engine.clear_remote_presence();
                        engine.adapter = None;
                        if !engine.host { engine.synced = false; engine.epoch.clear(); engine.pending.clear(); engine.deferred_results.clear(); }
                        engine.status("KokoSync adapter disconnected");
                    }
                },
                None => break,
            },
        }
    }
    if engine.host {
        persistence.flush(engine.model.snapshot()).await?;
    }
    Ok(())
}

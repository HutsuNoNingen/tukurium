//! Host-owned, bounded edit leases and presentation state. Not part of saved worlds.
use crate::replication::REGISTRY;
use kokoverse_protocol::{EditSession, EditSessionRequest, Snapshot, WorldCommand};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const LEASE_TIME: Duration = Duration::from_secs(6);

#[derive(Default)]
pub struct EditSessions {
    sessions: HashMap<String, (EditSession, Instant)>,
}

impl EditSessions {
    pub fn views(&self) -> Vec<EditSession> {
        let mut result: Vec<_> = self.sessions.values().map(|(s, _)| s.clone()).collect();
        result.sort_by(|a, b| {
            (&a.target.instance_id, a.target.local_id, &a.field_id).cmp(&(
                &b.target.instance_id,
                b.target.local_id,
                &b.field_id,
            ))
        });
        result
    }

    pub fn prune(&mut self, snapshot: &Snapshot, now: Instant) {
        self.sessions.retain(|_, (s, last)| {
            now.duration_since(*last) < LEASE_TIME
                && Self::valid_target(snapshot, &s.target, &s.field_id, &s.context).is_ok()
        });
    }

    pub fn forget(&mut self, owner: &str) {
        self.sessions.retain(|_, (s, _)| s.owner != owner);
    }

    pub fn apply(
        &mut self,
        owner: &str,
        input: EditSessionRequest,
        snapshot: &Snapshot,
        now: Instant,
    ) -> Result<(), String> {
        self.prune(snapshot, now);
        if input.command_id.is_empty()
            || input.command_id.len() > 80
            || input.client_id.is_empty()
            || input.client_id.len() > 80
        {
            return Err("Invalid edit session request".into());
        }
        let key = format!(
            "{}:{}:{}",
            input.target.instance_id, input.target.local_id, input.field_id
        );
        if input.action == "end" {
            if self.sessions.get(&key).is_some_and(|(s, _)| {
                s.owner == owner && s.client_id == input.client_id && s.context == input.context
            }) {
                self.sessions.remove(&key);
            }
            return Ok(());
        }
        if !matches!(input.action.as_str(), "begin" | "update")
            || input.draft.text.len() > 4096
            || input.draft.part.len() > 40
            || input.draft.caret > 4096
            || input.draft.selection.iter().any(|v| *v > 4096)
        {
            return Err("Invalid edit draft".into());
        }
        Self::valid_target(snapshot, &input.target, &input.field_id, &input.context)?;
        if self.sessions.get(&key).is_some_and(|(s, _)| {
            s.owner != owner || s.client_id != input.client_id || s.context != input.context
        }) {
            return Err("This field is being edited in another editor".into());
        }
        if input.action == "update" && !self.sessions.contains_key(&key) {
            return Err("Editing lease expired; focus the field again".into());
        }
        if self.sessions.len() >= 64 && !self.sessions.contains_key(&key) {
            return Err("Too many active edits".into());
        }
        self.sessions.insert(
            key,
            (
                EditSession {
                    owner: owner.into(),
                    client_id: input.client_id,
                    context: input.context,
                    target: input.target,
                    field_id: input.field_id,
                    draft: input.draft,
                },
                now,
            ),
        );
        Ok(())
    }

    fn valid_target(
        snapshot: &Snapshot,
        target: &kokoverse_protocol::Address,
        field: &str,
        context: &Option<kokoverse_protocol::EditContext>,
    ) -> Result<(), String> {
        let (object, _, spec) = REGISTRY.resolve(snapshot, target, field)?;
        if spec.schema.access != kokoverse_protocol::FieldAccess::Edit {
            return Err("Field is controlled by commands".into());
        }
        if let Some(context) = context {
            let (source, component, _) =
                REGISTRY.resolve(snapshot, &context.source, &context.field_id)?;
            let values = (component.project)(source);
            let linked: kokoverse_protocol::Address =
                serde_json::from_value(values[&context.field_id].clone())
                    .map_err(|_| "Editor reference no longer exists")?;
            if linked != object.address {
                return Err("Editor target changed or editor was closed".into());
            }
        }
        Ok(())
    }

    pub fn check_command(&self, owner: &str, command: &WorldCommand) -> Result<(), String> {
        for (s, _) in self.sessions.values() {
            let conflicts = match command {
                WorldCommand::Edit { edit } => {
                    edit.target == s.target && edit.field_id == s.field_id
                }
                WorldCommand::Transform { command } => {
                    command.target.instance_id == s.target.instance_id
                        && s.target.local_id == 3
                        && matches!(s.field_id.as_str(), "position" | "rotation" | "scale")
                }
                _ => false,
            };
            if conflicts && s.owner != owner {
                return Err("This field is currently being edited by another user".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kokoverse_protocol::{Address, Edit, EditContext, EditDraft, TransformCommand};

    #[test]
    fn editor_without_a_panel_can_lease_a_component_field() {
        let model = crate::WorldModel::new();
        let snapshot = model.snapshot();
        let root = snapshot.objects[0].address.clone();
        let now = Instant::now();
        let mut sessions = EditSessions::default();
        let mut request = EditSessionRequest {
            command_id: "lease".into(),
            client_id: "custom-editor".into(),
            context: None,
            target: REGISTRY.field_address(&root, "position").unwrap(),
            field_id: "position".into(),
            action: "begin".into(),
            draft: EditDraft::default(),
        };
        sessions
            .apply("alice", request.clone(), &snapshot, now)
            .unwrap();
        let transform = WorldCommand::Transform {
            command: TransformCommand {
                command_id: "move".into(),
                target: root.clone(),
                position: [1.0; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
                expected_position_revision: "0".into(),
                expected_rotation_revision: "0".into(),
                expected_scale_revision: "0".into(),
            },
        };
        assert!(sessions.check_command("bob", &transform).is_err());
        let unrelated = WorldCommand::Edit {
            edit: Edit {
                command_id: "rename".into(),
                target: root,
                field_id: "name".into(),
                expected_revision: "0".into(),
                value: serde_json::json!("OK"),
            },
        };
        assert!(sessions.check_command("bob", &unrelated).is_ok());
        request.action = "end".into();
        sessions
            .apply("bob", request.clone(), &snapshot, now)
            .unwrap();
        assert_eq!(
            sessions.views().len(),
            1,
            "Another owner cannot release the lease"
        );
        sessions
            .apply("alice", request.clone(), &snapshot, now)
            .unwrap();
        request.action = "update".into();
        assert!(sessions.apply("alice", request, &snapshot, now).is_err());
        assert!(sessions.check_command("bob", &transform).is_ok());
    }

    #[test]
    fn reference_context_is_validated_and_removing_source_expires_lease() {
        let model = crate::WorldModel::new();
        let mut snapshot = model.snapshot();
        let root = snapshot.objects[0].address.clone();
        let panel = crate::panels::create(
            &snapshot,
            Some(root.clone()),
            [0.0; 3],
            [0.0, 0.0, 0.0, 1.0],
        )
        .unwrap();
        let source = REGISTRY
            .field_address(&panel.address, "inspector_target")
            .unwrap();
        snapshot.objects.push(panel);
        let mut request = EditSessionRequest {
            command_id: "lease".into(),
            client_id: "editor".into(),
            context: Some(EditContext {
                source,
                field_id: "inspector_target".into(),
            }),
            target: REGISTRY.field_address(&root, "name").unwrap(),
            field_id: "name".into(),
            action: "begin".into(),
            draft: EditDraft::default(),
        };
        let now = Instant::now();
        let mut sessions = EditSessions::default();
        sessions
            .apply("alice", request.clone(), &snapshot, now)
            .unwrap();
        snapshot.objects.pop();
        sessions.prune(&snapshot, now);
        assert!(sessions.views().is_empty());
        assert!(
            sessions
                .apply("alice", request.clone(), &snapshot, now)
                .is_err()
        );
        request.context = None;
        request.target = Address {
            instance_id: root.instance_id,
            local_id: 5,
        };
        request.field_id = "playback".into();
        assert!(sessions.apply("alice", request, &snapshot, now).is_err());
    }
}

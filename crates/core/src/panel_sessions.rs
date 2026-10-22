//! Host-owned, bounded edit leases and presentation state. Not part of saved worlds.
use kokoverse_protocol::{PanelEditSession, PanelInput, Snapshot, WorldCommand};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const LEASE_TIME: Duration = Duration::from_secs(6);

#[derive(Default)]
pub struct PanelSessions {
    sessions: HashMap<String, (PanelEditSession, Instant)>,
}

impl PanelSessions {
    pub fn views(&self) -> Vec<PanelEditSession> {
        let mut result: Vec<_> = self.sessions.values().map(|(s, _)| s.clone()).collect();
        result.sort_by(|a, b| {
            (&a.target.instance_id, &a.field_id).cmp(&(&b.target.instance_id, &b.field_id))
        });
        result
    }

    pub fn prune(&mut self, snapshot: &Snapshot, now: Instant) {
        self.sessions.retain(|_, (s, last)| {
            now.duration_since(*last) < LEASE_TIME
                && snapshot.objects.iter().any(|o| {
                    o.address == s.panel
                        && o.inspector
                            .as_ref()
                            .is_some_and(|i| i.target.as_ref() == Some(&s.target))
                })
                && snapshot
                    .objects
                    .iter()
                    .any(|o| o.address == s.target && o.fields.iter().any(|f| f.id == s.field_id))
        });
    }

    pub fn forget(&mut self, owner: &str) {
        self.sessions.retain(|_, (s, _)| s.owner != owner);
    }

    pub fn apply(
        &mut self,
        owner: &str,
        input: PanelInput,
        snapshot: &Snapshot,
        now: Instant,
    ) -> Result<(), String> {
        self.prune(snapshot, now);
        if input.command_id.is_empty()
            || input.command_id.len() > 80
            || input.client_id.is_empty()
            || input.client_id.len() > 80
        {
            return Err("Invalid panel request".into());
        }
        let key = format!("{}:{}", input.target.instance_id, input.field_id);
        if input.action == "end" {
            if self.sessions.get(&key).is_some_and(|(s, _)| {
                s.owner == owner && s.client_id == input.client_id && s.panel == input.panel
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
            return Err("Invalid panel draft".into());
        }
        if !snapshot.objects.iter().any(|o| {
            o.address == input.panel
                && o.inspector
                    .as_ref()
                    .is_some_and(|i| i.target.as_ref() == Some(&input.target))
        }) {
            return Err("Panel target changed or panel was closed".into());
        }
        if !snapshot
            .objects
            .iter()
            .any(|o| o.address == input.target && o.fields.iter().any(|f| f.id == input.field_id))
        {
            return Err("Field no longer exists".into());
        }
        if self.sessions.get(&key).is_some_and(|(s, _)| {
            s.owner != owner || s.client_id != input.client_id || s.panel != input.panel
        }) {
            return Err("This field is being edited in another panel".into());
        }
        if input.action == "update" && !self.sessions.contains_key(&key) {
            return Err("Editing lease expired; focus the field again".into());
        }
        if self.sessions.len() >= 64 && !self.sessions.contains_key(&key) {
            return Err("Too many active panel edits".into());
        }
        self.sessions.insert(
            key,
            (
                PanelEditSession {
                    owner: owner.into(),
                    client_id: input.client_id,
                    panel: input.panel,
                    target: input.target,
                    field_id: input.field_id,
                    draft: input.draft,
                },
                now,
            ),
        );
        Ok(())
    }

    pub fn check_command(&self, owner: &str, command: &WorldCommand) -> Result<(), String> {
        for (s, _) in self.sessions.values() {
            let conflicts = match command {
                WorldCommand::Edit { edit } => {
                    edit.target == s.target && edit.field_id == s.field_id
                }
                WorldCommand::Transform { command } => {
                    command.target == s.target
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

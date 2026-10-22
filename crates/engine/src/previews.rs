use kokoverse_core::WorldModel;
use kokoverse_protocol::ObjectPreview;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Previews {
    entries: BTreeMap<String, (ObjectPreview, Instant)>,
    remote_sequences: HashMap<String, u64>,
    pub dirty: bool,
}

impl Previews {
    pub fn set(&mut self, owner: String, preview: Option<ObjectPreview>, now: Instant) {
        match preview {
            Some(preview) => {
                self.dirty |= self
                    .entries
                    .get(&owner)
                    .is_none_or(|(old, _)| old != &preview);
                self.entries.insert(owner, (preview, now));
            }
            None => self.remove(&owner),
        }
    }
    pub fn receive(
        &mut self,
        owner: String,
        sequence: u64,
        preview: Option<ObjectPreview>,
        now: Instant,
    ) {
        if self
            .remote_sequences
            .get(&owner)
            .is_some_and(|old| sequence <= *old)
        {
            return;
        }
        self.remote_sequences.insert(owner.clone(), sequence);
        self.set(owner, preview, now);
    }
    pub fn remove(&mut self, owner: &str) {
        self.dirty |= self.entries.remove(owner).is_some();
    }
    pub fn forget(&mut self, owner: &str) {
        self.remove(owner);
        self.remote_sequences.remove(owner);
    }
    pub fn clear(&mut self) {
        self.dirty |= !self.entries.is_empty();
        self.entries.clear();
        self.remote_sequences.clear();
    }
    pub fn prune(&mut self, model: &WorldModel, now: Instant) {
        let count = self.entries.len();
        self.entries.retain(|_, (preview, last)| {
            now.duration_since(*last) < Duration::from_secs(1) && model.accepts_preview(preview)
        });
        self.dirty |= count != self.entries.len();
    }
    pub fn local(&self) -> Option<&ObjectPreview> {
        self.entries
            .iter()
            .find(|(owner, _)| owner.starts_with("local:"))
            .map(|(_, (preview, _))| preview)
    }
    pub fn local_owner(&self) -> Option<&str> {
        self.entries
            .keys()
            .find(|owner| owner.starts_with("local:"))
            .map(String::as_str)
    }
    pub fn positions(&self) -> Vec<ObjectPreview> {
        let mut targets = std::collections::HashSet::new();
        self.entries
            .values()
            .filter_map(|(p, _)| targets.insert(p.target.clone()).then_some(p.clone()))
            .collect()
    }
    pub fn body(&self) -> Value {
        json!({"kind":"object_previews", "previews":self.entries.iter().map(|(owner, (preview, _))|
            json!({"owner":owner, "preview":preview})).collect::<Vec<_>>()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn preview(model: &WorldModel) -> ObjectPreview {
        let object = model.snapshot().objects.remove(0);
        ObjectPreview {
            target: object.address,
            expected_revision: object
                .fields
                .iter()
                .find(|f| f.id == "position")
                .unwrap()
                .revision
                .clone(),
            position: [2.0, 1.0, 0.0],
            ..Default::default()
        }
    }
    #[test]
    fn cancel_watermark_prevents_late_packet_resurrection() {
        let model = WorldModel::new();
        let mut previews = Previews::default();
        let now = Instant::now();
        previews.receive("peer".into(), 1, Some(preview(&model)), now);
        previews.receive("peer".into(), 3, None, now);
        previews.receive("peer".into(), 2, Some(preview(&model)), now);
        assert!(previews.entries.is_empty());
    }
    #[test]
    fn stale_invalid_and_expired_previews_do_not_modify_world() {
        let model = WorldModel::new();
        let before = model.snapshot();
        let mut previews = Previews::default();
        let now = Instant::now();
        previews.set("peer".into(), Some(preview(&model)), now);
        previews.prune(&model, now + Duration::from_secs(1));
        assert!(previews.entries.is_empty());
        let mut stale = preview(&model);
        stale.expected_revision = "999".into();
        previews.set("peer".into(), Some(stale), now);
        previews.prune(&model, now);
        assert!(previews.entries.is_empty());
        let mut invalid = preview(&model);
        invalid.position[0] = f64::NAN;
        assert!(!model.accepts_preview(&invalid));
        invalid.position[0] = 1001.0;
        assert!(!model.accepts_preview(&invalid));
        assert_eq!(model.snapshot(), before);
    }
}

//! Transient participant state shares the ECS, but never advances persistent world revision.
use bevy_ecs::prelude::*;
use kokoverse_protocol::ParticipantView;
use std::collections::HashMap;

#[derive(Component)]
pub struct Participant(pub ParticipantView);

pub(crate) enum PresenceInput {
    Upsert(ParticipantView),
    Remove(String),
}
#[derive(Resource, Default)]
pub(crate) struct PresenceInputs(pub Vec<PresenceInput>);
#[derive(Resource, Default)]
pub(crate) struct PresenceIndex(pub HashMap<String, Entity>);
#[derive(Resource, Default)]
pub(crate) struct PresenceCache {
    pub revision: u64,
    pub participants: Vec<ParticipantView>,
    pub(crate) dirty: bool,
}

pub(crate) fn apply_presence(world: &mut World) {
    let inputs = std::mem::take(&mut world.resource_mut::<PresenceInputs>().0);
    for input in inputs {
        let changed = match input {
            PresenceInput::Upsert(view) => {
                let entity = world.resource::<PresenceIndex>().0.get(&view.id).copied();
                if let Some(entity) = entity {
                    if world.get::<Participant>(entity).unwrap().0 == view {
                        false
                    } else {
                        world.get_mut::<Participant>(entity).unwrap().0 = view;
                        true
                    }
                } else if world.resource::<PresenceIndex>().0.len() < 32 {
                    let id = view.id.clone();
                    let entity = world.spawn(Participant(view)).id();
                    world.resource_mut::<PresenceIndex>().0.insert(id, entity);
                    true
                } else {
                    false
                }
            }
            PresenceInput::Remove(id) => {
                if let Some(entity) = world.resource_mut::<PresenceIndex>().0.remove(&id) {
                    world.despawn(entity);
                    true
                } else {
                    false
                }
            }
        };
        if changed {
            world.resource_mut::<PresenceCache>().dirty = true;
        }
    }
}

pub(crate) fn extract_presence(people: Query<&Participant>, mut cache: ResMut<PresenceCache>) {
    if !cache.dirty {
        return;
    }
    cache.participants = people.iter().map(|p| p.0.clone()).collect();
    cache.participants.sort_by(|a, b| a.id.cmp(&b.id));
    cache.revision += 1;
    cache.dirty = false;
}

pub fn valid_pose(pose: &kokoverse_protocol::AvatarPose) -> bool {
    pose.position
        .iter()
        .all(|n| n.is_finite() && n.abs() <= 1000.0)
        && pose.yaw.is_finite()
        && pose.yaw.abs() <= 10000.0
        && pose.pitch.is_finite()
        && pose.pitch.abs() <= 1.5
}

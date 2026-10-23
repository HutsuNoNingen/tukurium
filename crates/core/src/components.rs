use bevy_ecs::prelude::*;
use kokoverse_protocol::Address;
use std::collections::HashMap;

#[derive(Component)]
pub struct Identity(pub Address);

#[derive(Component)]
pub struct ObjectRoot {
    pub revision: u64,
    pub renderer: Address,
}

#[derive(Component)]
pub struct Slot {
    pub name: String,
    pub parent: Option<Address>,
}

#[derive(Component, Clone, Copy)]
pub struct LocalTransform {
    pub translation: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
}

#[derive(Component, Clone, Copy, Default)]
pub struct GlobalTransform {
    pub translation: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
}

#[derive(Component)]
pub struct Collider(pub kokoverse_protocol::ColliderSpec);

#[derive(Component)]
pub struct MediaPlayback(pub kokoverse_protocol::VideoPlayback);

#[derive(Component, Clone, Default)]
pub struct ScriptComponents(pub Vec<kokoverse_protocol::ScriptComponent>);

#[derive(Component, Clone)]
pub struct WorldPanel(pub kokoverse_protocol::WorldPanel);

#[derive(Component, Clone)]
pub struct InspectorView(pub kokoverse_protocol::InspectorView);

#[derive(Component, Clone)]
pub struct CreationView(pub kokoverse_protocol::CreationView);

#[derive(Component)]
pub struct MeshRenderer {
    pub host: Address,
    pub color: [f64; 4],
    pub visual: kokoverse_protocol::Visual,
}

#[derive(Component, Default)]
pub(crate) struct Revisions(pub HashMap<String, u64>);

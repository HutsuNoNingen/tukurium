//! Session chat is transient ECS data, separate from persistent world revisions.
use bevy_ecs::prelude::*;
use kokoverse_protocol::SessionChatMessage;
use serde_json::Value;
use std::collections::VecDeque;

pub const MAX_MESSAGES: usize = 200;

#[derive(Component)]
pub struct SessionChat(pub SessionChatMessage);

#[derive(Resource, Default)]
pub struct ChatHistory(pub VecDeque<Entity>);

pub fn validate_value(value: &Value) -> Result<(), String> {
    let message: SessionChatMessage = serde_json::from_value(value.clone())
        .map_err(|_| "Invalid chat message".to_string())?;
    let text = message.text.trim();
    if message.id.is_empty() || message.id.len() > 80
        || message.sender_id.is_empty() || message.sender_id.len() > 256
        || message.sender_name.trim().is_empty() || message.sender_name.chars().count() > 40
        || text.is_empty() || text.chars().count() > 1000 || text.len() > 4096
        || text.chars().any(char::is_control)
    {
        return Err("Invalid chat message".into());
    }
    Ok(())
}

pub fn append(world: &mut World, message: SessionChatMessage) -> bool {
    if world.resource::<ChatHistory>().0.iter().any(|entity| {
        world.get::<SessionChat>(*entity).is_some_and(|entry| entry.0.id == message.id)
    }) {
        return false;
    }
    let entity = world.spawn(SessionChat(message)).id();
    let old = {
        let mut history = world.resource_mut::<ChatHistory>();
        history.0.push_back(entity);
        if history.0.len() > MAX_MESSAGES { history.0.pop_front() } else { None }
    };
    if let Some(old) = old { world.despawn(old); }
    true
}

pub fn replace(world: &mut World, messages: Vec<SessionChatMessage>) {
    let old = std::mem::take(&mut world.resource_mut::<ChatHistory>().0);
    for entity in old { world.despawn(entity); }
    for message in messages { append(world, message); }
}

pub fn messages(world: &World) -> Vec<SessionChatMessage> {
    world.resource::<ChatHistory>().0.iter()
        .filter_map(|entity| world.get::<SessionChat>(*entity).map(|entry| entry.0.clone()))
        .collect()
}

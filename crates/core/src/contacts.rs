//! Host-authoritative contact edges for world Collider components.
use crate::{collision::Bounds, resources::SnapshotCache, scripting::ScriptRuntime};
use bevy_ecs::prelude::*;
use glam::{DQuat, DVec3};
use kokoverse_protocol::{Address, ObjectView};
use kokoverse_scripting::Event;
use std::collections::HashSet;

#[derive(Resource, Default, Clone)]
pub(crate) struct ContactRuntime {
    pub authority: bool,
    previous: HashSet<(Address, Address)>,
}

fn bounds(object: &ObjectView) -> Option<Bounds> {
    let shape = object.collider.as_ref()?;
    Some(Bounds {
        address: object.address.clone(),
        center: object.world_position,
        rotation: DQuat::from_array(object.world_rotation).normalize(),
        scale: object.world_scale,
        half: DVec3::from_array(shape.half_extents) * DVec3::from_array(object.world_scale),
        solid: shape.solid,
    })
}

pub(crate) fn detect(
    snapshot: Res<SnapshotCache>,
    mut contacts: ResMut<ContactRuntime>,
    mut scripts: ResMut<ScriptRuntime>,
) {
    if !contacts.authority { return; }
    let shapes: Vec<_> = snapshot.0.objects.iter().filter_map(|object| bounds(object).map(|b| (object, b))).collect();
    let mut current = HashSet::new();
    for (source, source_bounds) in &shapes {
        if source.contact_sensor != Some(true) { continue; }
        for (other, other_bounds) in &shapes {
            if source.address == other.address || !source_bounds.intersects(other_bounds) { continue; }
            let pair = (source.address.clone(), other.address.clone());
            if !contacts.previous.contains(&pair) {
                queue(&mut scripts, source, other, Event::ContactStart);
            }
            current.insert(pair);
        }
    }
    for (source_id, other_id) in contacts.previous.difference(&current) {
        if let Some(source) = snapshot.0.objects.iter().find(|o| &o.address == source_id).filter(|o| o.contact_sensor == Some(true)) {
            let other = snapshot.0.objects.iter().find(|o| &o.address == other_id);
            if let Some(other) = other { queue(&mut scripts, source, other, Event::ContactEnd); }
        }
    }
    contacts.previous = current;
}

fn queue(runtime: &mut ScriptRuntime, source: &ObjectView, other: &ObjectView, event: Event) {
    for script in source.scripts.iter().filter(|s| s.enabled && match event {
        Event::ContactStart => s.schema.has_contact_start,
        Event::ContactEnd => s.schema.has_contact_end,
        _ => false,
    }) {
        if runtime.events.len() >= 128 { break; }
        runtime.events.push((source.address.clone(), script.id.clone(), None, event, Some(other.address.clone())));
    }
}

#[cfg(test)]
mod tests {
    use crate::{WorldModel, new_object, replication::REGISTRY};
    use kokoverse_protocol::{Edit, ScriptAction, ScriptCommand, WorldCommand};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn contact_edges_run_on_host_and_survive_replication() {
        let mut host = WorldModel::new();
        let mut snapshot = host.snapshot();
        let other = new_object("Other".into(), [0.5, 0.75, 0.0], [1.0; 4]);
        let other_id = other.address.clone();
        snapshot.objects.push(other);
        let source_id = snapshot.objects[0].address.clone();
        REGISTRY.change_component(&mut snapshot.objects[0], "ContactSensor", Some(&json!({"contact_enabled":true}))).unwrap();
        host.restore(snapshot).unwrap();
        let source = "return {name='ContactTest',on_contact_start=function(ctx) ctx:set_state('start',ctx:get_state('start',0)+1); ctx:set_state('other',ctx.other.instance_id) end,on_contact_end=function(ctx) ctx:set_state('ended',ctx:get_state('ended',0)+1) end}";
        host.enqueue(WorldCommand::Script { command: ScriptCommand {
            command_id: "attach".into(), target: source_id,
            action: ScriptAction::Attach { source: source.into() },
        }}).unwrap();
        assert!(host.tick().remove(0).result.is_ok());
        let mut guest = WorldModel::new();
        guest.restore(host.snapshot()).unwrap();
        host.set_script_authority(true);
        host.advance(Duration::from_secs_f64(crate::locomotion::FIXED_STEP));
        let outcomes = host.advance(Duration::ZERO);
        for outcome in outcomes { if let Ok(commit) = outcome.result { guest.apply_commit(commit).unwrap(); } }
        let host_snapshot = host.snapshot();
        let script = &host_snapshot.objects.iter().find(|o| o.contact_sensor == Some(true)).unwrap().scripts[0];
        assert_eq!(script.state["start"], json!(1));
        assert_eq!(script.state["other"], json!(other_id.instance_id));
        assert_eq!(guest.snapshot(), host.snapshot());

        host.restore(host.snapshot()).unwrap();
        host.advance(Duration::from_secs_f64(crate::locomotion::FIXED_STEP));
        for outcome in host.advance(Duration::ZERO) { if let Ok(commit) = outcome.result { guest.apply_commit(commit).unwrap(); } }
        assert_eq!(host.snapshot().objects.iter().find(|o| o.contact_sensor == Some(true)).unwrap().scripts[0].state["start"], json!(1));
        assert_eq!(guest.snapshot(), host.snapshot());

        let object = host.snapshot().objects.into_iter().find(|o| o.address == other_id).unwrap();
        let field = object.fields.iter().find(|f| f.id == "position").unwrap();
        host.enqueue(WorldCommand::Edit { edit: Edit {
            command_id: "move".into(), target: REGISTRY.field_address(&other_id, "position").unwrap(),
            field_id: "position".into(), expected_revision: field.revision.clone(),
            value: json!([5.0,0.75,0.0]),
        }}).unwrap();
        let commit = host.tick().remove(0).result.unwrap();
        guest.apply_commit(commit).unwrap();
        host.advance(Duration::from_secs_f64(crate::locomotion::FIXED_STEP));
        for outcome in host.advance(Duration::ZERO) { if let Ok(commit) = outcome.result { guest.apply_commit(commit).unwrap(); } }
        assert_eq!(host.snapshot().objects.iter().find(|o| o.contact_sensor == Some(true)).unwrap().scripts[0].state["ended"], json!(1));
        assert_eq!(guest.snapshot(), host.snapshot());
    }
}

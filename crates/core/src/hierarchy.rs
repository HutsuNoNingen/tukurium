//! Parent relationships and transform propagation belong to the world, not the renderer.
use crate::{components::*, resources::*};
use bevy_ecs::prelude::*;
use glam::{DQuat, DVec3};
use kokoverse_protocol::{Address, ObjectView, Snapshot};
use std::collections::{HashMap, HashSet};

pub(crate) fn compose(parent: GlobalTransform, local: LocalTransform) -> GlobalTransform {
    let q = DQuat::from_array(parent.rotation);
    let s = DVec3::from_array(parent.scale);
    GlobalTransform {
        translation: (DVec3::from_array(parent.translation)
            + q * (s * DVec3::from_array(local.translation)))
        .to_array(),
        rotation: (q * DQuat::from_array(local.rotation))
            .normalize()
            .to_array(),
        scale: (s * DVec3::from_array(local.scale)).to_array(),
    }
}

pub(crate) fn local_of(object: &ObjectView) -> LocalTransform {
    let get = |id: &str| {
        object
            .fields
            .iter()
            .find(|f| f.id == id)
            .unwrap()
            .value
            .clone()
    };
    LocalTransform {
        translation: serde_json::from_value(get("position")).unwrap(),
        rotation: serde_json::from_value(get("rotation")).unwrap(),
        scale: serde_json::from_value(get("scale")).unwrap(),
    }
}

pub(crate) fn global_of(object: &ObjectView) -> GlobalTransform {
    GlobalTransform {
        translation: object.world_position,
        rotation: object.world_rotation,
        scale: object.world_scale,
    }
}

pub(crate) fn relative(global: GlobalTransform, parent: GlobalTransform) -> LocalTransform {
    let q = DQuat::from_array(parent.rotation).inverse();
    let scale = DVec3::from_array(parent.scale);
    LocalTransform {
        translation: (q
            * (DVec3::from_array(global.translation) - DVec3::from_array(parent.translation))
            / scale)
            .to_array(),
        rotation: (q * DQuat::from_array(global.rotation))
            .normalize()
            .to_array(),
        scale: (DVec3::from_array(global.scale) / scale).to_array(),
    }
}

pub(crate) fn identity() -> GlobalTransform {
    GlobalTransform {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
    }
}

pub(crate) fn parent_transform(
    snapshot: &Snapshot,
    parent: &Option<Address>,
) -> Result<GlobalTransform, String> {
    parent.as_ref().map_or(Ok(identity()), |p| {
        snapshot
            .objects
            .iter()
            .find(|o| &o.address == p)
            .map(global_of)
            .ok_or("Parent no longer exists".into())
    })
}

pub(crate) fn validate_parent(
    snapshot: &Snapshot,
    target: &Address,
    parent: &Option<Address>,
) -> Result<(), String> {
    let mut at = parent.clone();
    let mut seen = HashSet::new();
    while let Some(address) = at {
        if address == *target || !seen.insert(address.clone()) {
            return Err("A Slot cannot be parented under itself or its descendants".into());
        }
        let object = snapshot
            .objects
            .iter()
            .find(|o| o.address == address)
            .ok_or("Parent no longer exists")?;
        at = object.parent.clone();
    }
    Ok(())
}

pub(crate) fn validate(snapshot: &Snapshot) -> Result<(), String> {
    for object in &snapshot.objects {
        validate_parent(snapshot, &object.address, &object.parent)?;
        let expected = compose(
            parent_transform(snapshot, &object.parent)?,
            local_of(object),
        );
        let close = |a: &[f64], b: &[f64]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-8);
        if !close(&expected.translation, &object.world_position)
            || !close(&expected.rotation, &object.world_rotation)
            || !close(&expected.scale, &object.world_scale)
        {
            return Err("Derived hierarchy transform does not match local fields".into());
        }
    }
    Ok(())
}

pub(crate) fn propagate(world: &mut World) {
    let nodes: HashMap<Address, (Entity, Option<Address>, LocalTransform)> = world
        .query::<(Entity, &Identity, &Slot, &LocalTransform)>()
        .iter(world)
        .map(|(e, id, slot, local)| (id.0.clone(), (e, slot.parent.clone(), *local)))
        .collect();
    let mut globals = HashMap::new();
    while globals.len() < nodes.len() {
        let before = globals.len();
        for (address, (_, parent, local)) in &nodes {
            if globals.contains_key(address) {
                continue;
            }
            let base = match parent {
                Some(p) => match globals.get(p) {
                    Some(global) => *global,
                    None => continue,
                },
                None => identity(),
            };
            globals.insert(address.clone(), compose(base, *local));
        }
        assert!(globals.len() > before, "validated Slot hierarchy");
    }
    for (address, global) in globals {
        let entity = nodes[&address].0;
        let current = world.get::<GlobalTransform>(entity).unwrap();
        if current.translation != global.translation
            || current.rotation != global.rotation
            || current.scale != global.scale
        {
            *world.get_mut::<GlobalTransform>(entity).unwrap() = global;
        }
    }
}

pub(crate) fn reparent(
    world: &mut World,
    entity: Entity,
    parent: Option<Address>,
    local: LocalTransform,
) {
    world.get_mut::<Slot>(entity).unwrap().parent = parent;
    *world.get_mut::<LocalTransform>(entity).unwrap() = local;
    for field in ["parent", "position", "rotation", "scale"] {
        *world
            .get_mut::<Revisions>(entity)
            .unwrap()
            .0
            .entry(field.into())
            .or_default() += 1;
    }
    world.get_mut::<ObjectRoot>(entity).unwrap().revision += 1;
}

pub(crate) fn subtree(snapshot: &Snapshot, root: &Address) -> Vec<Address> {
    let mut found = vec![root.clone()];
    loop {
        let more: Vec<_> = snapshot
            .objects
            .iter()
            .filter(|o| {
                o.parent.as_ref().is_some_and(|p| found.contains(p)) && !found.contains(&o.address)
            })
            .map(|o| o.address.clone())
            .collect();
        if more.is_empty() {
            return found;
        }
        found.extend(more);
    }
}

pub(crate) fn duplicate(snapshot: &Snapshot, root: &Address) -> Result<Vec<ObjectView>, String> {
    let addresses = subtree(snapshot, root);
    if snapshot.objects.len() + addresses.len() > crate::MAX_OBJECTS {
        return Err("Object limit reached (128)".into());
    }
    let mapping: HashMap<_, _> = addresses
        .iter()
        .map(|a| {
            (
                a.clone(),
                Address {
                    instance_id: uuid::Uuid::new_v4().to_string(),
                    local_id: 1,
                },
            )
        })
        .collect();
    let mut copies = Vec::new();
    for source in snapshot
        .objects
        .iter()
        .filter(|o| addresses.contains(&o.address))
    {
        let mut object = source.clone();
        object.address = mapping[&source.address].clone();
        object.renderer.instance_id = object.address.instance_id.clone();
        object.parent = source
            .parent
            .as_ref()
            .map(|p| mapping.get(p).unwrap_or(p).clone());
        object.object_revision = "0".into();
        object.world_position[0] += 1.8;
        let local = relative(
            global_of(&object),
            parent_transform(snapshot, &source.parent)?,
        );
        if &source.address == root {
            crate::validate_value("position", &serde_json::json!(local.translation))?;
        }
        for field in &mut object.fields {
            field.revision = "0".into();
            match field.id.as_str() {
                "parent" => field.value = serde_json::json!(object.parent),
                "position" if &source.address == root => {
                    field.value = serde_json::json!(local.translation)
                }
                "name" if &source.address == root => {
                    field.value = serde_json::json!(format!(
                        "{} copy",
                        field
                            .value
                            .as_str()
                            .unwrap()
                            .chars()
                            .take(75)
                            .collect::<String>()
                    ))
                }
                _ => {}
            }
        }
        if let Some(panel) = &mut object.panel {
            panel.attachment = None;
        }
        for script in &mut object.scripts {
            script.id = uuid::Uuid::new_v4().to_string();
            script.revision = "0".into();
            for value in script.parameters.values_mut() {
                if let Ok(mut reference) =
                    serde_json::from_value::<kokoverse_protocol::TypedReference>(value.clone())
                {
                    let target = match &mut reference {
                        kokoverse_protocol::TypedReference::Slot { target }
                        | kokoverse_protocol::TypedReference::Component { target }
                        | kokoverse_protocol::TypedReference::Field { target, .. } => target,
                    };
                    let key = Address {
                        instance_id: target.instance_id.clone(),
                        local_id: 1,
                    };
                    if let Some(mapped) = mapping.get(&key) {
                        target.instance_id = mapped.instance_id.clone();
                    }
                    *value = serde_json::to_value(reference).unwrap();
                }
            }
        }
        if object.panel.is_some() {
            crate::panels::append_fields(&mut object, &Revisions::default());
        }
        copies.push(object);
    }
    copies.sort_by_key(|o| o.address != mapping[root]);
    crate::scripting::validate_world_scripts(
        snapshot
            .objects
            .iter()
            .chain(&copies)
            .map(|o| o.scripts.as_slice()),
    )?;
    if snapshot
        .objects
        .iter()
        .chain(&copies)
        .filter(|o| o.panel.is_some())
        .count()
        > crate::panels::MAX_PANELS
    {
        return Err("Panel limit reached (16)".into());
    }
    let image_bytes: usize = snapshot
        .objects
        .iter()
        .chain(&copies)
        .map(|o| match &o.visual {
            kokoverse_protocol::Visual::Image { data_url, .. } => data_url.len(),
            _ => 0,
        })
        .sum();
    if image_bytes > crate::MAX_IMAGE_WORLD {
        return Err("World image budget exceeded".into());
    }
    Ok(copies)
}

pub(crate) fn world_to_local(
    world: &World,
    entity: Entity,
    global: GlobalTransform,
) -> LocalTransform {
    let parent = world.get::<Slot>(entity).unwrap().parent.as_ref();
    let base = parent.map_or_else(identity, |address| {
        *world
            .get::<GlobalTransform>(world.resource::<EntityIndex>().0[address])
            .unwrap()
    });
    relative(global, base)
}

#[derive(Component, Clone, Copy)]
pub(crate) struct EvaluatedGlobal(pub GlobalTransform);

pub(crate) fn evaluate(world: &mut World) -> Vec<kokoverse_protocol::EvaluatedTransform> {
    let nodes: HashMap<_, _> = world
        .query::<(
            Entity,
            &Identity,
            &Slot,
            &LocalTransform,
            Option<&crate::drives::DrivenFields>,
        )>()
        .iter(world)
        .map(|(e, id, slot, local, driven)| {
            let mut local = *local;
            if let Some(driven) = driven {
                if let Some(v) = driven.0.get("position") {
                    local.translation = serde_json::from_value(v.clone()).unwrap();
                }
                if let Some(v) = driven.0.get("rotation") {
                    local.rotation = serde_json::from_value(v.clone()).unwrap();
                }
                if let Some(v) = driven.0.get("scale") {
                    local.scale = serde_json::from_value(v.clone()).unwrap();
                }
            }
            (id.0.clone(), (e, slot.parent.clone(), local))
        })
        .collect();
    let mut resolved = HashMap::new();
    let previews = world
        .resource::<crate::collision::PreviewPositions>()
        .0
        .clone();
    let grab = world
        .resource::<crate::grabbing::GrabState>()
        .0
        .as_ref()
        .map(|g| g.view.preview.clone());
    while resolved.len() < nodes.len() {
        for (address, (_, parent, local)) in &nodes {
            if resolved.contains_key(address) {
                continue;
            }
            let base = match parent {
                Some(p) => match resolved.get(p) {
                    Some(v) => *v,
                    None => continue,
                },
                None => identity(),
            };
            let mut global = compose(base, *local);
            let entity = nodes[address].0;
            let preview = grab
                .as_ref()
                .filter(|p| &p.target == address)
                .or_else(|| previews.iter().find(|p| &p.target == address))
                .filter(|p| {
                    crate::collision::preview_matches(p, world.get::<Revisions>(entity).unwrap())
                });
            if let Some(p) = preview {
                global.translation = p.position;
                if let Some(q) = p.rotation {
                    global.rotation = q;
                }
                if let Some(s) = p.scale {
                    global.scale = s;
                }
            }
            resolved.insert(address.clone(), global);
        }
    }
    let mut output = Vec::new();
    for (address, (entity, parent, _)) in nodes {
        let global = resolved[&address];
        if parent.is_some() {
            world.entity_mut(entity).insert(EvaluatedGlobal(global));
            output.push(kokoverse_protocol::EvaluatedTransform {
                target: address,
                position: global.translation,
                rotation: global.rotation,
                scale: global.scale,
            });
        } else {
            world.entity_mut(entity).remove::<EvaluatedGlobal>();
        }
    }
    output.sort_by(|a, b| a.target.instance_id.cmp(&b.target.instance_id));
    output
}

pub(crate) fn update_evaluated(world: &mut World) {
    evaluate(world);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldModel;
    use kokoverse_protocol::{
        Edit, ScriptAction, ScriptCommand, StructuralCommand, StructuralOperation, WorldCommand,
    };
    use std::time::Duration;
    fn structure(model: &mut WorldModel, operation: StructuralOperation) -> crate::CommandOutcome {
        model
            .enqueue(WorldCommand::Structure {
                command: StructuralCommand {
                    command_id: "test".into(),
                    operation,
                },
            })
            .unwrap();
        model.tick().remove(0)
    }
    fn edit(model: &mut WorldModel, target: &Address, field: &str, value: serde_json::Value) {
        let object = model
            .snapshot()
            .objects
            .into_iter()
            .find(|o| &o.address == target)
            .unwrap();
        let data = object.fields.iter().find(|f| f.id == field).unwrap();
        model
            .enqueue(WorldCommand::Edit {
                edit: Edit {
                    command_id: "edit".into(),
                    target: Address {
                        local_id: data.component_id,
                        ..target.clone()
                    },
                    field_id: field.into(),
                    expected_revision: data.revision.clone(),
                    value,
                },
            })
            .unwrap();
        model.tick()[0].result.as_ref().unwrap();
    }
    #[test]
    fn hierarchy_round_trips_and_reparent_preserves_world_pose() {
        let mut model = WorldModel::new();
        let root = model.snapshot().objects[0].address.clone();
        let child = structure(
            &mut model,
            StructuralOperation::CreateChild {
                parent: root.clone(),
            },
        )
        .selected
        .unwrap();
        edit(
            &mut model,
            &child,
            "position",
            serde_json::json!([1.0, 0.0, 0.0]),
        );
        edit(
            &mut model,
            &root,
            "scale",
            serde_json::json!([2.0, 2.0, 2.0]),
        );
        let q = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2).to_array();
        edit(&mut model, &root, "rotation", serde_json::json!(q));
        let snapshot = model.snapshot();
        let object = snapshot
            .objects
            .iter()
            .find(|o| o.address == child)
            .unwrap();
        assert!((object.world_position[2] + 2.0).abs() < 1e-8);
        let mut replica = WorldModel::new();
        replica
            .restore(
                crate::replication::REGISTRY
                    .decode_world(crate::replication::REGISTRY.encode_world(&snapshot))
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(replica.snapshot(), snapshot);
        let result = structure(
            &mut model,
            StructuralOperation::Reparent {
                target: child.clone(),
                parent: None,
                expected_revision: object.object_revision.clone(),
            },
        );
        result.result.as_ref().unwrap();
        let after = model
            .snapshot()
            .objects
            .into_iter()
            .find(|o| o.address == child)
            .unwrap();
        assert_eq!(after.parent, None);
        assert!(
            DVec3::from_array(after.world_position)
                .abs_diff_eq(DVec3::from_array(object.world_position), 1e-8)
        );
    }
    #[test]
    fn cycles_and_dangling_parents_are_atomic_and_deleting_parent_deletes_descendants() {
        let mut model = WorldModel::new();
        let root = model.snapshot().objects[0].address.clone();
        let child = structure(
            &mut model,
            StructuralOperation::CreateChild {
                parent: root.clone(),
            },
        )
        .selected
        .unwrap();
        let before = model.snapshot();
        let root_object = before.objects.iter().find(|o| o.address == root).unwrap();
        assert!(
            structure(
                &mut model,
                StructuralOperation::Reparent {
                    target: root.clone(),
                    parent: Some(child.clone()),
                    expected_revision: root_object.object_revision.clone()
                }
            )
            .result
            .is_err()
        );
        assert_eq!(model.snapshot(), before);
        let mut forged = before.clone();
        forged
            .objects
            .iter_mut()
            .find(|o| o.address == root)
            .unwrap()
            .parent = Some(child);
        assert!(model.restore(forged).is_err());
        assert_eq!(model.snapshot(), before);
        let result = structure(
            &mut model,
            StructuralOperation::Delete {
                target: root,
                expected_revision: root_object.object_revision.clone(),
            },
        );
        assert_eq!(result.result.unwrap().removed.len(), 2);
        assert!(model.snapshot().objects.is_empty());
    }
    #[test]
    fn duplicating_a_subtree_keeps_children_under_the_new_root() {
        let mut model = WorldModel::new();
        let root = model.snapshot().objects[0].address.clone();
        let child = structure(
            &mut model,
            StructuralOperation::CreateChild {
                parent: root.clone(),
            },
        )
        .selected
        .unwrap();
        edit(
            &mut model,
            &child,
            "position",
            serde_json::json!([1.0, 0.0, 0.0]),
        );
        let before = model.snapshot();
        let revision = before
            .objects
            .iter()
            .find(|o| o.address == root)
            .unwrap()
            .object_revision
            .clone();
        let result = structure(
            &mut model,
            StructuralOperation::Duplicate {
                target: root,
                expected_revision: revision,
            },
        );
        result.result.as_ref().unwrap();
        let new_root = result.selected.unwrap();
        let snapshot = model.snapshot();
        assert_eq!(snapshot.objects.len(), 4);
        assert!(
            snapshot
                .objects
                .iter()
                .any(|o| o.parent.as_ref() == Some(&new_root))
        );
        let mut restored = WorldModel::new();
        restored.restore(snapshot.clone()).unwrap();
        assert_eq!(restored.snapshot(), snapshot);
    }
    #[test]
    fn local_parent_drive_moves_child_renderer_and_collider_without_commits() {
        let mut model = WorldModel::new();
        let root = model.snapshot().objects[0].address.clone();
        let child = structure(
            &mut model,
            StructuralOperation::CreateChild {
                parent: root.clone(),
            },
        )
        .selected
        .unwrap();
        edit(
            &mut model,
            &child,
            "position",
            serde_json::json!([2.0, 0.0, 0.0]),
        );
        model.enqueue(WorldCommand::Script{command:ScriptCommand{command_id:"drive".into(),target:root,action:ScriptAction::Attach{source:"return {name='QuarterTurn',drive=function(ctx) ctx:drive(ctx.self,'rotation',ctx:rotation_y(90)) end}".into()}}}).unwrap();
        model.tick()[0].result.as_ref().unwrap();
        let before = model.snapshot();
        model.advance(Duration::from_secs_f64(1.0 / 60.0));
        let frame = model.drive_frame();
        let transform = frame.transforms.iter().find(|t| t.target == child).unwrap();
        assert!((transform.position[2] + 2.0).abs() < 1e-8);
        let collision = &model.world.resource::<crate::collision::CollisionWorld>().0;
        assert_eq!(
            collision
                .iter()
                .find(|c| c.address == child)
                .unwrap()
                .center,
            transform.position
        );
        assert_eq!(model.snapshot(), before);
    }
}

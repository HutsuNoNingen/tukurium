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

/// Address → object lookup built once per validation pass.
/// Walking parent chains against this index keeps whole-snapshot
/// validation near O(N + total chain length) instead of rescanning
/// the object list for every hop of every chain.
pub(crate) type ObjectIndex<'a> = HashMap<&'a Address, &'a ObjectView>;

pub(crate) fn object_index(snapshot: &Snapshot) -> ObjectIndex<'_> {
    snapshot.objects.iter().map(|o| (&o.address, o)).collect()
}

pub(crate) fn parent_transform(
    snapshot: &Snapshot,
    parent: &Option<Address>,
) -> Result<GlobalTransform, String> {
    parent_transform_indexed(&object_index(snapshot), parent)
}

fn parent_transform_indexed(
    index: &ObjectIndex<'_>,
    parent: &Option<Address>,
) -> Result<GlobalTransform, String> {
    parent.as_ref().map_or(Ok(identity()), |p| {
        index
            .get(p)
            .map(|object| global_of(object))
            .ok_or("Parent no longer exists".into())
    })
}

pub(crate) fn validate_parent(
    snapshot: &Snapshot,
    target: &Address,
    parent: &Option<Address>,
) -> Result<(), String> {
    validate_parent_indexed(&object_index(snapshot), target, parent)
}

fn validate_parent_indexed(
    index: &ObjectIndex<'_>,
    target: &Address,
    parent: &Option<Address>,
) -> Result<(), String> {
    let mut at = parent.clone();
    let mut seen = HashSet::new();
    while let Some(address) = at {
        if address == *target || !seen.insert(address.clone()) {
            return Err("A Slot cannot be parented under itself or its descendants".into());
        }
        let object = index
            .get(&address)
            .copied()
            .ok_or("Parent no longer exists")?;
        at = object.parent.clone();
    }
    Ok(())
}

pub(crate) fn validate(snapshot: &Snapshot) -> Result<(), String> {
    let index = object_index(snapshot);
    for object in &snapshot.objects {
        validate_parent_indexed(&index, &object.address, &object.parent)?;
        let expected = compose(
            parent_transform_indexed(&index, &object.parent)?,
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
        if globals.len() == before {
            // Parent missing or a cycle: the hierarchy was not validated.
            // Leave unresolved entities at their previous transform instead
            // of hanging or panicking (same exclusion policy as evaluate).
            break;
        }
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
        if let Some(renderer) = &mut object.renderer { renderer.instance_id = object.address.instance_id.clone(); }
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

/// Convert a released world-space grab using the parent pose visible to this client.
/// A parent Drive or preview is transient and must not be written into shared fields.
pub(crate) fn local_from_evaluated_parent(
    world: &mut World,
    target: &Address,
    global: GlobalTransform,
) -> Option<LocalTransform> {
    let entity = *world.resource::<EntityIndex>().0.get(target)?;
    let parent = world.get::<Slot>(entity)?.parent.clone();
    let Some(parent) = parent else {
        return Some(relative(global, identity()));
    };
    let evaluated = evaluate(world)
        .into_iter()
        .find(|frame| frame.target == parent)?;
    Some(relative(
        global,
        GlobalTransform {
            translation: evaluated.position,
            rotation: evaluated.rotation,
            scale: evaluated.scale,
        },
    ))
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
        let before = resolved.len();
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
        if resolved.len() == before {
            // Parent missing or a cycle: the hierarchy was not validated.
            // Exclude the unresolved subtrees from the output instead of
            // looping forever (same exclusion policy as propagate).
            break;
        }
    }
    let mut output = Vec::new();
    for (address, (entity, parent, local)) in nodes {
        let Some(&global) = resolved.get(&address) else {
            continue;
        };
        if parent.is_some() {
            world.entity_mut(entity).insert(EvaluatedGlobal(global));
        } else {
            world.entity_mut(entity).remove::<EvaluatedGlobal>();
        }
        output.push(kokoverse_protocol::EvaluatedTransform {
            target: address,
            parent,
            object_revision: world.get::<ObjectRoot>(entity).unwrap().revision.to_string(),
            local_position: local.translation,
            local_rotation: local.rotation,
            local_scale: local.scale,
            position: global.translation,
            rotation: global.rotation,
            scale: global.scale,
        });
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
        Edit, PositionSpace, ScriptAction, ScriptCommand, StructuralCommand, StructuralOperation,
        TransformCommand, WorldCommand,
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
    fn released_child_uses_driven_parent_pose_for_local_transform() {
        let mut model = WorldModel::new();
        model.set_media_time(100.0);
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
        edit(
            &mut model,
            &root,
            "scale",
            serde_json::json!([1.5, 1.0, 0.75]),
        );
        model
            .enqueue(WorldCommand::Script {
                command: ScriptCommand {
                    command_id: "rotator".into(),
                    target: root.clone(),
                    action: ScriptAction::Attach {
                        source: crate::scripting::ROTATOR.into(),
                    },
                },
            })
            .unwrap();
        model.tick()[0].result.as_ref().unwrap();
        model.set_media_time(102.0);
        model.advance(Duration::from_millis(17));
        let frame = model.drive_frame();
        let shown = frame
            .transforms
            .iter()
            .find(|pose| pose.target == child)
            .unwrap();
        let global = GlobalTransform {
            translation: (DVec3::from_array(shown.position) + DVec3::new(0.5, 0.0, 0.2)).to_array(),
            rotation: shown.rotation,
            scale: shown.scale,
        };
        let local = local_from_evaluated_parent(&mut model.world, &child, global).unwrap();
        let base_local = relative(
            global,
            parent_transform(&model.snapshot(), &Some(root.clone())).unwrap(),
        );
        assert!(
            (DVec3::from_array(local.translation) - DVec3::from_array(base_local.translation))
                .length()
                > 0.5
        );
        let snapshot = model.snapshot();
        let object = snapshot
            .objects
            .iter()
            .find(|object| object.address == child)
            .unwrap();
        let revision = |id: &str| {
            object
                .fields
                .iter()
                .find(|field| field.id == id)
                .unwrap()
                .revision
                .clone()
        };
        let mut guest = WorldModel::new();
        guest.restore(snapshot.clone()).unwrap();
        model
            .enqueue(WorldCommand::Transform {
                command: TransformCommand {
                    command_id: "release".into(),
                    target: child.clone(),
                    expected_position_revision: revision("position"),
                    expected_rotation_revision: revision("rotation"),
                    expected_scale_revision: revision("scale"),
                    space: PositionSpace::Parent,
                    position: local.translation,
                    rotation: local.rotation,
                    scale: local.scale,
                },
            })
            .unwrap();
        let commit = model.tick().remove(0).result.unwrap();
        guest.apply_commit(commit).unwrap();
        assert_eq!(model.snapshot(), guest.snapshot());
        let saved = crate::replication::REGISTRY.encode_world(&model.snapshot());
        let mut restored = WorldModel::new();
        restored
            .restore(crate::replication::REGISTRY.decode_world(saved).unwrap())
            .unwrap();
        assert_eq!(model.snapshot(), restored.snapshot());
        model.advance(Duration::from_millis(17));
        let after = model
            .drive_frame()
            .transforms
            .into_iter()
            .find(|pose| pose.target == child)
            .unwrap();
        assert!(
            (DVec3::from_array(after.position) - DVec3::from_array(global.translation)).length()
                < 0.1
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
        model.enqueue(WorldCommand::Script{command:ScriptCommand{command_id:"drive".into(),target:root.clone(),action:ScriptAction::Attach{source:"return {name='QuarterTurn',drive=function(ctx) ctx:drive(ctx.self,'rotation',ctx:rotation_y(90)) end}".into()}}}).unwrap();
        model.tick()[0].result.as_ref().unwrap();
        let before = model.snapshot();
        model.advance(Duration::from_secs_f64(1.0 / 60.0));
        let frame = model.drive_frame();
        let transform = frame.transforms.iter().find(|t| t.target == child).unwrap();
        assert!((transform.position[2] + 2.0).abs() < 1e-8);
        assert_eq!(transform.parent.as_ref(), Some(&root));
        assert_eq!(transform.local_position, [2.0, 0.0, 0.0]);
        let root_pose = frame
            .transforms
            .iter()
            .find(|t| t.parent.is_none())
            .unwrap();
        assert_eq!(root_pose.local_rotation, root_pose.rotation);
        let root_object = before.objects.iter().find(|o| o.address == root).unwrap();
        assert_eq!(root_pose.object_revision, root_object.object_revision);
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
        model.set_collision_previews(vec![kokoverse_protocol::ObjectPreview {
            target: root.clone(),
            expected_revision: root_object.fields.iter().find(|f| f.id == "position").unwrap().revision.clone(),
            position: [8.0, 2.0, 3.0],
            ..Default::default()
        }]);
        model.advance(Duration::from_secs_f64(1.0 / 60.0));
        let frame = model.drive_frame();
        let root_pose = frame.transforms.iter().find(|t| t.target == root).unwrap();
        let child_pose = frame.transforms.iter().find(|t| t.target == child).unwrap();
        assert_eq!(root_pose.position, [8.0, 2.0, 3.0]);
        assert_eq!(root_pose.local_position, root_object.world_position);
        assert_eq!(child_pose.local_position, [2.0, 0.0, 0.0]);
        assert!((child_pose.position[2] - 1.0).abs() < 1e-8);
        assert_eq!(model.snapshot(), before);
    }

    fn bare_world() -> World {
        let mut world = World::new();
        world.insert_resource(crate::collision::PreviewPositions(vec![]));
        world.insert_resource(crate::grabbing::GrabState(None));
        world
    }

    fn spawn_node(world: &mut World, id: &str, parent: Option<Address>) -> Entity {
        world
            .spawn((
                Identity(Address {
                    instance_id: id.into(),
                    local_id: 0,
                }),
                Slot {
                    name: id.into(),
                    parent,
                },
                LocalTransform {
                    translation: [1.0, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                },
                GlobalTransform::default(),
                ObjectRoot {
                    revision: 1,
                    renderer: None,
                    item_identity: None,
                },
                Revisions::default(),
            ))
            .id()
    }

    fn address(id: &str) -> Address {
        Address {
            instance_id: id.into(),
            local_id: 0,
        }
    }

    #[test]
    fn broken_hierarchy_is_excluded_instead_of_hanging_or_panicking() {
        // Parent that does not exist: the child is excluded, the healthy
        // root still resolves, and neither entry point hangs or panics.
        let mut world = bare_world();
        let root = spawn_node(&mut world, "root", None);
        spawn_node(&mut world, "orphan", Some(address("ghost")));
        let output = evaluate(&mut world);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].target, address("root"));
        propagate(&mut world);
        assert_eq!(
            world.get::<GlobalTransform>(root).unwrap().translation,
            [1.0, 0.0, 0.0]
        );

        // A parent cycle: nothing resolves, both entry points just return.
        let mut world = bare_world();
        spawn_node(&mut world, "a", Some(address("b")));
        spawn_node(&mut world, "b", Some(address("a")));
        assert!(evaluate(&mut world).is_empty());
        propagate(&mut world);
    }
}

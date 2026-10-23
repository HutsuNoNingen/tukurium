//! Shared panel semantics; Godot owns only rendering, hit testing and local input.
use crate::{components::*, resources::*, systems::bump_field};
use bevy_ecs::prelude::*;
use kokoverse_protocol::{Address, ObjectView, PanelAttachment, Snapshot, Visual};
use serde_json::{Value, json};

pub const MAX_PANELS: usize = 16;
pub const FIELDS: [&str; 7] = [
    "panel_size",
    "panel_pin",
    "inspector_target",
    "panel_scroll",
    "inspector_scripts_open",
    "inspector_script_id",
    "inspector_tree",
];

pub fn valid_address(value: &Value) -> bool {
    value.is_null()
        || serde_json::from_value::<Address>(value.clone())
            .is_ok_and(|a| a.local_id == 1 && uuid::Uuid::parse_str(&a.instance_id).is_ok())
}

pub(crate) fn validate_field(id: &str, value: &Value) -> Result<(), String> {
    let valid = match id {
        "inspector_tree" => serde_json::from_value::<kokoverse_protocol::InspectorTree>(value.clone()).is_ok_and(|tree| {
            valid_address(&json!(tree.root)) && tree.collapsed.len() <= crate::MAX_OBJECTS && tree.collapsed.iter().all(|a| valid_address(&json!(a)))
        }),
        "panel_size" => serde_json::from_value::<[u32; 2]>(value.clone())
            .is_ok_and(|v| v.iter().all(|x| (256..=1536).contains(x))),
        "inspector_target" => valid_address(value),
        "inspector_scripts_open" => value.is_boolean(),
        "inspector_script_id" => {
            value.is_null()
                || value
                    .as_str()
                    .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
        }
        "panel_scroll" => serde_json::from_value::<[f64; 2]>(value.clone())
            .is_ok_and(|v| v.iter().all(|x| x.is_finite() && (0.0..=1.0).contains(x))),
        "panel_pin" => {
            value.is_null()
                || serde_json::from_value::<PanelAttachment>(value.clone()).is_ok_and(|a| {
                    !a.user_id.is_empty()
                        && a.user_id.len() <= 512
                        && a.offset.iter().all(|v| v.is_finite() && v.abs() <= 100.0)
                        && crate::validate_value("rotation", &json!(a.rotation)).is_ok()
                })
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("Invalid {id}"))
    }
}

pub(crate) fn append_fields(object: &mut ObjectView, revisions: &Revisions) {
    let (Some(panel), Some(inspector)) = (&object.panel, &object.inspector) else {
        return;
    };
    for (id, label, kind, value) in [
        ("inspector_tree", "Hierarchy", "inspector_tree", json!(inspector.tree)),
        (
            "panel_size",
            "Panel resolution",
            "panel_size",
            json!(panel.size),
        ),
        (
            "panel_pin",
            "Follow user",
            "panel_pin",
            json!(panel.attachment),
        ),
        (
            "inspector_target",
            "Inspected object",
            "address",
            json!(inspector.target),
        ),
        (
            "panel_scroll",
            "Scroll",
            "panel_scroll",
            json!(inspector.scroll),
        ),
        (
            "inspector_scripts_open",
            "Script editor",
            "bool",
            json!(inspector.scripts_open),
        ),
        (
            "inspector_script_id",
            "Selected script",
            "script_id",
            json!(inspector.script_id),
        ),
    ] {
        if let Some(f) = object.fields.iter_mut().find(|f| f.id == id) {
            f.value = value;
        } else {
            object
                .fields
                .push(crate::field(id, label, kind, value, revisions));
        }
    }
}

pub(crate) fn validate_object(object: &ObjectView) -> Result<(), String> {
    let is_panel = matches!(object.visual, Visual::Panel);
    if is_panel != object.panel.is_some() || is_panel != object.inspector.is_some() {
        return Err("Panel visual and components must agree".into());
    }
    if is_panel {
        let mut expected = object.clone();
        expected.fields.clear();
        append_fields(&mut expected, &Revisions::default());
        for field in expected.fields {
            let actual = object
                .fields
                .iter()
                .find(|f| f.id == field.id)
                .ok_or("Missing panel field")?;
            let panel = object.panel.as_ref().unwrap();
            let inspector = object.inspector.as_ref().unwrap();
            let matches = match field.id.as_str() {
                "panel_size" => {
                    serde_json::from_value::<[u32; 2]>(actual.value.clone()).ok()
                        == Some(panel.size)
                }
                "panel_pin" => {
                    serde_json::from_value::<Option<PanelAttachment>>(actual.value.clone()).ok()
                        == Some(panel.attachment.clone())
                }
                "panel_scroll" => {
                    serde_json::from_value::<[f64; 2]>(actual.value.clone()).ok()
                        == Some(inspector.scroll)
                }
                "inspector_target" => {
                    serde_json::from_value::<Option<Address>>(actual.value.clone()).ok()
                        == Some(inspector.target.clone())
                }
                "inspector_scripts_open" => actual.value == json!(inspector.scripts_open),
                "inspector_script_id" => actual.value == json!(inspector.script_id),
                "inspector_tree" => actual.value == json!(inspector.tree),
                _ => false,
            };
            if !matches || actual.value_type != field.value_type {
                return Err("Panel component does not match its fields".into());
            }
        }
    }
    Ok(())
}

pub(crate) fn create(
    snapshot: &Snapshot,
    target: Option<Address>,
    position: [f64; 3],
    rotation: [f64; 4],
) -> Result<ObjectView, String> {
    if snapshot.objects.len() >= crate::MAX_OBJECTS
        || snapshot
            .objects
            .iter()
            .filter(|o| o.panel.is_some())
            .count()
            >= MAX_PANELS
    {
        return Err("Panel limit reached (16 panels / 128 objects)".into());
    }
    if target
        .as_ref()
        .is_some_and(|a| !snapshot.objects.iter().any(|o| &o.address == a))
    {
        return Err("Inspector target no longer exists".into());
    }
    crate::validate_value("position", &json!(position))?;
    crate::validate_value("rotation", &json!(rotation))?;
    let mut object = crate::new_object("Inspector".into(), position, [1.0; 4]);
    object.visual = Visual::Panel;
    object.world_rotation = rotation;
    object.panel = Some(kokoverse_protocol::WorldPanel {
        size: [1120, 960],
        attachment: None,
    });
    object.inspector = Some(kokoverse_protocol::InspectorView {
        target,
        scroll: [0.0; 2],
        scripts_open: false,
        script_id: None,
        tree: Default::default(),
    });
    crate::set_default_collider(&mut object);
    Ok(object)
}

pub(crate) fn write_field(world: &mut World, entity: Entity, field: &str, value: Value) {
    match field {
        "inspector_tree" => world.get_mut::<InspectorView>(entity).unwrap().0.tree = serde_json::from_value(value).unwrap(),
        "panel_size" => {
            world.get_mut::<WorldPanel>(entity).unwrap().0.size =
                serde_json::from_value(value).unwrap()
        }
        "panel_pin" => {
            world.get_mut::<WorldPanel>(entity).unwrap().0.attachment =
                serde_json::from_value(value).unwrap()
        }
        "inspector_target" => {
            let mut inspector = world.get_mut::<InspectorView>(entity).unwrap();
            inspector.0.target = serde_json::from_value(value).unwrap();
            inspector.0.scroll = [0.0; 2];
            inspector.0.script_id = None;
            for field in ["panel_scroll", "inspector_script_id"] {
                *world
                    .get_mut::<Revisions>(entity)
                    .unwrap()
                    .0
                    .entry(field.into())
                    .or_default() += 1;
            }
        }
        "inspector_scripts_open" => {
            world
                .get_mut::<InspectorView>(entity)
                .unwrap()
                .0
                .scripts_open = serde_json::from_value(value).unwrap()
        }
        "inspector_script_id" => {
            world.get_mut::<InspectorView>(entity).unwrap().0.script_id =
                serde_json::from_value(value).unwrap()
        }
        "panel_scroll" => {
            world.get_mut::<InspectorView>(entity).unwrap().0.scroll =
                serde_json::from_value(value).unwrap()
        }
        _ => unreachable!(),
    }
}

pub(crate) fn apply_field(world: &mut World, entity: Entity, field: &str, value: Value) {
    write_field(world, entity, field, value);
    bump_field(world, entity, field);
}

pub(crate) fn target_removed(world: &mut World, address: &Address) {
    let trees: Vec<_> = world.query::<(Entity, &InspectorView)>().iter(world).filter_map(|(e,i)| {
        let mut tree=i.0.tree.clone();
        let original=tree.clone();
        if tree.root.as_ref()==Some(address) {tree.root=None;}
        tree.collapsed.retain(|a| a!=address);
        (tree!=original).then_some((e,tree))
    }).collect();
    for (entity,tree) in trees { apply_field(world,entity,"inspector_tree",json!(tree)); }
    let mut query = world.query::<(Entity, &InspectorView)>();
    let affected: Vec<_> = query
        .iter(world)
        .filter(|(_, i)| i.0.target.as_ref() == Some(address))
        .map(|(e, _)| e)
        .collect();
    for e in affected {
        apply_field(world, e, "inspector_target", Value::Null);
    }
}

/// Runs only on the authority, producing normal transform commits at the authoring boundary.
pub(crate) fn follow_users(world: &mut World) {
    let participants = world
        .resource::<crate::presence::PresenceCache>()
        .participants
        .clone();
    let mut query = world.query::<(Entity, &WorldPanel, &LocalTransform)>();
    let changes: Vec<_> = query
        .iter(world)
        .filter_map(|(e, p, t)| {
            let attachment = p.0.attachment.as_ref()?;
            let user = participants.iter().find(|u| u.id == attachment.user_id);
            Some((e, attachment.clone(), user.map(|u| u.pose.clone()), *t))
        })
        .collect();
    let mut changed = false;
    for (entity, attachment, pose, old) in changes {
        let Some(pose) = pose else {
            apply_field(world, entity, "panel_pin", Value::Null);
            changed = true;
            continue;
        };
        let (s, c) = pose.yaw.sin_cos();
        let [x, y, z] = attachment.offset;
        let position = [
            pose.position[0] + c * x + s * z,
            pose.position[1] + y,
            pose.position[2] - s * x + c * z,
        ];
        let (s, c) = (pose.yaw * 0.5).sin_cos();
        let [x, y, z, w] = attachment.rotation;
        let rotation = [c * x + s * z, c * y + s * w, c * z - s * x, c * w - s * y];
        if position != old.translation || rotation != old.rotation {
            if crate::validate_value("position", &json!(position)).is_err() {
                continue;
            }
            let mut transform = world.get_mut::<LocalTransform>(entity).unwrap();
            transform.translation = position;
            transform.rotation = rotation;
            for field in ["position", "rotation"] {
                *world
                    .get_mut::<Revisions>(entity)
                    .unwrap()
                    .0
                    .entry(field.into())
                    .or_default() += 1;
            }
            world.get_mut::<ObjectRoot>(entity).unwrap().revision += 1;
            changed = true;
        }
    }
    if changed {
        world.resource_mut::<WorldRevision>().0 += 1;
        world.resource_mut::<Applied>().0 = Some(AppliedCommand {
            ticket: 0,
            result: Ok(None),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WorldModel, edit_sessions::EditSessions};
    use kokoverse_protocol::{
        Edit, EditDraft, EditSessionRequest, StructuralCommand, StructuralOperation, WorldCommand,
    };

    fn panel(model: &mut WorldModel, target: Address) -> Address {
        model
            .enqueue(WorldCommand::Structure {
                command: StructuralCommand {
                    command_id: uuid::Uuid::new_v4().to_string(),
                    operation: StructuralOperation::CreateInspector {
                        target: Some(target),
                        position: [0.0, 1.5, 2.0],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                    },
                },
            })
            .unwrap();
        let outcome = model.tick().remove(0);
        outcome.result.unwrap();
        outcome.selected.unwrap()
    }
    #[test]
    fn panels_have_independent_state_and_survive_restore() {
        let mut model = WorldModel::new();
        let target = model.snapshot().objects[0].address.clone();
        let a = panel(&mut model, target.clone());
        let b = panel(&mut model, target.clone());
        model
            .enqueue(WorldCommand::Edit {
                edit: Edit {
                    command_id: "scroll".into(),
                    target: crate::replication::REGISTRY
                        .field_address(&a, "panel_scroll")
                        .unwrap(),
                    field_id: "panel_scroll".into(),
                    expected_revision: "0".into(),
                    value: json!([0.0, 0.6]),
                },
            })
            .unwrap();
        model.tick()[0].result.as_ref().unwrap();
        let snapshot = model.snapshot();
        assert_eq!(
            snapshot
                .objects
                .iter()
                .find(|o| o.address == a)
                .unwrap()
                .inspector
                .as_ref()
                .unwrap()
                .scroll,
            [0.0, 0.6]
        );
        assert_eq!(
            snapshot
                .objects
                .iter()
                .find(|o| o.address == b)
                .unwrap()
                .inspector
                .as_ref()
                .unwrap()
                .scroll,
            [0.0; 2]
        );
        let mut restored = WorldModel::new();
        restored.restore(snapshot.clone()).unwrap();
        assert_eq!(restored.snapshot(), snapshot);
        let mut wire = snapshot.clone();
        for object in &mut wire.objects {
            if let Some(field) = object
                .fields
                .iter_mut()
                .find(|f| f.id == "panel_scroll" && f.value == json!([0.0, 0.0]))
            {
                field.value = json!([0, 0]);
            }
        }
        restored.restore(wire).unwrap();
        let revision = snapshot
            .objects
            .iter()
            .find(|o| o.address == target)
            .unwrap()
            .object_revision
            .clone();
        restored
            .enqueue(WorldCommand::Structure {
                command: StructuralCommand {
                    command_id: "delete".into(),
                    operation: StructuralOperation::Delete {
                        target,
                        expected_revision: revision,
                    },
                },
            })
            .unwrap();
        restored.tick()[0].result.as_ref().unwrap();
        assert!(
            restored
                .snapshot()
                .objects
                .iter()
                .all(|o| o.inspector.as_ref().is_none_or(|i| i.target.is_none()))
        );
        let snapshot = restored.snapshot();
        restored.restore(snapshot).unwrap();
    }
    #[test]
    fn lease_spans_panels_but_expires_and_is_not_saved() {
        let mut model = WorldModel::new();
        let target = model.snapshot().objects[0].address.clone();
        let a = panel(&mut model, target.clone());
        let b = panel(&mut model, target.clone());
        let now = std::time::Instant::now();
        let mut leases = EditSessions::default();
        let request = EditSessionRequest {
            command_id: "1".into(),
            client_id: "a".into(),
            context: Some(kokoverse_protocol::EditContext {
                source: crate::replication::REGISTRY
                    .field_address(&a, "inspector_target")
                    .unwrap(),
                field_id: "inspector_target".into(),
            }),
            target: target.clone(),
            field_id: "name".into(),
            action: "begin".into(),
            draft: EditDraft::default(),
        };
        leases
            .apply("alice", request.clone(), &model.snapshot(), now)
            .unwrap();
        let mut end_payload = serde_json::to_value(&request).unwrap();
        end_payload["action"] = json!("end");
        end_payload["draft"] = json!({});
        let end: EditSessionRequest = serde_json::from_value(end_payload).unwrap();
        leases.apply("alice", end, &model.snapshot(), now).unwrap();
        assert!(leases.views().is_empty());
        leases
            .apply("alice", request.clone(), &model.snapshot(), now)
            .unwrap();
        let mut other = request.clone();
        other.context.as_mut().unwrap().source = crate::replication::REGISTRY
            .field_address(&b, "inspector_target")
            .unwrap();
        other.client_id = "b".into();
        assert!(
            leases
                .apply("bob", other.clone(), &model.snapshot(), now)
                .is_err()
        );
        let edit = WorldCommand::Edit {
            edit: Edit {
                command_id: "2".into(),
                target,
                field_id: "name".into(),
                expected_revision: "0".into(),
                value: json!("Bob"),
            },
        };
        assert!(leases.check_command("bob", &edit).is_err());
        assert!(leases.check_command("alice", &edit).is_ok());
        assert!(
            !serde_json::to_string(&model.snapshot())
                .unwrap()
                .contains("alice")
        );
        leases
            .apply(
                "bob",
                other,
                &model.snapshot(),
                now + std::time::Duration::from_secs(7),
            )
            .unwrap();
        leases.forget("bob");
        assert!(leases.views().is_empty());
    }
    #[test]
    fn rejects_forged_panel_data_and_limits_count() {
        let mut model = WorldModel::new();
        let target = model.snapshot().objects[0].address.clone();
        for _ in 0..MAX_PANELS {
            panel(&mut model, target.clone());
        }
        assert!(
            create(
                &model.snapshot(),
                Some(target),
                [0.0; 3],
                [0.0, 0.0, 0.0, 1.0]
            )
            .is_err()
        );
        let mut bad = model.snapshot();
        bad.objects
            .iter_mut()
            .find(|o| o.panel.is_some())
            .unwrap()
            .panel
            .as_mut()
            .unwrap()
            .size = [4000, 4000];
        assert!(model.restore(bad).is_err());
    }
}

//! Resolve typed references against the authoritative component registry.
use crate::replication::REGISTRY;
use kokoverse_protocol::{Snapshot, TypedReference};
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) fn validate(snapshot: &Snapshot, reference: &TypedReference) -> Result<(), String> {
    let address = reference.target();
    let object = snapshot
        .objects
        .iter()
        .find(|o| o.address.instance_id == address.instance_id)
        .ok_or("Referenced Slot no longer exists")?;
    match reference {
        TypedReference::Slot { .. } if address.local_id == 1 => Ok(()),
        TypedReference::Component { .. }
            if REGISTRY
                .components
                .iter()
                .any(|c| c.local_id == address.local_id && (c.present)(object)) =>
        {
            Ok(())
        }
        TypedReference::Field { field_id, .. } => {
            REGISTRY.resolve(snapshot, address, field_id).map(|_| ())
        }
        _ => Err("Reference kind does not match its target".into()),
    }
}

pub(crate) fn values(
    snapshot: &Snapshot,
    script: &kokoverse_protocol::ScriptComponent,
) -> BTreeMap<String, Value> {
    script
        .parameters
        .values()
        .filter_map(|value| {
            let reference = serde_json::from_value::<TypedReference>(value.clone()).ok()?;
            let TypedReference::Field { target, field_id } = &reference else {
                return None;
            };
            let (object, _, _) = REGISTRY.resolve(snapshot, target, field_id).ok()?;
            let value = object
                .fields
                .iter()
                .find(|f| &f.id == field_id)?
                .value
                .clone();
            Some((serde_json::to_string(&reference).unwrap(), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldModel;
    use kokoverse_protocol::*;
    use std::time::Duration;
    const COPY: &str = "return {name='CopyPosition',fields={source={type='field_ref'}},drive=function(ctx) local value=ctx:read(ctx.self.CopyPosition.source) if value then ctx:drive(ctx.self,'position',value) end end}";
    #[test]
    fn typed_references_drive_from_shared_fields_and_reject_wrong_addresses() {
        let mut model = WorldModel::new();
        let target = model.snapshot().objects[0].address.clone();
        model
            .enqueue(WorldCommand::Structure {
                command: StructuralCommand {
                    command_id: "source".into(),
                    operation: StructuralOperation::Create,
                },
            })
            .unwrap();
        let source = model.tick().remove(0).selected.unwrap();
        model
            .enqueue(WorldCommand::Script {
                command: ScriptCommand {
                    command_id: "attach".into(),
                    target: target.clone(),
                    action: ScriptAction::Attach {
                        source: COPY.into(),
                    },
                },
            })
            .unwrap();
        model.tick()[0].result.as_ref().unwrap();
        let current = model
            .snapshot()
            .objects
            .into_iter()
            .find(|o| o.address == target)
            .unwrap()
            .scripts[0]
            .clone();
        let make_update = |reference: TypedReference| WorldCommand::Script {
            command: ScriptCommand {
                command_id: "reference".into(),
                target: target.clone(),
                action: ScriptAction::Update {
                    script_id: current.id.clone(),
                    expected_revision: current.revision.clone(),
                    source: COPY.into(),
                    enabled: true,
                    parameters: BTreeMap::from([(
                        "source".into(),
                        serde_json::to_value(reference).unwrap(),
                    )]),
                },
            },
        };
        model
            .enqueue(make_update(TypedReference::Field {
                target: Address {
                    local_id: 2,
                    ..source.clone()
                },
                field_id: "position".into(),
            }))
            .unwrap();
        assert!(model.tick()[0].result.is_err());
        model
            .enqueue(make_update(TypedReference::Field {
                target: Address {
                    local_id: 3,
                    ..source.clone()
                },
                field_id: "position".into(),
            }))
            .unwrap();
        model.tick()[0].result.as_ref().unwrap();
        let before = model.snapshot();
        model.advance(Duration::ZERO);
        assert_eq!(
            model
                .drive_frame()
                .objects
                .iter()
                .find(|o| o.target == target)
                .unwrap()
                .values["position"],
            serde_json::json!(
                before
                    .objects
                    .iter()
                    .find(|o| o.address == source)
                    .unwrap()
                    .world_position
            )
        );
        assert_eq!(model.snapshot(), before);
        let source_object = before.objects.iter().find(|o| o.address == source).unwrap();
        model
            .enqueue(WorldCommand::Structure {
                command: StructuralCommand {
                    command_id: "delete".into(),
                    operation: StructuralOperation::Delete {
                        target: source,
                        expected_revision: source_object.object_revision.clone(),
                    },
                },
            })
            .unwrap();
        model.tick()[0].result.as_ref().unwrap();
        model.advance(Duration::ZERO);
        assert!(model.drive_frame().objects.is_empty());
    }

    #[test]
    fn bound_text_field_can_be_written_or_driven_without_replicating_drive_values() {
        let mut host = WorldModel::new();
        let owner = host.snapshot().objects[0].address.clone();
        host.enqueue(WorldCommand::Structure { command: StructuralCommand {
            command_id: "text".into(), operation: StructuralOperation::CreateSlot {
                parent: Some(owner.clone()), position: [0.0, 0.0, 0.8], rotation: None,
                position_space: PositionSpace::Parent,
                components: vec![InitialComponent { type_id: "WorldText".into(), values: serde_json::json!({"world_text":"Text","world_text_color":[1.0,1.0,1.0,1.0]}) }],
            },
        }}).unwrap();
        let text_slot = host.tick().remove(0).selected.unwrap();
        let reference = TypedReference::Field { target: Address { local_id: 11, ..text_slot.clone() }, field_id: "world_text".into() };
        assert!(crate::replication::REGISTRY.script_field(&host.snapshot(), &reference, &serde_json::json!("x".repeat(81)), false).is_err());
        let parent_field = TypedReference::Field { target: owner.clone(), field_id: "parent".into() };
        assert!(crate::replication::REGISTRY.script_field(&host.snapshot(), &parent_field, &serde_json::Value::Null, false).is_err());
        let name_field = TypedReference::Field { target: owner.clone(), field_id: "name".into() };
        assert!(crate::replication::REGISTRY.script_field(&host.snapshot(), &name_field, &serde_json::json!("Clock"), true).is_err());
        let source = "return {name='Clock',fields={display={type='field_ref'}},drive=function(ctx) ctx:drive_field(ctx.self.Clock.display, '00:42') end,on_interact=function(ctx) ctx:write_field(ctx.self.Clock.display, 'Started') end}";
        host.enqueue(WorldCommand::Script { command: ScriptCommand {
            command_id: "attach-clock".into(), target: owner.clone(),
            action: ScriptAction::Attach { source: source.into() },
        }}).unwrap();
        host.tick()[0].result.as_ref().unwrap();
        let script = host.snapshot().objects.into_iter().find(|object| object.address == owner).unwrap().scripts[0].clone();
        host.enqueue(WorldCommand::Script { command: ScriptCommand {
            command_id: "bind-clock".into(), target: owner.clone(),
            action: ScriptAction::Update {
                script_id: script.id, expected_revision: script.revision, source: source.into(), enabled: true,
                parameters: BTreeMap::from([("display".into(), serde_json::to_value(&reference).unwrap())]),
            },
        }}).unwrap();
        host.tick()[0].result.as_ref().unwrap();
        let before = host.snapshot();
        host.advance(Duration::ZERO);
        let driven = host.drive_frame().objects.into_iter().find(|object| object.target == text_slot).unwrap();
        assert_eq!(driven.values["world_text"], serde_json::json!("00:42"));
        assert_eq!(host.snapshot(), before);
        host.set_script_authority(true);
        host.enqueue(WorldCommand::Script { command: ScriptCommand {
            command_id: "click-clock".into(), target: owner,
            action: ScriptAction::Interact,
        }}).unwrap();
        host.advance(Duration::ZERO);
        let text = host.snapshot().objects.into_iter().find(|object| object.address == text_slot).unwrap();
        let text_revision = text.fields.iter().find(|field| field.id == "world_text").unwrap().revision.clone();
        assert_eq!(text.world_text.unwrap().text, "Started");
        assert_eq!(host.drive_frame().objects.iter().find(|object| object.target == text_slot).unwrap().values["world_text"], serde_json::json!("00:42"));
        host.enqueue(WorldCommand::Script { command: ScriptCommand {
            command_id: "click-clock-again".into(), target: host.snapshot().objects.iter().find(|object| object.scripts.iter().any(|script| script.schema.name == "Clock")).unwrap().address.clone(),
            action: ScriptAction::Interact,
        }}).unwrap();
        host.advance(Duration::ZERO);
        let after = host.snapshot().objects.into_iter().find(|object| object.address == text_slot).unwrap();
        assert_eq!(after.fields.iter().find(|field| field.id == "world_text").unwrap().revision, text_revision);
    }
}

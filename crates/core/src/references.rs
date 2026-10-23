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
}

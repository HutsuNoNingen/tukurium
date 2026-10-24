//! Local derived fields. These components never enter the shared snapshot or commit log.
use crate::resources::*;
use bevy_ecs::prelude::*;
use kokoverse_protocol::{DriveFrame, DrivenObject, ScriptStatus, TypedReference};
use kokoverse_scripting::{Event, Program};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub(crate) struct DrivenFields(pub BTreeMap<String, Value>);

struct Cached {
    source: String,
    revision: String,
    program: Option<Program>,
    error: Option<String>,
    values: BTreeMap<String, Value>,
    field_values: Vec<(TypedReference, Value)>,
}

#[derive(Resource)]
pub(crate) struct DriveRuntime {
    cache: HashMap<String, Cached>,
    cursor: usize,
    pub ready: bool,
    pub frame: DriveFrame,
}
impl Default for DriveRuntime {
    fn default() -> Self {
        Self {
            cache: HashMap::new(),
            cursor: 0,
            ready: true,
            frame: DriveFrame::default(),
        }
    }
}

pub(crate) fn evaluate(world: &mut World) {
    let mut runtime = world.remove_resource::<DriveRuntime>().unwrap();
    let now = world.resource::<MediaClock>().0;
    let snapshot = world.resource::<SnapshotCache>().0.clone();
    let mut frame = DriveFrame {
        world_time: now,
        ..Default::default()
    };
    let mut alive = std::collections::HashSet::new();
    let tasks: Vec<_> = snapshot
        .objects
        .iter()
        .flat_map(|object| {
            object
                .scripts
                .iter()
                .filter(|s| s.schema.has_drive)
                .map(move |script| (object, script))
        })
        .collect();
    let started = std::time::Instant::now();
    // Rotate evaluation order so a costly program cannot starve later programs.
    // Resolution order below stays stable, independently of this work budget.
    for offset in 0..tasks.len() {
        let (object, script) = tasks[(runtime.cursor + offset) % tasks.len()];
        alive.insert(script.id.clone());
        let cached = runtime
            .cache
            .entry(script.id.clone())
            .or_insert_with(|| Cached {
                source: String::new(),
                revision: String::new(),
                program: None,
                error: None,
                values: BTreeMap::new(),
                field_values: Vec::new(),
            });
        if cached.source != script.source || cached.revision != script.revision {
            cached.values.clear();
            cached.field_values.clear();
            if !runtime.ready || started.elapsed().as_millis() >= 10 {
                continue;
            }
            cached.source = script.source.clone();
            cached.revision = script.revision.clone();
            let compiled = Program::compile(&script.source).and_then(|p| {
                if p.schema == script.schema {
                    Ok(p)
                } else {
                    Err("Script schema does not match source".into())
                }
            });
            match compiled {
                Ok(program) => {
                    cached.program = Some(program);
                    cached.error = None;
                }
                Err(error) => {
                    cached.program = None;
                    cached.error = Some(error);
                }
            }
        }
        if runtime.ready && cached.error.is_none() && started.elapsed().as_millis() < 10 {
            // A paused Drive evaluates at its pause anchor, even if it reads WorldTime.
            let time = if script.enabled {
                now
            } else {
                script.clock.anchor
            };
            let mut local_object = object.clone();
            let local = crate::hierarchy::local_of(object);
            local_object.world_position = local.translation;
            local_object.world_rotation = local.rotation;
            local_object.world_scale = local.scale;
            match cached.program.as_ref().unwrap().run_at_with_fields(
                &local_object,
                script,
                Event::Drive,
                0.0,
                time,
                &crate::references::values(&snapshot, script),
            ) {
                Ok(output) => {
                    let validation = output.field_drives.iter().try_for_each(|(reference, value)| {
                        crate::replication::REGISTRY.script_field(&snapshot, reference, value, true).map(|_| ())
                    });
                    match validation {
                        Ok(()) => { cached.values = output.drives; cached.field_values = output.field_drives; }
                        Err(error) => { cached.values.clear(); cached.field_values.clear(); cached.error = Some(error); }
                    }
                }
                Err(error) => {
                    cached.values.clear();
                    cached.field_values.clear();
                    cached.error = Some(error.chars().take(1024).collect());
                }
            }
        }
    }
    runtime.cursor = runtime.cursor.wrapping_add(1);
    // Resolve all script outputs in stable snapshot order, including fields on other Slots.
    let mut resolved: HashMap<String, (BTreeMap<String, Value>, BTreeMap<String, String>)> = HashMap::new();
    for object in &snapshot.objects {
        for script in object.scripts.iter().filter(|s| s.schema.has_drive) {
            let Some(cached) = runtime.cache.get(&script.id) else {
                continue;
            };
            let mut error = cached.error.clone();
            if runtime.ready
                && cached.source == script.source
                && cached.revision == script.revision
                && error.is_none()
            {
                let mut proposed: Vec<(String, String, Value)> = cached.values.iter()
                    .map(|(field, value)| (object.address.instance_id.clone(), field.clone(), value.clone())).collect();
                for (reference, value) in &cached.field_values {
                    if let TypedReference::Field { target, field_id } = reference {
                        proposed.push((target.instance_id.clone(), field_id.clone(), value.clone()));
                    }
                }
                if let Some((_, field, _)) = proposed.iter().find(|(id, field, _)| {
                    resolved.get(id).is_some_and(|(values, _)| values.contains_key(field))
                }) {
                    error = Some(format!("Field {field} is already driven by another script"));
                } else {
                    for (id, field, value) in proposed {
                        let (values, sources) = resolved.entry(id).or_default();
                        values.insert(field.clone(), value);
                        sources.insert(field, script.id.clone());
                    }
                }
            }
            frame.statuses.push(ScriptStatus {
                target: object.address.clone(),
                script_id: script.id.clone(),
                schema: Some(script.schema.clone()),
                error,
            });
        }
    }
    // No driven result enters the shared snapshot or commit log.
    for object in &snapshot.objects {
        let (values, sources) = resolved.remove(&object.address.instance_id).unwrap_or_default();
        let entity = world.resource::<EntityIndex>().0[&object.address];
        let fields = DrivenFields(values.clone());
        if world.get::<DrivenFields>(entity) != Some(&fields) {
            world.entity_mut(entity).insert(fields);
        }
        if !values.is_empty() {
            frame.objects.push(DrivenObject {
                target: object.address.clone(),
                values,
                sources,
            });
        }
    }
    runtime.cache.retain(|id, _| alive.contains(id));
    frame.transforms = crate::hierarchy::evaluate(world);
    runtime.frame = frame;
    world.insert_resource(runtime);
}

#[cfg(test)]
mod tests {
    use crate::WorldModel;
    use kokoverse_protocol::{ScriptAction, ScriptCommand, WorldCommand};
    use std::time::Duration;

    fn attach(model: &mut WorldModel, source: &str) {
        model
            .enqueue(WorldCommand::Script {
                command: ScriptCommand {
                    command_id: "attach".into(),
                    target: model.snapshot().objects[0].address.clone(),
                    action: ScriptAction::Attach {
                        source: source.into(),
                    },
                },
            })
            .unwrap();
        model.tick()[0].result.as_ref().unwrap();
    }

    #[test]
    fn different_frame_rates_and_late_join_derive_same_rotation_without_commits() {
        let mut a = WorldModel::new();
        a.set_media_time(1000.0);
        attach(&mut a, crate::scripting::ROTATOR);
        let base = a.snapshot();
        let mut b = WorldModel::new();
        b.restore(base.clone()).unwrap();
        for i in 1..=30 {
            a.set_media_time(1000.0 + i as f64 / 30.0);
            assert!(a.advance(Duration::from_secs_f64(1.0 / 30.0)).is_empty());
        }
        for i in 1..=144 {
            b.set_media_time(1000.0 + i as f64 / 144.0);
            assert!(b.advance(Duration::from_secs_f64(1.0 / 144.0)).is_empty());
        }
        assert_eq!(a.drive_frame(), b.drive_frame());
        assert_eq!(a.snapshot(), base);
        assert_eq!(b.snapshot(), base);
        let mut late = WorldModel::new();
        late.restore(base).unwrap();
        late.set_media_time(1001.0);
        late.advance(Duration::ZERO);
        assert_eq!(a.drive_frame(), late.drive_frame());
        assert_ne!(
            a.drive_frame().objects[0].values["rotation"],
            serde_json::json!([0, 0, 0, 1])
        );
    }

    #[test]
    fn local_drives_reject_shared_writes_and_release_old_values() {
        let mut model = WorldModel::new();
        attach(
            &mut model,
            "return {name='Bad',drive=function(ctx) ctx:drive(ctx.self,'color',{1,0,0,1}); ctx:write(ctx.self,'position',{1,1,1}) end}",
        );
        let base = model.snapshot();
        model.advance(Duration::ZERO);
        assert!(model.drive_frame().objects.is_empty());
        assert!(
            model.drive_frame().statuses[0]
                .error
                .as_ref()
                .unwrap()
                .contains("ctx:write")
        );
        assert_eq!(model.snapshot(), base);
        crate::scripting::validate_world_scripts(base.objects.iter().map(|o| o.scripts.as_slice()))
            .unwrap();
    }
    #[test]
    fn pause_resume_and_remove_keep_phase_then_release_derived_fields() {
        let mut model = WorldModel::new();
        model.set_media_time(100.0);
        attach(&mut model, crate::scripting::ROTATOR);
        let update = |model: &mut WorldModel, enabled| {
            let object = &model.snapshot().objects[0];
            let script = &object.scripts[0];
            model
                .enqueue(WorldCommand::Script {
                    command: ScriptCommand {
                        command_id: "control".into(),
                        target: object.address.clone(),
                        action: ScriptAction::Update {
                            script_id: script.id.clone(),
                            expected_revision: script.revision.clone(),
                            source: script.source.clone(),
                            enabled,
                            parameters: script.parameters.clone(),
                        },
                    },
                })
                .unwrap();
            model.tick()[0].result.as_ref().unwrap();
        };
        model.set_media_time(101.0);
        model.advance(Duration::ZERO);
        let rotation = model.drive_frame().objects[0].values["rotation"].clone();
        update(&mut model, false);
        model.set_media_time(110.0);
        model.advance(Duration::ZERO);
        assert_eq!(model.drive_frame().objects[0].values["rotation"], rotation);
        assert_eq!(
            model.drive_frame().objects[0].sources["rotation"],
            model.snapshot().objects[0].scripts[0].id
        );
        update(&mut model, true);
        model.advance(Duration::ZERO);
        assert_eq!(model.drive_frame().objects[0].values["rotation"], rotation);
        model.set_media_time(111.0);
        model.advance(Duration::ZERO);
        assert_ne!(model.drive_frame().objects[0].values["rotation"], rotation);
        let object = &model.snapshot().objects[0];
        let script = &object.scripts[0];
        model
            .enqueue(WorldCommand::Script {
                command: ScriptCommand {
                    command_id: "remove".into(),
                    target: object.address.clone(),
                    action: ScriptAction::Remove {
                        script_id: script.id.clone(),
                        expected_revision: script.revision.clone(),
                    },
                },
            })
            .unwrap();
        model.advance(Duration::from_secs_f64(1.0 / 60.0));
        assert!(model.drive_frame().objects.is_empty());
        let collider = &model.world.resource::<crate::collision::CollisionWorld>().0[0];
        assert_eq!(collider.rotation, glam::DQuat::IDENTITY);
    }

    #[test]
    fn collision_uses_the_same_local_drive_rotation_as_renderer() {
        let mut model = WorldModel::new();
        model.set_media_time(100.0);
        let target = model.snapshot().objects[0].address.clone();
        model
            .enqueue(WorldCommand::Transform {
                command: kokoverse_protocol::TransformCommand {
                    command_id: "scale".into(),
                    target,
                    expected_position_revision: "0".into(),
                    expected_rotation_revision: "0".into(),
                    expected_scale_revision: "0".into(),
                    space: kokoverse_protocol::PositionSpace::World,
                    position: [0.0, 0.75, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [3.0, 1.0, 1.0],
                },
            })
            .unwrap();
        model.tick()[0].result.as_ref().unwrap();
        attach(&mut model, crate::scripting::ROTATOR);
        let base = model.snapshot();
        model.set_media_time(102.0);
        model.advance(Duration::from_secs_f64(1.0 / 60.0));
        let frame = model.drive_frame();
        let expected: [f64; 4] =
            serde_json::from_value(frame.objects[0].values["rotation"].clone()).unwrap();
        let collider = &model.world.resource::<crate::collision::CollisionWorld>().0[0];
        assert!(
            collider
                .rotation
                .abs_diff_eq(glam::DQuat::from_array(expected), 1e-12)
        );
        let dx = collider
            .raycast([5.0, 0.75, 0.0], [-1.0, 0.0, 0.0], 10.0)
            .unwrap()
            .0;
        let dz = collider
            .raycast([0.0, 0.75, 5.0], [0.0, 0.0, -1.0], 10.0)
            .unwrap()
            .0;
        assert!(
            dx > dz,
            "90 degree rotation must also rotate the long collision axis"
        );
        assert_eq!(model.snapshot(), base);
    }

    #[test]
    fn conflicting_drives_are_reported_instead_of_overwriting_each_other() {
        let mut model = WorldModel::new();
        attach(
            &mut model,
            "return {name='Red',drive=function(ctx) ctx:drive(ctx.self,'color',{1,0,0,1}) end}",
        );
        attach(
            &mut model,
            "return {name='Blue',drive=function(ctx) ctx:drive(ctx.self,'color',{0,0,1,1}) end}",
        );
        model.advance(Duration::ZERO);
        assert_eq!(
            model.drive_frame().objects[0].values["color"],
            serde_json::json!([1.0, 0.0, 0.0, 1.0])
        );
        assert!(
            model.drive_frame().statuses[1]
                .error
                .as_ref()
                .unwrap()
                .contains("already driven")
        );
        assert_eq!(
            model.drive_frame().objects[0].sources["color"],
            model.snapshot().objects[0].scripts[0].id
        );
    }
}

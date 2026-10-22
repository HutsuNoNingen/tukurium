//! Host-authored script frames share the normal ECS mutation/commit pipeline.
use crate::{resources::*, validate_value};
use bevy_ecs::prelude::*;
use kokoverse_protocol::{Address, ScriptAction, ScriptComponent, ScriptStatus};
use kokoverse_scripting::{Event, Program};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;
use uuid::Uuid;

pub use kokoverse_scripting::{CLICK_COLOR, ROTATOR};

struct Cached {
    revision: String,
    source: String,
    program: Option<Program>,
    error: Option<String>,
}

#[derive(Resource, Default)]
pub(crate) struct ScriptRuntime {
    cache: HashMap<String, Cached>,
    pub(crate) events: Vec<(Address, String)>,
    pub(crate) dt: f64,
    pub(crate) statuses: Vec<ScriptStatus>,
    cursor: usize,
}

pub(crate) struct FramePatch {
    pub entity: Entity,
    pub position: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
    pub color: [f64; 4],
    pub scripts: Vec<ScriptComponent>,
}

pub(crate) fn validate_world_scripts<'a>(
    objects: impl Iterator<Item = &'a [ScriptComponent]>,
) -> Result<(), String> {
    let mut count = 0;
    let mut bytes = 0;
    let mut ids = HashSet::new();
    for scripts in objects {
        if scripts.len() > kokoverse_scripting::MAX_SCRIPTS_PER_OBJECT {
            return Err("At most 4 script components per object".into());
        }
        let mut names = HashSet::new();
        for script in scripts {
            if Uuid::parse_str(&script.id).is_err()
                || !ids.insert(&script.id)
                || script.revision.parse::<u64>().is_err()
                || !names.insert(&script.schema.name)
            {
                return Err("Invalid or duplicate script component identity".into());
            }
            kokoverse_scripting::validate_component(script)?;
        }
        count += scripts.len();
        bytes += serde_json::to_vec(scripts)
            .map_err(|e| e.to_string())?
            .len();
    }
    if count > kokoverse_scripting::MAX_SCRIPTS_WORLD || bytes > 128 * 1024 {
        return Err("World script budget exceeded (32 components / 128 KiB)".into());
    }
    Ok(())
}

pub(crate) fn prepare_edit(
    current: &[ScriptComponent],
    action: ScriptAction,
) -> Result<Vec<ScriptComponent>, String> {
    let mut scripts = current.to_vec();
    match action {
        ScriptAction::Attach { source } => {
            if scripts.len() >= kokoverse_scripting::MAX_SCRIPTS_PER_OBJECT {
                return Err("At most 4 script components per object".into());
            }
            let program = Program::compile(&source)?;
            scripts.push(ScriptComponent {
                id: Uuid::new_v4().to_string(),
                revision: "0".into(),
                enabled: true,
                parameters: kokoverse_scripting::parameters(&program.schema, &BTreeMap::new())?,
                schema: program.schema,
                source,
                state: BTreeMap::new(),
            });
        }
        ScriptAction::Update {
            script_id,
            expected_revision,
            source,
            enabled,
            parameters,
        } => {
            let script = scripts
                .iter_mut()
                .find(|s| s.id == script_id)
                .ok_or("Script no longer exists")?;
            if script.revision != expected_revision {
                return Err("Script changed elsewhere. Review it and retry.".into());
            }
            let program = Program::compile(&source)?;
            let values = kokoverse_scripting::parameters(&program.schema, &parameters)?;
            script.revision = script
                .revision
                .parse::<u64>()
                .unwrap()
                .checked_add(1)
                .ok_or("Script revision exhausted")?
                .to_string();
            script.source = source;
            script.schema = program.schema;
            script.enabled = enabled;
            script.parameters = values;
        }
        ScriptAction::Remove {
            script_id,
            expected_revision,
        } => {
            let script = scripts
                .iter()
                .find(|s| s.id == script_id)
                .ok_or("Script no longer exists")?;
            if script.revision != expected_revision {
                return Err("Script changed elsewhere. Review it and retry.".into());
            }
            scripts.retain(|s| s.id != script_id);
        }
        ScriptAction::Interact => unreachable!(),
    }
    validate_world_scripts(std::iter::once(scripts.as_slice()))?;
    Ok(scripts)
}

pub(crate) fn prepare_frame(world: &mut World) {
    let snapshot = world.resource::<SnapshotCache>().0.clone();
    let mut runtime = world.remove_resource::<ScriptRuntime>().unwrap();
    let events = std::mem::take(&mut runtime.events);
    let mut delivered = HashSet::new();
    let dt = runtime.dt;
    let mut patches = Vec::new();
    let alive: HashSet<_> = snapshot
        .objects
        .iter()
        .flat_map(|o| o.scripts.iter().map(|s| s.id.clone()))
        .collect();
    runtime.cache.retain(|id, _| alive.contains(id));
    let started = Instant::now();
    let mut stored_bytes: usize = snapshot
        .objects
        .iter()
        .map(|o| serde_json::to_vec(&o.scripts).unwrap().len())
        .sum();
    let mut objects: Vec<_> = snapshot
        .objects
        .iter()
        .filter(|o| !o.scripts.is_empty())
        .cloned()
        .collect();
    if !objects.is_empty() {
        let count = objects.len();
        objects.rotate_left(runtime.cursor % count);
        runtime.cursor = (runtime.cursor + 1) % count;
    }
    for mut object in objects {
        let original = object.clone();
        for index in 0..object.scripts.len() {
            let script = object.scripts[index].clone();
            let stale = runtime
                .cache
                .get(&script.id)
                .is_none_or(|c| c.source != script.source || c.revision != script.revision);
            if stale {
                let (program, error) = match Program::compile(&script.source) {
                    Ok(p) if p.schema == script.schema => (Some(p), None),
                    Ok(_) => (None, Some("Script schema does not match its source".into())),
                    Err(e) => (None, Some(e.chars().take(1024).collect())),
                };
                runtime.cache.insert(
                    script.id.clone(),
                    Cached {
                        revision: script.revision.clone(),
                        source: script.source.clone(),
                        program,
                        error,
                    },
                );
            }
            let cached = runtime.cache.get_mut(&script.id).unwrap();
            if !script.enabled || cached.error.is_some() {
                delivered.extend(
                    events
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, id))| id == &script.id)
                        .map(|(index, _)| index),
                );
                continue;
            }
            if started.elapsed().as_millis() >= 10 {
                continue;
            }
            let program = cached.program.as_ref().unwrap();
            let mut callbacks = Vec::new();
            if dt > 0.0 {
                callbacks.push((Event::Update, None));
            }
            // Each accepted interaction runs exactly once, even if several arrive in one frame.
            callbacks.extend(
                events
                    .iter()
                    .enumerate()
                    .filter(|(_, (address, id))| *address == object.address && id == &script.id)
                    .map(|(index, _)| (Event::Interact, Some(index))),
            );
            for (event, event_index) in callbacks {
                if started.elapsed().as_millis() >= 10 {
                    break;
                }
                if let Some(index) = event_index {
                    delivered.insert(index);
                }
                match program.run(&object, &object.scripts[index], event, dt) {
                    Ok(output) => {
                        let values = [
                            ("position", serde_json::json!(output.position)),
                            ("rotation", serde_json::json!(output.rotation)),
                            ("scale", serde_json::json!(output.scale)),
                            ("color", serde_json::json!(output.color)),
                        ];
                        if let Some(error) = values
                            .iter()
                            .find_map(|(key, value)| validate_value(key, value).err())
                        {
                            cached.error = Some(error);
                            break;
                        }
                        let old = serde_json::to_vec(&object.scripts[index]).unwrap().len();
                        let old_state = object.scripts[index].state.clone();
                        object.scripts[index].state = output.state;
                        let proposed = stored_bytes - old
                            + serde_json::to_vec(&object.scripts[index]).unwrap().len();
                        if proposed > 128 * 1024 {
                            object.scripts[index].state = old_state;
                            cached.error = Some("World script storage budget exceeded".into());
                            break;
                        }
                        stored_bytes = proposed;
                        object.world_position = output.position;
                        object.world_rotation = output.rotation;
                        object.world_scale = output.scale;
                        for field in &mut object.fields {
                            if let Some((_, value)) = values.iter().find(|(id, _)| *id == field.id)
                            {
                                field.value = value.clone();
                            }
                        }
                    }
                    Err(error) => {
                        cached.error = Some(error.chars().take(1024).collect());
                        break;
                    }
                }
            }
        }
        if object != original {
            patches.push(FramePatch {
                entity: world.resource::<EntityIndex>().0[&object.address],
                position: object.world_position,
                rotation: object.world_rotation,
                scale: object.world_scale,
                color: serde_json::from_value(
                    object
                        .fields
                        .iter()
                        .find(|f| f.id == "color")
                        .unwrap()
                        .value
                        .clone(),
                )
                .unwrap(),
                scripts: object.scripts,
            });
        }
    }
    runtime.statuses = snapshot
        .objects
        .iter()
        .flat_map(|object| {
            object.scripts.iter().map(|script| ScriptStatus {
                target: object.address.clone(),
                script_id: script.id.clone(),
                schema: Some(script.schema.clone()),
                error: runtime.cache.get(&script.id).and_then(|c| c.error.clone()),
            })
        })
        .collect();
    runtime.events = events
        .into_iter()
        .enumerate()
        .filter(|(index, (_, id))| !delivered.contains(index) && alive.contains(id))
        .map(|(_, event)| event)
        .collect();
    world.insert_resource(runtime);
    if !patches.is_empty() && world.resource::<WorldRevision>().0 < u64::MAX {
        world.resource_mut::<Prepared>().0 = Some(PreparedCommand {
            ticket: 0,
            mutation: Ok(Mutation::ScriptFrame(patches)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldModel;
    use kokoverse_protocol::{ScriptCommand, StructuralCommand, StructuralOperation, WorldCommand};
    use std::time::Duration;

    fn command(model: &mut WorldModel, action: ScriptAction) -> Result<(), String> {
        let target = model.snapshot().objects[0].address.clone();
        model.enqueue(WorldCommand::Script {
            command: ScriptCommand {
                command_id: Uuid::new_v4().to_string(),
                target,
                action,
            },
        })?;
        model.tick().remove(0).result.map(|_| ())
    }

    #[test]
    fn host_runs_luau_and_guest_only_replicates_results() {
        let mut host = WorldModel::new();
        command(
            &mut host,
            ScriptAction::Attach {
                source: ROTATOR.into(),
            },
        )
        .unwrap();
        let initial = host.snapshot();
        let mut guest = WorldModel::new();
        guest.restore(initial.clone()).unwrap();
        assert!(guest.advance(Duration::from_secs(1)).is_empty());
        assert_eq!(guest.snapshot(), initial);
        host.set_script_authority(true);
        let outcomes = host.advance(Duration::from_millis(100));
        assert_eq!(outcomes.len(), 1);
        guest
            .apply_commit(outcomes.into_iter().next().unwrap().result.unwrap())
            .unwrap();
        assert_eq!(guest.snapshot(), host.snapshot());
        assert_ne!(
            host.snapshot().objects[0].world_rotation,
            initial.objects[0].world_rotation
        );
        assert_eq!(host.snapshot().objects[0].scripts[0].revision, "0");
    }

    #[test]
    fn config_revision_stays_editable_while_system_moves_object_and_duplicates_are_independent() {
        let mut host = WorldModel::new();
        command(
            &mut host,
            ScriptAction::Attach {
                source: ROTATOR.into(),
            },
        )
        .unwrap();
        host.set_script_authority(true);
        let script = host.snapshot().objects[0].scripts[0].clone();
        host.advance(Duration::from_millis(100));
        command(
            &mut host,
            ScriptAction::Update {
                script_id: script.id.clone(),
                expected_revision: script.revision.clone(),
                source: script.source.clone(),
                enabled: true,
                parameters: BTreeMap::from([("speed".into(), serde_json::json!(90))]),
            },
        )
        .unwrap();
        assert!(
            command(
                &mut host,
                ScriptAction::Remove {
                    script_id: script.id.clone(),
                    expected_revision: script.revision
                }
            )
            .is_err()
        );
        let original = host.snapshot().objects[0].clone();
        host.enqueue(WorldCommand::Structure {
            command: StructuralCommand {
                command_id: "copy".into(),
                operation: StructuralOperation::Duplicate {
                    target: original.address.clone(),
                    expected_revision: original.object_revision,
                },
            },
        })
        .unwrap();
        assert!(host.tick()[0].result.is_ok());
        let snapshot = host.snapshot();
        assert_ne!(
            snapshot.objects[0].scripts[0].id,
            snapshot.objects[1].scripts[0].id
        );
        assert_eq!(
            snapshot.objects[0].scripts[0].parameters,
            snapshot.objects[1].scripts[0].parameters
        );
    }

    #[test]
    fn interaction_state_survives_restore_and_bad_callbacks_are_atomic() {
        let mut host = WorldModel::new();
        host.set_script_authority(true);
        command(
            &mut host,
            ScriptAction::Attach {
                source: CLICK_COLOR.into(),
            },
        )
        .unwrap();
        command(&mut host, ScriptAction::Interact).unwrap();
        host.advance(Duration::ZERO);
        assert_eq!(
            host.snapshot().objects[0].scripts[0].state["active"],
            serde_json::json!(true)
        );
        let mut restored = WorldModel::new();
        restored.restore(host.snapshot()).unwrap();
        restored.set_script_authority(true);
        command(&mut restored, ScriptAction::Interact).unwrap();
        restored.advance(Duration::ZERO);
        assert_eq!(
            restored.snapshot().objects[0].scripts[0].state["active"],
            serde_json::json!(false)
        );
        let script = restored.snapshot().objects[0].scripts[0].clone();
        command(&mut restored,ScriptAction::Update{script_id:script.id,expected_revision:script.revision,source:"return { name='Failure', update=function(ctx) ctx:set_color(ctx.self,{1,0,0,1}); error('expected failure') end }".into(),enabled:true,parameters:BTreeMap::new()}).unwrap();
        let before = restored.snapshot();
        restored.advance(Duration::from_millis(100));
        assert_eq!(restored.snapshot(), before);
        assert!(
            restored.script_statuses()[0]
                .error
                .as_ref()
                .unwrap()
                .contains("expected failure")
        );
    }

    #[test]
    fn runaway_script_stops_and_schema_corruption_is_rejected() {
        let mut model = WorldModel::new();
        model.set_script_authority(true);
        command(
            &mut model,
            ScriptAction::Attach {
                source: "return {name='Loop', update=function() while true do end end}".into(),
            },
        )
        .unwrap();
        let before = model.snapshot();
        model.advance(Duration::from_millis(100));
        assert_eq!(model.snapshot(), before);
        assert!(
            model.script_statuses()[0]
                .error
                .as_ref()
                .unwrap()
                .contains("budget")
        );
        model.advance(Duration::from_millis(100));
        let mut invalid = model.snapshot();
        invalid.objects[0].scripts[0]
            .state
            .insert("invalid".into(), serde_json::json!({"nested":true}));
        assert!(model.restore(invalid).is_err());
        assert_eq!(model.snapshot(), before);
    }

    #[test]
    fn queued_interactions_are_bounded_and_delivered_once_across_frames() {
        let mut model=WorldModel::new();model.set_script_authority(true);
        command(&mut model,ScriptAction::Attach{source:"return {name='Counter',on_interact=function(ctx) ctx:set_state('count',ctx:get_state('count',0)+1) end}".into()}).unwrap();
        for _ in 0..128 {command(&mut model,ScriptAction::Interact).unwrap();}
        assert!(command(&mut model,ScriptAction::Interact).is_err());
        for _ in 0..128 {model.advance(Duration::ZERO);}
        assert_eq!(model.snapshot().objects[0].scripts[0].state["count"],serde_json::json!(128));
        assert!(model.world.resource::<ScriptRuntime>().events.is_empty());
    }
}

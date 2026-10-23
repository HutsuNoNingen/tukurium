//! The ordered simulation pipeline. Network and renderer code never write components.
use crate::{MAX_OBJECTS, components::*, new_object, resources::*, validate_value};
use bevy_ecs::prelude::*;
use kokoverse_protocol::{Address, StructuralOperation, WorldCommand, WorldCommit};
use std::collections::HashSet;

fn image_size(visual: &kokoverse_protocol::Visual) -> usize {
    match visual {
        kokoverse_protocol::Visual::Image { data_url, .. } => data_url.len(),
        _ => 0,
    }
}

type SlotData<'a> = (&'a Slot, &'a LocalTransform, &'a Revisions, &'a ObjectRoot);
type ValidationState<'a> = (
    Res<'a, EntityIndex>,
    Res<'a, WorldRevision>,
    Res<'a, MediaClock>,
    Res<'a, SnapshotCache>,
    Res<'a, crate::scripting::ScriptRuntime>,
);
type SnapshotRoot<'a> = (
    &'a Identity,
    &'a Slot,
    &'a LocalTransform,
    &'a GlobalTransform,
    &'a Revisions,
    &'a ObjectRoot,
    Option<&'a Collider>,
    Option<&'a MediaPlayback>,
    &'a ScriptComponents,
    Option<&'a WorldPanel>,
    Option<&'a InspectorView>,
    Option<&'a CreationView>,
);

/// Resolve stable addresses and validate against state committed by the preceding command.
pub(crate) fn validate_commands(
    mut queue: ResMut<InputQueue>,
    mut prepared: ResMut<Prepared>,
    state: ValidationState,
    slots: Query<SlotData>,
    meshes: Query<&MeshRenderer>,
    colliders: Query<&Collider>,
    optional: (Query<&MediaPlayback>, Query<&ScriptComponents>),
) {
    let (index, revision, clock, snapshot, runtime) = state;
    let (playback, scripts) = optional;
    prepared.0 = queue.0.pop_front().map(|(ticket, command)| {
        let validate = || -> Result<Mutation, String> {
            if revision.0 == u64::MAX { return Err("World revision exhausted".into()); }
            match command {
                WorldCommand::Script { command } => {
                    let entity=*index.0.get(&command.target).ok_or("Target no longer exists")?;
                    slots.get(entity).map_err(|_|"Expected an ObjectRoot Slot")?;
                    let current=&scripts.get(entity).map_err(|_|"Missing script container")?.0;
                    if matches!(command.action,kokoverse_protocol::ScriptAction::Interact) {
                        let handlers=current.iter().filter(|s|s.enabled && s.schema.has_interact).count();
                        if colliders.get(entity).is_err() || handlers==0 {return Err("This object has no active interaction".into());}
                        if runtime.events.len()+handlers>128 {return Err("Too many pending script interactions".into());}
                        return Ok(Mutation::ScriptInteract{entity});
                    }
                    let value=crate::scripting::prepare_edit(current,command.action,clock.0)?;
                    for script in &value {
                        for field in script.schema.fields.iter().filter(|f|f.value_type=="field_ref") {
                            let reference=&script.parameters[&field.id];
                            let unchanged=current.iter().find(|s|s.id==script.id).and_then(|s|s.parameters.get(&field.id))==Some(reference);
                            if !reference.is_null() && !unchanged {
                                crate::references::validate(&snapshot.0,&serde_json::from_value::<kokoverse_protocol::TypedReference>(reference.clone()).map_err(|_|"Invalid field reference")?)?;
                            }
                        }
                    }
                    crate::scripting::validate_world_scripts(snapshot.0.objects.iter().map(|o|if o.address==command.target {value.as_slice()} else {o.scripts.as_slice()}))?;
                    Ok(Mutation::SetScripts{entity,value})
                }
                WorldCommand::Edit { edit } => {
                    let (object, component, spec) = crate::replication::REGISTRY.resolve(&snapshot.0, &edit.target, &edit.field_id)?;
                    if spec.schema.access != kokoverse_protocol::FieldAccess::Edit {
                        return Err("Use the component's commands to change this field".into());
                    }
                    (spec.validate)(&edit.value)?;
                    if edit.field_id == "panel_pin" && !edit.value.is_null() && object.parent.is_some() { return Err("Move the panel to World before pinning it".into()); }
                    if edit.field_id == "inspector_tree" {
                        let tree: kokoverse_protocol::InspectorTree = serde_json::from_value(edit.value.clone()).unwrap();
                        if tree.root.as_ref().is_some_and(|root| !snapshot.0.objects.iter().any(|o| &o.address == root)) { return Err("Hierarchy root no longer exists".into()); }
                    }
                    let entity = index.0[&object.address];
                    let (_, _, revisions, _) = slots.get(entity).map_err(|_| "Expected an ObjectRoot Slot")?;
                    let expected = edit.expected_revision.parse::<u64>().map_err(|_| "Invalid revision")?;
                    if expected != revisions.0.get(&edit.field_id).copied().unwrap_or(0) {
                        return Err("This field was changed by another editor. Review its current value and retry.".into());
                    }
                    if edit.field_id == "inspector_target" && !edit.value.is_null() {
                        let address: Address = serde_json::from_value(edit.value.clone()).unwrap();
                        if !snapshot.0.objects.iter().any(|o|o.address==address) {return Err("Inspector target no longer exists".into());}
                    }
                    Ok(Mutation::SetField {entity, component_id: component.local_id, field:edit.field_id, value:edit.value})
                }

                WorldCommand::Transform { command } => {
                    let entity = *index.0.get(&command.target).ok_or("Target no longer exists")?;
                    let (_,_,revisions,_) = slots.get(entity).map_err(|_|"Expected an ObjectRoot Slot")?;
                    for (id,value,revision) in [
                        ("position",serde_json::json!(command.position),command.expected_position_revision),
                        ("rotation",serde_json::json!(command.rotation),command.expected_rotation_revision),
                        ("scale",serde_json::json!(command.scale),command.expected_scale_revision)] {
                        validate_value(id,&value)?;
                        if revision.parse::<u64>().ok()!=Some(*revisions.0.get(id).unwrap_or(&0)) {
                            return Err("Transform changed elsewhere; review the current value".into());
                        }
                    }
                    let object = snapshot.0.objects.iter().find(|o| o.address == command.target).unwrap();
                    let local = crate::hierarchy::relative(GlobalTransform{translation:command.position,rotation:command.rotation,scale:command.scale},crate::hierarchy::parent_transform(&snapshot.0,&object.parent)?);
                    for (id,value) in [("position",serde_json::json!(local.translation)),("rotation",serde_json::json!(local.rotation)),("scale",serde_json::json!(local.scale))] { validate_value(id,&value)?; }
                    Ok(Mutation::SetTransform{entity,position:command.position,rotation:command.rotation,scale:command.scale})
                }
                WorldCommand::Playback { command } => {
                    use kokoverse_protocol::PlaybackAction;
                    let entity=*index.0.get(&command.target).ok_or("Target no longer exists")?;
                    let (_,_,revisions,root)=slots.get(entity).map_err(|_|"Expected an ObjectRoot Slot")?;
                    if command.expected_revision.parse::<u64>().ok()!=Some(*revisions.0.get("playback").unwrap_or(&0)) { return Err("Playback changed elsewhere; try the control again".into()); }
                    let mut value=playback.get(entity).map_err(|_|"This object has no video playback")?.0.clone();
                    let visual=&meshes.get(index.0[root.renderer.as_ref().ok_or("Renderer is missing")?]).map_err(|_|"Renderer is missing")?.visual;
                    let duration=match visual {kokoverse_protocol::Visual::Video{duration_seconds,..}=>*duration_seconds,_=>return Err("Expected a video".into())};
                    value.position_seconds=value.position_at(clock.0,duration);
                    value.anchor_seconds=clock.0;
                    match command.action {
                        PlaybackAction::Play=>{ if duration>0.0 && value.position_seconds>=duration {value.position_seconds=0.0;} value.playing=true; },
                        PlaybackAction::Pause=>value.playing=false,
                        PlaybackAction::Restart=>{value.position_seconds=0.0;value.playing=true;},
                        PlaybackAction::Seek{position_seconds}=>{
                            if !position_seconds.is_finite() || !(0.0..=604800.0).contains(&position_seconds) {return Err("Invalid playback position".into());}
                            value.position_seconds=if duration>0.0 {position_seconds.min(duration)} else {position_seconds};
                        },
                        PlaybackAction::Loop{looping}=>value.looping=looping,
                    }
                    Ok(Mutation::SetPlayback{entity,value})
                }
                WorldCommand::Structure { command } => {
                    let deleting = matches!(&command.operation, StructuralOperation::Delete { .. });
                    match command.operation {
                        StructuralOperation::CreateSlot { parent } => {
                            if slots.iter().count() >= MAX_OBJECTS { return Err("Object limit reached (128)".into()); }
                            let mut object = new_object("Slot".into(), [0.0, 0.0, 0.0], [1.0; 4]);
                            crate::hierarchy::validate_parent(&snapshot.0, &object.address, &parent)?;
                            if let Some(parent_object) = parent.as_ref().and_then(|p| snapshot.0.objects.iter().find(|o| &o.address == p)) {
                                object.world_position = parent_object.world_position;
                                object.world_rotation = parent_object.world_rotation;
                                object.world_scale = parent_object.world_scale;
                            }
                            object.renderer = None;
                            object.visual = kokoverse_protocol::Visual::None;
                            object.collider = None;
                            object.parent = parent;
                            object.fields.retain(|f| f.id != "color");
                            crate::sync_object_fields(&mut object);
                            Ok(Mutation::Insert(Box::new(object)))
                        }
                        StructuralOperation::CreateChild { parent } => {
                            if slots.iter().count() >= MAX_OBJECTS { return Err("Object limit reached (128)".into()); }
                            let parent_object = snapshot.0.objects.iter().find(|o| o.address == parent).ok_or("Parent no longer exists")?;
                            let mut object = new_object("Child".into(), parent_object.world_position, [0.2,0.65,0.8,1.0]);
                            crate::hierarchy::validate_parent(&snapshot.0, &object.address, &Some(parent.clone()))?;
                            object.parent = Some(parent);
                            object.world_rotation = parent_object.world_rotation;
                            object.world_scale = parent_object.world_scale;
                            for field in &mut object.fields {
                                field.value = match field.id.as_str() {
                                    "position" => serde_json::json!([0.0,0.0,0.0]),
                                    "parent" => serde_json::json!(object.parent),
                                    _ => field.value.clone(),
                                };
                            }
                            Ok(Mutation::Insert(Box::new(object)))
                        }
                        StructuralOperation::Reparent { target, parent, expected_revision } => {
                            let object = snapshot.0.objects.iter().find(|o| o.address == target).ok_or("Target no longer exists")?;
                            if object.object_revision != expected_revision { return Err("Object changed before reparenting".into()); }
                            if object.panel.as_ref().is_some_and(|p| p.attachment.is_some()) { return Err("Unpin the panel before reparenting".into()); }
                            crate::hierarchy::validate_parent(&snapshot.0, &target, &parent)?;
                            let local = crate::hierarchy::relative(crate::hierarchy::global_of(object), crate::hierarchy::parent_transform(&snapshot.0,&parent)?);
                            validate_value("position", &serde_json::json!(local.translation))?;
                            validate_value("rotation", &serde_json::json!(local.rotation))?;
                            validate_value("scale", &serde_json::json!(local.scale))?;
                            Ok(Mutation::Reparent {entity:index.0[&target],parent,local})
                        }
                        StructuralOperation::CreateInspector {target,position,rotation} => Ok(Mutation::Insert(Box::new(crate::panels::create(&snapshot.0,target,position,rotation)?))),
                        StructuralOperation::CreateCreationPanel {position,rotation} => Ok(Mutation::Insert(Box::new(crate::panels::create_creation(&snapshot.0,position,rotation)?))),
                        StructuralOperation::Create => {
                            let count = slots.iter().count();
                            if count >= MAX_OBJECTS { return Err("Object limit reached (128)".into()); }
                            Ok(Mutation::Insert(Box::new(new_object(format!("Cube {}", count + 1), [count as f64 * 2.2, 0.75, 0.0], [0.2, 0.65, 0.8, 1.0]))))
                        }
                        StructuralOperation::CreateImage {data_url} => {
                            let count = slots.iter().count();
                            if count >= MAX_OBJECTS { return Err("Object limit reached (128)".into()); }
                            if meshes.iter().map(|m|image_size(&m.visual)).sum::<usize>() + data_url.len() > crate::MAX_IMAGE_WORLD { return Err("World image budget exceeded; remove an image first".into()); }
                            let visual = crate::validate_image(&data_url)?;
                            let mut object = new_object(format!("Image {}",count+1),[count as f64 * 2.2,1.2,0.0],[1.0,1.0,1.0,1.0]);
                            object.visual = visual;
                            crate::set_default_collider(&mut object);
                            Ok(Mutation::Insert(Box::new(object)))
                        }
                        StructuralOperation::ImportAsset {name, position, rotation, visual} => {
                            if slots.iter().count() >= MAX_OBJECTS { return Err("Object limit reached (128)".into()); }
                            crate::validate_visual(&visual)?;
                            if matches!(visual,kokoverse_protocol::Visual::Panel) {return Err("Use CreateInspector to create a panel".into());}
                            validate_value("name", &serde_json::json!(name))?;
                            validate_value("position", &serde_json::json!(position))?;
                            validate_value("rotation", &serde_json::json!(rotation))?;
                            if meshes.iter().map(|m|image_size(&m.visual)).sum::<usize>() + image_size(&visual) > crate::MAX_IMAGE_WORLD { return Err("World image budget exceeded; remove an image first".into()); }
                            let mut object = new_object(name, position, [1.0;4]);
                            object.visual = visual;
                            object.world_rotation = rotation;
                            crate::set_default_collider(&mut object);
                            crate::initialize_playback(&mut object,clock.0);
                            crate::sync_object_fields(&mut object);
                            Ok(Mutation::Insert(Box::new(object)))
                        }
                        StructuralOperation::Duplicate { target, expected_revision } | StructuralOperation::Delete { target, expected_revision } => {
                            let entity = *index.0.get(&target).ok_or("Target no longer exists")?;
                            let (_, _, _, root) = slots.get(entity).map_err(|_| "Expected an ObjectRoot Slot")?;
                            if expected_revision.parse::<u64>().map_err(|_| "Invalid object revision")? != root.revision {
                                return Err("Object changed before this operation. Review it and retry.".into());
                            }
                            if deleting { return Ok(Mutation::Delete(target)); }
                            Ok(Mutation::InsertMany(crate::hierarchy::duplicate(&snapshot.0,&target)?))
                        }
                    }
                }
            }
        };
        PreparedCommand { ticket, mutation: validate() }
    });
}

/// Exclusive because spawning/despawning changes archetypes and the stable-ID registry together.
/// This is the only runtime system allowed to author persistent component values.
pub(crate) fn apply_commands(world: &mut World) {
    let Some(prepared) = world.resource_mut::<Prepared>().0.take() else {
        return;
    };
    let editor = world.resource_mut::<Authors>().0.remove(&prepared.ticket);
    let result = match prepared.mutation {
        Err(error) => Err(error),
        Ok(mutation) => {
            let selected = match mutation {
                Mutation::Reparent {entity,parent,local} => {
                    crate::hierarchy::reparent(world,entity,parent,local);
                    Some(world.get::<Identity>(entity).unwrap().0.clone())
                }
                Mutation::SetField {
                    entity,
                    component_id,
                    field,
                    value,
                } => {
                    crate::replication::apply_field(world, entity, component_id, &field, value);
                    Some(world.get::<Identity>(entity).unwrap().0.clone())
                }
                Mutation::SetScripts { entity, value } => {
                    world.entity_mut(entity).insert(ScriptComponents(value));
                    world.get_mut::<ObjectRoot>(entity).unwrap().revision += 1;
                    Some(world.get::<Identity>(entity).unwrap().0.clone())
                }
                Mutation::ScriptInteract { entity } => {
                    let target = world.get::<Identity>(entity).unwrap().0.clone();
                    let events: Vec<_> = world
                        .get::<ScriptComponents>(entity)
                        .unwrap()
                        .0
                        .iter()
                        .filter(|s| s.enabled && s.schema.has_interact)
                        .map(|s| (target.clone(), s.id.clone()))
                        .collect();
                    world
                        .resource_mut::<crate::scripting::ScriptRuntime>()
                        .events
                        .extend(events);
                    world.get_mut::<ObjectRoot>(entity).unwrap().revision += 1;
                    Some(target)
                }
                Mutation::ScriptFrame(patches) => {
                    for patch in patches {
                        let entity = patch.entity;
                        let renderer = world.get::<ObjectRoot>(entity).unwrap().renderer.as_ref()
                            .and_then(|address| world.resource::<EntityIndex>().0.get(address)).copied();
                        for (id, changed) in [
                            (
                                "position",
                                world.get::<LocalTransform>(entity).unwrap().translation
                                    != patch.position,
                            ),
                            (
                                "rotation",
                                world.get::<LocalTransform>(entity).unwrap().rotation
                                    != patch.rotation,
                            ),
                            (
                                "scale",
                                world.get::<LocalTransform>(entity).unwrap().scale != patch.scale,
                            ),
                            (
                                "color",
                                renderer.is_some_and(|r| world.get::<MeshRenderer>(r).unwrap().color != patch.color),
                            ),
                        ] {
                            if changed {
                                *world
                                    .get_mut::<Revisions>(entity)
                                    .unwrap()
                                    .0
                                    .entry(id.into())
                                    .or_default() += 1;
                            }
                        }
                        *world.get_mut::<LocalTransform>(entity).unwrap() = LocalTransform {
                            translation: patch.position,
                            rotation: patch.rotation,
                            scale: patch.scale,
                        };
                        if let Some(renderer) = renderer { world.get_mut::<MeshRenderer>(renderer).unwrap().color = patch.color; }
                        world.get_mut::<ScriptComponents>(entity).unwrap().0 = patch.scripts;
                        world.get_mut::<ObjectRoot>(entity).unwrap().revision += 1;
                    }
                    None
                }
                Mutation::SetPlayback { entity, value } => {
                    world.entity_mut(entity).insert(MediaPlayback(value));
                    bump_field(world, entity, "playback");
                    Some(world.get::<Identity>(entity).unwrap().0.clone())
                }
                Mutation::SetTransform {
                    entity,
                    position,
                    rotation,
                    scale,
                } => {
                    let local = crate::hierarchy::world_to_local(world, entity, GlobalTransform { translation: position, rotation, scale });
                    *world.get_mut::<LocalTransform>(entity).unwrap() = local;
                    for id in ["position", "rotation", "scale"] {
                        *world
                            .get_mut::<Revisions>(entity)
                            .unwrap()
                            .0
                            .entry(id.into())
                            .or_default() += 1;
                    }
                    world.get_mut::<ObjectRoot>(entity).unwrap().revision += 1;
                    Some(world.get::<Identity>(entity).unwrap().0.clone())
                }
                Mutation::Insert(object) => {
                    let selected = object.address.clone();
                    insert_object(world, *object);
                    Some(selected)
                }
                Mutation::InsertMany(objects) => {
                    let selected=objects.first().map(|o|o.address.clone());
                    for object in objects { insert_object(world,object); }
                    selected
                }
                Mutation::Delete(address) => {
                    let descendants = crate::hierarchy::subtree(&world.resource::<SnapshotCache>().0, &address);
                    for address in descendants {
                        crate::panels::target_removed(world, &address);
                        remove_object(world, &address);
                        world.resource_mut::<ChangeSet>().removed.push(address);
                    }
                    None
                }
            };
            if let Some(editor) = editor
                && let Some(address) = &selected
                && let Some(entity) = world.resource::<EntityIndex>().0.get(address).copied()
            {
                let mut root = world.get_mut::<ObjectRoot>(entity).unwrap();
                let identity =
                    root.item_identity
                        .get_or_insert_with(|| kokoverse_protocol::ItemIdentity {
                            creator_id: editor.user_id.clone(),
                            owner_id: editor.user_id.clone(),
                            source_version: None,
                            permissions: kokoverse_protocol::ItemPermissions {
                                modify: true,
                                redistribute: true,
                            },
                            editors: vec![],
                        });
                identity.editors.retain(|e| e.user_id != editor.user_id);
                identity.editors.push(editor);
            }
            world.resource_mut::<WorldRevision>().0 += 1;
            Ok(selected)
        }
    };
    world.resource_mut::<Applied>().0 = Some(AppliedCommand {
        ticket: prepared.ticket,
        result,
    });
}

pub(crate) fn bump_field(world: &mut World, entity: Entity, field: &str) {
    *world
        .get_mut::<Revisions>(entity)
        .unwrap()
        .0
        .entry(field.into())
        .or_default() += 1;
    world.get_mut::<ObjectRoot>(entity).unwrap().revision += 1;
}

/// Roots currently have no parent. Hierarchy propagation can replace this system later.
pub(crate) fn update_transforms(world: &mut World) {
    crate::hierarchy::propagate(world);
}

type ChangedRoot = Or<(
    Changed<Slot>,
    Changed<LocalTransform>,
    Changed<GlobalTransform>,
    Changed<Revisions>,
    Changed<ObjectRoot>,
)>;

/// Use ECS change detection after mutations, including newly spawned objects.
pub(crate) fn collect_changes(
    roots: Query<&Identity, (With<ObjectRoot>, ChangedRoot)>,
    mut changes: ResMut<ChangeSet>,
) {
    for identity in &roots {
        changes.upserts.insert(identity.0.clone());
    }
}

/// Build an immutable view only when the ECS changed; reads of snapshot() do no queries.
pub(crate) fn extract_snapshot(
    roots: Query<SnapshotRoot>,
    meshes: Query<&MeshRenderer>,
    index: Res<EntityIndex>,
    revision: Res<WorldRevision>,
    changes: Res<ChangeSet>,
    mut cache: ResMut<SnapshotCache>,
) {
    if changes.upserts.is_empty() && changes.removed.is_empty() {
        return;
    }
    let mut objects = Vec::with_capacity(roots.iter().len());
    for (
        identity,
        slot,
        local,
        global,
        revisions,
        root,
        collider,
        playback,
        scripts,
        panel,
        inspector,
        creation,
    ) in &roots
    {
        let mesh = root.renderer.as_ref().map(|address| meshes.get(index.0[address]).expect("registered renderer"));
        let mut object = kokoverse_protocol::ObjectView {
            parent: slot.parent.clone(),
            item_identity: root.item_identity.clone(),
            address: identity.0.clone(),
            renderer: root.renderer.clone(),
            object_revision: root.revision.to_string(),
            world_position: global.translation,
            world_rotation: global.rotation,
            world_scale: global.scale,
            collider: collider.map(|c| c.0.clone()),
            visual: mesh.map_or(kokoverse_protocol::Visual::None, |m| m.visual.clone()),
            playback: playback.map(|p| p.0.clone()),
            scripts: scripts.0.clone(),
            panel: panel.map(|p| p.0.clone()),
            inspector: inspector.map(|i| i.0.clone()),
            creation: creation.map(|c| c.0.clone()),
            fields: vec![
                crate::field(
                    "name",
                    "Name",
                    "string",
                    serde_json::json!(slot.name),
                    revisions,
                ),
                crate::field("parent", "Parent", "address", serde_json::json!(slot.parent), revisions),

                crate::field(
                    "position",
                    "Position",
                    "vec3",
                    serde_json::json!(local.translation),
                    revisions,
                ),
                crate::field(
                    "rotation",
                    "Rotation",
                    "quat",
                    serde_json::json!(local.rotation),
                    revisions,
                ),
                crate::field(
                    "scale",
                    "Scale",
                    "vec3",
                    serde_json::json!(local.scale),
                    revisions,
                ),
                crate::field(
                    "collider",
                    "Collider",
                    "collider",
                    serde_json::json!(collider.map(|c| &c.0)),
                    revisions,
                ),
                crate::field(
                    "playback",
                    "Playback",
                    "playback",
                    serde_json::json!(playback.map(|p| &p.0)),
                    revisions,
                ),
            ],
        };
        if let Some(mesh) = mesh {
            object.fields.insert(3, crate::field("color", "Color", "color", serde_json::json!(mesh.color), revisions));
        }
        crate::panels::append_fields(&mut object, revisions);
        objects.push(object);
    }
    objects.sort_by(|a, b| a.address.instance_id.cmp(&b.address.instance_id));
    cache.0 = kokoverse_protocol::Snapshot {
        world_revision: revision.0.to_string(),
        objects,
    };
}

/// Publish one atomic commit per accepted command, after derived transforms and extraction.
pub(crate) fn publish_changes(
    mut applied: ResMut<Applied>,
    mut changes: ResMut<ChangeSet>,
    cache: Res<SnapshotCache>,
    mut output: ResMut<OutputQueue>,
) {
    let upserts: HashSet<Address> = std::mem::take(&mut changes.upserts);
    let removed = std::mem::take(&mut changes.removed);
    let Some(applied) = applied.0.take() else {
        return;
    };
    let selected = applied.result.as_ref().ok().cloned().flatten();
    let result = applied.result.map(|_| WorldCommit {
        world_revision: cache.0.world_revision.clone(),
        upserts: cache
            .0
            .objects
            .iter()
            .filter(|o| upserts.contains(&o.address))
            .cloned()
            .collect(),
        removed,
    });
    output.0.push(CommandOutcome {
        ticket: applied.ticket,
        result,
        selected,
    });
}

use super::*;
use kokoverse_protocol::{Edit, StructuralCommand, StructuralOperation};

fn edit(model: &WorldModel, field: &str, value: Value) -> WorldCommand {
    let object = &model.snapshot().objects[0];
    WorldCommand::Edit {
        edit: Edit {
            command_id: "test".into(),
            target: replication::REGISTRY
                .field_address(&object.address, field)
                .unwrap(),
            field_id: field.into(),
            expected_revision: object
                .fields
                .iter()
                .find(|f| f.id == field)
                .unwrap()
                .revision
                .clone(),
            value,
        },
    }
}
fn structure(operation: StructuralOperation) -> WorldCommand {
    WorldCommand::Structure {
        command: StructuralCommand {
            command_id: "test".into(),
            operation,
        },
    }
}
fn run(model: &mut WorldModel, command: WorldCommand) -> CommandOutcome {
    model.enqueue(command).unwrap();
    model.tick().remove(0)
}

#[test]
fn primitives_replicate_and_restore_without_mesh_assets() {
    use kokoverse_protocol::{PrimitiveShape, Visual};
    let mut host = WorldModel::new();
    let mut guest = WorldModel::new();
    guest.restore(host.snapshot()).unwrap();
    for shape in [PrimitiveShape::Box, PrimitiveShape::Sphere, PrimitiveShape::Capsule,
        PrimitiveShape::Cylinder, PrimitiveShape::Cone, PrimitiveShape::Torus,
        PrimitiveShape::Grid, PrimitiveShape::Quad] {
        let result = run(&mut host, structure(StructuralOperation::ImportAsset {
            name: format!("{shape:?}"), position: [2.0, 0.75, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0], visual: Visual::Primitive { shape },
        }));
        let id = result.selected.unwrap();
        guest.apply_commit(result.result.unwrap()).unwrap();
        assert_eq!(host.snapshot(), guest.snapshot());
        let object = guest.snapshot().objects.into_iter().find(|o| o.address == id).unwrap();
        assert_eq!(object.visual, Visual::Primitive { shape });
        assert_eq!(object.collider.unwrap().half_extents, shape.half_extents());
    }
    let bytes = serde_json::to_vec(&host.snapshot()).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("asset_id"));
    let mut restored = WorldModel::new();
    restored.restore(serde_json::from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(host.snapshot(), restored.snapshot());
    assert!(serde_json::from_value::<Visual>(json!({"kind":"primitive","shape":"unknown"})).is_err());
}

#[test]
fn creation_panels_use_shared_surface_components_and_survive_restore() {
    use kokoverse_protocol::{PrimitiveShape, Visual};
    let mut host = WorldModel::new();
    let mut guest = WorldModel::new();
    guest.restore(host.snapshot()).unwrap();
    let created = run(&mut host, structure(StructuralOperation::CreateCreationPanel {
        position: [2.0, 1.5, 1.0], rotation: [0.0, 0.0, 0.0, 1.0],
    }));
    let id = created.selected.unwrap();
    guest.apply_commit(created.result.unwrap()).unwrap();
    let object = host.snapshot().objects.into_iter().find(|o| o.address == id).unwrap();
    assert_eq!(object.visual, Visual::Panel);
    assert!(object.inspector.is_none());
    assert_eq!(object.creation.unwrap().shape, PrimitiveShape::Box);
    assert_eq!(object.collider.unwrap().half_extents, panels::half_extents([1120, 720]));
    for (field, value) in [("creation_shape", json!("sphere")), ("position", json!([3.0, 2.0, 1.0])), ("scale", json!([1.5, 1.5, 1.5]))] {
        let result = run(&mut host, WorldCommand::Edit { edit: Edit {
            command_id: field.into(), target: replication::REGISTRY.field_address(&id, field).unwrap(),
            field_id: field.into(), expected_revision: "0".into(), value,
        }});
        guest.apply_commit(result.result.unwrap()).unwrap();
        assert_eq!(host.snapshot(), guest.snapshot());
    }
    for object in host.snapshot().objects {
        let wire = replication::REGISTRY.encode_object(&object);
        let decoded = replication::REGISTRY.decode_object(wire).unwrap();
        assert_eq!(decoded, object);
    }
    let mut restored = WorldModel::new();
    restored.restore(serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap()).unwrap()).unwrap();
    assert_eq!(restored.snapshot(), host.snapshot());
}

#[test]
fn transform_commit_is_atomic_and_replicates_all_fields() {
    use kokoverse_protocol::TransformCommand;
    let mut host = WorldModel::new();
    let mut guest = WorldModel::new();
    guest.restore(host.snapshot()).unwrap();
    let target = host.snapshot().objects[0].address.clone();
    let mut command = TransformCommand {
        command_id: "transform".into(),
        target,
        expected_position_revision: "0".into(),
        expected_rotation_revision: "0".into(),
        expected_scale_revision: "0".into(),
        position: [1.0, 2.0, 3.0],
        rotation: glam::DQuat::from_rotation_y(0.8).to_array(),
        scale: [1.5, 2.0, 0.5],
    };
    command.expected_scale_revision = "1".into();
    let before = host.snapshot();
    assert!(
        run(
            &mut host,
            WorldCommand::Transform {
                command: command.clone()
            }
        )
        .result
        .is_err()
    );
    assert_eq!(host.snapshot(), before);
    command.expected_scale_revision = "0".into();
    let commit = run(
        &mut host,
        WorldCommand::Transform {
            command: command.clone(),
        },
    )
    .result
    .unwrap();
    guest.apply_commit(commit).unwrap();
    assert_eq!(host.snapshot(), guest.snapshot());
    let after = host.snapshot();
    assert_eq!(after.world_revision, "1");
    let object = &after.objects[0];
    assert_eq!(object.object_revision, "1");
    assert_eq!(object.world_rotation, command.rotation);
    assert_eq!(object.world_scale, command.scale);
    for id in ["position", "rotation", "scale"] {
        assert_eq!(
            object.fields.iter().find(|f| f.id == id).unwrap().revision,
            "1"
        );
    }
    assert!(
        run(&mut host, WorldCommand::Transform { command })
            .result
            .is_err()
    );
    assert_eq!(host.snapshot(), after);
}

#[test]
fn old_saved_world_is_rejected_without_mutating_the_world() {
    let mut model = WorldModel::new();
    let before = model.snapshot();
    let mut old = before.clone();
    old.objects[0]
        .fields
        .retain(|f| ["name", "position", "color"].contains(&f.id.as_str()));
    assert!(model.restore(old).is_err());
    assert_eq!(model.snapshot(), before);
}

#[test]
fn schedule_defers_mutation_and_extracts_derived_transform() {
    let mut model = WorldModel::new();
    let before = model.snapshot();
    model
        .enqueue(edit(&model, "position", json!([2, 3, 4])))
        .unwrap();
    assert_eq!(model.snapshot(), before);
    let outcomes = model.tick();
    assert!(outcomes[0].result.is_ok());
    assert_eq!(model.snapshot().objects[0].world_position, [2.0, 3.0, 4.0]);
    assert_eq!(model.snapshot().world_revision, "1");
    assert!(model.tick().is_empty());
    assert_eq!(model.snapshot().world_revision, "1");
}

#[test]
fn same_tick_commands_check_each_preceding_commit() {
    let mut model = WorldModel::new();
    let name = edit(&model, "name", json!("First"));
    let color = edit(&model, "color", json!([1, 0, 0, 1]));
    model.enqueue(name.clone()).unwrap();
    model.enqueue(name).unwrap();
    model.enqueue(color).unwrap();
    let results = model.tick();
    assert!(results[0].result.is_ok());
    assert!(results[1].result.is_err());
    assert!(results[2].result.is_ok());
    assert_eq!(model.snapshot().world_revision, "2");
}

#[test]
fn duplicate_is_independent_and_delete_cleans_both_entities() {
    let mut model = WorldModel::new();
    let original = model.snapshot().objects[0].clone();
    let copied = run(
        &mut model,
        structure(StructuralOperation::Duplicate {
            target: original.address.clone(),
            expected_revision: "0".into(),
        }),
    );
    let copy = copied.selected.unwrap();
    assert_ne!(copy.instance_id, original.address.instance_id);
    let command = WorldCommand::Edit {
        edit: Edit {
            command_id: "edit".into(),
            target: replication::REGISTRY.field_address(&copy, "color").unwrap(),
            field_id: "color".into(),
            expected_revision: "0".into(),
            value: json!([1, 0, 0, 1]),
        },
    };
    assert!(run(&mut model, command).result.is_ok());
    assert_eq!(
        model
            .snapshot()
            .objects
            .iter()
            .find(|o| o.address == original.address)
            .unwrap(),
        &original
    );
    let stale = structure(StructuralOperation::Delete {
        target: copy.clone(),
        expected_revision: "0".into(),
    });
    assert!(run(&mut model, stale).result.is_err());
    assert!(
        run(
            &mut model,
            structure(StructuralOperation::Delete {
                target: copy.clone(),
                expected_revision: "1".into()
            })
        )
        .result
        .is_ok()
    );
    assert_eq!(model.world.resource::<EntityIndex>().0.len(), 2);
    assert!(!model.world.resource::<EntityIndex>().0.contains_key(&copy));
}

#[test]
fn delete_last_and_create_again() {
    let mut model = WorldModel::new();
    let original = model.snapshot().objects[0].address.clone();
    let deleted = run(
        &mut model,
        structure(StructuralOperation::Delete {
            target: original,
            expected_revision: "0".into(),
        }),
    );
    assert!(deleted.result.is_ok());
    assert!(model.snapshot().objects.is_empty());
    assert!(
        run(&mut model, structure(StructuralOperation::Create))
            .result
            .is_ok()
    );
    assert_eq!(model.snapshot().objects.len(), 1);
}

#[test]
fn invalid_snapshot_and_commit_are_atomic() {
    let mut model = WorldModel::new();
    let before = model.snapshot();
    let mut bad = before.clone();
    bad.objects[0].renderer.instance_id = Uuid::new_v4().to_string();
    assert!(model.restore(bad).is_err());
    assert_eq!(before, model.snapshot());
    let bad = WorldCommit {
        world_revision: "1".into(),
        upserts: vec![],
        removed: vec![Address {
            instance_id: Uuid::new_v4().to_string(),
            local_id: 1,
        }],
    };
    assert!(model.apply_commit(bad).is_err());
    assert_eq!(before, model.snapshot());
}

#[test]
fn delta_replication_preserves_unaffected_entities() {
    let mut host = WorldModel::new();
    let mut guest = WorldModel::new();
    guest.restore(host.snapshot()).unwrap();
    let original = host.snapshot().objects[0].address.clone();
    let stable_entity = guest.world.resource::<EntityIndex>().0[&original];
    let commit = run(&mut host, structure(StructuralOperation::Create))
        .result
        .unwrap();
    guest.apply_commit(commit).unwrap();
    assert_eq!(host.snapshot(), guest.snapshot());
    assert_eq!(
        guest.world.resource::<EntityIndex>().0[&original],
        stable_entity
    );
}

#[test]
fn presence_is_transient_coalesced_and_survives_snapshot_restore() {
    use kokoverse_protocol::{AvatarPose, ParticipantView};
    let mut model = WorldModel::new();
    let view = ParticipantView {
        avatar_texture: None,
        id: "peer:epoch".into(),
        name: "Guest".into(),
        pose: AvatarPose {
            position: [1.0, 0.0, 2.0],
            yaw: 0.5,
            pitch: 0.0,
        },
    };
    model.update_participant(view.clone()).unwrap();
    let mut latest = view.clone();
    latest.pose.position[0] = 4.0;
    model.update_participant(latest.clone()).unwrap();
    model.tick();
    assert_eq!(model.participants(), vec![latest.clone()]);
    assert_eq!(model.snapshot().world_revision, "0");
    let revision = model.presence_revision();
    model.update_participant(latest.clone()).unwrap();
    model.tick();
    assert_eq!(model.presence_revision(), revision);
    let mut invalid = latest.clone();
    invalid.pose.position[0] = f64::INFINITY;
    assert!(model.update_participant(invalid).is_err());
    model.restore(model.snapshot()).unwrap();
    assert_eq!(model.participants(), vec![latest]);
    model.remove_participant(&view.id);
    model.tick();
    assert!(model.participants().is_empty());
    assert_eq!(model.snapshot().world_revision, "0");
}

#[test]
fn avatar_texture_references_are_validated_and_do_not_edit_the_world() {
    let mut model = WorldModel::new();
    let original = model.snapshot();
    let mut person = kokoverse_protocol::ParticipantView {
        id: "a".into(),
        name: "Painter".into(),
        pose: kokoverse_protocol::AvatarPose {
            position: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
        },
        avatar_texture: Some("f".repeat(64)),
    };
    model.update_participant(person.clone()).unwrap();
    model.tick();
    assert_eq!(model.snapshot(), original);
    model.restore(original.clone()).unwrap();
    assert_eq!(model.participants(), vec![person.clone()]);
    person.avatar_texture = Some("../private.png".into());
    assert!(model.update_participant(person).is_err());
    assert_eq!(model.snapshot(), original);
}

#[test]
fn image_panels_validate_bytes_dimensions_and_replication() {
    use base64::Engine;
    let image = image::DynamicImage::new_rgb8(8, 4);
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    let data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    );
    let mut host = WorldModel::new();
    let mut guest = WorldModel::new();
    guest.restore(host.snapshot()).unwrap();
    let commit = run(
        &mut host,
        structure(StructuralOperation::CreateImage {
            data_url: data_url.clone(),
        }),
    )
    .result
    .unwrap();
    assert!(matches!(
        &commit.upserts[0].visual,
        kokoverse_protocol::Visual::Image {
            width: 8,
            height: 4,
            ..
        }
    ));
    guest.apply_commit(commit).unwrap();
    assert_eq!(host.snapshot(), guest.snapshot());
    let before = host.snapshot();
    assert!(
        run(
            &mut host,
            structure(StructuralOperation::CreateImage {
                data_url: "data:image/png;base64,AAAA".into()
            })
        )
        .result
        .is_err()
    );
    assert_eq!(host.snapshot(), before);
    assert!(
        validate_image(&format!(
            "data:image/png;base64,{}",
            "A".repeat(MAX_IMAGE_BYTES * 2)
        ))
        .is_err()
    );
    let mut corrupt = before.clone();
    let panel = corrupt
        .objects
        .iter_mut()
        .find(|o| matches!(o.visual, kokoverse_protocol::Visual::Image { .. }))
        .unwrap();
    if let kokoverse_protocol::Visual::Image { width, .. } = &mut panel.visual {
        *width = 100;
    }
    assert!(guest.restore(corrupt).is_err());
    assert_eq!(guest.snapshot(), before);
}

fn video_model() -> (WorldModel, Address) {
    let mut model = WorldModel::new();
    model.set_media_time(100.0);
    let result = run(
        &mut model,
        structure(StructuralOperation::ImportAsset {
            name: "Screen".into(),
            position: [0.0, 1.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            visual: kokoverse_protocol::Visual::Video {
                asset_id: "a".repeat(64),
                width: 320,
                height: 180,
                duration_seconds: 10.0,
            },
        }),
    );
    let address = result.result.unwrap().upserts[0].address.clone();
    (model, address)
}

fn playback_command(
    model: &WorldModel,
    target: &Address,
    action: kokoverse_protocol::PlaybackAction,
) -> WorldCommand {
    let snapshot = model.snapshot();
    let object = snapshot
        .objects
        .iter()
        .find(|o| &o.address == target)
        .unwrap();
    WorldCommand::Playback {
        command: kokoverse_protocol::PlaybackCommand {
            command_id: "playback".into(),
            target: target.clone(),
            expected_revision: object
                .fields
                .iter()
                .find(|f| f.id == "playback")
                .unwrap()
                .revision
                .clone(),
            action,
        },
    }
}

fn playback_state(model: &WorldModel, target: &Address) -> kokoverse_protocol::VideoPlayback {
    model
        .snapshot()
        .objects
        .iter()
        .find(|o| &o.address == target)
        .unwrap()
        .playback
        .clone()
        .unwrap()
}

#[test]
fn playback_uses_host_clock_and_replicates_only_on_operations() {
    use kokoverse_protocol::PlaybackAction;
    let (mut host, address) = video_model();
    let mut guest = WorldModel::new();
    guest.set_media_time(9999.0);
    guest.restore(host.snapshot()).unwrap();
    assert_eq!(playback_state(&host, &address).anchor_seconds, 100.0);
    let before = host.snapshot();
    host.set_media_time(103.5);
    host.tick();
    assert_eq!(
        host.snapshot(),
        before,
        "Time alone must not publish world revisions"
    );
    let pause = playback_command(&host, &address, PlaybackAction::Pause);
    let commit = run(&mut host, pause).result.unwrap();
    guest.apply_commit(commit).unwrap();
    assert_eq!(host.snapshot(), guest.snapshot());
    assert_eq!(playback_state(&host, &address).position_seconds, 3.5);
    host.set_media_time(500.0);
    assert_eq!(
        playback_state(&host, &address).position_at(500.0, 10.0),
        3.5
    );
    for (time, action, expected, playing) in [
        (500.0, PlaybackAction::Play, 3.5, true),
        (502.0, PlaybackAction::Pause, 5.5, false),
        (
            504.0,
            PlaybackAction::Seek {
                position_seconds: 8.0,
            },
            8.0,
            false,
        ),
        (505.0, PlaybackAction::Restart, 0.0, true),
        (516.0, PlaybackAction::Pause, 1.0, false),
        (520.0, PlaybackAction::Loop { looping: false }, 1.0, false),
        (521.0, PlaybackAction::Play, 1.0, true),
        (541.0, PlaybackAction::Pause, 10.0, false),
        (542.0, PlaybackAction::Play, 0.0, true),
    ] {
        host.set_media_time(time);
        let command = playback_command(&host, &address, action);
        guest
            .apply_commit(run(&mut host, command).result.unwrap())
            .unwrap();
        assert_eq!(guest.snapshot(), host.snapshot());
        let state = playback_state(&host, &address);
        assert_eq!((state.position_seconds, state.playing), (expected, playing));
        assert_eq!(state.anchor_seconds, time);
    }
    let mut late = WorldModel::new();
    late.restore(host.snapshot()).unwrap();
    assert_eq!(
        playback_state(&late, &address).position_at(545.0, 10.0),
        3.0
    );
}

#[test]
fn invalid_and_conflicting_playback_operations_are_atomic() {
    use kokoverse_protocol::PlaybackAction;
    let (mut model, address) = video_model();
    let stale = playback_command(&model, &address, PlaybackAction::Pause);
    let command = playback_command(
        &model,
        &address,
        PlaybackAction::Seek {
            position_seconds: 7.0,
        },
    );
    run(&mut model, command).result.unwrap();
    let before = model.snapshot();
    assert!(run(&mut model, stale).result.is_err());
    for position in [-1.0, f64::NAN, f64::INFINITY, 604801.0] {
        let command = playback_command(
            &model,
            &address,
            PlaybackAction::Seek {
                position_seconds: position,
            },
        );
        assert!(run(&mut model, command).result.is_err());
    }
    let direct = WorldCommand::Edit {
        edit: Edit {
            command_id: "spoof".into(),
            target: address.clone(),
            field_id: "playback".into(),
            expected_revision: "1".into(),
            value: json!({"playing":true,"looping":true,"position_seconds":0,"anchor_seconds":0}),
        },
    };
    assert!(run(&mut model, direct).result.is_err());
    let cube = model
        .snapshot()
        .objects
        .iter()
        .find(|o| o.playback.is_none())
        .unwrap()
        .address
        .clone();
    let command = playback_command(&model, &cube, PlaybackAction::Play);
    assert!(run(&mut model, command).result.is_err());
    assert_eq!(model.snapshot(), before);
}

#[test]
fn legacy_video_snapshot_and_invalid_state_are_rejected() {
    let (mut model, address) = video_model();
    let mut old = model.snapshot();
    for object in &mut old.objects {
        object.fields.retain(|f| f.id != "playback");
        object.playback = None;
    }
    model.set_media_time(200.0);
    assert!(model.restore(old).is_err());
    let before = model.snapshot();
    for invalid in [-1.0, f64::INFINITY] {
        let mut snapshot = before.clone();
        let video = snapshot
            .objects
            .iter_mut()
            .find(|o| o.address == address)
            .unwrap();
        video.playback.as_mut().unwrap().anchor_seconds = invalid;
        sync_object_fields(video);
        assert!(model.restore(snapshot).is_err());
        assert_eq!(model.snapshot(), before);
    }
}

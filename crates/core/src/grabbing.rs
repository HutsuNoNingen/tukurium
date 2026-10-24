//! Renderer-independent collider picking and view-relative transform manipulation.
use crate::{
    Collider, Identity, Revisions,
    collision::{CollisionWorld, EYE_HEIGHT, preview_matches},
    locomotion::LocalControl,
    presence::{Participant, PresenceIndex},
};
use bevy_ecs::prelude::*;
use glam::{DMat3, DQuat, DVec3};
use kokoverse_protocol::{GrabView, InteractionRay, ObjectPreview};
use std::time::{Duration, Instant};
#[derive(Clone)]
pub(crate) struct Held {
    pub view: GrabView,
    anchor: DVec3,
    last_yaw: f64,
    last_view: DQuat,
    follow_pitch: bool,
    last_align: Option<Instant>,
}
#[derive(Resource, Default, Clone)]
pub(crate) struct GrabState(pub Option<Held>);
fn basis(yaw: f64, pitch: f64) -> [DVec3; 3] {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    [
        DVec3::new(cy, 0.0, -sy),
        DVec3::new(-sy * sp, cp, -cy * sp),
        DVec3::new(-sy * cp, -sp, -cy * cp),
    ]
}
fn view_rotation(yaw: f64, pitch: f64) -> DQuat {
    let [right, up, forward] = basis(yaw, pitch);
    DQuat::from_mat3(&DMat3::from_cols(right, up, -forward))
}
pub(crate) fn begin(world: &mut World) -> Result<(), String> {
    if world.resource::<GrabState>().0.is_some() {
        return Err("Already holding an object".into());
    }
    let control = world.resource::<LocalControl>();
    let entity = world
        .resource::<PresenceIndex>()
        .0
        .get(&control.id)
        .ok_or("Player missing")?;
    let origin = DVec3::from_array(world.get::<Participant>(*entity).unwrap().0.pose.position)
        + DVec3::Y * EYE_HEIGHT;
    let axes = basis(control.input.yaw, control.input.pitch);
    let yaw = control.input.yaw;
    let view_rotation = view_rotation(yaw, control.input.pitch);
    let (bounds, distance) = world
        .resource::<CollisionWorld>()
        .0
        .iter()
        .filter_map(|b| {
            b.raycast(origin.to_array(), axes[2].to_array(), 6.0)
                .map(|(t, _)| (b, t))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .ok_or("Aim at a collider within 6 m")?;
    let anchor = bounds
        .frame()
        .inverse()
        .transform_point3(origin + axes[2] * distance);
    let address = bounds.address.clone();
    let position = bounds.center;
    let rotation = bounds.rotation.to_array();
    let scale = bounds.scale;
    let mut query = world.query::<(&Identity, &Revisions)>();
    let (_, rev) = query
        .iter(world)
        .find(|(id, _)| id.0 == address)
        .ok_or("Object missing")?;
    let revision = |id: &str| rev.0.get(id).copied().unwrap_or(0).to_string();
    let preview = ObjectPreview {
        target: address,
        expected_revision: revision("position"),
        position,
        rotation: Some(rotation),
        scale: Some(scale),
        expected_rotation_revision: Some(revision("rotation")),
        expected_scale_revision: Some(revision("scale")),
    };
    world.resource_mut::<GrabState>().0 = Some(Held {
        view: GrabView {
            preview,
            distance: distance.clamp(0.5, 6.0),
            released: false,
        },
        anchor,
        last_yaw: yaw,
        last_view: view_rotation,
        follow_pitch: false,
        last_align: None,
    });
    Ok(())
}
pub(crate) fn interaction_ray(world: &World) -> Option<InteractionRay> {
    let control = world.resource::<LocalControl>();
    if !control.input.active || !control.input.face_view {
        return None;
    }
    let entity = world.resource::<PresenceIndex>().0.get(&control.id)?;
    let origin = DVec3::from_array(world.get::<Participant>(*entity)?.0.pose.position)
        + DVec3::Y * EYE_HEIGHT;
    let dir = basis(control.input.yaw, control.input.pitch)[2];
    let (distance, target, holding) = if let Some(held) = world.resource::<GrabState>().0.as_ref() {
        (
            held.view.distance,
            Some(held.view.preview.target.clone()),
            !held.view.released,
        )
    } else {
        let hit = world
            .resource::<CollisionWorld>()
            .0
            .iter()
            .filter_map(|b| {
                b.raycast(origin.to_array(), dir.to_array(), 6.0)
                    .map(|(d, _)| (d, b.address.clone()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        hit.map_or((6.0, None, false), |(d, t)| (d, Some(t), false))
    };
    Some(InteractionRay {
        origin: origin.to_array(),
        point: (origin + dir * distance).to_array(),
        target,
        holding,
    })
}
pub(crate) fn update_grab(
    control: Res<LocalControl>,
    players: Query<&Participant>,
    roots: Query<(&Identity, &Revisions), With<Collider>>,
    mut grab: ResMut<GrabState>,
) {
    let Some(held) = grab.0.as_mut() else {
        return;
    };
    if !roots
        .iter()
        .any(|(id, r)| id.0 == held.view.preview.target && preview_matches(&held.view.preview, r))
    {
        grab.0 = None;
        return;
    }
    if held.view.released {
        return;
    }
    let Some(player) = players.iter().find(|p| p.0.id == control.id) else {
        grab.0 = None;
        return;
    };
    let origin = DVec3::from_array(player.0.pose.position) + DVec3::Y * EYE_HEIGHT;
    let dir = basis(control.input.yaw, control.input.pitch)[2];
    let view = view_rotation(control.input.yaw, control.input.pitch);
    let change = if held.follow_pitch {
        view * held.last_view.conjugate()
    } else {
        DQuat::from_rotation_y(control.input.yaw - held.last_yaw)
    };
    let rotation = (change * DQuat::from_array(held.view.preview.rotation.unwrap())).normalize();
    held.view.preview.rotation = Some(rotation.to_array());
    held.last_view = view;
    held.last_yaw = control.input.yaw;
    let scale = DVec3::from_array(held.view.preview.scale.unwrap());
    let position = origin + dir * held.view.distance - rotation * (held.anchor * scale);
    held.view.preview.position = position
        .clamp(DVec3::splat(-1000.0), DVec3::splat(1000.0))
        .to_array();
}
pub(crate) fn rotate(world: &mut World, delta: [f64; 2], vertical: bool) {
    if !delta.iter().all(|v| v.is_finite() && v.abs() <= 1.0) {
        return;
    }
    let control = world.resource::<LocalControl>();
    let [right, up, _] = basis(control.input.yaw, control.input.pitch);
    if let Some(held) = world
        .resource_mut::<GrabState>()
        .0
        .as_mut()
        .filter(|h| !h.view.released)
    {
        let q = DQuat::from_array(held.view.preview.rotation.unwrap());
        let change = DQuat::from_axis_angle(if vertical { DVec3::Y } else { up }, delta[0])
            * DQuat::from_axis_angle(right, if vertical { 0.0 } else { delta[1] });
        held.view.preview.rotation = Some((change * q).normalize().to_array());
    }
}
pub(crate) fn scale(world: &mut World, factor: f64) {
    if !factor.is_finite() || !(0.5..=2.0).contains(&factor) {
        return;
    }
    if let Some(held) = world
        .resource_mut::<GrabState>()
        .0
        .as_mut()
        .filter(|h| !h.view.released)
    {
        let old = held.view.preview.scale.unwrap();
        let min = old.iter().copied().fold(f64::INFINITY, f64::min);
        let max = old.iter().copied().fold(0.0, f64::max);
        let factor = factor.clamp(0.05 / min, 20.0 / max);
        held.view.preview.scale = Some(old.map(|v| v * factor));
    }
}
pub(crate) fn align(world: &mut World) {
    if let Some(held) = world
        .resource_mut::<GrabState>()
        .0
        .as_mut()
        .filter(|h| !h.view.released)
    {
        let now = Instant::now();
        if held
            .last_align
            .is_some_and(|t| now.duration_since(t) < Duration::from_millis(500))
        {
            held.follow_pitch = !held.follow_pitch;
            held.last_align = None;
        } else {
            held.last_align = Some(now);
        }
        let q = DQuat::from_array(held.view.preview.rotation.unwrap());
        let axes = [
            DVec3::X,
            -DVec3::X,
            DVec3::Y,
            -DVec3::Y,
            DVec3::Z,
            -DVec3::Z,
        ];
        let closest = axes
            .into_iter()
            .map(|axis| q * axis)
            .max_by(|a, b| a.y.total_cmp(&b.y))
            .unwrap();
        held.view.preview.rotation = Some(
            (DQuat::from_rotation_arc(closest, DVec3::Y) * q)
                .normalize()
                .to_array(),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::WorldModel;
    use kokoverse_protocol::{
        AvatarPose, Edit, ParticipantView, PlayerInput, PositionSpace, ScriptAction, ScriptCommand,
        StructuralCommand, StructuralOperation, TransformCommand, WorldCommand,
    };
    use std::time::Duration;
    fn setup() -> WorldModel {
        let mut m = WorldModel::new();
        m.update_participant(ParticipantView {
            avatar_texture: None,
            id: "local".into(),
            name: "Test".into(),
            pose: AvatarPose {
                position: [0.0, 0.0, 3.0],
                yaw: 0.0,
                pitch: 0.0,
            },
        })
        .unwrap();
        m.tick();
        m.enable_local_player("local").unwrap();
        m.set_player_input(PlayerInput {
            active: true,
            face_view: true,
            ..Default::default()
        })
        .unwrap();
        m.advance(Duration::from_millis(17));
        m
    }
    #[test]
    fn view_relative_grab_is_transient_until_release_commit() {
        let mut m = setup();
        let before = m.snapshot();
        let ray = m.interaction_ray().unwrap();
        assert_eq!(ray.target, Some(before.objects[0].address.clone()));
        assert!((ray.point[2] - 0.75).abs() < 0.0001);
        m.begin_grab().unwrap();
        m.set_player_input(PlayerInput {
            active: true,
            face_view: true,
            yaw: 0.4,
            ..Default::default()
        })
        .unwrap();
        m.advance(Duration::from_millis(17));
        let held = m.grab_view().unwrap();
        assert!(m.interaction_ray().unwrap().holding);
        assert!(held.preview.position[0] < -0.5);
        assert_eq!(m.snapshot(), before);
        let old_distance = held.distance;
        m.adjust_grab_distance(0.5);
        m.advance(Duration::from_millis(17));
        assert_eq!(
            m.grab_view().unwrap().distance,
            old_distance + 0.5 * old_distance.max(1.0)
        );
        let (released, local) = m.release_grab().unwrap();
        assert_eq!(local.translation, released.position);
        assert!(m.release_grab().is_none());
        m.enqueue(WorldCommand::Edit {
            edit: Edit {
                command_id: "release".into(),
                target: crate::replication::REGISTRY
                    .field_address(&released.target, "position")
                    .unwrap(),
                field_id: "position".into(),
                expected_revision: released.expected_revision,
                value: serde_json::json!(released.position),
            },
        })
        .unwrap();
        m.advance(Duration::from_millis(17));
        assert_eq!(m.snapshot().objects[0].world_position, released.position);
        assert!(m.grab_view().is_none());
    }
    #[test]
    fn releasing_child_of_rotator_keeps_its_visible_position() {
        use glam::DVec3;

        let mut m = setup();
        let root = m.snapshot().objects[0].address.clone();
        m.enqueue(WorldCommand::Structure {
            command: StructuralCommand {
                command_id: "child".into(),
                operation: StructuralOperation::CreateChild {
                    parent: root.clone(),
                },
            },
        })
        .unwrap();
        let child = m.tick().remove(0).selected.unwrap();
        let object = m
            .snapshot()
            .objects
            .into_iter()
            .find(|o| o.address == child)
            .unwrap();
        let position = object.fields.iter().find(|f| f.id == "position").unwrap();
        m.enqueue(WorldCommand::Edit {
            edit: Edit {
                command_id: "place-child".into(),
                target: crate::replication::REGISTRY
                    .field_address(&child, "position")
                    .unwrap(),
                field_id: "position".into(),
                expected_revision: position.revision.clone(),
                value: serde_json::json!([0.0, 0.0, 2.0]),
            },
        })
        .unwrap();
        m.tick()[0].result.as_ref().unwrap();
        m.advance(Duration::from_millis(17));
        assert_eq!(m.interaction_ray().unwrap().target, Some(child.clone()));
        m.begin_grab().unwrap();
        m.enqueue(WorldCommand::Script {
            command: ScriptCommand {
                command_id: "rotator".into(),
                target: root.clone(),
                action: ScriptAction::Attach {
                    source: crate::scripting::ROTATOR.into(),
                },
            },
        })
        .unwrap();
        m.tick()[0].result.as_ref().unwrap();
        m.set_media_time(102.0);
        m.advance(Duration::from_millis(17));
        m.set_player_input(PlayerInput {
            active: true,
            face_view: true,
            yaw: 0.2,
            ..Default::default()
        })
        .unwrap();
        m.advance(Duration::from_millis(17));
        let (preview, local) = m.release_grab().unwrap();
        assert_eq!(preview.target, child);
        assert!(
            (DVec3::from_array(local.translation) - DVec3::from_array(preview.position)).length()
                > 0.2
        );
        m.enqueue(WorldCommand::Transform {
            command: TransformCommand {
                command_id: "release".into(),
                target: child.clone(),
                expected_position_revision: preview.expected_revision,
                expected_rotation_revision: preview.expected_rotation_revision.unwrap(),
                expected_scale_revision: preview.expected_scale_revision.unwrap(),
                space: PositionSpace::Parent,
                position: local.translation,
                rotation: local.rotation,
                scale: local.scale,
            },
        })
        .unwrap();
        m.tick()[0].result.as_ref().unwrap();
        m.advance(Duration::from_millis(17));
        let shown = m
            .drive_frame()
            .transforms
            .into_iter()
            .find(|t| t.target == child)
            .unwrap();
        assert!(
            (DVec3::from_array(shown.position) - DVec3::from_array(preview.position)).length()
                < 0.1
        );
    }
    #[test]
    fn cancel_and_out_of_range_grab_leave_world_unchanged() {
        let mut m = setup();
        let before = m.snapshot();
        m.begin_grab().unwrap();
        m.set_player_input(PlayerInput {
            active: true,
            face_view: true,
            yaw: 0.5,
            ..Default::default()
        })
        .unwrap();
        m.advance(Duration::from_millis(17));
        m.cancel_grab();
        assert_eq!(m.snapshot(), before);
        m.set_player_input(PlayerInput {
            active: true,
            face_view: true,
            ..Default::default()
        })
        .unwrap();
        m.begin_grab().unwrap();
        m.cancel_grab();
        m.set_player_input(PlayerInput {
            active: true,
            face_view: true,
            yaw: std::f64::consts::PI,
            ..Default::default()
        })
        .unwrap();
        assert!(m.begin_grab().is_err());
        assert!(m.interaction_ray().unwrap().target.is_none());
        assert!(m.grab_view().is_none());
    }

    #[test]
    fn rotation_and_scale_preserve_grip_and_straighten_without_losing_heading() {
        use glam::{DQuat, DVec3};
        let mut m = setup();
        m.begin_grab().unwrap();
        let anchor = m
            .world
            .resource::<super::GrabState>()
            .0
            .as_ref()
            .unwrap()
            .anchor;
        m.rotate_grab([0.4, 0.2], false);
        m.scale_grab(1.5);
        m.advance(Duration::from_millis(17));
        let held = m.grab_view().unwrap();
        let p = &held.preview;
        assert_eq!(p.scale.unwrap(), [1.5; 3]);
        let grip = DVec3::from_array(p.position)
            + DQuat::from_array(p.rotation.unwrap())
                * (anchor * DVec3::from_array(p.scale.unwrap()));
        let ray = m.interaction_ray().unwrap();
        assert!((grip - DVec3::from_array(ray.point)).length() < 1e-6);
        m.align_grab();
        let rotation = DQuat::from_array(m.grab_view().unwrap().preview.rotation.unwrap());
        assert!((rotation * DVec3::Y - DVec3::Y).length() < 1e-6);
        assert!(rotation.dot(DQuat::IDENTITY).abs() < 0.99);
        m.rotate_grab([0.3, 0.9], true);
        let rotation = DQuat::from_array(m.grab_view().unwrap().preview.rotation.unwrap());
        assert!((rotation * DVec3::Y - DVec3::Y).length() < 1e-6);
    }

    #[test]
    fn held_orientation_follows_yaw_and_double_align_toggles_pitch_following() {
        use glam::{DQuat, DVec3};
        let mut m = setup();
        m.begin_grab().unwrap();
        let input = |yaw, pitch| PlayerInput {
            active: true,
            face_view: true,
            yaw,
            pitch,
            ..Default::default()
        };
        m.set_player_input(input(0.4, 0.2)).unwrap();
        m.advance(Duration::from_millis(17));
        let q = DQuat::from_array(m.grab_view().unwrap().preview.rotation.unwrap());
        assert!(q.dot(DQuat::from_rotation_y(0.4)).abs() > 0.99999);
        m.align_grab();
        m.align_grab();
        m.set_player_input(input(0.4, 0.4)).unwrap();
        m.advance(Duration::from_millis(17));
        let q = DQuat::from_array(m.grab_view().unwrap().preview.rotation.unwrap());
        assert!((q * DVec3::Y - DVec3::Y).length() > 0.1);
    }

    #[test]
    fn non_solid_image_is_pickable_and_removing_collider_cancels_grab() {
        let mut m = setup();
        let mut snapshot = m.snapshot();
        let object = &mut snapshot.objects[0];
        let image = image::DynamicImage::new_rgb8(8, 4);
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        use base64::Engine;
        object.visual = crate::validate_image(&format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
        ))
        .unwrap();
        object.world_position = [0.0, 1.15, 0.0];
        object
            .fields
            .iter_mut()
            .find(|f| f.id == "position")
            .unwrap()
            .value = serde_json::json!(object.world_position);
        crate::set_default_collider(object);
        assert!(!object.collider.as_ref().unwrap().solid);
        m.restore(snapshot).unwrap();
        m.begin_grab().unwrap();
        m.rotate_grab([0.3, 0.2], false);
        m.scale_grab(1.1);
        m.advance(Duration::from_millis(17));
        let target = m.grab_view().unwrap().preview.target;
        m.enqueue(WorldCommand::Edit {
            edit: Edit {
                command_id: "remove".into(),
                target: crate::replication::REGISTRY
                    .field_address(&target, "collider")
                    .unwrap(),
                field_id: "collider".into(),
                expected_revision: "0".into(),
                value: serde_json::Value::Null,
            },
        })
        .unwrap();
        m.advance(Duration::from_millis(17));
        assert!(m.grab_view().is_none());
        assert!(m.begin_grab().is_err());
    }
}

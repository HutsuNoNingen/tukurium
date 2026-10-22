//! Local character simulation. Remote participants are replicated poses, never simulated here.
use crate::{
    collision::{self, CollisionWorld},
    grabbing::GrabState,
    presence::{Participant, PresenceCache, PresenceIndex},
};
use bevy_ecs::prelude::*;
use kokoverse_protocol::{CharacterState, PlayerInput};

pub const FIXED_STEP: f64 = 1.0 / 60.0;
const SPEED: f64 = 3.8;
const GRAVITY: f64 = 18.0;
const JUMP_SPEED: f64 = 8.0;
const INPUT_TIMEOUT: f64 = 0.25;

#[derive(Component, Clone)]
pub(crate) struct CharacterMotion(
    pub CharacterState,
    pub Option<(kokoverse_protocol::Address, glam::DAffine3)>,
);

#[derive(Resource, Clone, Default)]
pub(crate) struct LocalControl {
    pub id: String,
    pub input: PlayerInput,
    age: f64,
    jump_pending: bool,
}

pub fn valid_input(input: &PlayerInput) -> bool {
    input
        .movement
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 1.0)
        && input.yaw.is_finite()
        && input.yaw.abs() <= 10000.0
        && input.pitch.is_finite()
        && input.pitch.abs() <= 1.5
}

pub(crate) fn set_input(control: &mut LocalControl, input: PlayerInput) {
    control.jump_pending |= input.active && input.jump && !control.input.jump;
    control.input = input;
    control.age = 0.0;
}

pub(crate) fn stop(control: &mut LocalControl) {
    control.input = PlayerInput::default();
    control.jump_pending = false;
}

pub(crate) fn attach(world: &mut World, id: &str) -> Result<(), String> {
    let entity = *world
        .resource::<PresenceIndex>()
        .0
        .get(id)
        .ok_or("Local participant missing")?;
    let old_id = world.resource::<LocalControl>().id.clone();
    if old_id != id
        && let Some(old) = world.resource::<PresenceIndex>().0.get(&old_id).copied()
    {
        world.entity_mut(old).remove::<CharacterMotion>();
    }
    if !world.entity(entity).contains::<CharacterMotion>() {
        let grounded = world.get::<Participant>(entity).unwrap().0.pose.position[1] <= 0.0;
        world.entity_mut(entity).insert(CharacterMotion(
            CharacterState {
                velocity: [0.0; 3],
                grounded,
            },
            None,
        ));
    }
    world.resource_mut::<LocalControl>().id = id.into();
    Ok(())
}

pub(crate) fn simulate_character(
    mut control: ResMut<LocalControl>,
    mut people: Query<(&mut Participant, &mut CharacterMotion)>,
    mut cache: ResMut<PresenceCache>,
    colliders: Res<CollisionWorld>,
    grab: Res<GrabState>,
) {
    control.age += FIXED_STEP;
    if control.age > INPUT_TIMEOUT {
        stop(&mut control);
    }
    let input = &control.input;
    let ignored = grab
        .0
        .as_ref()
        .filter(|g| !g.view.released)
        .map(|g| &g.view.preview.target);
    for (mut participant, mut motion) in &mut people {
        let before_pose = participant.0.pose.clone();
        let before_motion = motion.0.clone();
        let pose = &mut participant.0.pose;
        // Carry a standing character with the platform before applying their own movement.
        if let Some((address, old_frame)) = motion.1.clone()
            && ignored != Some(&address)
            && let Some(platform) = colliders.0.iter().find(|b| b.address == address)
        {
            let point = glam::DVec3::from_array(pose.position);
            let delta = (platform
                .frame()
                .transform_point3(old_frame.inverse().transform_point3(point))
                - point)
                .to_array();
            collision::move_and_slide(
                &mut pose.position,
                delta,
                &mut motion.0.velocity,
                &colliders.0,
                Some(&address),
            );
        }
        collision::depenetrate(&mut pose.position, &colliders.0, ignored);
        motion.0.grounded = motion.0.velocity[1] <= 0.0
            && (pose.position[1] <= 0.005
                || collision::support(pose.position, &colliders.0, ignored).is_some());
        if motion.0.grounded {
            motion.0.velocity[1] = 0.0;
        }
        let mut movement = if input.active {
            input.movement
        } else {
            [0.0; 2]
        };
        let length = movement[0].hypot(movement[1]).max(1.0);
        movement[0] /= length;
        movement[1] /= length;
        let (sin, cos) = input.yaw.sin_cos();
        let dx = cos * movement[0] + sin * movement[1];
        let dz = -sin * movement[0] + cos * movement[1];
        motion.0.velocity[0] = dx * SPEED;
        motion.0.velocity[2] = dz * SPEED;
        if input.active {
            pose.pitch = input.pitch;
            if input.face_view {
                pose.yaw = input.yaw;
            } else if dx.abs() + dz.abs() > 0.00001 {
                pose.yaw = (-dx).atan2(-dz);
            }
        }
        if control.jump_pending && motion.0.grounded {
            motion.0.velocity[1] = JUMP_SPEED;
            motion.0.grounded = false;
        }
        let mut delta = [
            motion.0.velocity[0] * FIXED_STEP,
            0.0,
            motion.0.velocity[2] * FIXED_STEP,
        ];
        if !motion.0.grounded {
            delta[1] = motion.0.velocity[1] * FIXED_STEP - 0.5 * GRAVITY * FIXED_STEP * FIXED_STEP;
            motion.0.velocity[1] -= GRAVITY * FIXED_STEP;
        }
        collision::move_and_slide(
            &mut pose.position,
            delta,
            &mut motion.0.velocity,
            &colliders.0,
            ignored,
        );
        let standing = collision::support(pose.position, &colliders.0, ignored);
        motion.0.grounded =
            motion.0.velocity[1] <= 0.0 && (pose.position[1] <= 0.005 || standing.is_some());
        motion.1 = standing
            .filter(|_| motion.0.grounded)
            .map(|b| (b.address.clone(), b.frame()));
        if motion.0.grounded {
            if pose.position[1] <= 0.005 {
                pose.position[1] = 0.0;
            }
            motion.0.velocity[1] = 0.0;
        }
        if pose.position[1] <= 0.0 {
            pose.position[1] = 0.0;
            motion.0.velocity[1] = 0.0;
            motion.0.grounded = true;
        } else if pose.position[1] >= 1000.0 {
            pose.position[1] = 1000.0;
            motion.0.velocity[1] = motion.0.velocity[1].min(0.0);
        }
        if before_pose != *pose || before_motion != motion.0 {
            cache.dirty = true;
        }
    }
    control.jump_pending = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldModel;
    use kokoverse_protocol::{AvatarPose, Edit, ParticipantView, WorldCommand};
    use std::time::Duration;

    fn model() -> WorldModel {
        let mut model = WorldModel::new();
        let mut snapshot = model.snapshot();
        snapshot.objects[0].world_position = [20.0, 0.75, 0.0];
        snapshot.objects[0]
            .fields
            .iter_mut()
            .find(|f| f.id == "position")
            .unwrap()
            .value = serde_json::json!([20.0, 0.75, 0.0]);
        model.restore(snapshot).unwrap();
        model
            .update_participant(ParticipantView {
                avatar_texture: None,
                id: "local".into(),
                name: "Test".into(),
                pose: AvatarPose {
                    position: [0.0; 3],
                    yaw: 0.0,
                    pitch: 0.0,
                },
            })
            .unwrap();
        model.tick();
        model.enable_local_player("local").unwrap();
        model
    }
    fn step(model: &mut WorldModel) {
        model.advance(Duration::from_secs_f64(FIXED_STEP));
    }
    fn input() -> PlayerInput {
        PlayerInput {
            active: true,
            ..Default::default()
        }
    }

    #[test]
    fn camera_relative_movement_normalizes_diagonals_and_does_not_revise_world() {
        let mut model = model();
        let mut intent = input();
        intent.movement = [1.0, 1.0];
        model.set_player_input(intent.clone()).unwrap();
        let world = model.snapshot();
        for _ in 0..6 {
            step(&mut model);
        }
        let p = model.local_player().unwrap().pose.position;
        assert!((p[0].hypot(p[2]) - 0.38).abs() < 1e-6);
        intent.movement = [0.0, -1.0];
        intent.yaw = std::f64::consts::FRAC_PI_2;
        model.set_player_input(intent).unwrap();
        step(&mut model);
        let next = model.local_player().unwrap().pose.position;
        assert!(next[0] < p[0] && (next[2] - p[2]).abs() < 1e-6);
        assert_eq!(model.snapshot(), world);
    }

    #[test]
    fn jump_is_edge_triggered_and_gravity_lands_without_a_renderer() {
        let mut model = model();
        let mut intent = input();
        intent.jump = true;
        let mut peak: f64 = 0.0;
        for _ in 0..60 {
            model.set_player_input(intent.clone()).unwrap();
            step(&mut model);
            peak = peak.max(model.local_player().unwrap().pose.position[1]);
        }
        assert!(peak > 1.7 && peak < 1.8);
        assert!(model.local_motion().unwrap().grounded);
        assert_eq!(model.local_player().unwrap().pose.position[1], 0.0);
        intent.jump = false;
        model.set_player_input(intent.clone()).unwrap();
        step(&mut model);
        intent.jump = true;
        model.set_player_input(intent).unwrap();
        step(&mut model);
        assert!(model.local_player().unwrap().pose.position[1] > 0.0);
    }

    #[test]
    fn input_timeout_and_release_stop_walking_but_not_gravity() {
        let mut model = model();
        let mut intent = input();
        intent.movement = [1.0, 0.0];
        intent.jump = true;
        model.set_player_input(intent.clone()).unwrap();
        for _ in 0..60 {
            step(&mut model);
        }
        let p = model.local_player().unwrap().pose.position;
        assert!(p[0] > 0.8 && p[0] <= 0.951);
        assert_eq!(p[1], 0.0);
        assert!(model.local_motion().unwrap().grounded);
        model.set_player_input(intent).unwrap();
        step(&mut model);
        let stopped = model.local_player().unwrap().pose.position[0];
        model.stop_player_input();
        for _ in 0..60 {
            step(&mut model);
        }
        assert_eq!(model.local_player().unwrap().pose.position[0], stopped);
        assert_eq!(model.local_player().unwrap().pose.position[1], 0.0);
    }

    #[test]
    fn editing_and_snapshot_restore_preserve_airborne_character_state() {
        let mut model = model();
        let mut intent = input();
        intent.jump = true;
        model.set_player_input(intent).unwrap();
        step(&mut model);
        let player = model.local_player();
        let motion = model.local_motion();
        let object = model.snapshot().objects.remove(0);
        for n in 0..80 {
            model
                .enqueue(WorldCommand::Edit {
                    edit: Edit {
                        command_id: n.to_string(),
                        target: object.address.clone(),
                        field_id: "name".into(),
                        expected_revision: "0".into(),
                        value: serde_json::json!("Edit"),
                    },
                })
                .unwrap();
        }
        model.tick();
        assert_eq!(model.local_player(), player);
        assert_eq!(model.local_motion(), motion);
        model.restore(model.snapshot()).unwrap();
        assert_eq!(model.local_player(), player);
        assert_eq!(model.local_motion(), motion);
        step(&mut model);
        assert!(model.local_player().unwrap().pose.position[1] > player.unwrap().pose.position[1]);
    }

    #[test]
    fn remote_pose_is_not_simulated_and_invalid_input_is_atomic() {
        let mut model = model();
        let remote = ParticipantView {
            avatar_texture: None,
            id: "peer".into(),
            name: "Remote".into(),
            pose: AvatarPose {
                position: [2.0, 5.0, 2.0],
                yaw: 0.0,
                pitch: 0.0,
            },
        };
        model.update_participant(remote.clone()).unwrap();
        model.tick();
        let mut intent = input();
        intent.movement[0] = 2.0;
        assert!(model.set_player_input(intent).is_err());
        for _ in 0..60 {
            step(&mut model);
        }
        assert!(model.participants().contains(&remote));
        assert_eq!(model.local_player().unwrap().pose.position, [0.0; 3]);
    }
}

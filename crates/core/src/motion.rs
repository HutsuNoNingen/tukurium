//! Small host-authoritative X/Z motion system for board games.
use crate::resources::*;
use bevy_ecs::prelude::*;
use glam::{DQuat, DVec3};
use kokoverse_protocol::ObjectView;

pub const STEP: f64 = 0.05;

#[derive(Resource, Default)]
pub(crate) struct MotionStep(pub f64);

pub(crate) struct MotionPatch {
    pub entity: Entity,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
}

pub(crate) fn prepare_frame(world: &mut World) {
    let dt = world.resource::<MotionStep>().0;
    if dt <= 0.0 || world.resource::<WorldRevision>().0 == u64::MAX { return; }
    let snapshot = world.resource::<SnapshotCache>().0.clone();
    let mut patches = Vec::new();
    for object in &snapshot.objects {
        let Some(motion) = object.planar_motion.as_ref().filter(|m| m.enabled) else { continue; };
        if motion.velocity[0] == 0.0 && motion.velocity[2] == 0.0 { continue; }
        let (position, velocity) = step(object, &snapshot.objects, dt);
        if position == object.world_position && velocity == motion.velocity { continue; }
        let entity = world.resource::<EntityIndex>().0[&object.address];
        patches.push(MotionPatch { entity, position, velocity });
    }
    if !patches.is_empty() {
        world.resource_mut::<Prepared>().0 = Some(PreparedCommand {
            ticket: 0,
            mutation: Ok(Mutation::MotionFrame { patches }),
        });
    }
}

fn step(object: &ObjectView, objects: &[ObjectView], dt: f64) -> ([f64; 3], [f64; 3]) {
    let motion = object.planar_motion.as_ref().unwrap();
    let half = object.collider.as_ref().map(|c| c.half_extents).unwrap_or([0.1; 3]);
    let radius = half[0].max(half[2]) * object.world_scale[0].max(object.world_scale[2]);
    let mut position = object.world_position;
    let mut velocity = motion.velocity;
    let mut remaining = dt;
    let bounds = motion.bounds;
    for _ in 0..4 {
        if remaining <= 1e-6 || velocity[0].hypot(velocity[2]) < 0.01 { break; }
        let delta = [velocity[0] * remaining, 0.0, velocity[2] * remaining];
        let mut hit: Option<(f64, [f64; 3])> = None;
        for (axis, lo, hi) in [(0, bounds[0] + radius, bounds[1] - radius), (2, bounds[2] + radius, bounds[3] - radius)] {
            if lo >= hi { continue; }
            let endpoint = position[axis] + delta[axis];
            let candidate = if delta[axis] < -1e-9 && endpoint < lo {
                Some(((lo - position[axis]) / delta[axis], if axis == 0 { [1.0,0.0,0.0] } else { [0.0,0.0,1.0] }))
            } else if delta[axis] > 1e-9 && endpoint > hi {
                Some(((hi - position[axis]) / delta[axis], if axis == 0 { [-1.0,0.0,0.0] } else { [0.0,0.0,-1.0] }))
            } else { None };
            if let Some((t, normal)) = candidate && (0.0..=1.0).contains(&t) && hit.is_none_or(|h| t < h.0) {
                hit = Some((t, normal));
            }
        }
        for other in objects.iter().filter(|o| o.address != object.address && o.collider.as_ref().is_some_and(|c| c.solid)) {
            if let Some((t, normal)) = sweep(other, position, delta, radius, half[1] * object.world_scale[1])
                && hit.is_none_or(|h| t < h.0) { hit = Some((t, normal)); }
        }
        if let Some((t, normal)) = hit {
            let t = t.clamp(0.0, 1.0);
            position[0] += delta[0] * t + normal[0] * 0.0001;
            position[2] += delta[2] * t + normal[2] * 0.0001;
            let dot = velocity[0] * normal[0] + velocity[2] * normal[2];
            velocity[0] = (velocity[0] - (1.0 + motion.bounce) * dot * normal[0]).clamp(-50.0, 50.0);
            velocity[2] = (velocity[2] - (1.0 + motion.bounce) * dot * normal[2]).clamp(-50.0, 50.0);
            remaining *= 1.0 - t;
        } else {
            position[0] += delta[0];
            position[2] += delta[2];
            break;
        }
    }
    let decay = (-motion.drag * dt).exp();
    velocity[0] *= decay;
    velocity[2] *= decay;
    if velocity[0].hypot(velocity[2]) < 0.01 { velocity = [0.0; 3]; }
    position[0] = position[0].clamp(-1000.0, 1000.0);
    position[2] = position[2].clamp(-1000.0, 1000.0);
    (position, velocity)
}

fn sweep(other: &ObjectView, position: [f64; 3], delta: [f64; 3], radius: f64, half_y: f64) -> Option<(f64, [f64; 3])> {
    let collider = other.collider.as_ref()?;
    let rotation = DQuat::from_array(other.world_rotation).normalize();
    let inverse = rotation.conjugate();
    let center = DVec3::from_array(other.world_position);
    let local = inverse * (DVec3::from_array(position) - center);
    let direction = inverse * DVec3::from_array(delta);
    let half = DVec3::from_array(collider.half_extents) * DVec3::from_array(other.world_scale);
    let inflated = half + DVec3::new(radius, half_y, radius);
    let (t, normal) = crate::collision::ray_box(local.to_array(), direction.to_array(), (-inflated).to_array(), inflated.to_array(), 1.0)?;
    let normal = rotation * DVec3::from_array(normal);
    let horizontal = DVec3::new(normal.x, 0.0, normal.z);
    if horizontal.length_squared() < 0.25 { return None; }
    Some((t, horizontal.normalize().to_array()))
}

#[cfg(test)]
mod tests {
    use crate::{WorldModel, new_object, replication::REGISTRY};
    use kokoverse_protocol::{Address, ScriptAction, ScriptCommand, TypedReference, WorldCommand};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::time::Duration;

    #[test]
    fn planar_motion_bounces_off_a_paddle_and_replicates() {
        let mut host = WorldModel::new();
        let mut snapshot = host.snapshot();
        snapshot.objects.push(new_object("Paddle".into(), [2.0,0.75,0.0], [1.0;4]));
        REGISTRY.change_component(&mut snapshot.objects[0], "PlanarMotion", Some(&json!({
            "planar_enabled":true,
            "planar_velocity":[5.0,0.0,0.0],
            "planar_drag":0.0,
            "planar_bounce":1.0,
            "planar_bounds":[-10.0,10.0,-10.0,10.0]
        }))).unwrap();
        host.restore(snapshot).unwrap();
        let mut guest = WorldModel::new();
        guest.restore(host.snapshot()).unwrap();
        host.set_script_authority(true);
        for _ in 0..6 {
            for outcome in host.advance(Duration::from_secs_f64(super::STEP)) {
                guest.apply_commit(outcome.result.unwrap()).unwrap();
            }
        }
        let puck = host.snapshot().objects.into_iter().find(|o| o.planar_motion.is_some()).unwrap();
        assert!(puck.planar_motion.as_ref().unwrap().velocity[0] < 0.0);
        assert!(puck.world_position[0] < 0.5);
        assert_eq!(guest.snapshot(), host.snapshot());
        let encoded = REGISTRY.encode_world(&host.snapshot());
        assert_eq!(REGISTRY.decode_world(encoded).unwrap(), host.snapshot());
        let mut restored = WorldModel::new();
        restored.restore(host.snapshot()).unwrap();
        assert_eq!(restored.snapshot(), host.snapshot());
    }

    #[test]
    fn disabled_planar_motion_stays_still() {
        let mut model = WorldModel::new();
        let mut snapshot = model.snapshot();
        REGISTRY.change_component(&mut snapshot.objects[0], "PlanarMotion", Some(&json!({
            "planar_enabled":false,
            "planar_velocity":[2.0,0.0,0.0],
            "planar_drag":0.0,
            "planar_bounce":0.9,
            "planar_bounds":[-3.0,3.0,-5.0,5.0]
        }))).unwrap();
        model.restore(snapshot).unwrap();
        model.set_script_authority(true);
        let before = model.snapshot();
        assert!(model.advance(Duration::from_millis(100)).is_empty());
        assert_eq!(model.snapshot(), before);
    }

    #[test]
    fn script_write_launches_planar_motion() {
        let mut host = WorldModel::new();
        let mut snapshot = host.snapshot();
        let puck = snapshot.objects[0].address.clone();
        REGISTRY.change_component(&mut snapshot.objects[0], "PlanarMotion", Some(&json!({
            "planar_enabled":true,
            "planar_velocity":[0.0,0.0,0.0],
            "planar_drag":0.0,
            "planar_bounce":1.0,
            "planar_bounds":[-10.0,10.0,-10.0,10.0]
        }))).unwrap();
        host.restore(snapshot).unwrap();
        let source = "return {name='Launcher',fields={velocity={type='field_ref'}},on_interact=function(ctx) ctx:write_field(ctx.self.Launcher.velocity,{5,0,0}) end}";
        host.enqueue(WorldCommand::Script { command: ScriptCommand {
            command_id: "attach-launcher".into(), target: puck.clone(),
            action: ScriptAction::Attach { source: source.into() },
        }}).unwrap();
        host.tick()[0].result.as_ref().unwrap();
        let script = host.snapshot().objects.iter().find(|o| o.address == puck).unwrap().scripts[0].clone();
        let reference = TypedReference::Field {
            target: Address { instance_id: puck.instance_id.clone(), local_id: 19 },
            field_id: "planar_velocity".into(),
        };
        host.enqueue(WorldCommand::Script { command: ScriptCommand {
            command_id: "bind-launcher".into(), target: puck.clone(),
            action: ScriptAction::Update {
                script_id: script.id, expected_revision: script.revision, source: source.into(), enabled: true,
                parameters: BTreeMap::from([("velocity".into(), serde_json::to_value(reference).unwrap())]),
            },
        }}).unwrap();
        host.tick()[0].result.as_ref().unwrap();
        host.set_script_authority(true);
        host.enqueue(WorldCommand::Script { command: ScriptCommand {
            command_id: "launch".into(), target: puck.clone(), action: ScriptAction::Interact,
        }}).unwrap();
        host.advance(Duration::ZERO);
        assert_eq!(host.snapshot().objects.iter().find(|o| o.address == puck).unwrap().planar_motion.as_ref().unwrap().velocity, [5.0,0.0,0.0]);
        host.advance(Duration::from_secs_f64(super::STEP));
        assert!(host.snapshot().objects.iter().find(|o| o.address == puck).unwrap().world_position[0] > 0.0);
    }
}

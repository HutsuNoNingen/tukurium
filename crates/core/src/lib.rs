//! A renderer-independent ECS world with a scheduled command/transform/output pipeline.
mod collision;
mod components;
mod grabbing;
pub mod locomotion;
pub mod presence;
mod resources;
pub mod scripting;
pub mod systems;

use bevy_ecs::prelude::*;
pub use components::*;
use kokoverse_protocol::{Address, Field, ObjectView, Snapshot, WorldCommand, WorldCommit};
pub use resources::CommandOutcome;
use resources::*;
use serde_json::{Value, json};
use std::collections::HashSet;
use uuid::Uuid;

pub const MAX_OBJECTS: usize = 128;
pub(crate) const MAX_IMAGE_BYTES: usize = 192 * 1024;
pub(crate) const MAX_IMAGE_WORLD: usize = 512 * 1024;
const MAX_QUEUED: usize = 256;

pub struct WorldModel {
    world: World,
    schedule: Schedule,
    next_ticket: u64,
    simulation: Schedule,
    accumulator: f64,
    script_schedule: Schedule,
    script_accumulator: f64,
    script_authority: bool,
}

impl Default for WorldModel {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldModel {
    fn empty() -> Self {
        let mut world = World::new();
        world.init_resource::<EntityIndex>();
        world.init_resource::<WorldRevision>();
        world.init_resource::<MediaClock>();
        world.init_resource::<InputQueue>();
        world.init_resource::<Prepared>();
        world.init_resource::<Applied>();
        world.init_resource::<ChangeSet>();
        world.init_resource::<OutputQueue>();
        world.init_resource::<presence::PresenceInputs>();
        world.init_resource::<presence::PresenceIndex>();
        world.init_resource::<presence::PresenceCache>();
        world.init_resource::<locomotion::LocalControl>();
        world.init_resource::<collision::CollisionWorld>();
        world.init_resource::<collision::PreviewPositions>();
        world.init_resource::<grabbing::GrabState>();
        world.init_resource::<scripting::ScriptRuntime>();
        world.insert_resource(SnapshotCache(Snapshot {
            world_revision: "0".into(),
            objects: vec![],
        }));
        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                systems::validate_commands,
                systems::apply_commands,
                presence::apply_presence,
                systems::update_transforms,
                systems::collect_changes,
                systems::extract_snapshot,
                systems::publish_changes,
                presence::extract_presence,
            )
                .chain(),
        );
        let mut simulation = Schedule::default();
        simulation.add_systems(
            (
                grabbing::update_grab,
                collision::sync_colliders,
                locomotion::simulate_character,
                presence::extract_presence,
            )
                .chain(),
        );
        let mut script_schedule = Schedule::default();
        script_schedule.add_systems(
            (
                scripting::prepare_frame,
                systems::apply_commands,
                systems::update_transforms,
                systems::collect_changes,
                systems::extract_snapshot,
                systems::publish_changes,
            )
                .chain(),
        );
        Self {
            world,
            schedule,
            next_ticket: 0,
            simulation,
            accumulator: 0.0,
            script_schedule,
            script_accumulator: 0.0,
            script_authority: false,
        }
    }

    pub fn new() -> Self {
        let mut model = Self::empty();
        insert_object(
            &mut model.world,
            new_object(
                "Shared cube".into(),
                [0.0, 0.75, 0.0],
                [0.2, 0.65, 0.8, 1.0],
            ),
        );
        model.schedule.run(&mut model.world);
        model
    }

    /// Enqueue intent only. Nothing persistent changes until tick() runs the schedule.
    pub fn enqueue(&mut self, command: WorldCommand) -> Result<u64, String> {
        if self.world.resource::<InputQueue>().0.len() >= MAX_QUEUED {
            return Err("Editor command queue is full".into());
        }
        self.next_ticket = self
            .next_ticket
            .checked_add(1)
            .ok_or("Command tickets exhausted")?;
        self.world
            .resource_mut::<InputQueue>()
            .0
            .push_back((self.next_ticket, command));
        Ok(self.next_ticket)
    }

    /// Ordered command boundaries preserve per-field conflicts within one engine tick.
    pub fn tick(&mut self) -> Vec<CommandOutcome> {
        let count = self.world.resource::<InputQueue>().0.len().clamp(1, 128);
        for _ in 0..count {
            self.schedule.run(&mut self.world);
        }
        std::mem::take(&mut self.world.resource_mut::<OutputQueue>().0)
    }

    /// Editing passes do not advance time. Character simulation has its own fixed steps.
    pub fn advance(&mut self, elapsed: std::time::Duration) -> Vec<CommandOutcome> {
        let mut outcomes = self.tick();
        self.accumulator += elapsed.as_secs_f64().min(0.25);
        while self.accumulator + 1e-9 >= locomotion::FIXED_STEP {
            self.simulation.run(&mut self.world);
            self.accumulator -= locomotion::FIXED_STEP;
        }
        if self.script_authority {
            self.script_accumulator += elapsed.as_secs_f64().min(0.25);
            let interact = !self
                .world
                .resource::<scripting::ScriptRuntime>()
                .events
                .is_empty();
            if self.script_accumulator >= 0.05 || interact {
                let dt = if self.script_accumulator >= 0.05 {
                    std::mem::take(&mut self.script_accumulator)
                } else {
                    0.0
                };
                self.world.resource_mut::<scripting::ScriptRuntime>().dt = dt;
                self.script_schedule.run(&mut self.world);
                outcomes.extend(std::mem::take(
                    &mut self.world.resource_mut::<OutputQueue>().0,
                ));
            }
        }
        outcomes
    }

    pub fn set_script_authority(&mut self, enabled: bool) {
        self.script_authority = enabled;
    }
    pub fn script_statuses(&self) -> Vec<kokoverse_protocol::ScriptStatus> {
        self.world
            .resource::<scripting::ScriptRuntime>()
            .statuses
            .clone()
    }

    pub fn enable_local_player(&mut self, id: &str) -> Result<(), String> {
        locomotion::attach(&mut self.world, id)
    }

    pub fn local_player(&self) -> Option<kokoverse_protocol::ParticipantView> {
        let id = &self.world.resource::<locomotion::LocalControl>().id;
        let entity = self.world.resource::<presence::PresenceIndex>().0.get(id)?;
        Some(self.world.get::<presence::Participant>(*entity)?.0.clone())
    }

    pub fn local_motion(&self) -> Option<kokoverse_protocol::CharacterState> {
        let id = &self.world.resource::<locomotion::LocalControl>().id;
        let entity = self.world.resource::<presence::PresenceIndex>().0.get(id)?;
        Some(
            self.world
                .get::<locomotion::CharacterMotion>(*entity)?
                .0
                .clone(),
        )
    }

    pub fn set_player_input(
        &mut self,
        input: kokoverse_protocol::PlayerInput,
    ) -> Result<(), String> {
        if !locomotion::valid_input(&input) {
            return Err("Invalid player input".into());
        }
        locomotion::set_input(
            &mut self.world.resource_mut::<locomotion::LocalControl>(),
            input,
        );
        Ok(())
    }

    pub fn stop_player_input(&mut self) {
        locomotion::stop(&mut self.world.resource_mut::<locomotion::LocalControl>());
    }

    pub fn set_collision_previews(&mut self, previews: Vec<kokoverse_protocol::ObjectPreview>) {
        self.world.resource_mut::<collision::PreviewPositions>().0 = previews;
    }

    pub fn begin_grab(&mut self) -> Result<(), String> {
        // Input can arrive between simulation steps, including immediately after cancellation.
        let mut refresh = Schedule::default();
        refresh.add_systems(collision::sync_colliders);
        refresh.run(&mut self.world);
        grabbing::begin(&mut self.world)
    }

    pub fn grab_view(&self) -> Option<kokoverse_protocol::GrabView> {
        self.world
            .resource::<grabbing::GrabState>()
            .0
            .as_ref()
            .map(|held| held.view.clone())
    }

    pub fn adjust_grab_distance(&mut self, delta: f64) {
        if delta.is_finite()
            && delta.abs() <= 1.0
            && let Some(held) = self.world.resource_mut::<grabbing::GrabState>().0.as_mut()
            && !held.view.released
        {
            held.view.distance =
                (held.view.distance + delta * held.view.distance.max(1.0)).clamp(0.5, 6.0);
        }
    }

    pub fn release_grab(&mut self) -> Option<kokoverse_protocol::ObjectPreview> {
        let mut update = Schedule::default();
        update.add_systems(grabbing::update_grab);
        update.run(&mut self.world);
        let mut grab = self.world.resource_mut::<grabbing::GrabState>();
        let held = grab.0.as_mut()?;
        if held.view.released {
            return None;
        }
        held.view.released = true;
        Some(held.view.preview.clone())
    }

    pub fn cancel_grab(&mut self) {
        self.world.resource_mut::<grabbing::GrabState>().0 = None;
    }
    pub fn rotate_grab(&mut self, delta: [f64; 2], vertical: bool) {
        grabbing::rotate(&mut self.world, delta, vertical);
    }
    pub fn scale_grab(&mut self, factor: f64) {
        grabbing::scale(&mut self.world, factor);
    }
    pub fn align_grab(&mut self) {
        grabbing::align(&mut self.world);
    }

    pub fn interaction_ray(&self) -> Option<kokoverse_protocol::InteractionRay> {
        grabbing::interaction_ray(&self.world)
    }

    pub fn snapshot(&self) -> Snapshot {
        self.world.resource::<SnapshotCache>().0.clone()
    }
    pub fn set_media_time(&mut self, time: f64) {
        if time.is_finite() && time >= 0.0 {
            self.world.resource_mut::<MediaClock>().0 = time;
        }
    }

    pub fn accepts_preview(&self, preview: &kokoverse_protocol::ObjectPreview) -> bool {
        if preview
            .rotation
            .is_some_and(|r| validate_value("rotation", &json!(r)).is_err())
            || preview
                .scale
                .is_some_and(|s| validate_value("scale", &json!(s)).is_err())
            || preview.rotation.is_some() != preview.expected_rotation_revision.is_some()
            || preview.scale.is_some() != preview.expected_scale_revision.is_some()
        {
            return false;
        }
        preview
            .position
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1000.0)
            && self
                .world
                .resource::<SnapshotCache>()
                .0
                .objects
                .iter()
                .any(|object| {
                    object.address == preview.target
                        && object.fields.iter().any(|field| {
                            field.id == "position" && field.revision == preview.expected_revision
                        })
                        && preview.expected_rotation_revision.as_ref().is_none_or(|r| {
                            object
                                .fields
                                .iter()
                                .any(|f| f.id == "rotation" && &f.revision == r)
                        })
                        && preview.expected_scale_revision.as_ref().is_none_or(|r| {
                            object
                                .fields
                                .iter()
                                .any(|f| f.id == "scale" && &f.revision == r)
                        })
                })
    }

    pub fn update_participant(
        &mut self,
        participant: kokoverse_protocol::ParticipantView,
    ) -> Result<(), String> {
        if participant.id.is_empty()
            || participant.id.len() > 256
            || participant.name.trim().is_empty()
            || participant.name.chars().count() > 40
            || !presence::valid_pose(&participant.pose)
            || participant
                .avatar_texture
                .as_ref()
                .is_some_and(|id| !kokoverse_protocol::valid_asset_id(id))
        {
            return Err("Invalid participant pose or name".into());
        }
        let mut queue = self.world.resource_mut::<presence::PresenceInputs>();
        // Coalesce high-frequency pose inputs before the next ECS tick.
        queue.0.retain(
            |input| !matches!(input, presence::PresenceInput::Upsert(p) if p.id == participant.id),
        );
        if queue.0.len() >= 64 {
            return Err("Presence queue full".into());
        }
        queue.0.push(presence::PresenceInput::Upsert(participant));
        Ok(())
    }
    pub fn remove_participant(&mut self, id: &str) {
        let mut queue = self.world.resource_mut::<presence::PresenceInputs>();
        queue
            .0
            .retain(|input| !matches!(input, presence::PresenceInput::Upsert(p) if p.id == id));
        if queue.0.len() < 64 {
            queue.0.push(presence::PresenceInput::Remove(id.into()));
        }
    }
    pub fn participants(&self) -> Vec<kokoverse_protocol::ParticipantView> {
        self.world
            .resource::<presence::PresenceCache>()
            .participants
            .clone()
    }
    pub fn presence_revision(&self) -> u64 {
        self.world.resource::<presence::PresenceCache>().revision
    }

    /// Validate everything before touching the live ECS. Used at initial synchronization.
    pub fn restore(&mut self, mut snapshot: Snapshot) -> Result<(), String> {
        // Upgrade saved position-only worlds without changing their revisions or stable IDs.
        for object in &mut snapshot.objects {
            if object.fields.len() == 3
                && ["name", "position", "color"]
                    .iter()
                    .all(|id| object.fields.iter().any(|f| f.id == *id))
            {
                set_default_collider(object);
                sync_object_fields(object);
            }
            if object.fields.len() == 6 && !object.fields.iter().any(|f| f.id == "playback") {
                initialize_playback(object, self.world.resource::<MediaClock>().0);
                sync_object_fields(object);
            }
        }
        validate_snapshot(&snapshot)?;
        let mut replacement = Self::empty();
        *replacement.world.resource_mut::<MediaClock>() = *self.world.resource::<MediaClock>();
        for participant in self.participants() {
            replacement.update_participant(participant)?;
        }
        replacement.world.resource_mut::<WorldRevision>().0 =
            snapshot.world_revision.parse().unwrap();
        replacement.world.resource_mut::<SnapshotCache>().0 = snapshot.clone();
        for object in snapshot.objects {
            insert_object(&mut replacement.world, object);
        }
        replacement.schedule.run(&mut replacement.world);
        let control = self.world.resource::<locomotion::LocalControl>().clone();
        if !control.id.is_empty() {
            replacement.enable_local_player(&control.id)?;
            let old_entity = self.world.resource::<presence::PresenceIndex>().0[&control.id];
            if let Some(motion) = self
                .world
                .get::<locomotion::CharacterMotion>(old_entity)
                .cloned()
            {
                let entity = replacement.world.resource::<presence::PresenceIndex>().0[&control.id];
                replacement.world.entity_mut(entity).insert(motion);
            }
            *replacement.world.resource_mut::<locomotion::LocalControl>() = control;
        }
        replacement.accumulator = self.accumulator;
        *replacement
            .world
            .resource_mut::<collision::PreviewPositions>() =
            self.world.resource::<collision::PreviewPositions>().clone();
        *replacement.world.resource_mut::<grabbing::GrabState>() =
            self.world.resource::<grabbing::GrabState>().clone();
        *self = replacement;
        Ok(())
    }

    /// Apply validated per-object changes without rebuilding unaffected entities.
    pub fn apply_commit(&mut self, commit: WorldCommit) -> Result<(), String> {
        let next = self
            .world
            .resource::<WorldRevision>()
            .0
            .checked_add(1)
            .ok_or("World revision exhausted")?;
        if commit.world_revision.parse::<u64>().ok() != Some(next) {
            return Err("Missing world revision".into());
        }
        if !self.world.resource::<InputQueue>().0.is_empty() {
            return Err("Cannot replicate with queued authoring commands".into());
        }
        let mut candidate = self.snapshot();
        let mut seen = HashSet::new();
        for address in &commit.removed {
            if !seen.insert(address.clone())
                || !candidate.objects.iter().any(|o| &o.address == address)
            {
                return Err("Invalid removed object".into());
            }
            candidate.objects.retain(|o| &o.address != address);
        }
        for object in &commit.upserts {
            if !seen.insert(object.address.clone()) {
                return Err("Duplicate commit address".into());
            }
            candidate.objects.retain(|o| o.address != object.address);
            candidate.objects.push(object.clone());
        }
        if seen.is_empty() {
            return Err("Empty commit".into());
        }
        candidate.world_revision = commit.world_revision.clone();
        validate_snapshot(&candidate)?;
        for address in &commit.removed {
            remove_object(&mut self.world, address);
            self.world
                .resource_mut::<ChangeSet>()
                .removed
                .push(address.clone());
        }
        for object in commit.upserts {
            if self
                .world
                .resource::<EntityIndex>()
                .0
                .contains_key(&object.address)
            {
                remove_object(&mut self.world, &object.address);
            }
            insert_object(&mut self.world, object);
        }
        self.world.resource_mut::<WorldRevision>().0 = next;
        self.schedule.run(&mut self.world);
        Ok(())
    }
}

pub(crate) fn field(
    id: &str,
    label: &str,
    value_type: &str,
    value: Value,
    revs: &Revisions,
) -> Field {
    Field {
        id: id.into(),
        label: label.into(),
        value_type: value_type.into(),
        revision: revs.0.get(id).unwrap_or(&0).to_string(),
        value,
    }
}

pub(crate) fn new_object(name: String, position: [f64; 3], color: [f64; 4]) -> ObjectView {
    let id = Uuid::new_v4().to_string();
    ObjectView {
        address: Address {
            instance_id: id.clone(),
            local_id: 1,
        },
        renderer: Address {
            instance_id: id,
            local_id: 2,
        },
        object_revision: "0".into(),
        world_position: position,
        world_rotation: kokoverse_protocol::identity_rotation(),
        world_scale: kokoverse_protocol::unit_scale(),
        collider: Some(kokoverse_protocol::ColliderSpec {
            half_extents: [0.75; 3],
            solid: true,
        }),
        visual: kokoverse_protocol::Visual::Cube,
        playback: None,
        scripts: Vec::new(),
        fields: vec![
            field("name", "Name", "string", json!(name), &Revisions::default()),
            field(
                "position",
                "Position",
                "vec3",
                json!(position),
                &Revisions::default(),
            ),
            field(
                "color",
                "Color",
                "color",
                json!(color),
                &Revisions::default(),
            ),
            field(
                "rotation",
                "Rotation",
                "quat",
                json!(kokoverse_protocol::identity_rotation()),
                &Revisions::default(),
            ),
            field(
                "scale",
                "Scale",
                "vec3",
                json!([1.0, 1.0, 1.0]),
                &Revisions::default(),
            ),
            field(
                "collider",
                "Collider",
                "collider",
                json!({"half_extents":[0.75,0.75,0.75],"solid":true}),
                &Revisions::default(),
            ),
            field(
                "playback",
                "Playback",
                "playback",
                Value::Null,
                &Revisions::default(),
            ),
        ],
    }
}

pub(crate) fn set_default_collider(object: &mut ObjectView) {
    let half_extents = match &object.visual {
        kokoverse_protocol::Visual::Cube => [0.75; 3],
        kokoverse_protocol::Visual::Image { width, height, .. }
        | kokoverse_protocol::Visual::Video { width, height, .. } => {
            let ratio = *width as f64 / *height as f64;
            if ratio >= 1.0 {
                [1.0, 1.0 / ratio, 0.03]
            } else {
                [ratio, 1.0, 0.03]
            }
        }
        kokoverse_protocol::Visual::Model { half_extents, .. } => *half_extents,
    };
    object.collider = Some(kokoverse_protocol::ColliderSpec {
        half_extents,
        solid: !matches!(
            object.visual,
            kokoverse_protocol::Visual::Image { .. } | kokoverse_protocol::Visual::Video { .. }
        ),
    });
    sync_object_fields(object);
}

pub(crate) fn sync_object_fields(object: &mut ObjectView) {
    for (id, label, kind, value) in [
        ("rotation", "Rotation", "quat", json!(object.world_rotation)),
        ("scale", "Scale", "vec3", json!(object.world_scale)),
        ("collider", "Collider", "collider", json!(object.collider)),
        ("playback", "Playback", "playback", json!(object.playback)),
    ] {
        if let Some(f) = object.fields.iter_mut().find(|f| f.id == id) {
            f.value = value;
        } else {
            object
                .fields
                .push(field(id, label, kind, value, &Revisions::default()));
        }
    }
}

pub(crate) fn initialize_playback(object: &mut ObjectView, now: f64) {
    object.playback = matches!(object.visual, kokoverse_protocol::Visual::Video { .. }).then_some(
        kokoverse_protocol::VideoPlayback {
            playing: true,
            looping: true,
            position_seconds: 0.0,
            anchor_seconds: now,
        },
    );
}

fn validate_snapshot(snapshot: &Snapshot) -> Result<(), String> {
    scripting::validate_world_scripts(snapshot.objects.iter().map(|o| o.scripts.as_slice()))?;
    let revision = snapshot
        .world_revision
        .parse::<u64>()
        .map_err(|_| "Invalid world revision")?;
    if snapshot.objects.len() > MAX_OBJECTS {
        return Err("Object limit exceeded".into());
    }
    let mut ids = HashSet::new();
    let mut image_bytes = 0;
    for object in &snapshot.objects {
        validate_visual(&object.visual)?;
        if let kokoverse_protocol::Visual::Image {
            data_url,
            width,
            height,
        } = &object.visual
        {
            image_bytes += data_url.len();
            if *width == 0 || *height == 0 || image_bytes > MAX_IMAGE_WORLD {
                return Err("World image budget exceeded (512 KiB encoded)".into());
            }
        }
        if Uuid::parse_str(&object.address.instance_id).is_err()
            || object.address.local_id != 1
            || object.renderer.instance_id != object.address.instance_id
            || object.renderer.local_id != 2
            || !ids.insert(&object.address)
        {
            return Err("Invalid ObjectRoot ownership".into());
        }
        let object_revision = object
            .object_revision
            .parse::<u64>()
            .map_err(|_| "Invalid object revision")?;
        if object_revision > revision
            || object.fields.len() != 7
            || matches!(object.visual, kokoverse_protocol::Visual::Video { .. })
                != object.playback.is_some()
        {
            return Err("Invalid object schema".into());
        }
        let mut fields = HashSet::new();
        for f in &object.fields {
            validate_value(&f.id, &f.value)?;
            let field_revision = f
                .revision
                .parse::<u64>()
                .map_err(|_| "Invalid field revision")?;
            let expected_type = match f.id.as_str() {
                "name" => "string",
                "position" => "vec3",
                "rotation" => "quat",
                "scale" => "vec3",
                "collider" => "collider",
                "playback" => "playback",
                _ => "color",
            };
            if field_revision > object_revision
                || f.value_type != expected_type
                || !fields.insert(&f.id)
            {
                return Err("Invalid field schema".into());
            }
            if f.id == "position"
                && serde_json::from_value::<[f64; 3]>(f.value.clone()).unwrap()
                    != object.world_position
            {
                return Err("Derived transform does not match local transform".into());
            }
            if (f.id == "rotation"
                && serde_json::from_value::<[f64; 4]>(f.value.clone()).unwrap()
                    != object.world_rotation)
                || (f.id == "scale"
                    && serde_json::from_value::<[f64; 3]>(f.value.clone()).unwrap()
                        != object.world_scale)
                || (f.id == "collider"
                    && serde_json::from_value::<Option<kokoverse_protocol::ColliderSpec>>(
                        f.value.clone(),
                    )
                    .unwrap()
                        != object.collider)
                || (f.id == "playback"
                    && serde_json::from_value::<Option<kokoverse_protocol::VideoPlayback>>(
                        f.value.clone(),
                    )
                    .unwrap()
                        != object.playback)
            {
                return Err("Derived transform or collider does not match component fields".into());
            }
        }
    }
    Ok(())
}

pub fn validate_visual(visual: &kokoverse_protocol::Visual) -> Result<(), String> {
    match visual {
        kokoverse_protocol::Visual::Cube => Ok(()),
        kokoverse_protocol::Visual::Image { data_url, .. } => {
            if &validate_image(data_url)? == visual {
                Ok(())
            } else {
                Err("Invalid image dimensions".into())
            }
        }
        kokoverse_protocol::Visual::Model {
            asset_id,
            half_extents,
        } => {
            if asset_id.len() == 64
                && asset_id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && half_extents
                    .iter()
                    .all(|v| v.is_finite() && (0.001..=1.001).contains(v))
            {
                Ok(())
            } else {
                Err("Invalid model reference or bounds".into())
            }
        }
        kokoverse_protocol::Visual::Video {
            asset_id,
            width,
            height,
            duration_seconds,
        } => {
            if asset_id.len() == 64
                && asset_id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && (1..=4096).contains(width)
                && (1..=4096).contains(height)
                && duration_seconds.is_finite()
                && (0.0..=604800.0).contains(duration_seconds)
            {
                Ok(())
            } else {
                Err("Invalid video reference or dimensions".into())
            }
        }
    }
}

pub(crate) fn validate_image(data_url: &str) -> Result<kokoverse_protocol::Visual, String> {
    use base64::Engine;
    let (encoded, format) = if let Some(data) = data_url.strip_prefix("data:image/png;base64,") {
        (data, image::ImageFormat::Png)
    } else if let Some(data) = data_url.strip_prefix("data:image/jpeg;base64,") {
        (data, image::ImageFormat::Jpeg)
    } else {
        return Err("Choose a PNG or JPEG image".into());
    };
    if encoded.len() > MAX_IMAGE_BYTES * 4 / 3 + 4 {
        return Err("Image is too large (192 KiB decoded)".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Invalid image encoding")?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("Image is too large".into());
    }
    let reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| "Invalid image header")?;
    if width == 0 || height == 0 || width > 2048 || height > 2048 {
        return Err("Image dimensions must be at most 2048 × 2048".into());
    }
    image::load_from_memory_with_format(&bytes, format).map_err(|_| "Invalid image data")?;
    Ok(kokoverse_protocol::Visual::Image {
        data_url: data_url.into(),
        width,
        height,
    })
}

pub(crate) fn validate_value(field: &str, value: &Value) -> Result<(), String> {
    let valid = match field {
        "name" => value
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= 80),
        "rotation" => serde_json::from_value::<[f64; 4]>(value.clone()).is_ok_and(|q| {
            q.iter().all(|v| v.is_finite())
                && (q.iter().map(|v| v * v).sum::<f64>() - 1.0).abs() < 0.0001
        }),
        "scale" => serde_json::from_value::<[f64; 3]>(value.clone())
            .is_ok_and(|s| s.iter().all(|v| v.is_finite() && (0.05..=20.0).contains(v))),
        "collider" => {
            value.is_null()
                || serde_json::from_value::<kokoverse_protocol::ColliderSpec>(value.clone())
                    .is_ok_and(|c| {
                        c.half_extents
                            .iter()
                            .all(|v| v.is_finite() && (0.001..=50.0).contains(v))
                    })
        }
        "playback" => {
            value.is_null()
                || serde_json::from_value::<kokoverse_protocol::VideoPlayback>(value.clone())
                    .is_ok_and(|p| {
                        p.position_seconds.is_finite()
                            && (0.0..=1e12).contains(&p.position_seconds)
                            && p.anchor_seconds.is_finite()
                            && (0.0..=1e12).contains(&p.anchor_seconds)
                    })
        }
        "position" | "color" => value.as_array().is_some_and(|a| {
            a.len() == if field == "position" { 3 } else { 4 }
                && a.iter().all(|v| {
                    v.as_f64().is_some_and(|n| {
                        n.is_finite()
                            && if field == "position" {
                                n.abs() <= 1000.0
                            } else {
                                (0.0..=1.0).contains(&n)
                            }
                    })
                })
        }),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("Invalid value for {field}"))
    }
}

#[cfg(test)]
mod tests;

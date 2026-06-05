use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::assets::AssetPriority;
use crate::core::*;

const NAVIGATION_ENTITY_RADIUS_METERS: f32 = 0.36;
const NAVIGATION_PROP_RADIUS_METERS: f32 = 0.25;
const NAVIGATION_MIN_PROGRESS_METERS: f32 = 0.001;
const NAVIGATION_FRACTURED_CRACK_DENSITY_THRESHOLD: f32 = 0.5;
const NAVIGATION_OVERHEAD_Z_METERS: f32 = 1.65;
const NAVIGATION_WALL_HALF_EXTENTS: [f32; 2] = [1.15, 0.08];
const NAVIGATION_HUMAN_HALF_EXTENTS: [f32; 2] = [0.42, 0.3];
const NAVIGATION_PROP_HALF_EXTENTS: [f32; 2] = [0.34, 0.34];

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentView<T> {
    entries: Vec<(EntityId, T)>,
}

impl<T> ComponentView<T> {
    pub fn new(entries: Vec<(EntityId, T)>) -> Self {
        Self { entries }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(EntityId, T)> {
        self.entries.iter()
    }

    pub fn find(&self, entity: EntityId) -> Option<&T> {
        self.entries
            .iter()
            .find_map(|(id, value)| (*id == entity).then_some(value))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Renderable {
    pub mesh: MeshAssetHandle,
    pub material: MaterialId,
    pub visible: bool,
    pub fracture_replacement_mesh: Option<MeshAssetHandle>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalBody {
    pub mass_kg: f32,
    pub material_id: MaterialId,
    pub dynamic: bool,
    pub fragile: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanState {
    pub human_id: HumanId,
    pub quality_tier: QualityTier,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentState {
    pub persona: AgentPersonaId,
    pub emotional_state: EmotionState,
    pub ai_lod: QualityTier,
}

pub const POPULATION_SUMMARY_SCHEMA: SchemaVersion = SchemaVersion {
    name: "PopulationSummary",
    version: 5,
};

#[derive(Clone, Debug, PartialEq)]
pub struct PopulationSummary {
    pub schema_version: SchemaVersion,
    pub expected_active_npcs: u32,
    pub expected_background_crowd: u32,
    pub expected_vehicle_or_transit_count: u32,
    pub crowd_spawn_rule_count: u32,
    pub traffic_rule_count: u32,
    pub simulated_crowd_rule_count: u32,
    pub summary_only_crowd_rule_count: u32,
    pub average_crowd_density: f32,
    pub average_traffic_density: f32,
    pub faction_control_count: u32,
    pub alertness: f32,
    pub commerce_activity: f32,
    pub observation_coverage: f32,
}

impl Default for PopulationSummary {
    fn default() -> Self {
        Self {
            schema_version: POPULATION_SUMMARY_SCHEMA,
            expected_active_npcs: 0,
            expected_background_crowd: 0,
            expected_vehicle_or_transit_count: 0,
            crowd_spawn_rule_count: 0,
            traffic_rule_count: 0,
            simulated_crowd_rule_count: 0,
            summary_only_crowd_rule_count: 0,
            average_crowd_density: 0.0,
            average_traffic_density: 0.0,
            faction_control_count: 0,
            alertness: 0.0,
            commerce_activity: 0.0,
            observation_coverage: 0.0,
        }
    }
}

impl PopulationSummary {
    pub fn total_expected_population(&self) -> u32 {
        self.expected_active_npcs
            .saturating_add(self.expected_background_crowd)
            .saturating_add(self.expected_vehicle_or_transit_count)
    }

    pub fn normalized_activity(&self) -> f32 {
        let population_pressure = self.total_expected_population() as f32 / 256.0;
        (population_pressure * 0.42
            + self.average_crowd_density * 0.2
            + self.average_traffic_density * 0.16
            + self.alertness * 0.12
            + self.commerce_activity * 0.1)
            .clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PopulationSummaryValidationReport {
    pub passed: bool,
    pub issues: Vec<PopulationSummaryValidationIssue>,
}

impl PopulationSummaryValidationReport {
    pub fn issue_count(&self) -> usize {
        self.issues.len()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PopulationSummaryValidationIssue {
    pub code: String,
    pub message: String,
}

pub fn validate_population_summary(
    summary: &PopulationSummary,
) -> PopulationSummaryValidationReport {
    let mut issues = Vec::new();

    if summary.schema_version != POPULATION_SUMMARY_SCHEMA {
        issues.push(population_summary_issue(
            "population_summary_schema_mismatch",
            "population summaries must use the shared v5 schema contract",
        ));
    }
    if summary.expected_background_crowd > 256 {
        issues.push(population_summary_issue(
            "population_summary_unbounded_crowd",
            "background crowd summaries must stay within the runtime population budget",
        ));
    }
    if summary.expected_vehicle_or_transit_count > 64 {
        issues.push(population_summary_issue(
            "population_summary_unbounded_traffic",
            "traffic summaries must stay within the runtime population budget",
        ));
    }
    if summary.expected_background_crowd > 0 && summary.crowd_spawn_rule_count == 0 {
        issues.push(population_summary_issue(
            "population_summary_missing_crowd_rules",
            "background crowd counts must be backed by at least one crowd rule",
        ));
    }
    if summary.expected_vehicle_or_transit_count > 0 && summary.traffic_rule_count == 0 {
        issues.push(population_summary_issue(
            "population_summary_missing_traffic_rules",
            "traffic counts must be backed by at least one traffic or transit rule",
        ));
    }
    if summary.simulated_crowd_rule_count + summary.summary_only_crowd_rule_count
        > summary.crowd_spawn_rule_count
    {
        issues.push(population_summary_issue(
            "population_summary_lod_count_mismatch",
            "crowd LOD counters cannot exceed the number of crowd spawn rules",
        ));
    }
    for (code, label, value) in [
        (
            "population_summary_invalid_crowd_density",
            "average crowd density",
            summary.average_crowd_density,
        ),
        (
            "population_summary_invalid_traffic_density",
            "average traffic density",
            summary.average_traffic_density,
        ),
        (
            "population_summary_invalid_alertness",
            "alertness",
            summary.alertness,
        ),
        (
            "population_summary_invalid_commerce",
            "commerce activity",
            summary.commerce_activity,
        ),
        (
            "population_summary_invalid_observation",
            "observation coverage",
            summary.observation_coverage,
        ),
    ] {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            issues.push(population_summary_issue(
                code,
                format!("{label} must be normalized"),
            ));
        }
    }

    PopulationSummaryValidationReport {
        passed: issues.is_empty(),
        issues,
    }
}

fn population_summary_issue(
    code: impl Into<String>,
    message: impl Into<String>,
) -> PopulationSummaryValidationIssue {
    PopulationSummaryValidationIssue {
        code: code.into(),
        message: message.into(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CityCellRuntimeRef {
    pub cell_id: u64,
    pub district_id: u64,
    pub population_summary: PopulationSummary,
    pub quality_tier: QualityTier,
    pub loaded: bool,
    pub resident_asset_count: u32,
    pub active_story_thread_count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityTemplate {
    pub entity_id: Option<EntityId>,
    pub name: String,
    pub transform: Transform,
    pub renderable: Option<Renderable>,
    pub physical_body: Option<PhysicalBody>,
    pub material_state: Option<MaterialState>,
    pub human: Option<HumanState>,
    pub agent: Option<AgentState>,
    pub tags: TagSet,
}

impl EntityTemplate {
    pub fn new(name: impl Into<String>, transform: Transform) -> Self {
        Self {
            entity_id: None,
            name: name.into(),
            transform,
            renderable: None,
            physical_body: None,
            material_state: None,
            human: None,
            agent: None,
            tags: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldSnapshot {
    pub frame_id: FrameId,
    pub sim_time: SimTime,
    pub materials: ComponentView<MaterialDescriptor>,
    pub names: ComponentView<String>,
    pub tags: ComponentView<TagSet>,
    pub transforms: ComponentView<Transform>,
    pub renderables: ComponentView<Renderable>,
    pub physical_bodies: ComponentView<PhysicalBody>,
    pub material_states: ComponentView<MaterialState>,
    pub humans: ComponentView<HumanState>,
    pub agents: ComponentView<AgentState>,
    pub city_cells: ComponentView<CityCellRuntimeRef>,
}

pub const PRIMARY_INTERACTION_DEFAULT_REACH_METERS: f32 = 2.75;
pub const PRIMARY_INTERACTION_DEFAULT_AIM_ALIGNMENT_THRESHOLD: f32 = 0.72;
pub const PRIMARY_INTERACTION_DEFAULT_CLOSE_AIM_LATERAL_METERS: f32 = 0.45;
pub const PRIMARY_INTERACTION_DEFAULT_FORCE_NEWTONS: f32 = 2_000.0;
pub const PRIMARY_INTERACTION_DEFAULT_IMPULSE_SECONDS: f32 = 48.0;
pub const PRIMARY_INTERACTION_DEFAULT_FRACTURED_CRACK_DENSITY_THRESHOLD: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrimaryInteractionSettings {
    pub player_entity: EntityId,
    pub reach_meters: f32,
    pub aim_alignment_threshold: f32,
    pub close_aim_lateral_meters: f32,
    pub force_newtons: f32,
    pub impulse_seconds: f32,
    pub fractured_crack_density_threshold: f32,
}

impl PrimaryInteractionSettings {
    pub fn new(player_entity: EntityId) -> Self {
        Self {
            player_entity,
            reach_meters: PRIMARY_INTERACTION_DEFAULT_REACH_METERS,
            aim_alignment_threshold: PRIMARY_INTERACTION_DEFAULT_AIM_ALIGNMENT_THRESHOLD,
            close_aim_lateral_meters: PRIMARY_INTERACTION_DEFAULT_CLOSE_AIM_LATERAL_METERS,
            force_newtons: PRIMARY_INTERACTION_DEFAULT_FORCE_NEWTONS,
            impulse_seconds: PRIMARY_INTERACTION_DEFAULT_IMPULSE_SECONDS,
            fractured_crack_density_threshold:
                PRIMARY_INTERACTION_DEFAULT_FRACTURED_CRACK_DENSITY_THRESHOLD,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrimaryInteraction {
    pub entity: EntityId,
    pub label: String,
    pub target_position: [f32; 2],
    pub distance_meters: f32,
    pub in_reach: bool,
    pub aim_alignment: f32,
    pub lateral_meters: f32,
    pub in_aim_cone: bool,
    pub already_fractured: bool,
    pub force: ForceCommand,
}

impl PrimaryInteraction {
    pub fn status_summary(&self) -> String {
        if self.already_fractured {
            return format!("fractured {}", self.label);
        }

        if self.in_reach {
            format!("target {}", self.label)
        } else if self.in_aim_cone {
            format!("aim {} {:.1} m", self.label, self.distance_meters)
        } else {
            format!("near {} {:.1} m", self.label, self.distance_meters)
        }
    }
}

pub fn primary_interaction_from_snapshot(
    snapshot: &WorldSnapshot,
    yaw_radians: f32,
    settings: PrimaryInteractionSettings,
) -> Option<PrimaryInteraction> {
    let player_position = snapshot
        .transforms
        .find(settings.player_entity)
        .map(|transform| {
            [
                transform.translation_meters.x,
                transform.translation_meters.y,
            ]
        })?;
    let mut best: Option<(u8, f32, f32, f32, PrimaryInteraction)> = None;

    for (entity, transform) in snapshot.transforms.iter() {
        if *entity == settings.player_entity {
            continue;
        }

        let tags = snapshot.tags.find(*entity).map(Vec::as_slice);
        let Some(priority) = primary_interaction_priority(tags, snapshot, *entity) else {
            continue;
        };

        let target_position = [
            transform.translation_meters.x,
            transform.translation_meters.y,
        ];
        let dx = target_position[0] - player_position[0];
        let dy = target_position[1] - player_position[1];
        let distance_meters = (dx * dx + dy * dy).sqrt();
        let in_reach = distance_meters <= settings.reach_meters;
        let aim = primary_interaction_aim(dx, dy, yaw_radians, settings);
        let label = snapshot
            .names
            .find(*entity)
            .cloned()
            .unwrap_or_else(|| format!("entity {entity}"));
        let force_direction = primary_interaction_force_direction(dx, dy, yaw_radians);
        let already_fractured =
            primary_interaction_entity_is_fractured(snapshot, *entity, settings);

        let interaction = PrimaryInteraction {
            entity: *entity,
            label,
            target_position,
            distance_meters,
            in_reach,
            aim_alignment: aim.alignment,
            lateral_meters: aim.lateral_meters,
            in_aim_cone: aim.in_cone,
            already_fractured,
            force: ForceCommand {
                entity: *entity,
                vector_newtons: Vec3::new(
                    force_direction[0] * settings.force_newtons,
                    force_direction[1] * settings.force_newtons,
                    0.0,
                ),
                impulse_newton_seconds: settings.impulse_seconds,
                source: Some(settings.player_entity),
            },
        };

        let selection_bucket = primary_interaction_selection_bucket(&interaction);
        let aim_error = 1.0 - aim.alignment;
        let distance_score = if in_reach {
            distance_meters
        } else {
            distance_meters + settings.reach_meters
        };

        if best.as_ref().is_none_or(
            |(best_bucket, best_priority, best_aim_error, best_distance, _)| {
                let same_aim_error = (aim_error - *best_aim_error).abs() <= f32::EPSILON;
                selection_bucket < *best_bucket
                    || (selection_bucket == *best_bucket && priority < *best_priority)
                    || (selection_bucket == *best_bucket
                        && priority == *best_priority
                        && aim_error < *best_aim_error)
                    || (selection_bucket == *best_bucket
                        && priority == *best_priority
                        && same_aim_error
                        && distance_score < *best_distance)
            },
        ) {
            best = Some((
                selection_bucket,
                priority,
                aim_error,
                distance_score,
                interaction,
            ));
        }
    }

    best.map(|(_, _, _, _, interaction)| interaction)
}

pub fn primary_interaction_entity_is_fractured(
    snapshot: &WorldSnapshot,
    entity: EntityId,
    settings: PrimaryInteractionSettings,
) -> bool {
    snapshot
        .material_states
        .find(entity)
        .is_some_and(|state| state.crack_density >= settings.fractured_crack_density_threshold)
}

fn primary_interaction_priority(
    tags: Option<&[String]>,
    snapshot: &WorldSnapshot,
    entity: EntityId,
) -> Option<f32> {
    if world_tags_have(tags, "destructible") {
        return Some(0.0);
    }

    let physical_body = snapshot.physical_bodies.find(entity)?;
    physical_body.fragile.then_some(1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PrimaryInteractionAim {
    alignment: f32,
    lateral_meters: f32,
    in_cone: bool,
}

fn primary_interaction_aim(
    dx: f32,
    dy: f32,
    yaw_radians: f32,
    settings: PrimaryInteractionSettings,
) -> PrimaryInteractionAim {
    let distance_meters = (dx * dx + dy * dy).sqrt();
    if distance_meters <= f32::EPSILON {
        return PrimaryInteractionAim {
            alignment: 1.0,
            lateral_meters: 0.0,
            in_cone: true,
        };
    }

    let forward = [yaw_radians.sin(), yaw_radians.cos()];
    let alignment = ((dx * forward[0] + dy * forward[1]) / distance_meters).clamp(-1.0, 1.0);
    let lateral_meters = (dx * forward[1] - dy * forward[0]).abs();
    let in_cone = alignment >= settings.aim_alignment_threshold
        || (alignment >= 0.0
            && distance_meters <= settings.reach_meters
            && lateral_meters <= settings.close_aim_lateral_meters);

    PrimaryInteractionAim {
        alignment,
        lateral_meters,
        in_cone,
    }
}

fn primary_interaction_selection_bucket(interaction: &PrimaryInteraction) -> u8 {
    match (
        interaction.already_fractured,
        interaction.in_reach,
        interaction.in_aim_cone,
    ) {
        (false, true, true) => 0,
        (false, true, false) => 1,
        (false, false, true) => 2,
        (true, true, true) => 3,
        (true, true, false) => 4,
        (true, false, true) => 5,
        (false, false, false) => 6,
        (true, false, false) => 7,
    }
}

fn primary_interaction_force_direction(dx: f32, dy: f32, yaw_radians: f32) -> [f32; 2] {
    let length = (dx * dx + dy * dy).sqrt();
    if length > f32::EPSILON {
        return [dx / length, dy / length];
    }

    [yaw_radians.sin(), yaw_radians.cos()]
}

fn world_tags_have(tags: Option<&[String]>, expected: &str) -> bool {
    tags.is_some_and(|tags| tags.iter().any(|tag| tag == expected))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveEntityStepMode {
    Toward,
    AwayFrom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveEntityStepCommand {
    pub entity: EntityId,
    pub mode: MoveEntityStepMode,
    pub reference_meters: Vec3,
    pub max_step_meters: f32,
    pub stop_radius_meters: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorldCommand {
    SpawnEntity(EntityTemplate),
    DespawnEntity(EntityId),
    SetTransform(EntityId, Transform),
    MoveEntityStep(MoveEntityStepCommand),
    SetAgentState(EntityId, AgentState),
    ApplyForce(ForceCommand),
    ApplyDamage(DamageCommand),
    ReplaceMesh(EntityId, MeshAssetHandle),
    UpdateMaterialState(EntityId, MaterialStateDelta),
    EmitSound(AudioEvent),
    EmitDialogue(DialogueEvent),
    EmitStoryEvent(StoryEvent),
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldEvent {
    pub event_id: WorldEventId,
    pub tick: u64,
    pub location_meters: Vec3,
    pub actors: Vec<EntityId>,
    pub kind: WorldEventKind,
    pub physical_evidence: EvidenceRefs,
    pub narrative_tags: TagSet,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorldEventKind {
    EntitySpawned {
        entity: EntityId,
        name: String,
    },
    EntityDespawned {
        entity: EntityId,
    },
    TransformChanged {
        entity: EntityId,
    },
    NavigationMoveBlocked {
        entity: EntityId,
    },
    AgentStateChanged {
        entity: EntityId,
    },
    ForceApplied {
        entity: EntityId,
    },
    DamageApplied {
        entity: EntityId,
        amount: f32,
    },
    MeshReplaced {
        entity: EntityId,
    },
    MaterialStateChanged {
        entity: EntityId,
    },
    GlassWallFractured {
        entity: EntityId,
    },
    MetalBent {
        entity: EntityId,
        plastic_strain: f32,
    },
    FlexibleConstraintResolved {
        entity: EntityId,
        correction_meters: f32,
    },
    PowerTransformerOverheated {
        entity: EntityId,
    },
    StreetFlooded,
    ToxicGasReleased,
    NpcWitnessedCrime {
        witness: EntityId,
        event: WorldEventId,
    },
    NpcHeardSound {
        listener: EntityId,
        event: WorldEventId,
        source_entity: Option<EntityId>,
        confidence: f32,
    },
    AgentMemoryUpdated {
        agent: EntityId,
        source_event: Option<WorldEventId>,
        memory_kind: String,
        content: String,
        importance: f32,
        confidence: f32,
    },
    AgentIntentProposed {
        agent: EntityId,
        action: String,
        source_event: Option<WorldEventId>,
        validation_passed: bool,
    },
    AgentDecisionExplained {
        agent: EntityId,
        source_event: Option<WorldEventId>,
        goal: String,
        selected_lod: String,
        available_actions: Vec<String>,
        accepted_actions: Vec<String>,
        rejected_actions: Vec<String>,
        reasons: Vec<String>,
    },
    FactionLostTerritory {
        faction: FactionId,
    },
    FactionReputationChanged {
        faction: FactionId,
        subject: EntityId,
        delta: f32,
        reason: String,
    },
    SecurityAlertRaised {
        faction: FactionId,
        source_event: WorldEventId,
        threat: EntityId,
        severity: f32,
    },
    SurveillanceIncreased {
        location: LocationId,
        source_event: WorldEventId,
        amount: f32,
    },
    VoiceLineSpoken {
        speaker: EntityId,
    },
    SpeechSynthesized {
        speaker: EntityId,
        audio_clip: AudioClipHandle,
        duration_seconds: f32,
        phoneme_count: usize,
        viseme_count: usize,
    },
    FacialAnimationApplied {
        entity: EntityId,
        viseme_count: usize,
        duration_seconds: f32,
    },
    HumanAppearanceUpdated {
        entity: EntityId,
        skin_wetness: f32,
        injury_overlay: f32,
        bruising: f32,
        dirt: f32,
    },
    PlayerIdentityExposed,
    SoundEmitted {
        kind: AudioEventKind,
        source_entity: Option<EntityId>,
        material_id: Option<MaterialId>,
        intensity: f32,
        radius_meters: f32,
        occlusion_hint: f32,
    },
    AudioFrameMixed {
        active_sound_count: usize,
        material_aware_sound_count: usize,
        peak_intensity: f32,
        mixed_audio_buffer: AudioBufferHandle,
    },
    DialogueEmitted {
        speaker: EntityId,
        target: Option<EntityId>,
        text: String,
        emotion: EmotionState,
        voice_persona: Option<VoicePersonaId>,
    },
    StoryEventEmitted {
        label: String,
    },
    AssetStreamingRequested {
        asset_id: AssetId,
        requester: ModuleId,
        requested_quality: QualityTier,
        priority: AssetPriority,
        reason: String,
    },
    AssetBecameResident {
        asset_id: AssetId,
        quality_tier: QualityTier,
    },
    Custom(String),
}

impl WorldEventKind {
    pub fn faction(&self) -> Option<FactionId> {
        match self {
            Self::FactionLostTerritory { faction }
            | Self::FactionReputationChanged { faction, .. }
            | Self::SecurityAlertRaised { faction, .. } => Some(*faction),
            _ => None,
        }
    }

    pub fn physical_type(&self) -> Option<PhysicalEventType> {
        match self {
            Self::ForceApplied { .. } => Some(PhysicalEventType::Impulse),
            Self::DamageApplied { .. } => Some(PhysicalEventType::Damage),
            Self::MaterialStateChanged { .. } => Some(PhysicalEventType::MaterialChange),
            Self::GlassWallFractured { .. } => Some(PhysicalEventType::Fracture),
            Self::MetalBent { .. } => Some(PhysicalEventType::Deformation),
            Self::FlexibleConstraintResolved { .. } => Some(PhysicalEventType::Constraint),
            Self::PowerTransformerOverheated { .. } => Some(PhysicalEventType::ThermalElectrical),
            Self::StreetFlooded => Some(PhysicalEventType::Fluid),
            Self::ToxicGasReleased => Some(PhysicalEventType::Gas),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalEventType {
    Impulse,
    Damage,
    MaterialChange,
    Fracture,
    Deformation,
    Constraint,
    ThermalElectrical,
    Fluid,
    Gas,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioEvent {
    pub event_id: WorldEventId,
    pub source_entity: Option<EntityId>,
    pub location: Vec3,
    pub event_kind: AudioEventKind,
    pub material_id: Option<MaterialId>,
    pub intensity: f32,
    pub radius_meters: f32,
    pub occlusion_hint: f32,
    pub tags: TagSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioEventKind {
    GlassImpact,
    GlassShatter,
    MetalBend,
    ConcreteCrack,
    Footstep,
    WaterSplash,
    SteamLeak,
    Gunshot,
    Explosion,
    ElectricBuzz,
    NeonHum,
    DoorOpen,
    ClothRustle,
    HumanBreath,
    VoiceSpeech,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DialogueEvent {
    pub speaker: EntityId,
    pub target: Option<EntityId>,
    pub text: String,
    pub emotion: EmotionState,
    pub voice_persona: Option<VoicePersonaId>,
    pub location: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoryEvent {
    pub label: String,
    pub actors: Vec<EntityId>,
    pub location: Vec3,
    pub tags: TagSet,
}

#[derive(Clone, Debug, Default)]
pub struct CommandSink {
    pub commands: Vec<WorldCommand>,
    pub events: Vec<WorldEvent>,
}

impl CommandSink {
    pub fn command(&mut self, command: WorldCommand) {
        self.commands.push(command);
    }

    pub fn event(&mut self, event: WorldEvent) {
        self.events.push(event);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EventLedger {
    events: Vec<WorldEvent>,
}

impl EventLedger {
    pub fn record(&mut self, event: WorldEvent) {
        self.events.push(event);
    }

    pub fn all(&self) -> &[WorldEvent] {
        &self.events
    }

    pub fn recent(&self, limit: usize) -> Vec<WorldEvent> {
        let start = self.events.len().saturating_sub(limit);
        self.events[start..].to_vec()
    }

    pub fn by_actor(&self, actor: EntityId) -> Vec<&WorldEvent> {
        self.events
            .iter()
            .filter(|event| event.actors.contains(&actor))
            .collect()
    }

    pub fn by_faction(&self, faction: FactionId) -> Vec<&WorldEvent> {
        self.query(&EventFilter::default().with_faction(faction))
    }

    pub fn by_physical_type(&self, physical_type: PhysicalEventType) -> Vec<&WorldEvent> {
        self.query(&EventFilter::default().with_physical_type(physical_type))
    }

    pub fn by_tick_range(&self, start_tick: u64, end_tick: u64) -> Vec<&WorldEvent> {
        self.events
            .iter()
            .filter(|event| (start_tick..=end_tick).contains(&event.tick))
            .collect()
    }

    pub fn by_tag(&self, tag: &str) -> Vec<&WorldEvent> {
        self.events
            .iter()
            .filter(|event| {
                event
                    .narrative_tags
                    .iter()
                    .any(|candidate| candidate == tag)
            })
            .collect()
    }

    pub fn by_story_tag(&self, tag: &str) -> Vec<&WorldEvent> {
        self.by_tag(tag)
    }

    pub fn by_evidence_strength(&self, min_evidence_count: usize) -> Vec<&WorldEvent> {
        self.query(&EventFilter::default().with_min_evidence_count(min_evidence_count))
    }

    pub fn near(&self, center: Vec3, radius_meters: f32) -> Vec<&WorldEvent> {
        self.events
            .iter()
            .filter(|event| location_matches(event.location_meters, center, radius_meters))
            .collect()
    }

    pub fn query(&self, filter: &EventFilter) -> Vec<&WorldEvent> {
        filter.select(self.events.iter())
    }

    pub fn contains_kind(&self, predicate: impl Fn(&WorldEventKind) -> bool) -> bool {
        self.events.iter().any(|event| predicate(&event.kind))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EventFilter {
    pub start_tick: Option<u64>,
    pub end_tick: Option<u64>,
    pub actor: Option<EntityId>,
    pub faction: Option<FactionId>,
    pub narrative_tag: Option<String>,
    pub physical_evidence: Option<String>,
    pub physical_type: Option<PhysicalEventType>,
    pub min_evidence_count: Option<usize>,
    pub near: Option<(Vec3, f32)>,
    pub limit: Option<usize>,
}

impl EventFilter {
    pub fn with_tick_range(mut self, start_tick: u64, end_tick: u64) -> Self {
        self.start_tick = Some(start_tick);
        self.end_tick = Some(end_tick);
        self
    }

    pub fn with_actor(mut self, actor: EntityId) -> Self {
        self.actor = Some(actor);
        self
    }

    pub fn with_faction(mut self, faction: FactionId) -> Self {
        self.faction = Some(faction);
        self
    }

    pub fn with_story_tag(mut self, tag: impl Into<String>) -> Self {
        self.narrative_tag = Some(tag.into());
        self
    }

    pub fn with_narrative_tag(self, tag: impl Into<String>) -> Self {
        self.with_story_tag(tag)
    }

    pub fn with_physical_evidence(mut self, evidence: impl Into<String>) -> Self {
        self.physical_evidence = Some(evidence.into());
        self
    }

    pub fn with_physical_type(mut self, physical_type: PhysicalEventType) -> Self {
        self.physical_type = Some(physical_type);
        self
    }

    pub fn with_min_evidence_count(mut self, min_evidence_count: usize) -> Self {
        self.min_evidence_count = Some(min_evidence_count);
        self
    }

    pub fn near(mut self, center: Vec3, radius_meters: f32) -> Self {
        self.near = Some((center, radius_meters));
        self
    }

    pub fn limited_to(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn select<'a>(
        &self,
        events: impl IntoIterator<Item = &'a WorldEvent>,
    ) -> Vec<&'a WorldEvent> {
        if self.limit == Some(0) {
            return Vec::new();
        }

        let mut selected = Vec::new();
        for event in events {
            if self.matches(event) {
                selected.push(event);
                if self.limit.is_some_and(|limit| selected.len() == limit) {
                    break;
                }
            }
        }
        selected
    }

    pub fn matches(&self, event: &WorldEvent) -> bool {
        if self
            .start_tick
            .is_some_and(|start_tick| event.tick < start_tick)
        {
            return false;
        }
        if self.end_tick.is_some_and(|end_tick| event.tick > end_tick) {
            return false;
        }
        if self
            .actor
            .is_some_and(|actor| !event.actors.contains(&actor))
        {
            return false;
        }
        if self
            .faction
            .is_some_and(|faction| event.kind.faction() != Some(faction))
        {
            return false;
        }
        if self.narrative_tag.as_ref().is_some_and(|tag| {
            !event
                .narrative_tags
                .iter()
                .any(|candidate| candidate == tag)
        }) {
            return false;
        }
        if self.physical_evidence.as_ref().is_some_and(|evidence| {
            !event
                .physical_evidence
                .iter()
                .any(|candidate| candidate == evidence)
        }) {
            return false;
        }
        if self
            .physical_type
            .is_some_and(|physical_type| event.kind.physical_type() != Some(physical_type))
        {
            return false;
        }
        if self
            .min_evidence_count
            .is_some_and(|min_count| event.physical_evidence.len() < min_count)
        {
            return false;
        }
        if self.near.is_some_and(|(center, radius)| {
            !location_matches(event.location_meters, center, radius)
        }) {
            return false;
        }

        true
    }
}

fn location_matches(location: Vec3, center: Vec3, radius_meters: f32) -> bool {
    radius_meters.is_finite() && radius_meters >= 0.0 && location.distance(center) <= radius_meters
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    MissingEntity(EntityId),
    MissingAgent(EntityId),
    MissingMaterialState(EntityId),
    DuplicateEntity(EntityId),
    EntityMismatch {
        command_entity: EntityId,
        payload_entity: EntityId,
    },
    InvalidMagnitude {
        field: &'static str,
    },
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEntity(entity) => write!(f, "entity {entity} does not exist"),
            Self::MissingAgent(entity) => write!(f, "entity {entity} has no agent state"),
            Self::MissingMaterialState(entity) => {
                write!(f, "entity {entity} has no material state")
            }
            Self::DuplicateEntity(entity) => write!(f, "entity {entity} already exists"),
            Self::EntityMismatch {
                command_entity,
                payload_entity,
            } => write!(
                f,
                "command targets entity {command_entity}, but payload targets entity {payload_entity}"
            ),
            Self::InvalidMagnitude { field } => {
                write!(f, "command field {field} must be finite and non-negative")
            }
        }
    }
}

impl Error for CommandError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventValidationError {
    TickMismatch {
        expected: u64,
        actual: u64,
    },
    MissingEntity {
        field: &'static str,
        entity: EntityId,
    },
    MissingMaterial {
        field: &'static str,
        material_id: MaterialId,
    },
    MissingEvent {
        field: &'static str,
        event_id: WorldEventId,
    },
    EmptyText {
        field: &'static str,
    },
    InvalidAsset {
        field: &'static str,
        asset_id: AssetId,
    },
    InvalidMagnitude {
        field: &'static str,
    },
}

impl fmt::Display for EventValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TickMismatch { expected, actual } => {
                write!(
                    f,
                    "event tick {actual} does not match frame tick {expected}"
                )
            }
            Self::MissingEntity { field, entity } => {
                write!(f, "event field {field} references missing entity {entity}")
            }
            Self::MissingMaterial { field, material_id } => {
                write!(
                    f,
                    "event field {field} references missing material {material_id}"
                )
            }
            Self::MissingEvent { field, event_id } => {
                write!(f, "event field {field} references unknown event {event_id}")
            }
            Self::EmptyText { field } => write!(f, "event field {field} must not be empty"),
            Self::InvalidAsset { field, asset_id } => {
                write!(f, "event field {field} has invalid asset id {asset_id}")
            }
            Self::InvalidMagnitude { field } => {
                write!(f, "event field {field} must be finite and non-negative")
            }
        }
    }
}

impl Error for EventValidationError {}

#[derive(Clone, Debug)]
pub struct WorldState {
    next_entity_id: EntityId,
    next_event_seq: u64,
    names: BTreeMap<EntityId, String>,
    tags: BTreeMap<EntityId, TagSet>,
    transforms: BTreeMap<EntityId, Transform>,
    renderables: BTreeMap<EntityId, Renderable>,
    physical_bodies: BTreeMap<EntityId, PhysicalBody>,
    material_states: BTreeMap<EntityId, MaterialState>,
    humans: BTreeMap<EntityId, HumanState>,
    agents: BTreeMap<EntityId, AgentState>,
    pending_forces: Vec<ForceCommand>,
    pub materials: BTreeMap<MaterialId, MaterialDescriptor>,
    pub event_ledger: EventLedger,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldSaveData {
    pub next_entity_id: EntityId,
    pub next_event_seq: u64,
    pub names: BTreeMap<EntityId, String>,
    pub tags: BTreeMap<EntityId, TagSet>,
    pub transforms: BTreeMap<EntityId, Transform>,
    pub renderables: BTreeMap<EntityId, Renderable>,
    pub physical_bodies: BTreeMap<EntityId, PhysicalBody>,
    pub material_states: BTreeMap<EntityId, MaterialState>,
    pub humans: BTreeMap<EntityId, HumanState>,
    pub agents: BTreeMap<EntityId, AgentState>,
    pub pending_forces: Vec<ForceCommand>,
    pub materials: BTreeMap<MaterialId, MaterialDescriptor>,
    pub event_ledger: EventLedger,
}

impl Default for WorldState {
    fn default() -> Self {
        Self {
            next_entity_id: 1,
            next_event_seq: 0,
            names: BTreeMap::new(),
            tags: BTreeMap::new(),
            transforms: BTreeMap::new(),
            renderables: BTreeMap::new(),
            physical_bodies: BTreeMap::new(),
            material_states: BTreeMap::new(),
            humans: BTreeMap::new(),
            agents: BTreeMap::new(),
            pending_forces: Vec::new(),
            materials: BTreeMap::new(),
            event_ledger: EventLedger::default(),
        }
    }
}

impl WorldState {
    pub fn register_material(&mut self, material: MaterialDescriptor) {
        self.materials.insert(material.id, material);
    }

    pub fn spawn_entity_template(
        &mut self,
        template: EntityTemplate,
    ) -> Result<EntityId, CommandError> {
        let entity = template.entity_id.unwrap_or_else(|| {
            let entity = self.next_entity_id;
            self.next_entity_id += 1;
            entity
        });

        if self.transforms.contains_key(&entity) {
            return Err(CommandError::DuplicateEntity(entity));
        }

        self.next_entity_id = self.next_entity_id.max(entity + 1);
        self.names.insert(entity, template.name);
        self.tags.insert(entity, template.tags);
        self.transforms.insert(entity, template.transform);

        if let Some(renderable) = template.renderable {
            self.renderables.insert(entity, renderable);
        }
        if let Some(body) = template.physical_body {
            self.physical_bodies.insert(entity, body);
        }
        if let Some(material_state) = template.material_state {
            self.material_states.insert(entity, material_state);
        }
        if let Some(human) = template.human {
            self.humans.insert(entity, human);
        }
        if let Some(agent) = template.agent {
            self.agents.insert(entity, agent);
        }

        Ok(entity)
    }

    pub fn snapshot(&self, frame_id: FrameId, sim_time: SimTime) -> WorldSnapshot {
        WorldSnapshot {
            frame_id,
            sim_time,
            materials: ComponentView::new(
                self.materials
                    .iter()
                    .map(|(id, value)| (*id, value.clone()))
                    .collect(),
            ),
            names: ComponentView::new(
                self.names
                    .iter()
                    .map(|(id, value)| (*id, value.clone()))
                    .collect(),
            ),
            tags: ComponentView::new(
                self.tags
                    .iter()
                    .map(|(id, value)| (*id, value.clone()))
                    .collect(),
            ),
            transforms: ComponentView::new(
                self.transforms
                    .iter()
                    .map(|(id, value)| (*id, *value))
                    .collect(),
            ),
            renderables: ComponentView::new(
                self.renderables
                    .iter()
                    .map(|(id, value)| (*id, value.clone()))
                    .collect(),
            ),
            physical_bodies: ComponentView::new(
                self.physical_bodies
                    .iter()
                    .map(|(id, value)| (*id, value.clone()))
                    .collect(),
            ),
            material_states: ComponentView::new(
                self.material_states
                    .iter()
                    .map(|(id, value)| (*id, *value))
                    .collect(),
            ),
            humans: ComponentView::new(
                self.humans
                    .iter()
                    .map(|(id, value)| (*id, value.clone()))
                    .collect(),
            ),
            agents: ComponentView::new(
                self.agents
                    .iter()
                    .map(|(id, value)| (*id, value.clone()))
                    .collect(),
            ),
            city_cells: ComponentView::new(Vec::new()),
        }
    }

    pub fn save_data(&self) -> WorldSaveData {
        WorldSaveData {
            next_entity_id: self.next_entity_id,
            next_event_seq: self.next_event_seq,
            names: self.names.clone(),
            tags: self.tags.clone(),
            transforms: self.transforms.clone(),
            renderables: self.renderables.clone(),
            physical_bodies: self.physical_bodies.clone(),
            material_states: self.material_states.clone(),
            humans: self.humans.clone(),
            agents: self.agents.clone(),
            pending_forces: self.pending_forces.clone(),
            materials: self.materials.clone(),
            event_ledger: self.event_ledger.clone(),
        }
    }

    pub fn load_data(save_data: WorldSaveData) -> Self {
        Self {
            next_entity_id: save_data.next_entity_id,
            next_event_seq: save_data.next_event_seq,
            names: save_data.names,
            tags: save_data.tags,
            transforms: save_data.transforms,
            renderables: save_data.renderables,
            physical_bodies: save_data.physical_bodies,
            material_states: save_data.material_states,
            humans: save_data.humans,
            agents: save_data.agents,
            pending_forces: save_data.pending_forces,
            materials: save_data.materials,
            event_ledger: save_data.event_ledger,
        }
    }

    pub fn apply_command(
        &mut self,
        command: WorldCommand,
        tick: u64,
    ) -> Result<Vec<WorldEvent>, CommandError> {
        match command {
            WorldCommand::SpawnEntity(template) => {
                let location = template.transform.translation_meters;
                let name = template.name.clone();
                let entity = self.spawn_entity_template(template)?;
                Ok(vec![self.event(
                    tick,
                    location,
                    vec![entity],
                    WorldEventKind::EntitySpawned { entity, name },
                    Vec::new(),
                    vec!["spawn".to_string()],
                )])
            }
            WorldCommand::DespawnEntity(entity) => {
                let location = self.entity_location(entity)?;
                self.names.remove(&entity);
                self.tags.remove(&entity);
                self.transforms.remove(&entity);
                self.renderables.remove(&entity);
                self.physical_bodies.remove(&entity);
                self.material_states.remove(&entity);
                self.humans.remove(&entity);
                self.agents.remove(&entity);
                Ok(vec![self.event(
                    tick,
                    location,
                    vec![entity],
                    WorldEventKind::EntityDespawned { entity },
                    Vec::new(),
                    vec!["despawn".to_string()],
                )])
            }
            WorldCommand::SetTransform(entity, transform) => {
                self.require_entity(entity)?;
                require_finite_command_transform(transform)?;
                self.transforms.insert(entity, transform);
                Ok(vec![self.event(
                    tick,
                    transform.translation_meters,
                    vec![entity],
                    WorldEventKind::TransformChanged { entity },
                    Vec::new(),
                    Vec::new(),
                )])
            }
            WorldCommand::MoveEntityStep(step) => {
                self.require_entity(step.entity)?;
                require_valid_move_entity_step(step)?;
                let current = *self
                    .transforms
                    .get(&step.entity)
                    .ok_or(CommandError::MissingEntity(step.entity))?;
                require_finite_command_transform(current)?;
                match transform_after_move_entity_step(self, current, step) {
                    MoveEntityStepOutcome::Moved(transform) => {
                        self.transforms.insert(step.entity, transform);
                        Ok(vec![self.event(
                            tick,
                            transform.translation_meters,
                            vec![step.entity],
                            WorldEventKind::TransformChanged {
                                entity: step.entity,
                            },
                            Vec::new(),
                            vec!["navigation_step".to_string()],
                        )])
                    }
                    MoveEntityStepOutcome::Blocked => Ok(vec![self.event(
                        tick,
                        current.translation_meters,
                        vec![step.entity],
                        WorldEventKind::NavigationMoveBlocked {
                            entity: step.entity,
                        },
                        vec!["navigation_blocked".to_string()],
                        vec!["navigation".to_string(), "blocked".to_string()],
                    )]),
                    MoveEntityStepOutcome::Noop => Ok(Vec::new()),
                }
            }
            WorldCommand::SetAgentState(entity, agent_state) => {
                let location = self.entity_location(entity)?;
                if !self.agents.contains_key(&entity) {
                    return Err(CommandError::MissingAgent(entity));
                }
                self.agents.insert(entity, agent_state);
                Ok(vec![self.event(
                    tick,
                    location,
                    vec![entity],
                    WorldEventKind::AgentStateChanged { entity },
                    vec!["agent_state_delta".to_string()],
                    vec!["ai".to_string(), "state".to_string()],
                )])
            }
            WorldCommand::ApplyForce(force) => {
                let location = self.entity_location(force.entity)?;
                self.pending_forces.push(force);
                Ok(vec![self.event(
                    tick,
                    location,
                    vec![force.entity],
                    WorldEventKind::ForceApplied {
                        entity: force.entity,
                    },
                    Vec::new(),
                    vec!["physics_input".to_string()],
                )])
            }
            WorldCommand::ApplyDamage(damage) => {
                let location = self.entity_location(damage.entity)?;
                Ok(vec![self.event(
                    tick,
                    location,
                    vec![damage.entity],
                    WorldEventKind::DamageApplied {
                        entity: damage.entity,
                        amount: damage.amount,
                    },
                    Vec::new(),
                    vec!["damage".to_string()],
                )])
            }
            WorldCommand::ReplaceMesh(entity, mesh) => {
                let location = self.entity_location(entity)?;
                let renderable = self
                    .renderables
                    .get_mut(&entity)
                    .ok_or(CommandError::MissingEntity(entity))?;
                renderable.mesh = mesh;
                Ok(vec![self.event(
                    tick,
                    location,
                    vec![entity],
                    WorldEventKind::MeshReplaced { entity },
                    vec!["mesh_delta".to_string()],
                    vec!["destruction".to_string()],
                )])
            }
            WorldCommand::UpdateMaterialState(entity, delta) => {
                if entity != delta.entity {
                    return Err(CommandError::EntityMismatch {
                        command_entity: entity,
                        payload_entity: delta.entity,
                    });
                }
                let location = self.entity_location(entity)?;
                let state = self
                    .material_states
                    .get_mut(&entity)
                    .ok_or(CommandError::MissingMaterialState(entity))?;
                state.apply_delta(delta);
                Ok(vec![self.event(
                    tick,
                    location,
                    vec![entity],
                    WorldEventKind::MaterialStateChanged { entity },
                    vec!["material_state_delta".to_string()],
                    vec!["material".to_string()],
                )])
            }
            WorldCommand::EmitSound(audio) => Ok(vec![self.event(
                tick,
                audio.location,
                audio.source_entity.into_iter().collect(),
                WorldEventKind::SoundEmitted {
                    kind: audio.event_kind,
                    source_entity: audio.source_entity,
                    material_id: audio.material_id,
                    intensity: audio.intensity,
                    radius_meters: audio.radius_meters,
                    occlusion_hint: audio.occlusion_hint,
                },
                Vec::new(),
                audio.tags,
            )]),
            WorldCommand::EmitDialogue(dialogue) => Ok(vec![self.event(
                tick,
                dialogue.location,
                vec![dialogue.speaker],
                WorldEventKind::DialogueEmitted {
                    speaker: dialogue.speaker,
                    target: dialogue.target,
                    text: dialogue.text,
                    emotion: dialogue.emotion,
                    voice_persona: dialogue.voice_persona,
                },
                Vec::new(),
                vec!["dialogue".to_string()],
            )]),
            WorldCommand::EmitStoryEvent(story) => Ok(vec![self.event(
                tick,
                story.location,
                story.actors,
                WorldEventKind::StoryEventEmitted { label: story.label },
                Vec::new(),
                story.tags,
            )]),
        }
    }

    pub fn validate_commands(
        &self,
        commands: &[WorldCommand],
        tick: u64,
    ) -> Result<(), CommandError> {
        let mut scratch = self.clone();
        for command in commands {
            scratch.apply_command(command.clone(), tick)?;
        }

        Ok(())
    }

    pub fn validate_events(
        &self,
        events: &[WorldEvent],
        tick: u64,
        recent_events: &[WorldEvent],
    ) -> Result<(), EventValidationError> {
        for event in events {
            self.validate_event(event, tick, recent_events)?;
        }

        Ok(())
    }

    pub fn validate_event(
        &self,
        event: &WorldEvent,
        tick: u64,
        recent_events: &[WorldEvent],
    ) -> Result<(), EventValidationError> {
        if event.tick != tick {
            return Err(EventValidationError::TickMismatch {
                expected: tick,
                actual: event.tick,
            });
        }
        require_finite_vec3("location_meters", event.location_meters)?;
        for actor in &event.actors {
            self.require_event_entity("actors", *actor)?;
        }

        match &event.kind {
            WorldEventKind::EntitySpawned { entity, name } => {
                self.require_event_entity("entity", *entity)?;
                require_non_empty("name", name)?;
            }
            WorldEventKind::EntityDespawned { entity }
            | WorldEventKind::TransformChanged { entity }
            | WorldEventKind::NavigationMoveBlocked { entity }
            | WorldEventKind::ForceApplied { entity }
            | WorldEventKind::MeshReplaced { entity }
            | WorldEventKind::GlassWallFractured { entity }
            | WorldEventKind::PowerTransformerOverheated { entity } => {
                self.require_event_entity("entity", *entity)?;
            }
            WorldEventKind::AgentStateChanged { entity } => {
                self.require_event_entity("entity", *entity)?;
                self.require_event_agent("entity", *entity)?;
            }
            WorldEventKind::MaterialStateChanged { entity } => {
                self.require_event_entity("entity", *entity)?;
                self.require_event_material_state("entity", *entity)?;
            }
            WorldEventKind::DamageApplied { entity, amount } => {
                self.require_event_entity("entity", *entity)?;
                require_finite_non_negative("amount", *amount)?;
            }
            WorldEventKind::MetalBent {
                entity,
                plastic_strain,
                ..
            } => {
                self.require_event_entity("entity", *entity)?;
                require_finite_non_negative("plastic_strain", *plastic_strain)?;
            }
            WorldEventKind::FlexibleConstraintResolved {
                entity,
                correction_meters,
                ..
            } => {
                self.require_event_entity("entity", *entity)?;
                require_finite_non_negative("correction_meters", *correction_meters)?;
            }
            WorldEventKind::StreetFlooded | WorldEventKind::ToxicGasReleased => {}
            WorldEventKind::NpcWitnessedCrime { witness, event } => {
                self.require_event_entity("witness", *witness)?;
                self.require_known_event("event", *event, recent_events)?;
            }
            WorldEventKind::NpcHeardSound {
                listener,
                event,
                source_entity,
                confidence,
            } => {
                self.require_event_entity("listener", *listener)?;
                self.require_known_event("event", *event, recent_events)?;
                if let Some(source_entity) = source_entity {
                    self.require_event_entity("source_entity", *source_entity)?;
                }
                require_finite_non_negative("confidence", *confidence)?;
            }
            WorldEventKind::AgentMemoryUpdated {
                agent,
                source_event,
                memory_kind,
                content,
                importance,
                confidence,
            } => {
                self.require_event_entity("agent", *agent)?;
                self.require_optional_known_event("source_event", *source_event, recent_events)?;
                require_non_empty("memory_kind", memory_kind)?;
                require_non_empty("content", content)?;
                require_finite_non_negative("importance", *importance)?;
                require_finite_non_negative("confidence", *confidence)?;
            }
            WorldEventKind::AgentIntentProposed {
                agent,
                action,
                source_event,
                ..
            } => {
                self.require_event_entity("agent", *agent)?;
                self.require_optional_known_event("source_event", *source_event, recent_events)?;
                require_non_empty("action", action)?;
            }
            WorldEventKind::AgentDecisionExplained {
                agent,
                source_event,
                goal,
                selected_lod,
                available_actions,
                accepted_actions,
                rejected_actions,
                reasons,
            } => {
                self.require_event_entity("agent", *agent)?;
                self.require_optional_known_event("source_event", *source_event, recent_events)?;
                require_non_empty("goal", goal)?;
                require_non_empty("selected_lod", selected_lod)?;
                require_non_empty_values("available_actions", available_actions)?;
                require_non_empty_values("accepted_actions", accepted_actions)?;
                require_non_empty_values("rejected_actions", rejected_actions)?;
                require_non_empty_values("reasons", reasons)?;
            }
            WorldEventKind::FactionLostTerritory { .. } => {}
            WorldEventKind::FactionReputationChanged {
                subject,
                delta,
                reason,
                ..
            } => {
                self.require_event_entity("subject", *subject)?;
                require_finite("delta", *delta)?;
                require_non_empty("reason", reason)?;
            }
            WorldEventKind::SecurityAlertRaised {
                source_event,
                threat,
                severity,
                ..
            } => {
                self.require_known_event("source_event", *source_event, recent_events)?;
                self.require_event_entity("threat", *threat)?;
                require_finite_non_negative("severity", *severity)?;
            }
            WorldEventKind::SurveillanceIncreased {
                source_event,
                amount,
                ..
            } => {
                self.require_known_event("source_event", *source_event, recent_events)?;
                require_finite_non_negative("amount", *amount)?;
            }
            WorldEventKind::VoiceLineSpoken { speaker } => {
                self.require_event_entity("speaker", *speaker)?;
            }
            WorldEventKind::SpeechSynthesized {
                speaker,
                duration_seconds,
                ..
            } => {
                self.require_event_entity("speaker", *speaker)?;
                require_finite_non_negative("duration_seconds", *duration_seconds)?;
            }
            WorldEventKind::FacialAnimationApplied {
                entity,
                duration_seconds,
                ..
            } => {
                self.require_event_entity("entity", *entity)?;
                self.require_event_human("entity", *entity)?;
                require_finite_non_negative("duration_seconds", *duration_seconds)?;
            }
            WorldEventKind::HumanAppearanceUpdated {
                entity,
                skin_wetness,
                injury_overlay,
                bruising,
                dirt,
            } => {
                self.require_event_entity("entity", *entity)?;
                self.require_event_human("entity", *entity)?;
                require_finite_non_negative("skin_wetness", *skin_wetness)?;
                require_finite_non_negative("injury_overlay", *injury_overlay)?;
                require_finite_non_negative("bruising", *bruising)?;
                require_finite_non_negative("dirt", *dirt)?;
            }
            WorldEventKind::PlayerIdentityExposed => {}
            WorldEventKind::SoundEmitted {
                source_entity,
                material_id,
                intensity,
                radius_meters,
                occlusion_hint,
                ..
            } => {
                if let Some(source_entity) = source_entity {
                    self.require_event_entity("source_entity", *source_entity)?;
                }
                if let Some(material_id) = material_id {
                    self.require_event_material("material_id", *material_id)?;
                }
                require_finite_non_negative("intensity", *intensity)?;
                require_finite_non_negative("radius_meters", *radius_meters)?;
                require_finite_non_negative("occlusion_hint", *occlusion_hint)?;
            }
            WorldEventKind::AudioFrameMixed { peak_intensity, .. } => {
                require_finite_non_negative("peak_intensity", *peak_intensity)?;
            }
            WorldEventKind::DialogueEmitted {
                speaker,
                target,
                text,
                ..
            } => {
                self.require_event_entity("speaker", *speaker)?;
                if let Some(target) = target {
                    self.require_event_entity("target", *target)?;
                }
                require_non_empty("text", text)?;
            }
            WorldEventKind::StoryEventEmitted { label } => {
                require_non_empty("label", label)?;
            }
            WorldEventKind::AssetStreamingRequested {
                asset_id,
                requester,
                reason,
                ..
            } => {
                require_asset_id("asset_id", *asset_id)?;
                if *requester == 0 {
                    return Err(EventValidationError::InvalidMagnitude { field: "requester" });
                }
                require_non_empty("reason", reason)?;
            }
            WorldEventKind::AssetBecameResident { asset_id, .. } => {
                require_asset_id("asset_id", *asset_id)?;
            }
            WorldEventKind::Custom(label) => {
                require_non_empty("label", label)?;
            }
        }

        Ok(())
    }

    pub fn apply_commands_transactional(
        &mut self,
        commands: Vec<WorldCommand>,
        tick: u64,
    ) -> Result<Vec<WorldEvent>, CommandError> {
        self.validate_commands(&commands, tick)?;

        let mut events = Vec::new();
        for command in commands {
            events.extend(self.apply_command(command, tick)?);
        }

        Ok(events)
    }

    pub fn record_event(&mut self, event: WorldEvent) {
        self.event_ledger.record(event);
    }

    pub fn system_event(
        &mut self,
        tick: u64,
        location_meters: Vec3,
        actors: Vec<EntityId>,
        kind: WorldEventKind,
        physical_evidence: EvidenceRefs,
        narrative_tags: TagSet,
    ) -> WorldEvent {
        self.event(
            tick,
            location_meters,
            actors,
            kind,
            physical_evidence,
            narrative_tags,
        )
    }

    pub fn take_forces(&mut self) -> Vec<ForceCommand> {
        std::mem::take(&mut self.pending_forces)
    }

    fn require_entity(&self, entity: EntityId) -> Result<(), CommandError> {
        self.transforms
            .contains_key(&entity)
            .then_some(())
            .ok_or(CommandError::MissingEntity(entity))
    }

    fn require_event_entity(
        &self,
        field: &'static str,
        entity: EntityId,
    ) -> Result<(), EventValidationError> {
        self.transforms
            .contains_key(&entity)
            .then_some(())
            .ok_or(EventValidationError::MissingEntity { field, entity })
    }

    fn require_event_human(
        &self,
        field: &'static str,
        entity: EntityId,
    ) -> Result<(), EventValidationError> {
        self.humans
            .contains_key(&entity)
            .then_some(())
            .ok_or(EventValidationError::MissingEntity { field, entity })
    }

    fn require_event_agent(
        &self,
        field: &'static str,
        entity: EntityId,
    ) -> Result<(), EventValidationError> {
        self.agents
            .contains_key(&entity)
            .then_some(())
            .ok_or(EventValidationError::MissingEntity { field, entity })
    }

    fn require_event_material_state(
        &self,
        field: &'static str,
        entity: EntityId,
    ) -> Result<(), EventValidationError> {
        self.material_states
            .contains_key(&entity)
            .then_some(())
            .ok_or(EventValidationError::MissingEntity { field, entity })
    }

    fn require_event_material(
        &self,
        field: &'static str,
        material_id: MaterialId,
    ) -> Result<(), EventValidationError> {
        self.materials
            .contains_key(&material_id)
            .then_some(())
            .ok_or(EventValidationError::MissingMaterial { field, material_id })
    }

    fn require_optional_known_event(
        &self,
        field: &'static str,
        event_id: Option<WorldEventId>,
        recent_events: &[WorldEvent],
    ) -> Result<(), EventValidationError> {
        if let Some(event_id) = event_id {
            self.require_known_event(field, event_id, recent_events)?;
        }

        Ok(())
    }

    fn require_known_event(
        &self,
        field: &'static str,
        event_id: WorldEventId,
        recent_events: &[WorldEvent],
    ) -> Result<(), EventValidationError> {
        let known = self
            .event_ledger
            .all()
            .iter()
            .chain(recent_events.iter())
            .any(|event| event.event_id == event_id);
        known
            .then_some(())
            .ok_or(EventValidationError::MissingEvent { field, event_id })
    }

    fn entity_location(&self, entity: EntityId) -> Result<Vec3, CommandError> {
        self.transforms
            .get(&entity)
            .map(|transform| transform.translation_meters)
            .ok_or(CommandError::MissingEntity(entity))
    }

    fn event(
        &mut self,
        tick: u64,
        location_meters: Vec3,
        actors: Vec<EntityId>,
        kind: WorldEventKind,
        physical_evidence: EvidenceRefs,
        narrative_tags: TagSet,
    ) -> WorldEvent {
        self.next_event_seq += 1;
        WorldEvent {
            event_id: ((tick as u128) << 64) | self.next_event_seq as u128,
            tick,
            location_meters,
            actors,
            kind,
            physical_evidence,
            narrative_tags,
        }
    }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), EventValidationError> {
    (!value.trim().is_empty())
        .then_some(())
        .ok_or(EventValidationError::EmptyText { field })
}

fn require_non_empty_values(
    field: &'static str,
    values: &[String],
) -> Result<(), EventValidationError> {
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(EventValidationError::EmptyText { field });
    }

    Ok(())
}

fn require_asset_id(field: &'static str, asset_id: AssetId) -> Result<(), EventValidationError> {
    (asset_id != 0)
        .then_some(())
        .ok_or(EventValidationError::InvalidAsset { field, asset_id })
}

fn require_valid_move_entity_step(step: MoveEntityStepCommand) -> Result<(), CommandError> {
    require_finite_command_vec3("reference_meters", step.reference_meters)?;
    require_finite_command_non_negative("max_step_meters", step.max_step_meters)?;
    require_finite_command_non_negative("stop_radius_meters", step.stop_radius_meters)?;
    Ok(())
}

fn require_finite_command_transform(transform: Transform) -> Result<(), CommandError> {
    require_finite_command_vec3("translation_meters", transform.translation_meters)?;
    require_finite_command_vec3("scale", transform.scale)?;
    for (field, value) in [
        ("rotation_radians.x", transform.rotation_radians.x),
        ("rotation_radians.y", transform.rotation_radians.y),
        ("rotation_radians.z", transform.rotation_radians.z),
        ("rotation_radians.w", transform.rotation_radians.w),
    ] {
        require_finite_command(field, value)?;
    }
    Ok(())
}

fn require_finite_command_vec3(field: &'static str, value: Vec3) -> Result<(), CommandError> {
    if value.x.is_finite() && value.y.is_finite() && value.z.is_finite() {
        Ok(())
    } else {
        Err(CommandError::InvalidMagnitude { field })
    }
}

fn require_finite_command_non_negative(
    field: &'static str,
    value: f32,
) -> Result<(), CommandError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(CommandError::InvalidMagnitude { field })
    }
}

fn require_finite_command(field: &'static str, value: f32) -> Result<(), CommandError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(CommandError::InvalidMagnitude { field })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MoveEntityStepOutcome {
    Moved(Transform),
    Blocked,
    Noop,
}

fn transform_after_move_entity_step(
    world: &WorldState,
    mut transform: Transform,
    step: MoveEntityStepCommand,
) -> MoveEntityStepOutcome {
    let current = transform.translation_meters;
    let (dx, dy, travel_distance) = match step.mode {
        MoveEntityStepMode::Toward => {
            let dx = step.reference_meters.x - current.x;
            let dy = step.reference_meters.y - current.y;
            let horizontal_distance = (dx * dx + dy * dy).sqrt();
            let travel_distance =
                (horizontal_distance - step.stop_radius_meters).min(step.max_step_meters);
            (dx, dy, travel_distance)
        }
        MoveEntityStepMode::AwayFrom => {
            let dx = current.x - step.reference_meters.x;
            let dy = current.y - step.reference_meters.y;
            (dx, dy, step.max_step_meters)
        }
    };

    if travel_distance <= NAVIGATION_MIN_PROGRESS_METERS {
        return MoveEntityStepOutcome::Noop;
    }

    let horizontal_distance = (dx * dx + dy * dy).sqrt();
    let (direction_x, direction_y) = if horizontal_distance > f32::EPSILON {
        (dx / horizontal_distance, dy / horizontal_distance)
    } else {
        (1.0, 0.0)
    };

    let proposed = Vec3::new(
        current.x + direction_x * travel_distance,
        current.y + direction_y * travel_distance,
        current.z,
    );
    let constrained = constrain_move_entity_step(world, step.entity, current, proposed);
    if current.distance(constrained) <= NAVIGATION_MIN_PROGRESS_METERS {
        return MoveEntityStepOutcome::Blocked;
    }

    transform.translation_meters = constrained;
    MoveEntityStepOutcome::Moved(transform)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavigationCollisionAxis {
    X,
    Y,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct NavigationCollisionRect {
    min: [f32; 2],
    max: [f32; 2],
}

fn constrain_move_entity_step(
    world: &WorldState,
    entity: EntityId,
    current: Vec3,
    proposed: Vec3,
) -> Vec3 {
    let current_xy = [current.x, current.y];
    let mut constrained_xy = current_xy;
    constrained_xy[0] = proposed.x;
    constrained_xy = resolve_navigation_collision_axis(
        world,
        entity,
        current_xy,
        constrained_xy,
        NavigationCollisionAxis::X,
    );

    let after_x = constrained_xy;
    constrained_xy[1] = proposed.y;
    constrained_xy = resolve_navigation_collision_axis(
        world,
        entity,
        after_x,
        constrained_xy,
        NavigationCollisionAxis::Y,
    );

    Vec3::new(constrained_xy[0], constrained_xy[1], proposed.z)
}

fn resolve_navigation_collision_axis(
    world: &WorldState,
    entity: EntityId,
    current_position: [f32; 2],
    attempted_position: [f32; 2],
    axis: NavigationCollisionAxis,
) -> [f32; 2] {
    let mut position = attempted_position;
    for rect in navigation_collision_rects(world, entity) {
        if let Some(resolved_axis_value) =
            navigation_collision_axis_resolution(current_position, position, rect, axis)
        {
            match axis {
                NavigationCollisionAxis::X => position[0] = resolved_axis_value,
                NavigationCollisionAxis::Y => position[1] = resolved_axis_value,
            }
        }
    }
    position
}

fn navigation_collision_axis_resolution(
    current_position: [f32; 2],
    attempted_position: [f32; 2],
    rect: NavigationCollisionRect,
    axis: NavigationCollisionAxis,
) -> Option<f32> {
    if navigation_point_inside_rect(attempted_position, rect) {
        return Some(match axis {
            NavigationCollisionAxis::X => navigation_resolve_axis_value(
                current_position[0],
                attempted_position[0],
                rect.min[0],
                rect.max[0],
            ),
            NavigationCollisionAxis::Y => navigation_resolve_axis_value(
                current_position[1],
                attempted_position[1],
                rect.min[1],
                rect.max[1],
            ),
        });
    }

    navigation_axis_sweep_collision_value(current_position, attempted_position, rect, axis)
}

fn navigation_axis_sweep_collision_value(
    current_position: [f32; 2],
    attempted_position: [f32; 2],
    rect: NavigationCollisionRect,
    axis: NavigationCollisionAxis,
) -> Option<f32> {
    match axis {
        NavigationCollisionAxis::X => navigation_sweep_axis_value(
            current_position[0],
            attempted_position[0],
            attempted_position[1],
            rect.min[0],
            rect.max[0],
            rect.min[1],
            rect.max[1],
        ),
        NavigationCollisionAxis::Y => navigation_sweep_axis_value(
            current_position[1],
            attempted_position[1],
            attempted_position[0],
            rect.min[1],
            rect.max[1],
            rect.min[0],
            rect.max[0],
        ),
    }
}

fn navigation_sweep_axis_value(
    current: f32,
    attempted: f32,
    perpendicular: f32,
    min: f32,
    max: f32,
    perpendicular_min: f32,
    perpendicular_max: f32,
) -> Option<f32> {
    if perpendicular <= perpendicular_min || perpendicular >= perpendicular_max {
        return None;
    }

    if current <= min && attempted > min {
        Some(min)
    } else if current >= max && attempted < max {
        Some(max)
    } else {
        None
    }
}

fn navigation_resolve_axis_value(current: f32, attempted: f32, min: f32, max: f32) -> f32 {
    if current <= min {
        min
    } else if current >= max || max - attempted < attempted - min {
        max
    } else {
        min
    }
}

fn navigation_collision_rects(
    world: &WorldState,
    moving_entity: EntityId,
) -> Vec<NavigationCollisionRect> {
    let moving_radius = navigation_radius_for_entity(world, moving_entity);
    world
        .transforms
        .iter()
        .filter_map(|(entity, transform)| {
            navigation_collision_rect_for_entity(world, moving_entity, *entity, transform)
                .map(|rect| expanded_navigation_collision_rect(rect, moving_radius))
        })
        .collect()
}

fn navigation_collision_rect_for_entity(
    world: &WorldState,
    moving_entity: EntityId,
    entity: EntityId,
    transform: &Transform,
) -> Option<NavigationCollisionRect> {
    if entity == moving_entity || !world.physical_bodies.contains_key(&entity) {
        return None;
    }

    let tags = world.tags.get(&entity).map(Vec::as_slice);
    if navigation_tags_have(tags, "audio_zone:alley")
        || navigation_tags_have(tags, "ground")
        || navigation_tags_have(tags, "water_leak")
        || navigation_tags_have(tags, "hazard")
    {
        return None;
    }

    let half_extents = if navigation_tags_have(tags, "glass") || navigation_tags_have(tags, "wall")
    {
        if navigation_entity_is_fractured(world, entity) {
            return None;
        }
        NAVIGATION_WALL_HALF_EXTENTS
    } else if navigation_tags_have(tags, "npc") || world.humans.contains_key(&entity) {
        NAVIGATION_HUMAN_HALF_EXTENTS
    } else {
        if transform.translation_meters.z > NAVIGATION_OVERHEAD_Z_METERS {
            return None;
        }
        NAVIGATION_PROP_HALF_EXTENTS
    };

    Some(NavigationCollisionRect {
        min: [
            transform.translation_meters.x - half_extents[0],
            transform.translation_meters.y - half_extents[1],
        ],
        max: [
            transform.translation_meters.x + half_extents[0],
            transform.translation_meters.y + half_extents[1],
        ],
    })
}

fn expanded_navigation_collision_rect(
    rect: NavigationCollisionRect,
    padding: f32,
) -> NavigationCollisionRect {
    NavigationCollisionRect {
        min: [rect.min[0] - padding, rect.min[1] - padding],
        max: [rect.max[0] + padding, rect.max[1] + padding],
    }
}

fn navigation_radius_for_entity(world: &WorldState, entity: EntityId) -> f32 {
    let tags = world.tags.get(&entity).map(Vec::as_slice);
    if navigation_tags_have(tags, "npc")
        || navigation_tags_have(tags, "player")
        || world.humans.contains_key(&entity)
    {
        NAVIGATION_ENTITY_RADIUS_METERS
    } else {
        NAVIGATION_PROP_RADIUS_METERS
    }
}

fn navigation_point_inside_rect(point: [f32; 2], rect: NavigationCollisionRect) -> bool {
    point[0] > rect.min[0]
        && point[0] < rect.max[0]
        && point[1] > rect.min[1]
        && point[1] < rect.max[1]
}

fn navigation_entity_is_fractured(world: &WorldState, entity: EntityId) -> bool {
    world
        .material_states
        .get(&entity)
        .is_some_and(|state| state.crack_density >= NAVIGATION_FRACTURED_CRACK_DENSITY_THRESHOLD)
}

fn navigation_tags_have(tags: Option<&[String]>, expected: &str) -> bool {
    tags.is_some_and(|tags| tags.iter().any(|tag| tag == expected))
}

fn require_finite(field: &'static str, value: f32) -> Result<(), EventValidationError> {
    value
        .is_finite()
        .then_some(())
        .ok_or(EventValidationError::InvalidMagnitude { field })
}

fn require_finite_non_negative(
    field: &'static str,
    value: f32,
) -> Result<(), EventValidationError> {
    (value.is_finite() && value >= 0.0)
        .then_some(())
        .ok_or(EventValidationError::InvalidMagnitude { field })
}

fn require_finite_vec3(field: &'static str, value: Vec3) -> Result<(), EventValidationError> {
    (value.x.is_finite() && value.y.is_finite() && value.z.is_finite())
        .then_some(())
        .ok_or(EventValidationError::InvalidMagnitude { field })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(
        event_id: WorldEventId,
        tick: u64,
        location_meters: Vec3,
        actors: Vec<EntityId>,
        kind: WorldEventKind,
        physical_evidence: &[&str],
        narrative_tags: &[&str],
    ) -> WorldEvent {
        WorldEvent {
            event_id,
            tick,
            location_meters,
            actors,
            kind,
            physical_evidence: physical_evidence
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            narrative_tags: narrative_tags
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        }
    }

    fn primary_interaction_world(fractured_glass: bool) -> WorldState {
        let mut world = WorldState::default();
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(1),
                name: "player".to_string(),
                transform: Transform::at(Vec3::ZERO),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 80.0,
                    material_id: 1,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["player".to_string()],
            })
            .expect("spawn player");
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(2),
                name: "alley glass wall".to_string(),
                transform: Transform::at(Vec3::new(2.0, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 140.0,
                    material_id: 2,
                    dynamic: false,
                    fragile: true,
                }),
                material_state: Some(MaterialState {
                    crack_density: if fractured_glass { 0.8 } else { 0.0 },
                    ..MaterialState::default()
                }),
                human: None,
                agent: None,
                tags: vec![
                    "glass".to_string(),
                    "wall".to_string(),
                    "destructible".to_string(),
                ],
            })
            .expect("spawn glass");
        world
    }

    #[test]
    fn primary_interaction_selects_reachable_destructible_and_builds_force() {
        let world = primary_interaction_world(false);
        let snapshot = world.snapshot(1, SimTime::default());
        let settings = PrimaryInteractionSettings::new(1);

        let interaction = primary_interaction_from_snapshot(&snapshot, 0.0, settings)
            .expect("glass should be targetable");

        assert_eq!(interaction.entity, 2);
        assert_eq!(interaction.label, "alley glass wall");
        assert!(interaction.in_reach);
        assert!(!interaction.in_aim_cone);
        assert!(!interaction.already_fractured);
        assert_eq!(interaction.force.entity, 2);
        assert_eq!(interaction.force.source, Some(1));
        assert!(interaction.force.vector_newtons.x > 1_900.0);
        assert!(interaction.force.vector_newtons.y.abs() < 0.001);
        assert_eq!(interaction.status_summary(), "target alley glass wall");
    }

    #[test]
    fn primary_interaction_marks_fractured_targets_without_hiding_them() {
        let world = primary_interaction_world(true);
        let snapshot = world.snapshot(1, SimTime::default());
        let settings = PrimaryInteractionSettings::new(1);

        let interaction = primary_interaction_from_snapshot(&snapshot, 0.0, settings)
            .expect("fractured glass remains previewable");

        assert!(primary_interaction_entity_is_fractured(
            &snapshot, 2, settings
        ));
        assert!(interaction.already_fractured);
        assert_eq!(interaction.status_summary(), "fractured alley glass wall");
    }

    #[test]
    fn event_ledger_query_supports_writeup_filters() {
        let mut ledger = EventLedger::default();
        ledger.record(event(
            1,
            10,
            Vec3::ZERO,
            vec![1],
            WorldEventKind::GlassWallFractured { entity: 1 },
            &["fracture_impulse", "glass_shards"],
            &["crime", "loud"],
        ));
        ledger.record(event(
            2,
            11,
            Vec3::new(5.0, 0.0, 0.0),
            vec![2],
            WorldEventKind::FactionReputationChanged {
                faction: 700,
                subject: 2,
                delta: -0.25,
                reason: "public property damage".to_string(),
            },
            &["witness_statement"],
            &["story_consequence", "security"],
        ));
        ledger.record(event(
            3,
            12,
            Vec3::new(0.5, 0.0, 0.0),
            vec![1],
            WorldEventKind::SecurityAlertRaised {
                faction: 700,
                source_event: 1,
                threat: 1,
                severity: 0.8,
            },
            &["camera_feed", "security_feed"],
            &["security", "crime"],
        ));
        ledger.record(event(
            4,
            13,
            Vec3::new(0.25, 0.0, 0.0),
            vec![3],
            WorldEventKind::MetalBent {
                entity: 3,
                plastic_strain: 0.5,
            },
            &["plastic_strain"],
            &["metal", "collision"],
        ));

        let faction_alerts = ledger.query(
            &EventFilter::default()
                .with_faction(700)
                .with_story_tag("security")
                .with_min_evidence_count(2),
        );
        assert_eq!(faction_alerts.len(), 1);
        assert_eq!(faction_alerts[0].event_id, 3);

        let nearby_fractures = ledger.query(
            &EventFilter::default()
                .with_actor(1)
                .with_physical_type(PhysicalEventType::Fracture)
                .near(Vec3::ZERO, 1.0),
        );
        assert_eq!(nearby_fractures.len(), 1);
        assert_eq!(nearby_fractures[0].event_id, 1);

        let limited_crimes = ledger.query(
            &EventFilter::default()
                .with_tick_range(10, 12)
                .with_story_tag("crime")
                .near(Vec3::ZERO, 1.0)
                .limited_to(1),
        );
        assert_eq!(limited_crimes.len(), 1);
        assert_eq!(limited_crimes[0].event_id, 1);

        assert_eq!(ledger.by_faction(700).len(), 2);
        assert_eq!(
            ledger.by_physical_type(PhysicalEventType::Fracture).len(),
            1
        );
        assert_eq!(ledger.by_story_tag("crime").len(), 2);
        assert_eq!(ledger.by_evidence_strength(2).len(), 2);
        assert!(
            ledger
                .query(&EventFilter::default().near(Vec3::ZERO, f32::NAN))
                .is_empty()
        );
        assert!(
            ledger
                .query(&EventFilter::default().limited_to(0))
                .is_empty()
        );
    }

    #[test]
    fn transactional_command_batch_rejects_without_partial_mutation() {
        let mut world = WorldState::default();
        let entity = world
            .spawn_entity_template(EntityTemplate::new(
                "transaction prop",
                Transform::at(Vec3::ZERO),
            ))
            .expect("entity should spawn");
        let commands = vec![
            WorldCommand::ApplyForce(ForceCommand {
                entity,
                vector_newtons: Vec3::new(1.0, 0.0, 0.0),
                impulse_newton_seconds: 1.0,
                source: None,
            }),
            WorldCommand::ApplyForce(ForceCommand {
                entity: 999,
                vector_newtons: Vec3::new(1.0, 0.0, 0.0),
                impulse_newton_seconds: 1.0,
                source: None,
            }),
        ];

        let error = world
            .apply_commands_transactional(commands, 1)
            .expect_err("second command should fail validation");

        assert_eq!(error, CommandError::MissingEntity(999));
        assert!(world.save_data().pending_forces.is_empty());
    }

    #[test]
    fn transactional_command_batch_applies_valid_commands_in_order() {
        let mut world = WorldState::default();
        let entity = world
            .spawn_entity_template(EntityTemplate::new(
                "transaction prop",
                Transform::at(Vec3::ZERO),
            ))
            .expect("entity should spawn");
        let commands = vec![
            WorldCommand::ApplyForce(ForceCommand {
                entity,
                vector_newtons: Vec3::new(1.0, 0.0, 0.0),
                impulse_newton_seconds: 1.0,
                source: None,
            }),
            WorldCommand::SetTransform(entity, Transform::at(Vec3::new(2.0, 0.0, 0.0))),
        ];

        let events = world
            .apply_commands_transactional(commands, 1)
            .expect("commands should apply");

        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0].kind,
            WorldEventKind::ForceApplied { .. }
        ));
        assert!(matches!(
            events[1].kind,
            WorldEventKind::TransformChanged { .. }
        ));
        assert_eq!(world.save_data().pending_forces.len(), 1);
        assert_eq!(
            world
                .snapshot(1, SimTime::new(1.0 / 60.0, 1))
                .transforms
                .find(entity)
                .expect("entity transform should exist")
                .translation_meters,
            Vec3::new(2.0, 0.0, 0.0)
        );
    }

    #[test]
    fn move_entity_step_toward_target_preserves_standoff_in_core() {
        let mut world = WorldState::default();
        let entity = world
            .spawn_entity_template(EntityTemplate::new(
                "investigating npc",
                Transform::at(Vec3::new(5.5, 0.0, 0.25)),
            ))
            .expect("entity should spawn");

        let events = world
            .apply_commands_transactional(
                vec![WorldCommand::MoveEntityStep(MoveEntityStepCommand {
                    entity,
                    mode: MoveEntityStepMode::Toward,
                    reference_meters: Vec3::new(2.0, 0.0, 0.0),
                    max_step_meters: 1.5,
                    stop_radius_meters: 3.0,
                })],
                1,
            )
            .expect("movement step should apply");

        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].kind,
            WorldEventKind::TransformChanged { entity: changed } if changed == entity
        ));
        let transform = world
            .snapshot(1, SimTime::new(1.0 / 60.0, 1))
            .transforms
            .find(entity)
            .copied()
            .expect("entity transform should exist");
        assert_eq!(transform.translation_meters, Vec3::new(5.0, 0.0, 0.25));
    }

    #[test]
    fn move_entity_step_away_from_source_uses_core_transform_update() {
        let mut world = WorldState::default();
        let entity = world
            .spawn_entity_template(EntityTemplate::new(
                "fleeing npc",
                Transform::at(Vec3::new(5.5, 0.0, 0.25)),
            ))
            .expect("entity should spawn");

        world
            .apply_commands_transactional(
                vec![WorldCommand::MoveEntityStep(MoveEntityStepCommand {
                    entity,
                    mode: MoveEntityStepMode::AwayFrom,
                    reference_meters: Vec3::new(4.0, 0.0, 0.0),
                    max_step_meters: 2.0,
                    stop_radius_meters: 0.0,
                })],
                1,
            )
            .expect("movement step should apply");

        let transform = world
            .snapshot(1, SimTime::new(1.0 / 60.0, 1))
            .transforms
            .find(entity)
            .copied()
            .expect("entity transform should exist");
        assert_eq!(transform.translation_meters, Vec3::new(7.5, 0.0, 0.25));
    }

    #[test]
    fn move_entity_step_stops_before_intact_solid_footprints() {
        let mut world = WorldState::default();
        let entity = world
            .spawn_entity_template(EntityTemplate {
                entity_id: None,
                name: "investigating npc".to_string(),
                transform: Transform::at(Vec3::new(5.5, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 68.0,
                    material_id: 1,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: Some(HumanState {
                    human_id: 9,
                    quality_tier: QualityTier::NormalRuntime,
                }),
                agent: None,
                tags: vec!["npc".to_string()],
            })
            .expect("entity should spawn");
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: None,
                name: "intact glass wall".to_string(),
                transform: Transform::at(Vec3::new(2.0, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 140.0,
                    material_id: 1,
                    dynamic: false,
                    fragile: true,
                }),
                material_state: Some(MaterialState::default()),
                human: None,
                agent: None,
                tags: vec!["glass".to_string(), "wall".to_string()],
            })
            .expect("wall should spawn");

        world
            .apply_commands_transactional(
                vec![WorldCommand::MoveEntityStep(MoveEntityStepCommand {
                    entity,
                    mode: MoveEntityStepMode::Toward,
                    reference_meters: Vec3::ZERO,
                    max_step_meters: 10.0,
                    stop_radius_meters: 0.0,
                })],
                1,
            )
            .expect("movement should resolve against collision");

        let transform = world
            .snapshot(1, SimTime::new(1.0 / 60.0, 1))
            .transforms
            .find(entity)
            .copied()
            .expect("entity transform should exist");
        assert!(
            (transform.translation_meters.x - 3.51).abs() <= 0.0001,
            "expected movement to stop at expanded wall edge, got {:?}",
            transform.translation_meters
        );
        assert_eq!(transform.translation_meters.y, 0.0);
    }

    #[test]
    fn move_entity_step_emits_blocked_event_when_collision_prevents_progress() {
        let mut world = WorldState::default();
        let blocked_edge = 2.0 + NAVIGATION_WALL_HALF_EXTENTS[0] + NAVIGATION_ENTITY_RADIUS_METERS;
        let entity = world
            .spawn_entity_template(EntityTemplate {
                entity_id: None,
                name: "stuck npc".to_string(),
                transform: Transform::at(Vec3::new(blocked_edge, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 68.0,
                    material_id: 1,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: Some(HumanState {
                    human_id: 9,
                    quality_tier: QualityTier::NormalRuntime,
                }),
                agent: None,
                tags: vec!["npc".to_string()],
            })
            .expect("entity should spawn");
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: None,
                name: "intact glass wall".to_string(),
                transform: Transform::at(Vec3::new(2.0, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 140.0,
                    material_id: 1,
                    dynamic: false,
                    fragile: true,
                }),
                material_state: Some(MaterialState::default()),
                human: None,
                agent: None,
                tags: vec!["glass".to_string(), "wall".to_string()],
            })
            .expect("wall should spawn");

        let events = world
            .apply_commands_transactional(
                vec![WorldCommand::MoveEntityStep(MoveEntityStepCommand {
                    entity,
                    mode: MoveEntityStepMode::Toward,
                    reference_meters: Vec3::ZERO,
                    max_step_meters: 1.0,
                    stop_radius_meters: 0.0,
                })],
                1,
            )
            .expect("blocked movement should emit an event");

        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].kind,
            WorldEventKind::NavigationMoveBlocked { entity: blocked } if blocked == entity
        ));
        assert_eq!(events[0].location_meters, Vec3::new(blocked_edge, 0.0, 0.0));
        assert!(
            events[0]
                .physical_evidence
                .iter()
                .any(|tag| { tag == "navigation_blocked" })
        );
        let transform = world
            .snapshot(1, SimTime::new(1.0 / 60.0, 1))
            .transforms
            .find(entity)
            .copied()
            .expect("entity transform should exist");
        assert_eq!(
            transform.translation_meters,
            Vec3::new(blocked_edge, 0.0, 0.0)
        );
    }

    #[test]
    fn move_entity_step_allows_fractured_glass_to_become_passable() {
        let mut world = WorldState::default();
        let entity = world
            .spawn_entity_template(EntityTemplate {
                entity_id: None,
                name: "investigating npc".to_string(),
                transform: Transform::at(Vec3::new(5.5, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 68.0,
                    material_id: 1,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: Some(HumanState {
                    human_id: 9,
                    quality_tier: QualityTier::NormalRuntime,
                }),
                agent: None,
                tags: vec!["npc".to_string()],
            })
            .expect("entity should spawn");
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: None,
                name: "fractured glass wall".to_string(),
                transform: Transform::at(Vec3::new(2.0, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 140.0,
                    material_id: 1,
                    dynamic: false,
                    fragile: true,
                }),
                material_state: Some(MaterialState {
                    crack_density: 0.7,
                    ..MaterialState::default()
                }),
                human: None,
                agent: None,
                tags: vec!["glass".to_string(), "wall".to_string()],
            })
            .expect("wall should spawn");

        world
            .apply_commands_transactional(
                vec![WorldCommand::MoveEntityStep(MoveEntityStepCommand {
                    entity,
                    mode: MoveEntityStepMode::Toward,
                    reference_meters: Vec3::ZERO,
                    max_step_meters: 10.0,
                    stop_radius_meters: 0.0,
                })],
                1,
            )
            .expect("movement should pass through fractured glass");

        let transform = world
            .snapshot(1, SimTime::new(1.0 / 60.0, 1))
            .transforms
            .find(entity)
            .copied()
            .expect("entity transform should exist");
        assert_eq!(transform.translation_meters, Vec3::ZERO);
    }

    #[test]
    fn move_entity_step_rejects_invalid_magnitudes_transactionally() {
        let mut world = WorldState::default();
        let entity = world
            .spawn_entity_template(EntityTemplate::new(
                "invalid move npc",
                Transform::at(Vec3::new(5.5, 0.0, 0.25)),
            ))
            .expect("entity should spawn");

        let error = world
            .apply_commands_transactional(
                vec![WorldCommand::MoveEntityStep(MoveEntityStepCommand {
                    entity,
                    mode: MoveEntityStepMode::Toward,
                    reference_meters: Vec3::new(2.0, 0.0, 0.0),
                    max_step_meters: f32::NAN,
                    stop_radius_meters: 3.0,
                })],
                1,
            )
            .expect_err("invalid movement should fail validation");

        assert_eq!(
            error,
            CommandError::InvalidMagnitude {
                field: "max_step_meters"
            }
        );
        let transform = world
            .snapshot(1, SimTime::new(1.0 / 60.0, 1))
            .transforms
            .find(entity)
            .copied()
            .expect("entity transform should exist");
        assert_eq!(transform.translation_meters, Vec3::new(5.5, 0.0, 0.25));
    }

    #[test]
    fn set_agent_state_updates_agent_component_transactionally() {
        let mut world = WorldState::default();
        let mut template = EntityTemplate::new("alarmed npc", Transform::at(Vec3::ZERO));
        template.agent = Some(AgentState {
            persona: 7,
            emotional_state: EmotionState::default(),
            ai_lod: QualityTier::NormalRuntime,
        });
        let entity = world
            .spawn_entity_template(template)
            .expect("agent entity should spawn");
        let updated = AgentState {
            persona: 7,
            emotional_state: EmotionState::alarmed(),
            ai_lod: QualityTier::NormalRuntime,
        };

        let events = world
            .apply_commands_transactional(vec![WorldCommand::SetAgentState(entity, updated)], 1)
            .expect("agent update should apply");

        assert!(matches!(
            events[0].kind,
            WorldEventKind::AgentStateChanged { entity: changed } if changed == entity
        ));
        let snapshot = world.snapshot(1, SimTime::new(1.0 / 60.0, 1));
        assert_eq!(
            snapshot
                .agents
                .find(entity)
                .expect("agent state should exist")
                .emotional_state,
            EmotionState::alarmed()
        );
    }

    #[test]
    fn population_summary_validates_v5_schema_budgets_and_lod_counts() {
        let summary = PopulationSummary {
            expected_active_npcs: 3,
            expected_background_crowd: 72,
            expected_vehicle_or_transit_count: 6,
            crowd_spawn_rule_count: 3,
            traffic_rule_count: 2,
            simulated_crowd_rule_count: 2,
            summary_only_crowd_rule_count: 1,
            average_crowd_density: 0.76,
            average_traffic_density: 0.48,
            faction_control_count: 2,
            alertness: 0.62,
            commerce_activity: 0.88,
            observation_coverage: 0.56,
            ..PopulationSummary::default()
        };

        let report = validate_population_summary(&summary);

        assert!(report.passed);
        assert_eq!(report.issue_count(), 0);
        assert_eq!(summary.total_expected_population(), 81);
        assert!(summary.normalized_activity() > 0.35);
    }

    #[test]
    fn population_summary_rejects_unbounded_or_ungrounded_counts() {
        let summary = PopulationSummary {
            expected_background_crowd: 300,
            expected_vehicle_or_transit_count: 9,
            average_crowd_density: 1.4,
            simulated_crowd_rule_count: 2,
            summary_only_crowd_rule_count: 1,
            ..PopulationSummary::default()
        };

        let report = validate_population_summary(&summary);

        assert!(!report.passed);
        assert!(report.issues.iter().any(|issue| {
            issue.code == "population_summary_unbounded_crowd"
                && issue.message.contains("runtime population budget")
        }));
        assert!(
            report
                .issues
                .iter()
                .any(|issue| { issue.code == "population_summary_missing_crowd_rules" })
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| { issue.code == "population_summary_missing_traffic_rules" })
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| { issue.code == "population_summary_lod_count_mismatch" })
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| { issue.code == "population_summary_invalid_crowd_density" })
        );
    }
}

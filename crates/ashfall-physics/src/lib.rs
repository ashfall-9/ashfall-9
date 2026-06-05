use std::collections::{BTreeMap, BTreeSet};

use ashfall_core::core::*;
use ashfall_core::gpu::{
    ComputePipelineHandle, GpuDispatchKind, GpuGraphBuilder, GpuPassDesc, GpuPipelineDesc,
    GpuQueueKind, GpuResourceDesc, GpuResourceHandle, GpuResourceKind, GpuResourceLifetime,
};
use ashfall_core::runtime::{EngineModule, FrameContext, ModuleDescriptor, ModuleStateRecord};
use ashfall_core::world::{
    AudioEvent, AudioEventKind, CommandSink, PhysicalBody, Renderable, WorldCommand, WorldEvent,
    WorldEventKind, WorldSnapshot,
};

pub type BodyView = Vec<BodyRecord>;
pub type MaterialTable = Vec<MaterialDescriptor>;
pub type MaterialStateTable = Vec<(EntityId, MaterialState)>;
pub type ConstraintCommand = PhysicsConstraint;
pub type ContactManifold = Vec<ContactEvent>;
pub type MeshHandle = MeshAssetHandle;
pub type GpuBufferHandle = GpuResourceHandle;
pub type PhysicsTimingReport = PerformanceCounters;
pub type FluidRegionId = u64;
pub type VolumeFieldId = u64;
const PHYSICS_MODULE_STATE_VERSION: u32 = 1;
pub const PLANAR_MOTION_DEFAULT_RADIUS_METERS: f32 = 0.36;
pub const PLANAR_MOTION_DEFAULT_OVERHEAD_Z_METERS: f32 = 1.65;
pub const PLANAR_MOTION_DEFAULT_FRACTURED_CRACK_DENSITY_THRESHOLD: f32 = 0.5;
pub const PLANAR_MOTION_DEFAULT_WALL_HALF_EXTENTS: [f32; 2] = [1.15, 0.08];
pub const PLANAR_MOTION_DEFAULT_HUMAN_HALF_EXTENTS: [f32; 2] = [0.42, 0.3];
pub const PLANAR_MOTION_DEFAULT_PROP_HALF_EXTENTS: [f32; 2] = [0.34, 0.34];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanarMotionSettings {
    pub bounds_min: [f32; 2],
    pub bounds_max: [f32; 2],
    pub moving_radius_meters: f32,
    pub overhead_z_meters: f32,
    pub fractured_crack_density_threshold: f32,
    pub wall_half_extents: [f32; 2],
    pub human_half_extents: [f32; 2],
    pub prop_half_extents: [f32; 2],
}

impl PlanarMotionSettings {
    pub fn new(bounds_min: [f32; 2], bounds_max: [f32; 2]) -> Self {
        Self {
            bounds_min,
            bounds_max,
            ..Self::default()
        }
    }

    pub fn clamp_to_bounds(&self, position: [f32; 2]) -> [f32; 2] {
        [
            position[0].clamp(self.bounds_min[0], self.bounds_max[0]),
            position[1].clamp(self.bounds_min[1], self.bounds_max[1]),
        ]
    }
}

impl Default for PlanarMotionSettings {
    fn default() -> Self {
        Self {
            bounds_min: [f32::NEG_INFINITY, f32::NEG_INFINITY],
            bounds_max: [f32::INFINITY, f32::INFINITY],
            moving_radius_meters: PLANAR_MOTION_DEFAULT_RADIUS_METERS,
            overhead_z_meters: PLANAR_MOTION_DEFAULT_OVERHEAD_Z_METERS,
            fractured_crack_density_threshold:
                PLANAR_MOTION_DEFAULT_FRACTURED_CRACK_DENSITY_THRESHOLD,
            wall_half_extents: PLANAR_MOTION_DEFAULT_WALL_HALF_EXTENTS,
            human_half_extents: PLANAR_MOTION_DEFAULT_HUMAN_HALF_EXTENTS,
            prop_half_extents: PLANAR_MOTION_DEFAULT_PROP_HALF_EXTENTS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PlanarCollisionRect {
    min: [f32; 2],
    max: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanarCollisionAxis {
    X,
    Y,
}

pub fn constrain_planar_motion(
    snapshot: &WorldSnapshot,
    moving_entity: EntityId,
    current_position: [f32; 2],
    proposed_position: [f32; 2],
    settings: PlanarMotionSettings,
) -> [f32; 2] {
    let current_position = settings.clamp_to_bounds(current_position);
    let mut constrained = current_position;

    constrained[0] = proposed_position[0];
    constrained = resolve_planar_collision_axis(
        snapshot,
        moving_entity,
        current_position,
        constrained,
        PlanarCollisionAxis::X,
        settings,
    );

    let after_x = constrained;
    constrained[1] = proposed_position[1];
    constrained = resolve_planar_collision_axis(
        snapshot,
        moving_entity,
        after_x,
        constrained,
        PlanarCollisionAxis::Y,
        settings,
    );

    settings.clamp_to_bounds(constrained)
}

fn resolve_planar_collision_axis(
    snapshot: &WorldSnapshot,
    moving_entity: EntityId,
    current_position: [f32; 2],
    attempted_position: [f32; 2],
    axis: PlanarCollisionAxis,
    settings: PlanarMotionSettings,
) -> [f32; 2] {
    let mut position = settings.clamp_to_bounds(attempted_position);
    for rect in planar_collision_rects(snapshot, moving_entity, settings) {
        if let Some(resolved_axis_value) =
            planar_collision_axis_resolution(current_position, position, rect, axis)
        {
            match axis {
                PlanarCollisionAxis::X => position[0] = resolved_axis_value,
                PlanarCollisionAxis::Y => position[1] = resolved_axis_value,
            }
        }
    }
    settings.clamp_to_bounds(position)
}

fn planar_collision_axis_resolution(
    current_position: [f32; 2],
    attempted_position: [f32; 2],
    rect: PlanarCollisionRect,
    axis: PlanarCollisionAxis,
) -> Option<f32> {
    if planar_point_inside_rect(attempted_position, rect) {
        return Some(match axis {
            PlanarCollisionAxis::X => planar_resolve_axis_value(
                current_position[0],
                attempted_position[0],
                rect.min[0],
                rect.max[0],
            ),
            PlanarCollisionAxis::Y => planar_resolve_axis_value(
                current_position[1],
                attempted_position[1],
                rect.min[1],
                rect.max[1],
            ),
        });
    }

    planar_axis_sweep_collision_value(current_position, attempted_position, rect, axis)
}

fn planar_axis_sweep_collision_value(
    current_position: [f32; 2],
    attempted_position: [f32; 2],
    rect: PlanarCollisionRect,
    axis: PlanarCollisionAxis,
) -> Option<f32> {
    match axis {
        PlanarCollisionAxis::X => planar_sweep_axis_value(
            current_position[0],
            attempted_position[0],
            attempted_position[1],
            rect.min[0],
            rect.max[0],
            rect.min[1],
            rect.max[1],
        ),
        PlanarCollisionAxis::Y => planar_sweep_axis_value(
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

fn planar_sweep_axis_value(
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

fn planar_resolve_axis_value(current: f32, attempted: f32, min: f32, max: f32) -> f32 {
    if current <= min {
        min
    } else if current >= max || max - attempted < attempted - min {
        max
    } else {
        min
    }
}

fn planar_collision_rects(
    snapshot: &WorldSnapshot,
    moving_entity: EntityId,
    settings: PlanarMotionSettings,
) -> Vec<PlanarCollisionRect> {
    snapshot
        .transforms
        .iter()
        .flat_map(|(entity, transform)| {
            planar_collision_rect_for_entity(snapshot, moving_entity, *entity, transform, settings)
        })
        .collect()
}

fn planar_collision_rect_for_entity(
    snapshot: &WorldSnapshot,
    moving_entity: EntityId,
    entity: EntityId,
    transform: &Transform,
    settings: PlanarMotionSettings,
) -> Vec<PlanarCollisionRect> {
    if entity == moving_entity || snapshot.physical_bodies.find(entity).is_none() {
        return Vec::new();
    }

    let tags = snapshot.tags.find(entity).map(Vec::as_slice);
    if physics_tags_have(tags, "audio_zone:alley")
        || physics_tags_have(tags, "ground")
        || physics_tags_have(tags, "water_leak")
        || physics_tags_have(tags, "hazard")
    {
        return Vec::new();
    }

    let center = [
        transform.translation_meters.x,
        transform.translation_meters.y,
    ];
    let half_extents = if physics_tags_have(tags, "glass") || physics_tags_have(tags, "wall") {
        if planar_entity_is_fractured(snapshot, entity, settings) {
            return Vec::new();
        }
        settings.wall_half_extents
    } else if physics_tags_have(tags, "npc") || snapshot.humans.find(entity).is_some() {
        settings.human_half_extents
    } else {
        if transform.translation_meters.z > settings.overhead_z_meters {
            return Vec::new();
        }
        settings.prop_half_extents
    };

    vec![expanded_planar_collision_rect(
        center,
        half_extents,
        settings.moving_radius_meters,
    )]
}

fn planar_entity_is_fractured(
    snapshot: &WorldSnapshot,
    entity: EntityId,
    settings: PlanarMotionSettings,
) -> bool {
    snapshot
        .material_states
        .find(entity)
        .is_some_and(|state| state.crack_density >= settings.fractured_crack_density_threshold)
}

fn expanded_planar_collision_rect(
    center: [f32; 2],
    half_extents: [f32; 2],
    padding: f32,
) -> PlanarCollisionRect {
    PlanarCollisionRect {
        min: [
            center[0] - half_extents[0] - padding,
            center[1] - half_extents[1] - padding,
        ],
        max: [
            center[0] + half_extents[0] + padding,
            center[1] + half_extents[1] + padding,
        ],
    }
}

fn planar_point_inside_rect(point: [f32; 2], rect: PlanarCollisionRect) -> bool {
    point[0] > rect.min[0]
        && point[0] < rect.max[0]
        && point[1] > rect.min[1]
        && point[1] < rect.max[1]
}

fn physics_tags_have(tags: Option<&[String]>, expected: &str) -> bool {
    tags.is_some_and(|tags| tags.iter().any(|tag| tag == expected))
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsInput {
    pub tick: u64,
    pub dt_seconds: f32,
    pub bodies: BodyView,
    pub material_table: MaterialTable,
    pub material_states: MaterialStateTable,
    pub forces: Vec<ForceCommand>,
    pub damage_events: Vec<DamageCommand>,
    pub constraints: Vec<ConstraintCommand>,
    pub world_fields: WorldFieldView,
    pub render_feedback: RenderImportanceFeedback,
    pub quality_budget: PhysicsBudget,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BodyRecord {
    pub entity: EntityId,
    pub name: String,
    pub tags: TagSet,
    pub transform: Transform,
    pub renderable: Option<Renderable>,
    pub physical_body: PhysicalBody,
    pub velocity: Velocity,
    pub visible: bool,
    pub lod: SimulationLodTier,
    pub story_importance: f32,
    pub lod_signals: SimulationLodSignals,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldFieldView {
    pub gravity_meters_per_second2: Vec3,
    pub wind_meters_per_second: Vec3,
    pub ambient_temperature_kelvin: f32,
    pub water_level_meters: f32,
    pub gas_fields: Vec<VolumeFieldId>,
}

impl Default for WorldFieldView {
    fn default() -> Self {
        Self {
            gravity_meters_per_second2: Vec3::new(0.0, -9.81, 0.0),
            wind_meters_per_second: Vec3::ZERO,
            ambient_temperature_kelvin: 293.15,
            water_level_meters: 0.0,
            gas_fields: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsBudget {
    pub quality: QualityTier,
    pub max_fracture_pieces: usize,
    pub max_active_contacts: usize,
    pub max_fluid_regions: usize,
    pub max_gas_regions: usize,
    pub hero_radius_meters: f32,
}

impl PhysicsBudget {
    pub fn for_quality(quality: QualityTier) -> Self {
        match quality {
            QualityTier::Disabled => Self {
                quality,
                max_fracture_pieces: 0,
                max_active_contacts: 0,
                max_fluid_regions: 0,
                max_gas_regions: 0,
                hero_radius_meters: 0.0,
            },
            QualityTier::BackgroundApproximation => Self {
                quality,
                max_fracture_pieces: 2,
                max_active_contacts: 8,
                max_fluid_regions: 1,
                max_gas_regions: 1,
                hero_radius_meters: 6.0,
            },
            QualityTier::NormalRuntime => Self {
                quality,
                max_fracture_pieces: 8,
                max_active_contacts: 64,
                max_fluid_regions: 4,
                max_gas_regions: 3,
                hero_radius_meters: 18.0,
            },
            QualityTier::HeroHighFidelityRuntime => Self {
                quality,
                max_fracture_pieces: 24,
                max_active_contacts: 128,
                max_fluid_regions: 8,
                max_gas_regions: 6,
                hero_radius_meters: 32.0,
            },
            QualityTier::ReferenceOfflineValidation => Self {
                quality,
                max_fracture_pieces: 64,
                max_active_contacts: 512,
                max_fluid_regions: 16,
                max_gas_regions: 12,
                hero_radius_meters: 64.0,
            },
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderImportanceFeedback {
    pub visible_entities: Vec<EntityId>,
    pub screen_size_scores: Vec<(EntityId, f32)>,
    pub camera_distance_scores: Vec<(EntityId, f32)>,
    pub requested_physics_lod: Vec<SimulationLodRequest>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimulationLodRequest {
    pub entity: EntityId,
    pub requested_lod: SimulationLodTier,
    pub priority: f32,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SimulationLodSignals {
    pub camera_importance: f32,
    pub energy_score: f32,
    pub player_interaction: bool,
    pub high_energy: bool,
    pub ai_focus: bool,
    pub story_important: bool,
    pub mission_or_faction_relevant: bool,
    pub renderer_requested: bool,
    pub hazard_volume: bool,
    pub settled: bool,
    pub requested_lod: Option<SimulationLodTier>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SimulationLodTier {
    Dormant,
    SimpleRigid,
    RigidWithMaterialState,
    LocalDeformation,
    HeroSimulation,
    ReferenceValidation,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PhysicsConstraint {
    FixedJoint {
        a: EntityId,
        b: EntityId,
        break_impulse: f32,
    },
    Cable {
        entity: EntityId,
        anchor: Vec3,
        max_length_meters: f32,
    },
    ClothPin {
        entity: EntityId,
        attach_point: String,
        stiffness: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsOutput {
    pub tick: u64,
    pub transforms: Vec<(EntityId, Transform)>,
    pub velocities: Vec<(EntityId, Velocity)>,
    pub contacts: Vec<ContactEvent>,
    pub fractures: Vec<FractureEvent>,
    pub plastic_deformations: Vec<PlasticDeformationEvent>,
    pub constraint_resolutions: Vec<ConstraintResolutionEvent>,
    pub mesh_deltas: Vec<MeshDelta>,
    pub fluid_deltas: Vec<FluidDelta>,
    pub gas_deltas: Vec<GasDelta>,
    pub material_state_deltas: Vec<MaterialStateDelta>,
    pub audio_events: Vec<AudioEvent>,
    pub world_events: Vec<WorldEvent>,
    pub solver_reports: Vec<SolverReport>,
    pub debug_report: PhysicsDebugReport,
    pub validation_report: PhysicsValidationReport,
    pub timing: PhysicsTimingReport,
}

impl PhysicsOutput {
    pub fn empty(tick: u64) -> Self {
        Self {
            tick,
            transforms: Vec::new(),
            velocities: Vec::new(),
            contacts: Vec::new(),
            fractures: Vec::new(),
            plastic_deformations: Vec::new(),
            constraint_resolutions: Vec::new(),
            mesh_deltas: Vec::new(),
            fluid_deltas: Vec::new(),
            gas_deltas: Vec::new(),
            material_state_deltas: Vec::new(),
            audio_events: Vec::new(),
            world_events: Vec::new(),
            solver_reports: Vec::new(),
            debug_report: PhysicsDebugReport::empty(tick),
            validation_report: PhysicsValidationReport::default(),
            timing: PerformanceCounters::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SolverReport {
    pub family: SolverFamily,
    pub active_entities: usize,
    pub output_count: usize,
    pub cpu_milliseconds: f32,
    pub gpu_milliseconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolverFamily {
    RigidBody,
    Fracture,
    Liquid,
    Gas,
    SoftBody,
    PlasticDeformation,
    LodManager,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsDebugReport {
    pub tick: u64,
    pub quality: QualityTier,
    pub body_lods: Vec<BodyLodDebugEntry>,
    pub lod_summary: PhysicsLodSummary,
    pub budget_clamps: PhysicsBudgetClampReport,
    pub active_solver_families: Vec<SolverFamily>,
}

impl PhysicsDebugReport {
    pub fn empty(tick: u64) -> Self {
        Self {
            tick,
            quality: QualityTier::Disabled,
            body_lods: Vec::new(),
            lod_summary: PhysicsLodSummary::default(),
            budget_clamps: PhysicsBudgetClampReport::default(),
            active_solver_families: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BodyLodDebugEntry {
    pub entity: EntityId,
    pub name: String,
    pub lod: SimulationLodTier,
    pub reason: LodSelectionReason,
    pub distance_to_player_meters: f32,
    pub visible: bool,
    pub fragile: bool,
    pub story_importance: f32,
    pub signals: SimulationLodSignals,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhysicsLodSummary {
    pub body_count: usize,
    pub dormant_count: usize,
    pub simple_rigid_count: usize,
    pub material_state_count: usize,
    pub local_deformation_count: usize,
    pub hero_simulation_count: usize,
    pub reference_validation_count: usize,
    pub active_simulated_count: usize,
    pub promotion_count: usize,
    pub demotion_count: usize,
    pub high_energy_count: usize,
    pub ai_focus_count: usize,
    pub renderer_requested_count: usize,
    pub mission_relevant_count: usize,
    pub hazard_volume_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LodSelectionReason {
    Disabled,
    ReferenceValidation,
    RendererRequested,
    PlayerInteraction,
    HighEnergy,
    MissionOrFaction,
    AiFocus,
    StoryImportant,
    HazardVolume,
    NearVisibleOrFragile,
    VisibleOrFragile,
    NearPlayer,
    FarSettledDormant,
    Background,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhysicsBudgetClampReport {
    pub max_contacts: usize,
    pub max_fracture_pieces: usize,
    pub max_fluid_regions: usize,
    pub max_gas_regions: usize,
    pub pre_cap_contacts: usize,
    pub contacts: usize,
    pub pre_cap_fracture_pieces: usize,
    pub fracture_pieces: usize,
    pub pre_cap_fluid_regions: usize,
    pub fluid_regions: usize,
    pub pre_cap_gas_regions: usize,
    pub gas_regions: usize,
    pub dropped_contacts: usize,
    pub dropped_fracture_pieces: usize,
    pub dropped_fluid_regions: usize,
    pub dropped_gas_regions: usize,
}

impl PhysicsBudgetClampReport {
    pub fn clamped(&self) -> bool {
        self.dropped_contacts > 0
            || self.dropped_fracture_pieces > 0
            || self.dropped_fluid_regions > 0
            || self.dropped_gas_regions > 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContactEvent {
    pub entities: (EntityId, Option<EntityId>),
    pub location: Vec3,
    pub normal: Vec3,
    pub impulse_newton_seconds: f32,
    pub material_ids: (MaterialId, Option<MaterialId>),
    pub friction: f32,
    pub restitution: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FractureEvent {
    pub event_id: WorldEventId,
    pub source_entity: EntityId,
    pub tick: u64,
    pub impact_location: Vec3,
    pub impulse: Vec3,
    pub material_id: MaterialId,
    pub energy_estimate: f32,
    pub pieces: Vec<FracturePiece>,
    pub exposed_surfaces: Vec<ExposedSurface>,
    pub evidence_strength: f32,
    pub entity: EntityId,
    pub impact_impulse: f32,
    pub location: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlasticDeformationEvent {
    pub event_id: WorldEventId,
    pub entity: EntityId,
    pub tick: u64,
    pub location: Vec3,
    pub impulse: Vec3,
    pub material_id: MaterialId,
    pub plastic_strain_delta: f32,
    pub dent_depth_meters: f32,
    pub affected_area_square_meters: f32,
    pub evidence_strength: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstraintResolutionEvent {
    pub event_id: WorldEventId,
    pub entity: EntityId,
    pub tick: u64,
    pub kind: ConstraintResolutionKind,
    pub original_transform: Transform,
    pub corrected_transform: Transform,
    pub correction_meters: f32,
    pub strain_ratio: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConstraintResolutionKind {
    CableLength {
        anchor: Vec3,
        max_length_meters: f32,
    },
    ClothPinWorldCollision {
        attach_point: String,
        stiffness: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct FracturePiece {
    pub piece_id: u64,
    pub new_entity_hint: Option<EntityId>,
    pub mesh: MeshAssetHandle,
    pub mesh_delta: MeshDelta,
    pub collision_shape: CollisionShapeDesc,
    pub mass_kg: f32,
    pub initial_velocity: Vec3,
    pub transform: Transform,
    pub material_state: MaterialState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshDelta {
    pub entity: EntityId,
    pub replacement_mesh: Option<MeshAssetHandle>,
    pub debris_meshes: Vec<MeshAssetHandle>,
    pub exposed_surfaces: Vec<ExposedSurface>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExposedSurface {
    pub material_id: MaterialId,
    pub area_square_meters: f32,
    pub roughness: f32,
    pub evidence_tag: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CollisionShapeDesc {
    Sphere {
        radius_meters: f32,
    },
    Capsule {
        radius_meters: f32,
        half_height_meters: f32,
    },
    Box {
        half_extents_meters: Vec3,
    },
    ConvexHull {
        point_count: u32,
    },
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FluidDelta {
    pub fluid_id: u64,
    pub region_id: FluidRegionId,
    pub bounds: Aabb,
    pub volume_cubic_meters: f32,
    pub material_id: MaterialId,
    pub changed_cells: SparseCellRange,
    pub surface_mesh: Option<MeshHandle>,
    pub particles: Option<GpuBufferHandle>,
    pub wetness_changes: Vec<(EntityId, f32)>,
    pub hazard_tags: TagSet,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GasDelta {
    pub gas_id: u64,
    pub volume_id: VolumeFieldId,
    pub bounds: Aabb,
    pub density: f32,
    pub gas_material: MaterialId,
    pub density_changed: SparseCellRange,
    pub temperature_changed: SparseCellRange,
    pub pressure_changed: SparseCellRange,
    pub visibility_blocking: f32,
    pub hazard_level: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SparseCellRange {
    pub min_cell: [i32; 3],
    pub max_cell: [i32; 3],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhysicsValidationReport {
    pub passed: bool,
    pub issues: Vec<PhysicsValidationIssue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsValidationIssue {
    pub severity: PhysicsValidationSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicsValidationSeverity {
    Info,
    Warning,
    Error,
}

pub struct ConsequencePhysicsModule {
    fracture_impulse_threshold: f32,
    water_leak_radius_meters: f32,
    fractured: BTreeSet<EntityId>,
    wetted_surfaces: BTreeSet<EntityId>,
    gas_releases: BTreeSet<EntityId>,
    last_output: Option<PhysicsOutput>,
}

impl Default for ConsequencePhysicsModule {
    fn default() -> Self {
        Self {
            fracture_impulse_threshold: 30.0,
            water_leak_radius_meters: 3.0,
            fractured: BTreeSet::new(),
            wetted_surfaces: BTreeSet::new(),
            gas_releases: BTreeSet::new(),
            last_output: None,
        }
    }
}

impl ConsequencePhysicsModule {
    pub fn last_output(&self) -> Option<&PhysicsOutput> {
        self.last_output.as_ref()
    }
}

impl EngineModule for ConsequencePhysicsModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            module_id: 20,
            name: "consequence_physics",
            schema: SchemaVersion {
                name: "PhysicsOutput",
                version: 1,
            },
            default_quality: QualityTier::NormalRuntime,
        }
    }

    fn tick(&mut self, frame: &FrameContext, out: &mut CommandSink) {
        let input = physics_input_from_frame(frame, PhysicsBudget::for_quality(frame.quality_tier));
        let output = solve_consequence_frame(
            &input,
            self.fracture_impulse_threshold,
            self.water_leak_radius_meters,
            &self.fractured,
            &self.wetted_surfaces,
            &self.gas_releases,
        );

        for event in &output.world_events {
            match event.kind {
                WorldEventKind::GlassWallFractured { entity } => {
                    self.fractured.insert(entity);
                }
                WorldEventKind::StreetFlooded => {
                    for actor in &event.actors {
                        if *actor != event.actors[0] {
                            self.wetted_surfaces.insert(*actor);
                        }
                    }
                }
                WorldEventKind::ToxicGasReleased => {
                    for actor in &event.actors {
                        self.gas_releases.insert(*actor);
                    }
                }
                _ => {}
            }
            out.event(event.clone());
        }

        for (entity, transform) in &output.transforms {
            out.command(WorldCommand::SetTransform(*entity, *transform));
        }
        for delta in &output.material_state_deltas {
            out.command(WorldCommand::UpdateMaterialState(delta.entity, *delta));
        }
        for mesh_delta in &output.mesh_deltas {
            if let Some(mesh) = mesh_delta.replacement_mesh {
                out.command(WorldCommand::ReplaceMesh(mesh_delta.entity, mesh));
            }
        }
        for audio in &output.audio_events {
            out.command(WorldCommand::EmitSound(audio.clone()));
        }

        self.last_output = Some(output);
    }

    fn schedule_gpu(&mut self, graph: &mut GpuGraphBuilder) {
        let output = self.last_output.as_ref();
        let fracture_piece_count = output
            .map(|output| {
                output
                    .fractures
                    .iter()
                    .map(|fracture| fracture.pieces.len())
                    .sum::<usize>()
                    + output.plastic_deformations.len()
            })
            .unwrap_or_default();
        let fluid_cell_count = output
            .map(|output| {
                output
                    .fluid_deltas
                    .iter()
                    .map(|fluid| sparse_cell_count(&fluid.changed_cells))
                    .sum::<u64>()
            })
            .unwrap_or_default();
        let gas_cell_count = output
            .map(|output| {
                output
                    .gas_deltas
                    .iter()
                    .map(|gas| sparse_cell_count(&gas.density_changed))
                    .sum::<u64>()
            })
            .unwrap_or_default();
        let constraint_count = output
            .map(|output| output.constraint_resolutions.len())
            .unwrap_or_default();
        let lod_body_count = output
            .map(|output| output.debug_report.body_lods.len())
            .unwrap_or_default();
        let body_fields = physics_gpu_resource(
            graph,
            "physics body fields",
            GpuResourceKind::Buffer,
            2 * 1024 * 1024,
            GpuResourceLifetime::Imported,
        );
        let sparse_field_output = physics_gpu_resource(
            graph,
            "physics sparse field output",
            GpuResourceKind::Image3D,
            4 * 1024 * 1024,
            GpuResourceLifetime::Transient,
        );
        let sparse_fields_pipeline = physics_compute_pipeline(
            graph,
            "physics_sparse_fields",
            "physics/sparse_fields.comp",
            QualityTier::BackgroundApproximation,
        );
        graph.add_pass(
            GpuPassDesc::new(
                "physics_sparse_fields",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(sparse_fields_pipeline),
                QualityTier::BackgroundApproximation,
            )
            .reads([body_fields])
            .writes([sparse_field_output]),
        );

        if lod_body_count > 0 {
            let lod_body_count = u64::try_from(lod_body_count).unwrap_or(u64::MAX);
            let lod_signal_input = physics_gpu_resource(
                graph,
                "physics simulation lod signal buffer",
                GpuResourceKind::Buffer,
                physics_resource_bytes(lod_body_count, 96),
                GpuResourceLifetime::Imported,
            );
            let lod_decision_output = physics_gpu_resource(
                graph,
                "physics simulation lod decision buffer",
                GpuResourceKind::Buffer,
                physics_resource_bytes(lod_body_count, 64),
                GpuResourceLifetime::Persistent,
            );
            let lod_pipeline = physics_compute_pipeline(
                graph,
                "physics_simulation_lod_selection",
                "physics/simulation_lod_selection.comp",
                QualityTier::BackgroundApproximation,
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "physics_simulation_lod_selection",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(lod_pipeline),
                    QualityTier::BackgroundApproximation,
                )
                .reads([body_fields, lod_signal_input])
                .writes([lod_decision_output]),
            );
        }

        if fluid_cell_count > 0 {
            let liquid_cell_input = physics_gpu_resource(
                graph,
                "physics liquid sparse cells",
                GpuResourceKind::Buffer,
                physics_resource_bytes(fluid_cell_count, 32),
                GpuResourceLifetime::Imported,
            );
            let liquid_field_output = physics_gpu_resource(
                graph,
                "physics liquid sparse field output",
                GpuResourceKind::Image3D,
                physics_resource_bytes(fluid_cell_count, 256).max(1024 * 1024),
                GpuResourceLifetime::Transient,
            );
            let liquid_pipeline = physics_compute_pipeline(
                graph,
                "physics_liquid_sparse_fields",
                "physics/liquid_sparse_fields.comp",
                QualityTier::NormalRuntime,
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "physics_liquid_sparse_fields",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(liquid_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([body_fields, liquid_cell_input])
                .writes([liquid_field_output]),
            );
        }

        if gas_cell_count > 0 {
            let gas_cell_input = physics_gpu_resource(
                graph,
                "physics gas sparse cells",
                GpuResourceKind::Buffer,
                physics_resource_bytes(gas_cell_count, 48),
                GpuResourceLifetime::Imported,
            );
            let gas_density_output = physics_gpu_resource(
                graph,
                "physics gas density field output",
                GpuResourceKind::Image3D,
                physics_resource_bytes(gas_cell_count, 384).max(1024 * 1024),
                GpuResourceLifetime::Transient,
            );
            let gas_pipeline = physics_compute_pipeline(
                graph,
                "physics_gas_sparse_fields",
                "physics/gas_sparse_fields.comp",
                QualityTier::NormalRuntime,
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "physics_gas_sparse_fields",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(gas_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([body_fields, gas_cell_input])
                .writes([gas_density_output]),
            );
        }

        if fracture_piece_count > 0 {
            let fracture_piece_count = u64::try_from(fracture_piece_count).unwrap_or(u64::MAX);
            let debris_input = physics_gpu_resource(
                graph,
                "physics fracture debris input",
                GpuResourceKind::Buffer,
                physics_resource_bytes(fracture_piece_count, 128),
                GpuResourceLifetime::Imported,
            );
            let debris_output = physics_gpu_resource(
                graph,
                "physics compacted debris output",
                GpuResourceKind::Buffer,
                physics_resource_bytes(fracture_piece_count, 96),
                GpuResourceLifetime::Transient,
            );
            let debris_pipeline = physics_compute_pipeline(
                graph,
                "physics_debris_compaction",
                "physics/debris_compaction.comp",
                QualityTier::NormalRuntime,
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "physics_debris_compaction",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(debris_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([body_fields, debris_input])
                .writes([debris_output]),
            );
        }

        if constraint_count > 0 {
            let constraint_count = u64::try_from(constraint_count).unwrap_or(u64::MAX);
            let constraint_input = physics_gpu_resource(
                graph,
                "physics soft constraint input",
                GpuResourceKind::Buffer,
                physics_resource_bytes(constraint_count, 96),
                GpuResourceLifetime::Imported,
            );
            let constraint_output = physics_gpu_resource(
                graph,
                "physics soft constraint output",
                GpuResourceKind::Buffer,
                physics_resource_bytes(constraint_count, 96),
                GpuResourceLifetime::Transient,
            );
            let constraint_pipeline = physics_compute_pipeline(
                graph,
                "physics_soft_constraints",
                "physics/soft_constraints.comp",
                QualityTier::BackgroundApproximation,
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "physics_soft_constraints",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(constraint_pipeline),
                    QualityTier::BackgroundApproximation,
                )
                .reads([body_fields, constraint_input])
                .writes([constraint_output]),
            );
        }
    }

    fn performance_counters(&self) -> PerformanceCounters {
        self.last_output
            .as_ref()
            .map(|output| PerformanceCounters {
                cpu_milliseconds: output.timing.cpu_milliseconds,
                gpu_milliseconds: output.timing.gpu_milliseconds
                    + (output.debug_report.body_lods.len() as f32 * 0.002).min(0.08),
                memory_bytes: 32 * 1024 * 1024
                    + output.debug_report.body_lods.len() as u64 * 32 * 1024
                    + output.fractures.len() as u64 * 256 * 1024
                    + output.fluid_deltas.len() as u64 * 512 * 1024
                    + output.gas_deltas.len() as u64 * 512 * 1024,
            })
            .unwrap_or(PerformanceCounters {
                cpu_milliseconds: 0.55,
                gpu_milliseconds: 0.2,
                memory_bytes: 32 * 1024 * 1024,
            })
    }

    fn save_state(&self) -> Option<ModuleStateRecord> {
        let descriptor = self.descriptor();
        let mut entries = vec![
            format!("fracture_threshold:{}", self.fracture_impulse_threshold),
            format!("water_radius:{}", self.water_leak_radius_meters),
        ];
        entries.extend(
            self.fractured
                .iter()
                .map(|entity| format!("fractured:{entity}")),
        );
        entries.extend(
            self.wetted_surfaces
                .iter()
                .map(|entity| format!("wetted:{entity}")),
        );
        entries.extend(
            self.gas_releases
                .iter()
                .map(|entity| format!("gas:{entity}")),
        );
        Some(ModuleStateRecord::new(
            descriptor.module_id,
            descriptor.schema,
            PHYSICS_MODULE_STATE_VERSION,
            entries,
        ))
    }

    fn load_state(&mut self, state: &ModuleStateRecord) {
        if state.state_version != PHYSICS_MODULE_STATE_VERSION {
            return;
        }
        if let Some(value) = state
            .entries_with_prefix("fracture_threshold:")
            .next()
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
        {
            self.fracture_impulse_threshold = value;
        }
        if let Some(value) = state
            .entries_with_prefix("water_radius:")
            .next()
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
        {
            self.water_leak_radius_meters = value;
        }
        self.fractured = state
            .entries_with_prefix("fractured:")
            .filter_map(|entity| entity.parse::<EntityId>().ok())
            .collect();
        self.wetted_surfaces = state
            .entries_with_prefix("wetted:")
            .filter_map(|entity| entity.parse::<EntityId>().ok())
            .collect();
        self.gas_releases = state
            .entries_with_prefix("gas:")
            .filter_map(|entity| entity.parse::<EntityId>().ok())
            .collect();
        self.last_output = None;
    }
}

pub fn physics_input_from_frame(
    frame: &FrameContext,
    quality_budget: PhysicsBudget,
) -> PhysicsInput {
    let player_location = frame
        .snapshot
        .tags
        .iter()
        .find_map(|(entity, tags)| {
            has_tag(tags, "player")
                .then(|| frame.snapshot.transforms.find(*entity))
                .flatten()
                .map(|transform| transform.translation_meters)
        })
        .unwrap_or(Vec3::ZERO);
    let render_feedback =
        render_importance_feedback_from_snapshot(&frame.snapshot, player_location);
    let frame_lod_signals =
        simulation_lod_signal_map(frame, player_location, &render_feedback, &quality_budget);

    let bodies = frame
        .snapshot
        .physical_bodies
        .iter()
        .map(|(entity, body)| {
            let transform = frame
                .snapshot
                .transforms
                .find(*entity)
                .copied()
                .unwrap_or_default();
            let tags = frame
                .snapshot
                .tags
                .find(*entity)
                .cloned()
                .unwrap_or_default();
            let visible = frame
                .snapshot
                .renderables
                .find(*entity)
                .is_some_and(|renderable| renderable.visible);
            let distance_to_player = transform.translation_meters.distance(player_location);
            let lod_signals = frame_lod_signals.get(entity).copied().unwrap_or_else(|| {
                base_lod_signals_for_body(
                    *entity,
                    &tags,
                    body,
                    visible,
                    distance_to_player,
                    &render_feedback,
                    &quality_budget,
                )
            });
            let (lod, _) = choose_lod_with_reason(
                distance_to_player,
                visible,
                body.fragile,
                lod_signals,
                &quality_budget,
            );
            BodyRecord {
                entity: *entity,
                name: frame
                    .snapshot
                    .names
                    .find(*entity)
                    .cloned()
                    .unwrap_or_else(|| format!("entity {entity}")),
                tags,
                transform,
                renderable: frame.snapshot.renderables.find(*entity).cloned(),
                physical_body: body.clone(),
                velocity: Velocity::default(),
                visible,
                lod,
                story_importance: if lod_signals.story_important
                    || lod_signals.mission_or_faction_relevant
                    || visible
                    || body.fragile
                {
                    1.0
                } else {
                    0.25
                },
                lod_signals,
            }
        })
        .collect();

    PhysicsInput {
        tick: frame.sim_time.tick,
        dt_seconds: frame.dt_seconds,
        bodies,
        material_table: Vec::new(),
        material_states: frame
            .snapshot
            .material_states
            .iter()
            .map(|(entity, state)| (*entity, *state))
            .collect(),
        forces: frame.forces.clone(),
        damage_events: frame
            .recent_events
            .iter()
            .filter_map(|event| {
                let WorldEventKind::DamageApplied { entity, amount } = event.kind else {
                    return None;
                };
                Some(DamageCommand {
                    entity,
                    amount,
                    source: event.actors.iter().copied().find(|actor| *actor != entity),
                })
            })
            .collect(),
        constraints: Vec::new(),
        world_fields: WorldFieldView::default(),
        render_feedback,
        quality_budget,
    }
}

fn render_importance_feedback_from_snapshot(
    snapshot: &WorldSnapshot,
    player_location: Vec3,
) -> RenderImportanceFeedback {
    let mut visible_entities = Vec::new();
    let mut screen_size_scores = Vec::new();
    let mut camera_distance_scores = Vec::new();
    let mut requested_physics_lod = Vec::new();

    for (entity, renderable) in snapshot.renderables.iter() {
        if !renderable.visible {
            continue;
        }
        let transform = snapshot
            .transforms
            .find(*entity)
            .copied()
            .unwrap_or_default();
        let distance = transform.translation_meters.distance(player_location);
        let distance_score = (1.0 - (distance / 72.0)).clamp(0.0, 1.0);
        let screen_score = (1.0 / (1.0 + distance * 0.12)).clamp(0.0, 1.0);

        visible_entities.push(*entity);
        screen_size_scores.push((*entity, screen_score));
        camera_distance_scores.push((*entity, distance_score));

        if screen_score > 0.45 {
            requested_physics_lod.push(SimulationLodRequest {
                entity: *entity,
                requested_lod: SimulationLodTier::LocalDeformation,
                priority: screen_score,
                reason: "visible renderable occupies meaningful screen area".to_string(),
            });
        }
    }

    RenderImportanceFeedback {
        visible_entities,
        screen_size_scores,
        camera_distance_scores,
        requested_physics_lod,
    }
}

fn simulation_lod_signal_map(
    frame: &FrameContext,
    player_location: Vec3,
    render_feedback: &RenderImportanceFeedback,
    quality_budget: &PhysicsBudget,
) -> BTreeMap<EntityId, SimulationLodSignals> {
    let mut signals = frame
        .snapshot
        .physical_bodies
        .iter()
        .map(|(entity, body)| {
            let transform = frame
                .snapshot
                .transforms
                .find(*entity)
                .copied()
                .unwrap_or_default();
            let tags = frame
                .snapshot
                .tags
                .find(*entity)
                .cloned()
                .unwrap_or_default();
            let visible = frame
                .snapshot
                .renderables
                .find(*entity)
                .is_some_and(|renderable| renderable.visible);
            (
                *entity,
                base_lod_signals_for_body(
                    *entity,
                    &tags,
                    body,
                    visible,
                    transform.translation_meters.distance(player_location),
                    render_feedback,
                    quality_budget,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();

    for force in &frame.forces {
        let Some(signal) = signals.get_mut(&force.entity) else {
            continue;
        };
        if force.source.is_some() {
            signal.player_interaction = true;
            request_lod(signal, SimulationLodTier::LocalDeformation);
        }
        signal.energy_score = signal
            .energy_score
            .max((force.impulse_newton_seconds / 60.0).clamp(0.0, 1.0));
        if force.impulse_newton_seconds >= 18.0 {
            signal.high_energy = true;
            request_lod(signal, SimulationLodTier::HeroSimulation);
        }
        signal.settled = false;
    }

    for event in &frame.recent_events {
        match &event.kind {
            WorldEventKind::ForceApplied { entity } => {
                mark_player_interaction(&mut signals, *entity);
            }
            WorldEventKind::DamageApplied { entity, amount } => {
                if let Some(signal) = signals.get_mut(entity) {
                    signal.player_interaction = true;
                    signal.energy_score = signal.energy_score.max((*amount / 60.0).clamp(0.0, 1.0));
                    signal.high_energy |= *amount >= 12.0;
                    request_lod(
                        signal,
                        if *amount >= 12.0 {
                            SimulationLodTier::HeroSimulation
                        } else {
                            SimulationLodTier::LocalDeformation
                        },
                    );
                    signal.settled = false;
                }
            }
            WorldEventKind::GlassWallFractured { entity }
            | WorldEventKind::MetalBent { entity, .. }
            | WorldEventKind::FlexibleConstraintResolved { entity, .. }
            | WorldEventKind::PowerTransformerOverheated { entity } => {
                mark_high_energy(&mut signals, *entity);
            }
            WorldEventKind::MaterialStateChanged { entity } => {
                if let Some(signal) = signals.get_mut(entity) {
                    request_lod(signal, SimulationLodTier::RigidWithMaterialState);
                    signal.settled = false;
                }
            }
            WorldEventKind::StreetFlooded | WorldEventKind::ToxicGasReleased => {
                for actor in &event.actors {
                    if let Some(signal) = signals.get_mut(actor) {
                        signal.hazard_volume = true;
                        request_lod(signal, SimulationLodTier::LocalDeformation);
                        signal.settled = false;
                    }
                }
            }
            WorldEventKind::SoundEmitted {
                source_entity: Some(entity),
                intensity,
                ..
            } => {
                if let Some(signal) = signals.get_mut(entity) {
                    signal.ai_focus |= *intensity > 0.35;
                    signal.high_energy |= *intensity > 0.75;
                    signal.energy_score = signal.energy_score.max((*intensity).clamp(0.0, 1.0));
                    request_lod(
                        signal,
                        if *intensity > 0.75 {
                            SimulationLodTier::HeroSimulation
                        } else {
                            SimulationLodTier::LocalDeformation
                        },
                    );
                    signal.settled = false;
                }
            }
            WorldEventKind::SoundEmitted { .. } => {}
            WorldEventKind::NpcHeardSound {
                source_entity: Some(entity),
                ..
            } => {
                mark_ai_focus(&mut signals, *entity);
            }
            WorldEventKind::NpcHeardSound { .. } => {}
            WorldEventKind::SecurityAlertRaised { threat, .. } => {
                mark_mission_relevant(&mut signals, *threat);
            }
            WorldEventKind::FactionReputationChanged { subject, .. } => {
                mark_mission_relevant(&mut signals, *subject);
            }
            WorldEventKind::NpcWitnessedCrime { witness, .. } => {
                mark_ai_focus(&mut signals, *witness);
            }
            WorldEventKind::PlayerIdentityExposed => {
                for (entity, signal) in &mut signals {
                    if frame
                        .snapshot
                        .tags
                        .find(*entity)
                        .is_some_and(|tags| has_tag(tags, "player"))
                    {
                        signal.mission_or_faction_relevant = true;
                        request_lod(signal, SimulationLodTier::LocalDeformation);
                    }
                }
            }
            _ => {}
        }
    }

    signals
}

fn base_lod_signals_for_body(
    entity: EntityId,
    tags: &[String],
    body: &PhysicalBody,
    visible: bool,
    distance_to_player: f32,
    render_feedback: &RenderImportanceFeedback,
    quality_budget: &PhysicsBudget,
) -> SimulationLodSignals {
    let renderer_screen_score = score_for_entity(&render_feedback.screen_size_scores, entity);
    let renderer_distance_score = score_for_entity(&render_feedback.camera_distance_scores, entity);
    let renderer_requested = render_feedback.visible_entities.contains(&entity)
        || render_feedback
            .requested_physics_lod
            .iter()
            .any(|request| request.entity == entity);
    let requested_lod = render_feedback
        .requested_physics_lod
        .iter()
        .filter(|request| request.entity == entity)
        .max_by(|left, right| {
            left.priority
                .partial_cmp(&right.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|request| request.requested_lod);
    let story_important = has_any_tag(
        tags,
        &[
            "story", "mission", "hero", "player", "npc", "security", "faction",
        ],
    );
    let hazard_volume = has_any_tag(
        tags,
        &["hazard", "toxic_gas_source", "water_leak", "steam_leak"],
    );
    let camera_importance = renderer_screen_score
        .max(renderer_distance_score)
        .max(if distance_to_player <= quality_budget.hero_radius_meters {
            0.65
        } else {
            0.0
        })
        .max(if visible { 0.4 } else { 0.0 })
        .clamp(0.0, 1.0);

    SimulationLodSignals {
        camera_importance,
        energy_score: 0.0,
        player_interaction: has_tag(tags, "player"),
        high_energy: false,
        ai_focus: false,
        story_important,
        mission_or_faction_relevant: story_important && has_any_tag(tags, &["mission", "faction"]),
        renderer_requested,
        hazard_volume,
        settled: !body.dynamic && !body.fragile && !visible && !story_important && !hazard_volume,
        requested_lod,
    }
}

fn mark_player_interaction(
    signals: &mut BTreeMap<EntityId, SimulationLodSignals>,
    entity: EntityId,
) {
    if let Some(signal) = signals.get_mut(&entity) {
        signal.player_interaction = true;
        signal.settled = false;
        request_lod(signal, SimulationLodTier::LocalDeformation);
    }
}

fn mark_high_energy(signals: &mut BTreeMap<EntityId, SimulationLodSignals>, entity: EntityId) {
    if let Some(signal) = signals.get_mut(&entity) {
        signal.high_energy = true;
        signal.energy_score = signal.energy_score.max(0.8);
        signal.settled = false;
        request_lod(signal, SimulationLodTier::HeroSimulation);
    }
}

fn mark_ai_focus(signals: &mut BTreeMap<EntityId, SimulationLodSignals>, entity: EntityId) {
    if let Some(signal) = signals.get_mut(&entity) {
        signal.ai_focus = true;
        signal.settled = false;
        request_lod(signal, SimulationLodTier::LocalDeformation);
    }
}

fn mark_mission_relevant(signals: &mut BTreeMap<EntityId, SimulationLodSignals>, entity: EntityId) {
    if let Some(signal) = signals.get_mut(&entity) {
        signal.mission_or_faction_relevant = true;
        signal.story_important = true;
        signal.settled = false;
        request_lod(signal, SimulationLodTier::LocalDeformation);
    }
}

fn request_lod(signal: &mut SimulationLodSignals, requested_lod: SimulationLodTier) {
    signal.requested_lod = Some(
        signal
            .requested_lod
            .unwrap_or(requested_lod)
            .max(requested_lod),
    );
}

fn score_for_entity(scores: &[(EntityId, f32)], entity: EntityId) -> f32 {
    scores
        .iter()
        .find_map(|(score_entity, score)| (*score_entity == entity).then_some(*score))
        .unwrap_or_default()
}

fn has_any_tag(tags: &[String], expected: &[&str]) -> bool {
    expected.iter().any(|expected| has_tag(tags, expected))
}

pub fn solve_consequence_frame(
    input: &PhysicsInput,
    fracture_impulse_threshold: f32,
    water_leak_radius_meters: f32,
    already_fractured: &BTreeSet<EntityId>,
    already_wetted: &BTreeSet<EntityId>,
    already_gas_released: &BTreeSet<EntityId>,
) -> PhysicsOutput {
    let mut output = PhysicsOutput::empty(input.tick);
    let body_map = input
        .bodies
        .iter()
        .map(|body| (body.entity, body))
        .collect::<BTreeMap<_, _>>();
    let material_states = input
        .material_states
        .iter()
        .map(|(entity, state)| (*entity, *state))
        .collect::<BTreeMap<_, _>>();
    let material_map = input
        .material_table
        .iter()
        .map(|material| (material.id, material))
        .collect::<BTreeMap<_, _>>();

    solve_fractures(
        input,
        fracture_impulse_threshold,
        already_fractured,
        &body_map,
        &material_states,
        &mut output,
    );
    solve_plastic_deformations(
        input,
        &body_map,
        &material_map,
        &material_states,
        &mut output,
    );
    solve_constraints(input, &body_map, &mut output);
    solve_liquids(
        input,
        water_leak_radius_meters,
        already_wetted,
        &body_map,
        &material_states,
        &mut output,
    );
    solve_gases(input, already_gas_released, &body_map, &mut output);
    let budget_clamps = cap_budgeted_outputs(&mut output, &input.quality_budget);
    output.validation_report = validate_physics_output(&output, &input.quality_budget);
    output.timing = PerformanceCounters {
        cpu_milliseconds: 0.35
            + output.fractures.len() as f32 * 0.04
            + output.plastic_deformations.len() as f32 * 0.025
            + output.constraint_resolutions.len() as f32 * 0.02
            + output.fluid_deltas.len() as f32 * 0.03
            + output.gas_deltas.len() as f32 * 0.03,
        gpu_milliseconds: if output.gas_deltas.is_empty() && output.fluid_deltas.is_empty() {
            0.05
        } else {
            0.2
        },
        memory_bytes: 16 * 1024 * 1024
            + output.fractures.len() as u64 * 256 * 1024
            + output.plastic_deformations.len() as u64 * 64 * 1024
            + output.constraint_resolutions.len() as u64 * 32 * 1024
            + output.fluid_deltas.len() as u64 * 512 * 1024
            + output.gas_deltas.len() as u64 * 512 * 1024,
    };
    output.solver_reports = solver_reports(&output, input);
    output.debug_report = build_physics_debug_report(input, &output, budget_clamps);
    output
}

pub fn validate_physics_output(
    output: &PhysicsOutput,
    budget: &PhysicsBudget,
) -> PhysicsValidationReport {
    let mut issues = Vec::new();
    for fracture in &output.fractures {
        if fracture.pieces.is_empty() {
            issues.push(validation_issue(
                PhysicsValidationSeverity::Error,
                "fracture_without_pieces",
                "fracture events must contain collision/render piece data",
            ));
        }
        if fracture.pieces.len() > budget.max_fracture_pieces {
            issues.push(validation_issue(
                PhysicsValidationSeverity::Warning,
                "fracture_piece_budget_exceeded",
                "fracture pieces exceed the active budget and should be clustered",
            ));
        }
    }
    for deformation in &output.plastic_deformations {
        if deformation.plastic_strain_delta <= 0.0 {
            issues.push(validation_issue(
                PhysicsValidationSeverity::Error,
                "deformation_without_strain",
                "plastic deformation events must increase material plastic_strain",
            ));
        }
        if !output
            .material_state_deltas
            .iter()
            .any(|delta| delta.entity == deformation.entity && delta.plastic_strain > 0.0)
        {
            issues.push(validation_issue(
                PhysicsValidationSeverity::Error,
                "deformation_without_material_delta",
                "plastic deformation must output a matching material-state delta",
            ));
        }
    }
    for resolution in &output.constraint_resolutions {
        if resolution.correction_meters <= 0.0 {
            issues.push(validation_issue(
                PhysicsValidationSeverity::Error,
                "constraint_without_correction",
                "constraint resolutions must include a positive correction distance",
            ));
        }
        if !output.transforms.iter().any(|(entity, transform)| {
            *entity == resolution.entity && *transform == resolution.corrected_transform
        }) {
            issues.push(validation_issue(
                PhysicsValidationSeverity::Error,
                "constraint_without_transform",
                "constraint resolutions must output a matching corrected transform",
            ));
        }
    }
    if output.fluid_deltas.len() > budget.max_fluid_regions {
        issues.push(validation_issue(
            PhysicsValidationSeverity::Warning,
            "fluid_region_budget_exceeded",
            "fluid region count exceeds the active budget",
        ));
    }
    if output.gas_deltas.len() > budget.max_gas_regions {
        issues.push(validation_issue(
            PhysicsValidationSeverity::Warning,
            "gas_region_budget_exceeded",
            "gas region count exceeds the active budget",
        ));
    }
    for fluid in &output.fluid_deltas {
        if fluid.wetness_changes.is_empty() {
            issues.push(validation_issue(
                PhysicsValidationSeverity::Warning,
                "fluid_without_wetness_change",
                "liquid deltas should include world material wetness effects",
            ));
        }
    }

    PhysicsValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == PhysicsValidationSeverity::Error),
        issues,
    }
}

pub fn effective_friction(physical: &PhysicalMaterial, state: &MaterialState) -> (f32, f32) {
    let wet_factor = 1.0 - state.moisture.clamp(0.0, 1.0) * 0.35;
    let soot_factor = 1.0 - state.soot.clamp(0.0, 1.0) * 0.2;
    let oil_factor = 1.0 - state.oil_contamination.clamp(0.0, 1.0) * 0.45;
    let biological_factor = 1.0 - state.biological_contamination.clamp(0.0, 1.0) * 0.08;
    (
        (physical.friction_static * wet_factor * soot_factor * oil_factor * biological_factor)
            .clamp(0.0, 2.0),
        (physical.friction_dynamic * wet_factor * soot_factor * oil_factor * biological_factor)
            .clamp(0.0, 2.0),
    )
}

fn solve_fractures(
    input: &PhysicsInput,
    fracture_impulse_threshold: f32,
    already_fractured: &BTreeSet<EntityId>,
    body_map: &BTreeMap<EntityId, &BodyRecord>,
    material_states: &BTreeMap<EntityId, MaterialState>,
    output: &mut PhysicsOutput,
) {
    for force in &input.forces {
        let Some(body) = body_map.get(&force.entity).copied() else {
            continue;
        };
        if !body.physical_body.fragile
            || force.impulse_newton_seconds < fracture_impulse_threshold
            || already_fractured.contains(&force.entity)
        {
            continue;
        }

        let location = body.transform.translation_meters;
        let event_id = deterministic_event_id(input.tick, 20, force.entity);
        let replacement_mesh = body
            .renderable
            .as_ref()
            .and_then(|renderable| renderable.fracture_replacement_mesh);
        let fracture = build_fracture_event(
            input,
            force,
            body,
            material_states,
            event_id,
            replacement_mesh,
        );
        let mesh_delta = MeshDelta {
            entity: body.entity,
            replacement_mesh,
            debris_meshes: fracture.pieces.iter().map(|piece| piece.mesh).collect(),
            exposed_surfaces: fracture.exposed_surfaces.clone(),
        };

        output.contacts.push(ContactEvent {
            entities: (force.source.unwrap_or(force.entity), Some(force.entity)),
            location,
            normal: normalized_or_default(force.vector_newtons),
            impulse_newton_seconds: force.impulse_newton_seconds,
            material_ids: (body.physical_body.material_id, None),
            friction: 0.45,
            restitution: 0.08,
        });
        output.world_events.push(WorldEvent {
            event_id,
            tick: input.tick,
            location_meters: location,
            actors: force.source.into_iter().chain([force.entity]).collect(),
            kind: WorldEventKind::GlassWallFractured {
                entity: force.entity,
            },
            physical_evidence: vec!["fracture_impulse".to_string(), "glass_shards".to_string()],
            narrative_tags: vec!["crime".to_string(), "loud".to_string()],
        });
        output
            .material_state_deltas
            .push(MaterialStateDelta::crack(force.entity, 1.0));
        output.mesh_deltas.push(mesh_delta);
        output.audio_events.push(AudioEvent {
            event_id: deterministic_event_id(input.tick, 21, force.entity),
            source_entity: Some(force.entity),
            location,
            event_kind: AudioEventKind::GlassShatter,
            material_id: Some(body.physical_body.material_id),
            intensity: 0.95,
            radius_meters: 28.0,
            occlusion_hint: 0.1,
            tags: vec!["fracture".to_string(), "glass".to_string()],
        });
        output.fractures.push(fracture);
    }
}

fn build_fracture_event(
    input: &PhysicsInput,
    force: &ForceCommand,
    body: &BodyRecord,
    material_states: &BTreeMap<EntityId, MaterialState>,
    event_id: WorldEventId,
    replacement_mesh: Option<MeshAssetHandle>,
) -> FractureEvent {
    let location = body.transform.translation_meters;
    let piece_count = fracture_piece_count(force.impulse_newton_seconds, &input.quality_budget);
    let mass_per_piece = body.physical_body.mass_kg / piece_count.max(1) as f32;
    let exposed_surface = ExposedSurface {
        material_id: body.physical_body.material_id,
        area_square_meters: (force.impulse_newton_seconds * 0.025).clamp(0.2, 4.0),
        roughness: 0.72,
        evidence_tag: "fresh_broken_edge".to_string(),
    };
    let material_state = material_states
        .get(&body.entity)
        .copied()
        .unwrap_or_default();
    let initial_velocity = scale_vec3(
        normalized_or_default(force.vector_newtons),
        (force.impulse_newton_seconds / body.physical_body.mass_kg.max(1.0)).clamp(0.1, 8.0),
    );
    let pieces = (0..piece_count)
        .map(|index| {
            let mesh = fracture_piece_mesh(body, replacement_mesh, index);
            let transform = Transform::at(Vec3::new(
                location.x + index as f32 * 0.04,
                location.y + (index % 3) as f32 * 0.03,
                location.z,
            ));
            FracturePiece {
                piece_id: index as u64 + 1,
                new_entity_hint: Some(body.entity * 1_000 + index as u64 + 1),
                mesh,
                mesh_delta: MeshDelta {
                    entity: body.entity,
                    replacement_mesh: Some(mesh),
                    debris_meshes: vec![mesh],
                    exposed_surfaces: vec![exposed_surface.clone()],
                },
                collision_shape: CollisionShapeDesc::ConvexHull {
                    point_count: 8 + index as u32,
                },
                mass_kg: mass_per_piece,
                initial_velocity: Vec3::new(
                    initial_velocity.x + index as f32 * 0.03,
                    initial_velocity.y,
                    initial_velocity.z,
                ),
                transform,
                material_state: MaterialState {
                    crack_density: 1.0,
                    ..material_state
                },
            }
        })
        .collect::<Vec<_>>();

    FractureEvent {
        event_id,
        source_entity: body.entity,
        tick: input.tick,
        impact_location: location,
        impulse: force.vector_newtons,
        material_id: body.physical_body.material_id,
        energy_estimate: 0.5 * body.physical_body.mass_kg * force.impulse_newton_seconds.powi(2),
        pieces,
        exposed_surfaces: vec![exposed_surface],
        evidence_strength: (force.impulse_newton_seconds / 60.0).clamp(0.0, 1.0),
        entity: body.entity,
        impact_impulse: force.impulse_newton_seconds,
        location,
    }
}

fn fracture_piece_mesh(
    body: &BodyRecord,
    replacement_mesh: Option<MeshAssetHandle>,
    index: usize,
) -> MeshAssetHandle {
    let base = replacement_mesh
        .or_else(|| body.renderable.as_ref().map(|renderable| renderable.mesh))
        .map(|mesh| mesh.0)
        .unwrap_or_else(|| generated_fracture_mesh_base(body.entity));
    MeshAssetHandle(base.saturating_add(index as AssetId))
}

fn generated_fracture_mesh_base(entity: EntityId) -> AssetId {
    (0xF2AC_7000_u128 << 64) | entity as u128
}

fn solve_plastic_deformations(
    input: &PhysicsInput,
    body_map: &BTreeMap<EntityId, &BodyRecord>,
    material_map: &BTreeMap<MaterialId, &MaterialDescriptor>,
    material_states: &BTreeMap<EntityId, MaterialState>,
    output: &mut PhysicsOutput,
) {
    let mut processed = BTreeSet::new();
    for force in &input.forces {
        if force.impulse_newton_seconds < 8.0 {
            continue;
        }
        let Some(body) = body_map.get(&force.entity).copied() else {
            continue;
        };
        let Some(deformation) = build_plastic_deformation_from_force(
            input.tick,
            force,
            body,
            material_map,
            material_states,
        ) else {
            continue;
        };
        processed.insert(force.entity);
        push_plastic_deformation_output(output, deformation);
    }

    for damage in &input.damage_events {
        if damage.amount < 5.0 || processed.contains(&damage.entity) {
            continue;
        }
        let Some(body) = body_map.get(&damage.entity).copied() else {
            continue;
        };
        let Some(deformation) = build_plastic_deformation_from_damage(
            input.tick,
            damage,
            body,
            material_map,
            material_states,
        ) else {
            continue;
        };
        push_plastic_deformation_output(output, deformation);
    }
}

fn build_plastic_deformation_from_force(
    tick: u64,
    force: &ForceCommand,
    body: &BodyRecord,
    material_map: &BTreeMap<MaterialId, &MaterialDescriptor>,
    material_states: &BTreeMap<EntityId, MaterialState>,
) -> Option<PlasticDeformationEvent> {
    if !is_ductile_metal(body, material_map) {
        return None;
    }
    let strain_delta = plastic_strain_delta(
        force.impulse_newton_seconds / (body.physical_body.mass_kg.max(1.0) * 1.5),
        material_states
            .get(&body.entity)
            .copied()
            .unwrap_or_default(),
    )?;

    Some(PlasticDeformationEvent {
        event_id: deterministic_event_id(tick, 28, body.entity),
        entity: body.entity,
        tick,
        location: body.transform.translation_meters,
        impulse: force.vector_newtons,
        material_id: body.physical_body.material_id,
        plastic_strain_delta: strain_delta,
        dent_depth_meters: (strain_delta * 0.08).clamp(0.002, 0.05),
        affected_area_square_meters: (force.impulse_newton_seconds * 0.012).clamp(0.05, 3.0),
        evidence_strength: (strain_delta * 2.5).clamp(0.0, 1.0),
    })
}

fn build_plastic_deformation_from_damage(
    tick: u64,
    damage: &DamageCommand,
    body: &BodyRecord,
    material_map: &BTreeMap<MaterialId, &MaterialDescriptor>,
    material_states: &BTreeMap<EntityId, MaterialState>,
) -> Option<PlasticDeformationEvent> {
    if !is_ductile_metal(body, material_map) {
        return None;
    }
    let strain_delta = plastic_strain_delta(
        damage.amount / body.physical_body.mass_kg.max(1.0),
        material_states
            .get(&body.entity)
            .copied()
            .unwrap_or_default(),
    )?;

    Some(PlasticDeformationEvent {
        event_id: deterministic_event_id(tick, 29, body.entity),
        entity: body.entity,
        tick,
        location: body.transform.translation_meters,
        impulse: Vec3::new(damage.amount, 0.0, 0.0),
        material_id: body.physical_body.material_id,
        plastic_strain_delta: strain_delta,
        dent_depth_meters: (strain_delta * 0.06).clamp(0.002, 0.045),
        affected_area_square_meters: (damage.amount * 0.02).clamp(0.05, 2.5),
        evidence_strength: (strain_delta * 2.0).clamp(0.0, 1.0),
    })
}

fn push_plastic_deformation_output(
    output: &mut PhysicsOutput,
    deformation: PlasticDeformationEvent,
) {
    output.contacts.push(ContactEvent {
        entities: (deformation.entity, None),
        location: deformation.location,
        normal: normalized_or_default(deformation.impulse),
        impulse_newton_seconds: (deformation.plastic_strain_delta * 120.0).max(1.0),
        material_ids: (deformation.material_id, None),
        friction: 0.5,
        restitution: 0.02,
    });
    output.material_state_deltas.push(MaterialStateDelta {
        plastic_strain: deformation.plastic_strain_delta,
        ..MaterialStateDelta::zero(deformation.entity)
    });
    output.mesh_deltas.push(MeshDelta {
        entity: deformation.entity,
        replacement_mesh: None,
        debris_meshes: Vec::new(),
        exposed_surfaces: vec![ExposedSurface {
            material_id: deformation.material_id,
            area_square_meters: deformation.affected_area_square_meters,
            roughness: 0.58,
            evidence_tag: "fresh_metal_dent".to_string(),
        }],
    });
    output.world_events.push(WorldEvent {
        event_id: deformation.event_id,
        tick: deformation.tick,
        location_meters: deformation.location,
        actors: vec![deformation.entity],
        kind: WorldEventKind::MetalBent {
            entity: deformation.entity,
            plastic_strain: deformation.plastic_strain_delta,
        },
        physical_evidence: vec!["plastic_strain".to_string(), "dented_metal".to_string()],
        narrative_tags: vec![
            "damage".to_string(),
            "material".to_string(),
            "metal".to_string(),
        ],
    });
    output.audio_events.push(AudioEvent {
        event_id: deterministic_event_id(deformation.tick, 30, deformation.entity),
        source_entity: Some(deformation.entity),
        location: deformation.location,
        event_kind: AudioEventKind::MetalBend,
        material_id: Some(deformation.material_id),
        intensity: (0.3 + deformation.plastic_strain_delta * 1.8).clamp(0.2, 0.95),
        radius_meters: (8.0 + deformation.plastic_strain_delta * 35.0).clamp(8.0, 24.0),
        occlusion_hint: 0.05,
        tags: vec!["metal".to_string(), "deformation".to_string()],
    });
    output.plastic_deformations.push(deformation);
}

fn solve_constraints(
    input: &PhysicsInput,
    body_map: &BTreeMap<EntityId, &BodyRecord>,
    output: &mut PhysicsOutput,
) {
    if input.quality_budget.quality == QualityTier::Disabled {
        return;
    }

    for constraint in &input.constraints {
        match constraint {
            PhysicsConstraint::Cable {
                entity,
                anchor,
                max_length_meters,
            } => {
                let Some(body) = body_map.get(entity).copied() else {
                    continue;
                };
                let max_length = max_length_meters.max(0.01);
                let current = body.transform.translation_meters;
                let offset = sub_vec3(current, *anchor);
                let length = vec3_length(offset);
                if length <= max_length {
                    continue;
                }

                let direction = normalized_or_default(offset);
                let corrected_position = add_vec3(*anchor, scale_vec3(direction, max_length));
                let mut corrected_transform = body.transform;
                corrected_transform.translation_meters = corrected_position;
                let correction_meters = length - max_length;
                push_constraint_resolution_output(
                    output,
                    body,
                    ConstraintResolutionEvent {
                        event_id: deterministic_event_id(input.tick, 31, *entity),
                        entity: *entity,
                        tick: input.tick,
                        kind: ConstraintResolutionKind::CableLength {
                            anchor: *anchor,
                            max_length_meters: max_length,
                        },
                        original_transform: body.transform,
                        corrected_transform,
                        correction_meters,
                        strain_ratio: (length / max_length - 1.0).max(0.0),
                    },
                );
            }
            PhysicsConstraint::ClothPin {
                entity,
                attach_point,
                stiffness,
            } => {
                let Some(body) = body_map.get(entity).copied() else {
                    continue;
                };
                let ground_y = input.world_fields.water_level_meters.max(0.0);
                let current = body.transform.translation_meters;
                if current.y >= ground_y {
                    continue;
                }

                let mut corrected_transform = body.transform;
                corrected_transform.translation_meters.y = ground_y;
                let correction_meters = ground_y - current.y;
                push_constraint_resolution_output(
                    output,
                    body,
                    ConstraintResolutionEvent {
                        event_id: deterministic_event_id(input.tick, 32, *entity),
                        entity: *entity,
                        tick: input.tick,
                        kind: ConstraintResolutionKind::ClothPinWorldCollision {
                            attach_point: attach_point.clone(),
                            stiffness: stiffness.clamp(0.0, 1.0),
                        },
                        original_transform: body.transform,
                        corrected_transform,
                        correction_meters,
                        strain_ratio: correction_meters * stiffness.clamp(0.0, 1.0),
                    },
                );
            }
            PhysicsConstraint::FixedJoint { .. } => {}
        }
    }
}

fn push_constraint_resolution_output(
    output: &mut PhysicsOutput,
    body: &BodyRecord,
    resolution: ConstraintResolutionEvent,
) {
    let normal = normalized_or_default(sub_vec3(
        resolution.corrected_transform.translation_meters,
        resolution.original_transform.translation_meters,
    ));
    output
        .transforms
        .push((resolution.entity, resolution.corrected_transform));
    output.velocities.push((
        resolution.entity,
        Velocity {
            linear_meters_per_second: Vec3::ZERO,
            angular_radians_per_second: Vec3::ZERO,
        },
    ));
    output.contacts.push(ContactEvent {
        entities: (resolution.entity, None),
        location: resolution.corrected_transform.translation_meters,
        normal,
        impulse_newton_seconds: (resolution.correction_meters * body.physical_body.mass_kg)
            .clamp(0.1, 120.0),
        material_ids: (body.physical_body.material_id, None),
        friction: 0.35,
        restitution: 0.0,
    });
    output.world_events.push(WorldEvent {
        event_id: resolution.event_id,
        tick: resolution.tick,
        location_meters: resolution.corrected_transform.translation_meters,
        actors: vec![resolution.entity],
        kind: WorldEventKind::FlexibleConstraintResolved {
            entity: resolution.entity,
            correction_meters: resolution.correction_meters,
        },
        physical_evidence: vec!["constraint_correction".to_string()],
        narrative_tags: constraint_resolution_tags(&resolution.kind),
    });
    output.constraint_resolutions.push(resolution);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LiquidSpillKind {
    Water,
    Oil,
    Biological,
}

impl LiquidSpillKind {
    fn from_source(body: &BodyRecord) -> Option<Self> {
        if has_tag(&body.tags, "water_leak") {
            Some(Self::Water)
        } else if has_any_tag(
            &body.tags,
            &["oil_leak", "oil_spill", "fuel_leak", "hydraulic_leak"],
        ) {
            Some(Self::Oil)
        } else if has_any_tag(
            &body.tags,
            &[
                "biological_leak",
                "biological_spill",
                "biohazard_leak",
                "blood_leak",
            ],
        ) {
            Some(Self::Biological)
        } else {
            None
        }
    }

    fn fluid_seed_offset(self) -> u64 {
        match self {
            Self::Water => 24,
            Self::Oil => 25,
            Self::Biological => 26,
        }
    }

    fn audio_seed_offset(self) -> u64 {
        match self {
            Self::Water => 22,
            Self::Oil => 32,
            Self::Biological => 33,
        }
    }

    fn event_seed_offset(self) -> u64 {
        match self {
            Self::Water => 23,
            Self::Oil => 34,
            Self::Biological => 35,
        }
    }

    fn material_delta(self, entity: EntityId) -> MaterialStateDelta {
        match self {
            Self::Water => MaterialStateDelta {
                moisture: 0.7,
                temperature: -4.0,
                ..MaterialStateDelta::zero(entity)
            },
            Self::Oil => MaterialStateDelta {
                moisture: 0.12,
                oil_contamination: 0.65,
                ..MaterialStateDelta::zero(entity)
            },
            Self::Biological => MaterialStateDelta {
                moisture: 0.08,
                biological_contamination: 0.58,
                ..MaterialStateDelta::zero(entity)
            },
        }
    }

    fn target_already_saturated(self, state: MaterialState) -> bool {
        match self {
            Self::Water => state.moisture >= 0.85,
            Self::Oil => state.oil_contamination >= 0.85,
            Self::Biological => state.biological_contamination >= 0.85,
        }
    }

    fn wetness_change(self) -> f32 {
        match self {
            Self::Water => 0.7,
            Self::Oil => 0.32,
            Self::Biological => 0.24,
        }
    }

    fn volume_cubic_meters(self) -> f32 {
        match self {
            Self::Water => 0.08,
            Self::Oil => 0.045,
            Self::Biological => 0.03,
        }
    }

    fn intensity(self) -> f32 {
        match self {
            Self::Water => 0.35,
            Self::Oil => 0.26,
            Self::Biological => 0.22,
        }
    }

    fn hazard_tags(self) -> Vec<String> {
        match self {
            Self::Water => ["water", "slippery"].as_slice(),
            Self::Oil => ["oil", "slippery", "contamination"].as_slice(),
            Self::Biological => ["biological_contamination", "biohazard", "slippery"].as_slice(),
        }
        .iter()
        .map(|tag| (*tag).to_string())
        .collect()
    }

    fn evidence_tags(self) -> Vec<String> {
        match self {
            Self::Water => ["water_flow", "wet_surface"].as_slice(),
            Self::Oil => ["oil_flow", "oil_contamination", "slick_surface"].as_slice(),
            Self::Biological => [
                "biological_trace",
                "biological_contamination",
                "contaminated_surface",
            ]
            .as_slice(),
        }
        .iter()
        .map(|tag| (*tag).to_string())
        .collect()
    }

    fn audio_tags(self) -> Vec<String> {
        match self {
            Self::Water => ["water", "leak"].as_slice(),
            Self::Oil => ["oil", "slick", "leak"].as_slice(),
            Self::Biological => ["biohazard", "biological_contamination", "leak"].as_slice(),
        }
        .iter()
        .map(|tag| (*tag).to_string())
        .collect()
    }
}

fn solve_liquids(
    input: &PhysicsInput,
    water_leak_radius_meters: f32,
    already_wetted: &BTreeSet<EntityId>,
    body_map: &BTreeMap<EntityId, &BodyRecord>,
    material_states: &BTreeMap<EntityId, MaterialState>,
    output: &mut PhysicsOutput,
) {
    for (leak, spill_kind) in input
        .bodies
        .iter()
        .filter_map(|body| LiquidSpillKind::from_source(body).map(|kind| (body, kind)))
    {
        for surface in input
            .bodies
            .iter()
            .filter(|body| has_tag(&body.tags, "wettable"))
        {
            if leak.entity == surface.entity
                || (spill_kind == LiquidSpillKind::Water
                    && already_wetted.contains(&surface.entity))
            {
                continue;
            }
            let distance = leak
                .transform
                .translation_meters
                .distance(surface.transform.translation_meters);
            if distance > water_leak_radius_meters {
                continue;
            }
            let material_state = material_states
                .get(&surface.entity)
                .copied()
                .unwrap_or_default();
            if spill_kind.target_already_saturated(material_state) {
                continue;
            }

            let material_id = body_map
                .get(&surface.entity)
                .map(|body| body.physical_body.material_id)
                .unwrap_or(surface.physical_body.material_id);
            let delta = spill_kind.material_delta(surface.entity);
            let fluid_id =
                deterministic_u64(input.tick, spill_kind.fluid_seed_offset(), surface.entity);
            output.material_state_deltas.push(delta);
            output.fluid_deltas.push(FluidDelta {
                fluid_id,
                region_id: fluid_id,
                bounds: Aabb::new(
                    Vec3::new(
                        surface.transform.translation_meters.x - 1.0,
                        surface.transform.translation_meters.y - 1.0,
                        surface.transform.translation_meters.z - 0.02,
                    ),
                    Vec3::new(
                        surface.transform.translation_meters.x + 1.0,
                        surface.transform.translation_meters.y + 1.0,
                        surface.transform.translation_meters.z + 0.08,
                    ),
                ),
                volume_cubic_meters: spill_kind.volume_cubic_meters(),
                material_id,
                changed_cells: sparse_range(surface.transform.translation_meters, 2),
                surface_mesh: surface
                    .renderable
                    .as_ref()
                    .map(|renderable| renderable.mesh),
                particles: Some(GpuResourceHandle(20_500 + surface.entity as u128)),
                wetness_changes: vec![(surface.entity, spill_kind.wetness_change())],
                hazard_tags: spill_kind.hazard_tags(),
            });
            output.audio_events.push(AudioEvent {
                event_id: deterministic_event_id(
                    input.tick,
                    spill_kind.audio_seed_offset(),
                    surface.entity,
                ),
                source_entity: Some(surface.entity),
                location: surface.transform.translation_meters,
                event_kind: AudioEventKind::WaterSplash,
                material_id: Some(material_id),
                intensity: spill_kind.intensity(),
                radius_meters: 8.0,
                occlusion_hint: 0.0,
                tags: spill_kind.audio_tags(),
            });
            output.world_events.push(WorldEvent {
                event_id: deterministic_event_id(
                    input.tick,
                    spill_kind.event_seed_offset(),
                    surface.entity,
                ),
                tick: input.tick,
                location_meters: surface.transform.translation_meters,
                actors: vec![leak.entity, surface.entity],
                kind: WorldEventKind::StreetFlooded,
                physical_evidence: spill_kind.evidence_tags(),
                narrative_tags: vec![
                    "hazard".to_string(),
                    "material".to_string(),
                    "liquid".to_string(),
                ],
            });
        }
    }
}

fn solve_gases(
    input: &PhysicsInput,
    already_gas_released: &BTreeSet<EntityId>,
    _body_map: &BTreeMap<EntityId, &BodyRecord>,
    output: &mut PhysicsOutput,
) {
    for source in input
        .bodies
        .iter()
        .filter(|body| has_tag(&body.tags, "steam_leak") || has_tag(&body.tags, "toxic_gas_source"))
    {
        if already_gas_released.contains(&source.entity) {
            continue;
        }
        let toxic = has_tag(&source.tags, "toxic_gas_source");
        let gas_kind = if toxic { "toxic" } else { "steam" };
        let density = if toxic { 0.9 } else { 0.45 };
        let visibility_blocking = if toxic { 0.72 } else { 0.45 };
        let hazard_level = if toxic { 0.86 } else { 0.28 };
        let bounds = Aabb::new(
            Vec3::new(
                source.transform.translation_meters.x - 2.0,
                source.transform.translation_meters.y - 2.0,
                source.transform.translation_meters.z,
            ),
            Vec3::new(
                source.transform.translation_meters.x + 2.0,
                source.transform.translation_meters.y + 2.0,
                source.transform.translation_meters.z + 3.0,
            ),
        );
        let density_changed = sparse_range(source.transform.translation_meters, 3);
        let temperature_changed = sparse_range(source.transform.translation_meters, 2);
        let pressure_changed = sparse_range(source.transform.translation_meters, 2);
        let volume_id = deterministic_u64(input.tick, 25, source.entity);
        output.gas_deltas.push(GasDelta {
            gas_id: volume_id,
            volume_id,
            bounds,
            density,
            gas_material: source.physical_body.material_id,
            density_changed,
            temperature_changed,
            pressure_changed,
            visibility_blocking,
            hazard_level,
        });
        output.world_events.push(WorldEvent {
            event_id: deterministic_event_id(input.tick, 26, source.entity),
            tick: input.tick,
            location_meters: source.transform.translation_meters,
            actors: vec![source.entity],
            kind: WorldEventKind::ToxicGasReleased,
            physical_evidence: vec![
                "gas_density_field".to_string(),
                format!("gas_kind:{gas_kind}"),
                format!("gas_density:{density:.2}"),
                format!("visibility_blocking:{visibility_blocking:.2}"),
                format!("hazard_level:{hazard_level:.2}"),
                format!("density_cells:{}", sparse_cell_count(&density_changed)),
                format!(
                    "temperature_cells:{}",
                    sparse_cell_count(&temperature_changed)
                ),
                format!("pressure_cells:{}", sparse_cell_count(&pressure_changed)),
                "volume_radius_meters:2.00".to_string(),
                "volume_height_meters:3.00".to_string(),
            ],
            narrative_tags: vec![
                "hazard".to_string(),
                "gas".to_string(),
                gas_kind.to_string(),
                "visibility_blocking".to_string(),
            ],
        });
        output.audio_events.push(AudioEvent {
            event_id: deterministic_event_id(input.tick, 27, source.entity),
            source_entity: Some(source.entity),
            location: source.transform.translation_meters,
            event_kind: AudioEventKind::SteamLeak,
            material_id: Some(source.physical_body.material_id),
            intensity: if toxic { 0.55 } else { 0.32 },
            radius_meters: 16.0,
            occlusion_hint: 0.05,
            tags: vec!["gas".to_string(), "pressure".to_string()],
        });
    }
}

fn cap_budgeted_outputs(
    output: &mut PhysicsOutput,
    budget: &PhysicsBudget,
) -> PhysicsBudgetClampReport {
    let pre_cap_contacts = output.contacts.len();
    let pre_cap_fluid_regions = output.fluid_deltas.len();
    let pre_cap_gas_regions = output.gas_deltas.len();
    let pre_cap_fracture_pieces = fracture_piece_total(output);

    if output.contacts.len() > budget.max_active_contacts {
        output.contacts.truncate(budget.max_active_contacts);
    }
    if output.fluid_deltas.len() > budget.max_fluid_regions {
        output.fluid_deltas.truncate(budget.max_fluid_regions);
    }
    if output.gas_deltas.len() > budget.max_gas_regions {
        output.gas_deltas.truncate(budget.max_gas_regions);
    }
    for fracture in &mut output.fractures {
        if fracture.pieces.len() > budget.max_fracture_pieces {
            fracture.pieces.truncate(budget.max_fracture_pieces);
        }
    }

    let contacts = output.contacts.len();
    let fluid_regions = output.fluid_deltas.len();
    let gas_regions = output.gas_deltas.len();
    let fracture_pieces = fracture_piece_total(output);
    PhysicsBudgetClampReport {
        max_contacts: budget.max_active_contacts,
        max_fracture_pieces: budget.max_fracture_pieces,
        max_fluid_regions: budget.max_fluid_regions,
        max_gas_regions: budget.max_gas_regions,
        pre_cap_contacts,
        contacts,
        pre_cap_fracture_pieces,
        fracture_pieces,
        pre_cap_fluid_regions,
        fluid_regions,
        pre_cap_gas_regions,
        gas_regions,
        dropped_contacts: pre_cap_contacts.saturating_sub(contacts),
        dropped_fracture_pieces: pre_cap_fracture_pieces.saturating_sub(fracture_pieces),
        dropped_fluid_regions: pre_cap_fluid_regions.saturating_sub(fluid_regions),
        dropped_gas_regions: pre_cap_gas_regions.saturating_sub(gas_regions),
    }
}

fn fracture_piece_total(output: &PhysicsOutput) -> usize {
    output
        .fractures
        .iter()
        .map(|fracture| fracture.pieces.len())
        .sum()
}

fn build_physics_debug_report(
    input: &PhysicsInput,
    output: &PhysicsOutput,
    budget_clamps: PhysicsBudgetClampReport,
) -> PhysicsDebugReport {
    let player_location = input
        .bodies
        .iter()
        .find(|body| has_tag(&body.tags, "player"))
        .map(|body| body.transform.translation_meters)
        .unwrap_or(Vec3::ZERO);
    let body_lods = input
        .bodies
        .iter()
        .map(|body| {
            let distance_to_player_meters =
                body.transform.translation_meters.distance(player_location);
            let (_, reason) = choose_lod_with_reason(
                distance_to_player_meters,
                body.visible,
                body.physical_body.fragile,
                body.lod_signals,
                &input.quality_budget,
            );
            BodyLodDebugEntry {
                entity: body.entity,
                name: body.name.clone(),
                lod: body.lod,
                reason,
                distance_to_player_meters,
                visible: body.visible,
                fragile: body.physical_body.fragile,
                story_importance: body.story_importance,
                signals: body.lod_signals,
            }
        })
        .collect::<Vec<_>>();
    let lod_summary = physics_lod_summary(&body_lods);
    let active_solver_families = output
        .solver_reports
        .iter()
        .filter(|report| {
            report.active_entities > 0
                || report.output_count > 0
                || report.family == SolverFamily::LodManager
        })
        .map(|report| report.family)
        .collect();

    PhysicsDebugReport {
        tick: input.tick,
        quality: input.quality_budget.quality,
        body_lods,
        lod_summary,
        budget_clamps,
        active_solver_families,
    }
}

fn physics_lod_summary(body_lods: &[BodyLodDebugEntry]) -> PhysicsLodSummary {
    let mut summary = PhysicsLodSummary {
        body_count: body_lods.len(),
        ..PhysicsLodSummary::default()
    };
    for entry in body_lods {
        match entry.lod {
            SimulationLodTier::Dormant => summary.dormant_count += 1,
            SimulationLodTier::SimpleRigid => summary.simple_rigid_count += 1,
            SimulationLodTier::RigidWithMaterialState => summary.material_state_count += 1,
            SimulationLodTier::LocalDeformation => summary.local_deformation_count += 1,
            SimulationLodTier::HeroSimulation => summary.hero_simulation_count += 1,
            SimulationLodTier::ReferenceValidation => summary.reference_validation_count += 1,
        }
        if entry.lod >= SimulationLodTier::RigidWithMaterialState {
            summary.active_simulated_count += 1;
        }
        if matches!(
            entry.reason,
            LodSelectionReason::RendererRequested
                | LodSelectionReason::PlayerInteraction
                | LodSelectionReason::HighEnergy
                | LodSelectionReason::MissionOrFaction
                | LodSelectionReason::AiFocus
                | LodSelectionReason::StoryImportant
                | LodSelectionReason::HazardVolume
                | LodSelectionReason::NearVisibleOrFragile
                | LodSelectionReason::VisibleOrFragile
                | LodSelectionReason::NearPlayer
        ) {
            summary.promotion_count += 1;
        }
        if matches!(
            entry.reason,
            LodSelectionReason::FarSettledDormant | LodSelectionReason::Background
        ) {
            summary.demotion_count += 1;
        }
        summary.high_energy_count += usize::from(entry.signals.high_energy);
        summary.ai_focus_count += usize::from(entry.signals.ai_focus);
        summary.renderer_requested_count += usize::from(entry.signals.renderer_requested);
        summary.mission_relevant_count += usize::from(entry.signals.mission_or_faction_relevant);
        summary.hazard_volume_count += usize::from(entry.signals.hazard_volume);
    }
    summary
}

fn solver_reports(output: &PhysicsOutput, input: &PhysicsInput) -> Vec<SolverReport> {
    let body_count = input.bodies.len();
    let lod_output_count = input
        .bodies
        .iter()
        .filter(|body| {
            body.lod >= SimulationLodTier::LocalDeformation
                || body.lod == SimulationLodTier::Dormant
        })
        .count();
    vec![
        SolverReport {
            family: SolverFamily::RigidBody,
            active_entities: body_count,
            output_count: output.contacts.len(),
            cpu_milliseconds: 0.08,
            gpu_milliseconds: 0.0,
        },
        SolverReport {
            family: SolverFamily::Fracture,
            active_entities: output.fractures.len(),
            output_count: output
                .fractures
                .iter()
                .map(|fracture| fracture.pieces.len())
                .sum(),
            cpu_milliseconds: output.fractures.len() as f32 * 0.06,
            gpu_milliseconds: 0.0,
        },
        SolverReport {
            family: SolverFamily::Liquid,
            active_entities: output.fluid_deltas.len(),
            output_count: output.fluid_deltas.len(),
            cpu_milliseconds: output.fluid_deltas.len() as f32 * 0.04,
            gpu_milliseconds: output.fluid_deltas.len() as f32 * 0.05,
        },
        SolverReport {
            family: SolverFamily::Gas,
            active_entities: output.gas_deltas.len(),
            output_count: output.gas_deltas.len(),
            cpu_milliseconds: output.gas_deltas.len() as f32 * 0.04,
            gpu_milliseconds: output.gas_deltas.len() as f32 * 0.08,
        },
        SolverReport {
            family: SolverFamily::SoftBody,
            active_entities: output.constraint_resolutions.len(),
            output_count: output.constraint_resolutions.len(),
            cpu_milliseconds: output.constraint_resolutions.len() as f32 * 0.025,
            gpu_milliseconds: 0.0,
        },
        SolverReport {
            family: SolverFamily::PlasticDeformation,
            active_entities: output.plastic_deformations.len(),
            output_count: output.plastic_deformations.len(),
            cpu_milliseconds: output.plastic_deformations.len() as f32 * 0.03,
            gpu_milliseconds: 0.0,
        },
        SolverReport {
            family: SolverFamily::LodManager,
            active_entities: body_count,
            output_count: lod_output_count,
            cpu_milliseconds: 0.02,
            gpu_milliseconds: (body_count as f32 * 0.002).min(0.08),
        },
    ]
}

fn choose_lod_with_reason(
    distance_to_player: f32,
    visible: bool,
    fragile: bool,
    signals: SimulationLodSignals,
    budget: &PhysicsBudget,
) -> (SimulationLodTier, LodSelectionReason) {
    if budget.quality == QualityTier::Disabled {
        return (SimulationLodTier::Dormant, LodSelectionReason::Disabled);
    }
    if budget.quality == QualityTier::ReferenceOfflineValidation {
        return (
            SimulationLodTier::ReferenceValidation,
            LodSelectionReason::ReferenceValidation,
        );
    }
    if signals.player_interaction {
        return (
            if distance_to_player <= budget.hero_radius_meters || signals.high_energy {
                SimulationLodTier::HeroSimulation
            } else {
                SimulationLodTier::LocalDeformation
            },
            LodSelectionReason::PlayerInteraction,
        );
    }
    if signals.high_energy || signals.energy_score > 0.72 {
        return (
            SimulationLodTier::HeroSimulation,
            LodSelectionReason::HighEnergy,
        );
    }
    if signals.mission_or_faction_relevant {
        return (
            if distance_to_player <= budget.hero_radius_meters * 1.5 {
                SimulationLodTier::HeroSimulation
            } else {
                SimulationLodTier::LocalDeformation
            },
            LodSelectionReason::MissionOrFaction,
        );
    }
    if signals.ai_focus {
        return (
            SimulationLodTier::LocalDeformation,
            LodSelectionReason::AiFocus,
        );
    }
    if signals.story_important {
        return (
            SimulationLodTier::LocalDeformation,
            LodSelectionReason::StoryImportant,
        );
    }
    if signals.hazard_volume {
        return (
            SimulationLodTier::LocalDeformation,
            LodSelectionReason::HazardVolume,
        );
    }
    if distance_to_player <= budget.hero_radius_meters * 0.4 && (visible || fragile) {
        return (
            SimulationLodTier::HeroSimulation,
            LodSelectionReason::NearVisibleOrFragile,
        );
    }
    if fragile || visible {
        return (
            SimulationLodTier::LocalDeformation,
            LodSelectionReason::VisibleOrFragile,
        );
    }
    if signals.renderer_requested {
        return (
            signals
                .requested_lod
                .unwrap_or(SimulationLodTier::RigidWithMaterialState),
            LodSelectionReason::RendererRequested,
        );
    }
    if distance_to_player <= budget.hero_radius_meters {
        (
            SimulationLodTier::RigidWithMaterialState,
            LodSelectionReason::NearPlayer,
        )
    } else if signals.settled && distance_to_player > budget.hero_radius_meters * 4.0 {
        (
            SimulationLodTier::Dormant,
            LodSelectionReason::FarSettledDormant,
        )
    } else {
        (
            SimulationLodTier::SimpleRigid,
            LodSelectionReason::Background,
        )
    }
}

fn fracture_piece_count(impulse: f32, budget: &PhysicsBudget) -> usize {
    ((impulse / 12.0).ceil() as usize)
        .clamp(3, budget.max_fracture_pieces.max(1))
        .max(1)
}

fn sparse_range(center: Vec3, radius_cells: i32) -> SparseCellRange {
    let base = [
        center.x.floor() as i32,
        center.y.floor() as i32,
        center.z.floor() as i32,
    ];
    SparseCellRange {
        min_cell: [
            base[0] - radius_cells,
            base[1] - radius_cells,
            base[2] - radius_cells,
        ],
        max_cell: [
            base[0] + radius_cells,
            base[1] + radius_cells,
            base[2] + radius_cells,
        ],
    }
}

fn sparse_cell_count(range: &SparseCellRange) -> u64 {
    let extent_x = sparse_axis_cell_count(range.min_cell[0], range.max_cell[0]);
    let extent_y = sparse_axis_cell_count(range.min_cell[1], range.max_cell[1]);
    let extent_z = sparse_axis_cell_count(range.min_cell[2], range.max_cell[2]);
    extent_x.saturating_mul(extent_y).saturating_mul(extent_z)
}

fn sparse_axis_cell_count(min_cell: i32, max_cell: i32) -> u64 {
    if max_cell < min_cell {
        return 0;
    }
    let min_cell = i64::from(min_cell);
    let max_cell = i64::from(max_cell);
    u64::try_from(max_cell - min_cell + 1).unwrap_or(u64::MAX)
}

fn physics_resource_bytes(item_count: u64, bytes_per_item: u64) -> u64 {
    item_count.saturating_mul(bytes_per_item)
}

fn constraint_resolution_tags(kind: &ConstraintResolutionKind) -> TagSet {
    match kind {
        ConstraintResolutionKind::CableLength { .. } => vec![
            "constraint".to_string(),
            "soft_body".to_string(),
            "cable".to_string(),
        ],
        ConstraintResolutionKind::ClothPinWorldCollision { .. } => vec![
            "constraint".to_string(),
            "soft_body".to_string(),
            "cloth".to_string(),
            "collision".to_string(),
        ],
    }
}

fn normalized_or_default(vector: Vec3) -> Vec3 {
    let len = (vector.x * vector.x + vector.y * vector.y + vector.z * vector.z).sqrt();
    if len <= f32::EPSILON {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(vector.x / len, vector.y / len, vector.z / len)
    }
}

fn scale_vec3(vector: Vec3, scale: f32) -> Vec3 {
    Vec3::new(vector.x * scale, vector.y * scale, vector.z * scale)
}

fn add_vec3(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(left.x + right.x, left.y + right.y, left.z + right.z)
}

fn sub_vec3(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(left.x - right.x, left.y - right.y, left.z - right.z)
}

fn vec3_length(vector: Vec3) -> f32 {
    (vector.x * vector.x + vector.y * vector.y + vector.z * vector.z).sqrt()
}

fn is_ductile_metal(
    body: &BodyRecord,
    material_map: &BTreeMap<MaterialId, &MaterialDescriptor>,
) -> bool {
    if body.physical_body.fragile {
        return false;
    }
    let tagged_as_metal = body.tags.iter().any(|tag| {
        let tag = tag.to_ascii_lowercase();
        tag.contains("metal")
            || tag.contains("steel")
            || tag.contains("ductile")
            || tag.contains("chrome")
    });
    let named_as_metal = {
        let name = body.name.to_ascii_lowercase();
        name.contains("metal")
            || name.contains("steel")
            || name.contains("ductile")
            || name.contains("chrome")
    };
    let material_is_ductile = material_map
        .get(&body.physical_body.material_id)
        .is_some_and(|material| {
            material.visual.metallic >= 0.55
                && material.physical.yield_stress > 1_000_000.0
                && material.physical.fracture_toughness >= 0.12
        });

    tagged_as_metal || named_as_metal || material_is_ductile
}

fn plastic_strain_delta(load_ratio: f32, current_state: MaterialState) -> Option<f32> {
    let remaining = (1.0 - current_state.plastic_strain).max(0.0);
    if remaining <= 0.005 {
        return None;
    }
    let delta = (load_ratio * 0.35).clamp(0.015, 0.28).min(remaining);
    (delta > 0.0).then_some(delta)
}

fn validation_issue(
    severity: PhysicsValidationSeverity,
    code: &'static str,
    message: &'static str,
) -> PhysicsValidationIssue {
    PhysicsValidationIssue {
        severity,
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn has_tag(tags: &[String], expected: &str) -> bool {
    tags.iter().any(|tag| tag == expected)
}

fn physics_gpu_resource(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    kind: GpuResourceKind,
    byte_len: u64,
    lifetime: GpuResourceLifetime,
) -> GpuResourceHandle {
    graph.declare_resource(
        GpuResourceDesc::new(label, kind, byte_len)
            .owned_by(20)
            .with_lifetime(lifetime),
    )
}

fn physics_compute_pipeline(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    shader_key: &'static str,
    quality_tier: QualityTier,
) -> ComputePipelineHandle {
    graph
        .create_compute_pipeline(GpuPipelineDesc::new(label, shader_key, quality_tier))
        .handle
}

fn deterministic_event_id(tick: u64, module: u64, local: EntityId) -> WorldEventId {
    ((tick as u128) << 80) | ((module as u128) << 64) | local as u128
}

fn deterministic_u64(tick: u64, module: u64, local: EntityId) -> u64 {
    ((tick & 0xFFFF) << 48) | ((module & 0xFFFF) << 32) | (local & 0xFFFF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ashfall_core::world::{
        CommandSink, EntityTemplate, HumanState, PhysicalBody, WorldSnapshot, WorldState,
    };

    fn basic_input() -> PhysicsInput {
        let glass_body = BodyRecord {
            entity: 2,
            name: "glass wall".to_string(),
            tags: vec!["glass".to_string(), "destructible".to_string()],
            transform: Transform::at(Vec3::new(2.0, 0.0, 0.0)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(2_000),
                material: 1,
                visible: true,
                fracture_replacement_mesh: Some(MeshAssetHandle(2_001)),
            }),
            physical_body: PhysicalBody {
                mass_kg: 140.0,
                material_id: 1,
                dynamic: false,
                fragile: true,
            },
            velocity: Velocity::default(),
            visible: true,
            lod: SimulationLodTier::HeroSimulation,
            story_importance: 1.0,
            lod_signals: SimulationLodSignals::default(),
        };
        let pipe = BodyRecord {
            entity: 5,
            name: "leaking pipe".to_string(),
            tags: vec!["water_leak".to_string()],
            transform: Transform::at(Vec3::new(-1.5, 0.5, 0.4)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(5_000),
                material: 5,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: PhysicalBody {
                mass_kg: 12.0,
                material_id: 2,
                dynamic: false,
                fragile: false,
            },
            velocity: Velocity::default(),
            visible: true,
            lod: SimulationLodTier::RigidWithMaterialState,
            story_importance: 0.8,
            lod_signals: SimulationLodSignals::default(),
        };
        let asphalt = BodyRecord {
            entity: 6,
            name: "wettable asphalt".to_string(),
            tags: vec!["wettable".to_string(), "asphalt".to_string()],
            transform: Transform::at(Vec3::new(-0.8, 0.0, -0.02)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(6_000),
                material: 2,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: PhysicalBody {
                mass_kg: 10_000.0,
                material_id: 2,
                dynamic: false,
                fragile: false,
            },
            velocity: Velocity::default(),
            visible: true,
            lod: SimulationLodTier::RigidWithMaterialState,
            story_importance: 0.8,
            lod_signals: SimulationLodSignals::default(),
        };
        PhysicsInput {
            tick: 1,
            dt_seconds: 1.0 / 60.0,
            bodies: vec![glass_body, pipe, asphalt],
            material_table: Vec::new(),
            material_states: vec![
                (2, MaterialState::default()),
                (
                    6,
                    MaterialState {
                        moisture: 0.25,
                        ..MaterialState::default()
                    },
                ),
            ],
            forces: vec![ForceCommand {
                entity: 2,
                vector_newtons: Vec3::new(2_000.0, 0.0, 0.0),
                impulse_newton_seconds: 48.0,
                source: Some(1),
            }],
            damage_events: Vec::new(),
            constraints: Vec::new(),
            world_fields: WorldFieldView::default(),
            render_feedback: RenderImportanceFeedback::default(),
            quality_budget: PhysicsBudget::for_quality(QualityTier::NormalRuntime),
        }
    }

    fn contamination_spill_input() -> PhysicsInput {
        let mut input = basic_input();
        input.tick = 4;
        input.forces.clear();
        input
            .bodies
            .retain(|body| !has_tag(&body.tags, "water_leak"));
        input.bodies.push(BodyRecord {
            entity: 7,
            name: "ruptured hydraulic oil line".to_string(),
            tags: vec!["oil_leak".to_string()],
            transform: Transform::at(Vec3::new(-1.1, 0.15, 0.18)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(7_000),
                material: 2,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: PhysicalBody {
                mass_kg: 18.0,
                material_id: 2,
                dynamic: false,
                fragile: false,
            },
            velocity: Velocity::default(),
            visible: true,
            lod: SimulationLodTier::RigidWithMaterialState,
            story_importance: 0.7,
            lod_signals: SimulationLodSignals::default(),
        });
        input.bodies.push(BodyRecord {
            entity: 9,
            name: "biohazard sample spill".to_string(),
            tags: vec!["biological_spill".to_string()],
            transform: Transform::at(Vec3::new(-0.5, 0.2, 0.08)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(9_000),
                material: 4,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: PhysicalBody {
                mass_kg: 4.0,
                material_id: 4,
                dynamic: false,
                fragile: false,
            },
            velocity: Velocity::default(),
            visible: true,
            lod: SimulationLodTier::RigidWithMaterialState,
            story_importance: 0.6,
            lod_signals: SimulationLodSignals::default(),
        });
        input.material_states = vec![(6, MaterialState::default())];
        input
    }

    fn metal_input_from_damage() -> PhysicsInput {
        PhysicsInput {
            tick: 3,
            dt_seconds: 1.0 / 60.0,
            bodies: vec![BodyRecord {
                entity: 8,
                name: "ductile steel access panel".to_string(),
                tags: vec!["metal".to_string(), "ductile".to_string()],
                transform: Transform::at(Vec3::new(1.5, 0.0, 0.0)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(8_000),
                    material: 80,
                    visible: true,
                    fracture_replacement_mesh: None,
                }),
                physical_body: PhysicalBody {
                    mass_kg: 40.0,
                    material_id: 80,
                    dynamic: false,
                    fragile: false,
                },
                velocity: Velocity::default(),
                visible: true,
                lod: SimulationLodTier::LocalDeformation,
                story_importance: 0.7,
                lod_signals: SimulationLodSignals::default(),
            }],
            material_table: vec![MaterialDescriptor {
                id: 80,
                name: "ductile steel".to_string(),
                visual: VisualMaterial {
                    base_color_linear: [0.45, 0.46, 0.48, 1.0],
                    roughness: 0.34,
                    metallic: 1.0,
                    transmission: 0.0,
                    subsurface: 0.0,
                    emission_linear: [0.0, 0.0, 0.0],
                    anisotropy: 0.28,
                    clearcoat: 0.2,
                    normal_displacement_strength: 0.22,
                    layer_count: 3,
                    transparency: 0.0,
                },
                physical: PhysicalMaterial {
                    density_kg_per_m3: 7_850.0,
                    young_modulus: 200_000_000_000.0,
                    poisson_ratio: 0.29,
                    yield_stress: 250_000_000.0,
                    fracture_toughness: 0.72,
                    hardness: 0.62,
                    viscosity: 0.0,
                    surface_tension: 0.0,
                    restitution: 0.03,
                    friction_static: 0.48,
                    friction_dynamic: 0.38,
                },
                acoustic: AcousticMaterial {
                    impact_brightness: 0.66,
                    resonance: 0.72,
                    absorption: 0.18,
                    wetness_muffle: 0.12,
                },
                thermal: ThermalMaterial {
                    heat_capacity: 490.0,
                    conductivity: 45.0,
                    ignition_temperature: 1_800.0,
                },
                electrical: ElectricalMaterial {
                    conductivity: 0.95,
                    dielectric_strength: 1_000.0,
                },
                procedural_source: None,
            }],
            material_states: vec![(8, MaterialState::default())],
            forces: Vec::new(),
            damage_events: vec![DamageCommand {
                entity: 8,
                amount: 60.0,
                source: Some(1),
            }],
            constraints: Vec::new(),
            world_fields: WorldFieldView::default(),
            render_feedback: RenderImportanceFeedback::default(),
            quality_budget: PhysicsBudget::for_quality(QualityTier::NormalRuntime),
        }
    }

    fn flexible_constraint_input(constraint: PhysicsConstraint, position: Vec3) -> PhysicsInput {
        PhysicsInput {
            tick: 4,
            dt_seconds: 1.0 / 60.0,
            bodies: vec![BodyRecord {
                entity: 10,
                name: "service cable segment".to_string(),
                tags: vec!["cable".to_string(), "cloth".to_string()],
                transform: Transform::at(position),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(10_000),
                    material: 10,
                    visible: true,
                    fracture_replacement_mesh: None,
                }),
                physical_body: PhysicalBody {
                    mass_kg: 6.0,
                    material_id: 10,
                    dynamic: true,
                    fragile: false,
                },
                velocity: Velocity {
                    linear_meters_per_second: Vec3::new(0.0, -3.0, 0.0),
                    angular_radians_per_second: Vec3::ZERO,
                },
                visible: true,
                lod: SimulationLodTier::LocalDeformation,
                story_importance: 0.5,
                lod_signals: SimulationLodSignals::default(),
            }],
            material_table: Vec::new(),
            material_states: Vec::new(),
            forces: Vec::new(),
            damage_events: Vec::new(),
            constraints: vec![constraint],
            world_fields: WorldFieldView::default(),
            render_feedback: RenderImportanceFeedback::default(),
            quality_budget: PhysicsBudget::for_quality(QualityTier::NormalRuntime),
        }
    }

    fn planar_motion_snapshot(fractured_wall: bool) -> WorldSnapshot {
        let mut world = WorldState::default();
        for template in [
            EntityTemplate {
                entity_id: Some(1),
                name: "player".to_string(),
                transform: Transform::at(Vec3::ZERO),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 80.0,
                    material_id: 4,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["player".to_string()],
            },
            EntityTemplate {
                entity_id: Some(2),
                name: "glass wall".to_string(),
                transform: Transform::at(Vec3::new(2.0, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 140.0,
                    material_id: 1,
                    dynamic: false,
                    fragile: true,
                }),
                material_state: Some(MaterialState {
                    crack_density: if fractured_wall { 0.72 } else { 0.0 },
                    ..MaterialState::default()
                }),
                human: None,
                agent: None,
                tags: vec!["glass".to_string(), "wall".to_string()],
            },
            EntityTemplate {
                entity_id: Some(3),
                name: "overhead sign".to_string(),
                transform: Transform::at(Vec3::new(1.0, 0.0, 2.4)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 40.0,
                    material_id: 4,
                    dynamic: false,
                    fragile: false,
                }),
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["prop".to_string()],
            },
        ] {
            world.spawn_entity_template(template).expect("spawn entity");
        }
        world.snapshot(1, SimTime::default())
    }

    #[test]
    fn planar_motion_constraint_blocks_intact_wall_and_clamps_bounds() {
        let snapshot = planar_motion_snapshot(false);
        let settings = PlanarMotionSettings::new([-5.0, -5.0], [5.0, 5.0]);

        let constrained = constrain_planar_motion(&snapshot, 1, [0.0, 0.0], [8.0, 0.0], settings);

        let expected_wall_edge =
            2.0 - PLANAR_MOTION_DEFAULT_WALL_HALF_EXTENTS[0] - PLANAR_MOTION_DEFAULT_RADIUS_METERS;
        assert!((constrained[0] - expected_wall_edge).abs() < 0.001);
        assert_eq!(constrained[1], 0.0);
    }

    #[test]
    fn planar_motion_constraint_allows_fractured_glass_and_overhead_props() {
        let snapshot = planar_motion_snapshot(true);
        let settings = PlanarMotionSettings::new([-5.0, -5.0], [5.0, 5.0]);

        let constrained = constrain_planar_motion(&snapshot, 1, [0.0, 0.0], [8.0, 0.0], settings);

        assert_eq!(constrained, [5.0, 0.0]);
    }

    #[test]
    fn solver_outputs_fracture_pieces_mesh_audio_and_validation() {
        let output = solve_consequence_frame(
            &basic_input(),
            30.0,
            3.0,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );

        assert!(output.validation_report.passed);
        assert_eq!(output.fractures.len(), 1);
        let fracture = &output.fractures[0];
        assert_eq!(fracture.source_entity, 2);
        assert!(!fracture.pieces.is_empty());
        assert!(fracture.pieces.iter().all(|piece| {
            piece.mass_kg > 0.0 && !matches!(piece.collision_shape, CollisionShapeDesc::None)
        }));
        assert!(output.mesh_deltas.iter().any(
            |delta| delta.entity == 2 && delta.replacement_mesh == Some(MeshAssetHandle(2_001))
        ));
        assert!(
            output
                .audio_events
                .iter()
                .any(|audio| audio.event_kind == AudioEventKind::GlassShatter)
        );
        assert!(
            output
                .contacts
                .iter()
                .any(|contact| { contact.entities.0 == 1 && contact.entities.1 == Some(2) })
        );
        assert!(
            output
                .solver_reports
                .iter()
                .any(|report| report.family == SolverFamily::Fracture)
        );
    }

    #[test]
    fn fracture_replacement_mesh_is_driven_by_renderable_hint() {
        let mut input = basic_input();
        let replacement_mesh = MeshAssetHandle(9_999);
        input.bodies[0]
            .renderable
            .as_mut()
            .expect("fragile body should be renderable")
            .fracture_replacement_mesh = Some(replacement_mesh);

        let output = solve_consequence_frame(
            &input,
            30.0,
            3.0,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );

        let delta = output
            .mesh_deltas
            .iter()
            .find(|delta| delta.entity == 2)
            .expect("fracture should emit mesh delta");
        assert_eq!(delta.replacement_mesh, Some(replacement_mesh));
        assert_eq!(output.fractures[0].pieces[0].mesh, replacement_mesh);
        assert!(
            output
                .fractures
                .iter()
                .flat_map(|fracture| fracture.pieces.iter())
                .all(|piece| piece.mesh.0 >= replacement_mesh.0)
        );
    }

    #[test]
    fn physics_debug_report_explains_solver_lod_selection() {
        let mut world = WorldState::default();
        for template in [
            EntityTemplate {
                entity_id: Some(1),
                name: "player".to_string(),
                transform: Transform::at(Vec3::ZERO),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 80.0,
                    material_id: 4,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["player".to_string()],
            },
            EntityTemplate {
                entity_id: Some(2),
                name: "near fragile glass".to_string(),
                transform: Transform::at(Vec3::new(2.0, 0.0, 0.0)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(2_000),
                    material: 1,
                    visible: true,
                    fracture_replacement_mesh: Some(MeshAssetHandle(2_001)),
                }),
                physical_body: Some(PhysicalBody {
                    mass_kg: 140.0,
                    material_id: 1,
                    dynamic: false,
                    fragile: true,
                }),
                material_state: Some(MaterialState::default()),
                human: None,
                agent: None,
                tags: vec!["glass".to_string(), "destructible".to_string()],
            },
            EntityTemplate {
                entity_id: Some(11),
                name: "far inactive service block".to_string(),
                transform: Transform::at(Vec3::new(80.0, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 400.0,
                    material_id: 2,
                    dynamic: false,
                    fragile: false,
                }),
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["background_prop".to_string()],
            },
        ] {
            world.spawn_entity_template(template).expect("spawn");
        }
        let sim_time = SimTime::new(1.0 / 60.0, 1);
        let frame = FrameContext {
            frame_id: 1,
            sim_time,
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(1, sim_time),
            recent_events: Vec::new(),
            forces: Vec::new(),
        };
        let input = physics_input_from_frame(
            &frame,
            PhysicsBudget::for_quality(QualityTier::NormalRuntime),
        );
        let output = solve_consequence_frame(
            &input,
            30.0,
            3.0,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );

        assert_eq!(output.debug_report.quality, QualityTier::NormalRuntime);
        assert!(
            output
                .debug_report
                .active_solver_families
                .contains(&SolverFamily::LodManager)
        );
        assert_eq!(output.debug_report.lod_summary.body_count, 3);
        assert_eq!(output.debug_report.lod_summary.hero_simulation_count, 2);
        assert_eq!(output.debug_report.lod_summary.dormant_count, 1);
        assert!(output.debug_report.lod_summary.renderer_requested_count >= 1);
        let glass = output
            .debug_report
            .body_lods
            .iter()
            .find(|entry| entry.entity == 2)
            .expect("glass lod should be inspectable");
        assert_eq!(glass.lod, SimulationLodTier::HeroSimulation);
        assert_eq!(glass.reason, LodSelectionReason::NearVisibleOrFragile);
        assert!(glass.distance_to_player_meters < 3.0);

        let background = output
            .debug_report
            .body_lods
            .iter()
            .find(|entry| entry.entity == 11)
            .expect("background lod should be inspectable");
        assert_eq!(background.lod, SimulationLodTier::Dormant);
        assert_eq!(background.reason, LodSelectionReason::FarSettledDormant);
        assert!(background.distance_to_player_meters > 70.0);
    }

    #[test]
    fn simulation_lod_manager_promotes_high_energy_and_mission_entities() {
        let mut world = WorldState::default();
        for template in [
            EntityTemplate {
                entity_id: Some(1),
                name: "player".to_string(),
                transform: Transform::at(Vec3::ZERO),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 80.0,
                    material_id: 4,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["player".to_string()],
            },
            EntityTemplate {
                entity_id: Some(12),
                name: "far energized shutter".to_string(),
                transform: Transform::at(Vec3::new(48.0, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 200.0,
                    material_id: 2,
                    dynamic: false,
                    fragile: false,
                }),
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["prop".to_string()],
            },
            EntityTemplate {
                entity_id: Some(13),
                name: "mission security terminal".to_string(),
                transform: Transform::at(Vec3::new(52.0, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 90.0,
                    material_id: 3,
                    dynamic: false,
                    fragile: false,
                }),
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["mission".to_string(), "security".to_string()],
            },
        ] {
            world.spawn_entity_template(template).expect("spawn");
        }
        let sim_time = SimTime::new(2.0 / 60.0, 2);
        let frame = FrameContext {
            frame_id: 2,
            sim_time,
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(2, sim_time),
            recent_events: vec![WorldEvent {
                event_id: 90,
                tick: 2,
                location_meters: Vec3::new(52.0, 0.0, 0.0),
                actors: vec![1],
                kind: WorldEventKind::SecurityAlertRaised {
                    faction: 700,
                    source_event: 88,
                    threat: 13,
                    severity: 0.8,
                },
                physical_evidence: Vec::new(),
                narrative_tags: vec!["mission".to_string(), "security".to_string()],
            }],
            forces: vec![ForceCommand {
                entity: 12,
                vector_newtons: Vec3::new(3_000.0, 0.0, 0.0),
                impulse_newton_seconds: 42.0,
                source: None,
            }],
        };

        let input = physics_input_from_frame(
            &frame,
            PhysicsBudget::for_quality(QualityTier::NormalRuntime),
        );
        let output = solve_consequence_frame(
            &input,
            100.0,
            3.0,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        let energized = output
            .debug_report
            .body_lods
            .iter()
            .find(|entry| entry.entity == 12)
            .expect("energized body should be in LOD report");
        assert_eq!(energized.lod, SimulationLodTier::HeroSimulation);
        assert_eq!(energized.reason, LodSelectionReason::HighEnergy);
        assert!(energized.signals.high_energy);

        let terminal = output
            .debug_report
            .body_lods
            .iter()
            .find(|entry| entry.entity == 13)
            .expect("mission terminal should be in LOD report");
        assert_eq!(terminal.lod, SimulationLodTier::LocalDeformation);
        assert_eq!(terminal.reason, LodSelectionReason::MissionOrFaction);
        assert!(terminal.signals.mission_or_faction_relevant);
        assert!(output.debug_report.lod_summary.high_energy_count >= 1);
        assert!(output.debug_report.lod_summary.mission_relevant_count >= 1);
    }

    #[test]
    fn physics_budget_clamp_report_counts_dropped_contacts() {
        let mut output = PhysicsOutput::empty(9);
        output.contacts = (0..12)
            .map(|index| ContactEvent {
                entities: (index, None),
                location: Vec3::ZERO,
                normal: Vec3::new(0.0, 1.0, 0.0),
                impulse_newton_seconds: 1.0,
                material_ids: (1, None),
                friction: 0.5,
                restitution: 0.1,
            })
            .collect();

        let clamp_report = cap_budgeted_outputs(
            &mut output,
            &PhysicsBudget::for_quality(QualityTier::BackgroundApproximation),
        );

        assert_eq!(output.contacts.len(), 8);
        assert_eq!(clamp_report.pre_cap_contacts, 12);
        assert_eq!(clamp_report.contacts, 8);
        assert_eq!(clamp_report.dropped_contacts, 4);
        assert!(clamp_report.clamped());
    }

    #[test]
    fn liquid_solver_wets_surface_and_outputs_fluid_delta() {
        let output = solve_consequence_frame(
            &basic_input(),
            30.0,
            3.0,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );

        assert!(
            output
                .fluid_deltas
                .iter()
                .any(|fluid| fluid.wetness_changes.contains(&(6, 0.7))
                    && fluid.hazard_tags.iter().any(|tag| tag == "slippery"))
        );
        assert!(
            output
                .material_state_deltas
                .iter()
                .any(|delta| { delta.entity == 6 && delta.moisture >= 0.7 })
        );
        assert!(output.world_events.iter().any(|event| {
            matches!(event.kind, WorldEventKind::StreetFlooded) && event.actors.contains(&6)
        }));
    }

    #[test]
    fn liquid_solver_outputs_oil_and_biological_contamination_deltas() {
        let output = solve_consequence_frame(
            &contamination_spill_input(),
            30.0,
            3.0,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );

        assert!(output.validation_report.passed);
        assert!(
            output
                .fluid_deltas
                .iter()
                .any(|fluid| fluid.wetness_changes.contains(&(6, 0.32))
                    && fluid.hazard_tags.iter().any(|tag| tag == "oil")
                    && fluid.hazard_tags.iter().any(|tag| tag == "contamination"))
        );
        assert!(output.fluid_deltas.iter().any(|fluid| {
            fluid.wetness_changes.contains(&(6, 0.24))
                && fluid.hazard_tags.iter().any(|tag| tag == "biohazard")
                && fluid
                    .hazard_tags
                    .iter()
                    .any(|tag| tag == "biological_contamination")
        }));
        assert!(output.material_state_deltas.iter().any(|delta| {
            delta.entity == 6 && delta.oil_contamination >= 0.6 && delta.moisture > 0.0
        }));
        assert!(output.material_state_deltas.iter().any(|delta| {
            delta.entity == 6 && delta.biological_contamination >= 0.5 && delta.moisture > 0.0
        }));
        assert!(output.world_events.iter().any(|event| {
            matches!(event.kind, WorldEventKind::StreetFlooded)
                && event.actors == vec![7, 6]
                && event
                    .physical_evidence
                    .iter()
                    .any(|evidence| evidence == "oil_contamination")
                && event.narrative_tags.iter().any(|tag| tag == "liquid")
        }));
        assert!(output.world_events.iter().any(|event| {
            matches!(event.kind, WorldEventKind::StreetFlooded)
                && event.actors == vec![9, 6]
                && event
                    .physical_evidence
                    .iter()
                    .any(|evidence| evidence == "biological_contamination")
        }));
        assert!(output.audio_events.iter().any(|audio| {
            audio.event_kind == AudioEventKind::WaterSplash
                && audio.tags.iter().any(|tag| tag == "oil")
        }));
        assert!(output.audio_events.iter().any(|audio| {
            audio.event_kind == AudioEventKind::WaterSplash
                && audio
                    .tags
                    .iter()
                    .any(|tag| tag == "biological_contamination")
        }));
    }

    #[test]
    fn metal_damage_updates_plastic_strain_and_audio() {
        let output = solve_consequence_frame(
            &metal_input_from_damage(),
            30.0,
            3.0,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );

        assert!(output.validation_report.passed);
        assert_eq!(output.plastic_deformations.len(), 1);
        let deformation = &output.plastic_deformations[0];
        assert_eq!(deformation.entity, 8);
        assert!(deformation.plastic_strain_delta > 0.0);
        assert!(deformation.dent_depth_meters > 0.0);
        assert!(output.material_state_deltas.iter().any(|delta| {
            delta.entity == 8 && delta.plastic_strain == deformation.plastic_strain_delta
        }));
        assert!(output.world_events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::MetalBent {
                    entity: 8,
                    plastic_strain
                } if plastic_strain == deformation.plastic_strain_delta
            )
        }));
        assert!(
            output
                .audio_events
                .iter()
                .any(|audio| audio.event_kind == AudioEventKind::MetalBend)
        );
        assert!(
            output
                .solver_reports
                .iter()
                .any(|report| report.family == SolverFamily::PlasticDeformation)
        );
    }

    #[test]
    fn cable_constraint_clamps_overextended_body() {
        let input = flexible_constraint_input(
            PhysicsConstraint::Cable {
                entity: 10,
                anchor: Vec3::ZERO,
                max_length_meters: 2.0,
            },
            Vec3::new(4.0, 0.0, 0.0),
        );

        let output = solve_consequence_frame(
            &input,
            30.0,
            3.0,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );

        assert!(output.validation_report.passed);
        assert_eq!(output.constraint_resolutions.len(), 1);
        let (_, transform) = output
            .transforms
            .iter()
            .find(|(entity, _)| *entity == 10)
            .expect("constraint should output corrected transform");
        assert!((transform.translation_meters.x - 2.0).abs() < 0.001);
        assert!(output.velocities.iter().any(|(entity, velocity)| {
            *entity == 10 && velocity.linear_meters_per_second == Vec3::ZERO
        }));
        assert!(output.world_events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::FlexibleConstraintResolved {
                    entity: 10,
                    correction_meters
                } if correction_meters > 1.9
            ) && event.narrative_tags.iter().any(|tag| tag == "cable")
        }));
        assert!(
            output
                .solver_reports
                .iter()
                .any(|report| report.family == SolverFamily::SoftBody && report.output_count == 1)
        );
    }

    #[test]
    fn cloth_pin_collision_lifts_body_to_world_plane() {
        let input = flexible_constraint_input(
            PhysicsConstraint::ClothPin {
                entity: 10,
                attach_point: "left_hem".to_string(),
                stiffness: 0.65,
            },
            Vec3::new(0.0, -0.4, 0.0),
        );

        let output = solve_consequence_frame(
            &input,
            30.0,
            3.0,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );

        assert!(output.validation_report.passed);
        assert_eq!(output.constraint_resolutions.len(), 1);
        let resolution = &output.constraint_resolutions[0];
        assert!(matches!(
            resolution.kind,
            ConstraintResolutionKind::ClothPinWorldCollision { .. }
        ));
        assert_eq!(resolution.corrected_transform.translation_meters.y, 0.0);
        assert!(resolution.correction_meters >= 0.4);
        assert!(
            output.contacts.iter().any(|contact| {
                contact.entities.0 == 10 && contact.impulse_newton_seconds > 0.0
            })
        );
        assert!(output.world_events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::FlexibleConstraintResolved { entity: 10, .. }
            ) && event.narrative_tags.iter().any(|tag| tag == "cloth")
                && event.narrative_tags.iter().any(|tag| tag == "collision")
        }));
    }

    #[test]
    fn gas_solver_outputs_visibility_and_hazard_volume() {
        let mut input = basic_input();
        input.bodies.push(BodyRecord {
            entity: 9,
            name: "toxic vent".to_string(),
            tags: vec!["toxic_gas_source".to_string()],
            transform: Transform::at(Vec3::new(4.0, 0.0, 0.0)),
            renderable: None,
            physical_body: PhysicalBody {
                mass_kg: 4.0,
                material_id: 99,
                dynamic: false,
                fragile: false,
            },
            velocity: Velocity::default(),
            visible: true,
            lod: SimulationLodTier::HeroSimulation,
            story_importance: 1.0,
            lod_signals: SimulationLodSignals::default(),
        });

        let output = solve_consequence_frame(
            &input,
            30.0,
            3.0,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );

        assert!(
            output
                .gas_deltas
                .iter()
                .any(|gas| gas.visibility_blocking > 0.5 && gas.hazard_level > 0.8)
        );
        let gas_event = output
            .world_events
            .iter()
            .find(|event| matches!(event.kind, WorldEventKind::ToxicGasReleased))
            .expect("gas solver should emit a gas ledger event");
        assert!(
            gas_event
                .physical_evidence
                .iter()
                .any(|evidence| evidence == "gas_kind:toxic")
        );
        assert!(
            gas_event
                .physical_evidence
                .iter()
                .any(|evidence| evidence == "gas_density:0.90")
        );
        assert!(
            gas_event
                .physical_evidence
                .iter()
                .any(|evidence| evidence == "visibility_blocking:0.72")
        );
        assert!(
            gas_event
                .physical_evidence
                .iter()
                .any(|evidence| evidence == "hazard_level:0.86")
        );
        assert!(gas_event.narrative_tags.iter().any(|tag| tag == "toxic"));
    }

    #[test]
    fn gpu_schedule_reflects_active_solver_families() {
        let mut input = basic_input();
        input.bodies.push(BodyRecord {
            entity: 9,
            name: "toxic vent".to_string(),
            tags: vec!["toxic_gas_source".to_string()],
            transform: Transform::at(Vec3::new(4.0, 0.0, 0.0)),
            renderable: None,
            physical_body: PhysicalBody {
                mass_kg: 4.0,
                material_id: 99,
                dynamic: false,
                fragile: false,
            },
            velocity: Velocity::default(),
            visible: true,
            lod: SimulationLodTier::HeroSimulation,
            story_importance: 1.0,
            lod_signals: SimulationLodSignals::default(),
        });
        input.bodies.push(BodyRecord {
            entity: 10,
            name: "service cable segment".to_string(),
            tags: vec!["cable".to_string(), "cloth".to_string()],
            transform: Transform::at(Vec3::new(4.0, 0.0, 0.0)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(10_000),
                material: 10,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: PhysicalBody {
                mass_kg: 6.0,
                material_id: 10,
                dynamic: true,
                fragile: false,
            },
            velocity: Velocity::default(),
            visible: true,
            lod: SimulationLodTier::LocalDeformation,
            story_importance: 0.5,
            lod_signals: SimulationLodSignals::default(),
        });
        input.constraints.push(PhysicsConstraint::Cable {
            entity: 10,
            anchor: Vec3::ZERO,
            max_length_meters: 2.0,
        });
        let output = solve_consequence_frame(
            &input,
            30.0,
            3.0,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert!(!output.fractures.is_empty());
        assert!(!output.fluid_deltas.is_empty());
        assert!(!output.gas_deltas.is_empty());
        assert!(!output.constraint_resolutions.is_empty());
        let mut module = ConsequencePhysicsModule {
            last_output: Some(output),
            ..ConsequencePhysicsModule::default()
        };
        let services = ashfall_core::gpu::GpuServices::new_vulkan();
        let mut graph = services.begin_frame(22);

        module.schedule_gpu(&mut graph);

        for expected in [
            "physics_sparse_fields",
            "physics_simulation_lod_selection",
            "physics_liquid_sparse_fields",
            "physics_gas_sparse_fields",
            "physics_debris_compaction",
            "physics_soft_constraints",
        ] {
            assert!(
                graph.passes().iter().any(|pass| pass.name == expected),
                "{expected} pass should be scheduled"
            );
        }
        let report = services.submit(graph);
        assert!(report.validation.passed);
        assert_eq!(report.pipeline_report.unknown_pipeline_count, 0);
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(20)
                && usage.label == "physics simulation lod decision buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "physics_simulation_lod_selection")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(20)
                && usage.label == "physics compacted debris output"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "physics_debris_compaction")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(20)
                && usage.label == "physics liquid sparse field output"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "physics_liquid_sparse_fields")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(20)
                && usage.label == "physics gas density field output"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "physics_gas_sparse_fields")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(20)
                && usage.label == "physics soft constraint output"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "physics_soft_constraints")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "physics_simulation_lod_selection"
                && pipeline.shader_key == "physics/simulation_lod_selection.comp"
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "physics_liquid_sparse_fields"
                && pipeline.shader_key == "physics/liquid_sparse_fields.comp"
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "physics_gas_sparse_fields"
                && pipeline.shader_key == "physics/gas_sparse_fields.comp"
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "physics_debris_compaction"
                && pipeline.shader_key == "physics/debris_compaction.comp"
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "physics_soft_constraints"
                && pipeline.shader_key == "physics/soft_constraints.comp"
        }));
    }

    #[test]
    fn wet_material_state_reduces_friction() {
        let physical = PhysicalMaterial {
            density_kg_per_m3: 2_300.0,
            young_modulus: 8_000_000_000.0,
            poisson_ratio: 0.35,
            yield_stress: 12_000_000.0,
            fracture_toughness: 0.4,
            hardness: 0.55,
            viscosity: 0.0,
            surface_tension: 0.0,
            restitution: 0.1,
            friction_static: 0.62,
            friction_dynamic: 0.5,
        };
        let dry = MaterialState::default();
        let wet = MaterialState {
            moisture: 1.0,
            ..MaterialState::default()
        };
        let oily = MaterialState {
            oil_contamination: 1.0,
            ..MaterialState::default()
        };
        let biological = MaterialState {
            biological_contamination: 1.0,
            ..MaterialState::default()
        };

        let dry_friction = effective_friction(&physical, &dry);
        let wet_friction = effective_friction(&physical, &wet);
        let oily_friction = effective_friction(&physical, &oily);
        let biological_friction = effective_friction(&physical, &biological);

        assert!(wet_friction.0 < dry_friction.0);
        assert!(wet_friction.1 < dry_friction.1);
        assert!(oily_friction.0 < wet_friction.0);
        assert!(oily_friction.1 < wet_friction.1);
        assert!(biological_friction.0 < dry_friction.0);
        assert!(biological_friction.1 < dry_friction.1);
    }

    #[test]
    fn module_emits_legacy_world_commands_from_structured_output() {
        let mut world = WorldState::default();
        for template in [
            EntityTemplate {
                entity_id: Some(1),
                name: "player".to_string(),
                transform: Transform::at(Vec3::ZERO),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 80.0,
                    material_id: 4,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: Some(HumanState {
                    human_id: 1,
                    quality_tier: QualityTier::NormalRuntime,
                }),
                agent: None,
                tags: vec!["player".to_string()],
            },
            EntityTemplate {
                entity_id: Some(2),
                name: "glass".to_string(),
                transform: Transform::at(Vec3::new(2.0, 0.0, 0.0)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(2_000),
                    material: 1,
                    visible: true,
                    fracture_replacement_mesh: Some(MeshAssetHandle(2_001)),
                }),
                physical_body: Some(PhysicalBody {
                    mass_kg: 140.0,
                    material_id: 1,
                    dynamic: false,
                    fragile: true,
                }),
                material_state: Some(MaterialState::default()),
                human: None,
                agent: None,
                tags: vec!["glass".to_string(), "destructible".to_string()],
            },
        ] {
            world.spawn_entity_template(template).expect("spawn");
        }
        let frame = FrameContext {
            frame_id: 1,
            sim_time: SimTime::new(1.0 / 60.0, 1),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(1, SimTime::new(1.0 / 60.0, 1)),
            recent_events: Vec::new(),
            forces: vec![ForceCommand {
                entity: 2,
                vector_newtons: Vec3::new(2_000.0, 0.0, 0.0),
                impulse_newton_seconds: 48.0,
                source: Some(1),
            }],
        };
        let mut module = ConsequencePhysicsModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        assert!(sink.events.iter().any(|event| {
            matches!(event.kind, WorldEventKind::GlassWallFractured { entity: 2 })
        }));
        assert!(sink.commands.iter().any(|command| {
            matches!(
                command,
                WorldCommand::ReplaceMesh(2, MeshAssetHandle(2_001))
            )
        }));
        assert!(
            module
                .last_output()
                .is_some_and(|output| !output.fractures.is_empty())
        );
    }
}

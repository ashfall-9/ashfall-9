use std::collections::{BTreeMap, BTreeSet, VecDeque};

use ashfall_core::assets::{
    ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION, AssetKind, AssetLoadState, AssetPackageChunkManifest,
    AssetPackageManifest, AssetRecord, AssetRegistry,
};
use ashfall_core::core::*;
use ashfall_core::gpu::{
    ComputePipelineHandle, GpuDispatchKind, GpuGraphBuilder, GpuPassDesc, GpuPipelineDesc,
    GpuQueueKind, GpuResourceDesc, GpuResourceHandle, GpuResourceKind, GpuResourceLifetime,
    GpuShaderPermutation,
};
use ashfall_core::runtime::{EngineModule, FrameContext, ModuleDescriptor};
use ashfall_core::validation::{
    VALIDATION_REPORT_SCHEMA, ValidationReport as SharedValidationReport,
};
use ashfall_core::world::{
    AgentState, CommandError, CommandSink, EntityTemplate, HumanState, PhysicalBody,
    PopulationSummary, PrimaryInteractionSettings, Renderable, WorldCommand, WorldEvent,
    WorldEventKind, WorldState, validate_population_summary,
};
use ashfall_materials::{
    CalibratedPhotoSet, GenerationQuality, MATERIAL_GLASS, MATERIAL_HUMAN_SKIN, MATERIAL_NEON_TUBE,
    MATERIAL_WATER, MATERIAL_WET_ASPHALT, MaterialCaptureEvidence, MaterialGenerationRequest,
    MaterialInstanceSeed, MaterialModelKind, MaterialOutputBudget, MaterialOutputChannel,
    MaterialScanSet, MaterialStateChannels, MaterialSurfaceHint, apply_instance_variation,
    default_alley_materials, generate_material,
};

pub const PLAYER_ID: EntityId = 1;
pub const GLASS_WALL_ID: EntityId = 2;
pub const NPC_MARA_ID: EntityId = 3;
pub const NEON_SIGN_ID: EntityId = 4;
pub const WATER_LEAK_ID: EntityId = 5;
pub const ALLEY_ASPHALT_ID: EntityId = 6;
pub const ALLEY_ACOUSTIC_ZONE_ID: EntityId = 7;
pub const ALLEY_SECURITY_CAMERA_ID: EntityId = 8;
pub const ALLEY_SERVICE_DOOR_ID: EntityId = 9;
pub const ALLEY_MARKET_STALL_ID: EntityId = 10;
pub const ALLEY_LOCATION_ID: LocationId = 100;
pub const ALLEY_SECURITY_FACTION_ID: FactionId = 700;
pub const ALLEY_OPPOSITION_FACTION_ID: FactionId = 701;
pub const ALLEY_GLASS_INTACT_MESH_ASSET: AssetId = 2_000;
pub const ALLEY_GLASS_FRACTURED_MESH_ASSET: AssetId = 2_001;
pub const ALLEY_MARA_HUMAN_BUNDLE_ASSET: AssetId = 3_000;
pub const ALLEY_NEON_SIGN_MESH_ASSET: AssetId = 4_000;
pub const ALLEY_SERVICE_PIPE_MESH_ASSET: AssetId = 5_000;
pub const ALLEY_WET_ASPHALT_MESH_ASSET: AssetId = 6_000;
pub const ALLEY_SECURITY_CAMERA_MESH_ASSET: AssetId = 7_000;
pub const ALLEY_SERVICE_DOOR_MESH_ASSET: AssetId = 7_100;
pub const ALLEY_MARKET_STALL_MESH_ASSET: AssetId = 7_200;
pub const WORLDGEN_MATERIAL_CACHE_ASSET_BASE: AssetId = 40_000_000_000_000;
pub const WORLDGEN_MATERIAL_CACHE_ASSET_SPAN: AssetId = 1_000_000_000_000;
pub const WORLDGEN_MATERIAL_CACHE_ASSET_BYTES: u64 = 2 * 1024 * 1024;
pub const WORLDGEN_MATERIAL_GRAPH_ASSET_BYTES: u64 = 256 * 1024;
pub const WORLDGEN_CITY_CELL_ASSET_BASE: AssetId = 41_000_000_000_000;
pub const WORLDGEN_CITY_CELL_ASSET_SPAN: AssetId = 1_000_000_000_000;
pub const WORLDGEN_CITY_CELL_ASSET_BYTES: u64 = 1024 * 1024;
pub const CITY_CELL_PACKAGE_SCHEMA_VERSION: u32 = 4;
pub const CITY_MATERIAL_PACKAGE_SCHEMA_VERSION: u32 = 1;
pub const WORLDGEN_SYSTEM_ID: ModuleId = 70;
pub const ALLEY_CAMERA_INITIAL_YAW_RADIANS: f32 = std::f32::consts::FRAC_PI_2;
pub const ALLEY_CAMERA_EYE_HEIGHT_METERS: f32 = 1.65;
pub const ALLEY_WALKABLE_MIN: [f32; 2] = [-5.08, -9.5];
pub const ALLEY_WALKABLE_MAX: [f32; 2] = [5.08, 13.5];
pub const ALLEY_WALK_SPEED_METERS_PER_SECOND: f32 = 2.8;
pub const ALLEY_FAST_WALK_SPEED_METERS_PER_SECOND: f32 = 5.8;

pub type CityId = u64;
pub type DistrictId = u64;
pub type WorldChunkId = u64;
pub type CityCellId = WorldChunkId;
pub type MaterialPaletteRef = u64;
pub type PlatformBudget = u64;
pub type FactionTemplate = String;
pub type CityProfile = String;
pub type DistrictRequest = String;
pub type StoryGenerationRequirements = Vec<String>;
pub type ValidationLevel = QualityTier;
pub type NavigationData = NavigationGraph;
pub type InfrastructureSlice = Vec<InfrastructureLinkId>;
pub type WorldValidationReport = ValidationReport;
pub type EntitySpawnSet = Vec<EntityTemplate>;
pub type MaterialAssignment = (EntityId, MaterialId);
pub type SocioProfile = String;
pub type FactionInfluence = (FactionId, f32);
pub type InfrastructureState = String;
pub type DistrictVisualTheme = String;
pub type InfrastructureLinkId = u64;
pub type StorySeedId = u64;
pub type SecretRef = u64;
pub type PhysicsCellStateHandle = AssetId;
pub type AiCellSummaryHandle = AssetId;
pub type AudioZoneHandle = AssetId;
pub type NavDataHandle = AssetId;
pub type LightProbeHandle = AssetId;
pub type EventHistoryHandle = AssetId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistrictKind {
    CorporateCore,
    RainAlleySlum,
    IndustrialDock,
    BlackMarket,
    ClinicDistrict,
}

impl DistrictKind {
    pub fn from_request(request: &str) -> Self {
        let normalized = request.to_ascii_lowercase();
        if normalized.contains("corporate") {
            Self::CorporateCore
        } else if normalized.contains("dock") || normalized.contains("industrial") {
            Self::IndustrialDock
        } else if normalized.contains("market") {
            Self::BlackMarket
        } else if normalized.contains("clinic") || normalized.contains("medical") {
            Self::ClinicDistrict
        } else {
            Self::RainAlleySlum
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::CorporateCore => "corporate core",
            Self::RainAlleySlum => "rain alley slum",
            Self::IndustrialDock => "industrial dock",
            Self::BlackMarket => "black market",
            Self::ClinicDistrict => "clinic district",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldGenerationRequest {
    pub request_id: u128,
    pub seed: u64,
    pub city_profile: CityProfile,
    pub district_requests: Vec<DistrictRequest>,
    pub target_platform_budget: PlatformBudget,
    pub story_requirements: StoryGenerationRequirements,
    pub material_palette: MaterialPaletteRef,
    pub faction_templates: Vec<FactionTemplate>,
    pub validation_level: ValidationLevel,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldTemplate {
    pub city_id: CityId,
    pub seed: u64,
    pub districts: Vec<CityDistrict>,
    pub materials: Vec<MaterialDescriptor>,
    pub infrastructure: InfrastructureGraphSet,
    pub factions: Vec<FactionSeed>,
    pub chunks: Vec<WorldChunk>,
    pub city_cell_packages: Vec<CityCellPackage>,
    pub spawn_sets: Vec<EntitySpawnSet>,
    pub material_assignments: Vec<MaterialAssignment>,
    pub story_seeds: Vec<StorySeed>,
    pub navigation_data: NavigationData,
    pub validation_report: WorldValidationReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldChunk {
    pub chunk_id: WorldChunkId,
    pub district_id: DistrictId,
    pub bounds: Aabb,
    pub entities: Vec<EntityTemplate>,
    pub population: CityCellPopulationProfile,
    pub population_summary: PopulationSummary,
    pub local_navigation: NavigationData,
    pub local_infrastructure: InfrastructureSlice,
    pub local_story_hooks: Vec<StorySeed>,
    pub streaming_dependencies: Vec<AssetId>,
    pub neighbor_links: Vec<WorldChunkId>,
    pub navigation_links: Vec<LocationId>,
    pub active_npc_seeds: Vec<AgentPersonaId>,
    pub material_palette: MaterialPaletteRef,
    pub physics_activation_hints: Vec<String>,
    pub quality_tier: QualityTier,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CityCellPackage {
    pub cell_id: CityCellId,
    pub package_asset: AssetId,
    pub schema_version: u32,
    pub bounds: Aabb,
    pub district_id: DistrictId,
    pub geometry_chunks: Vec<AssetId>,
    pub material_packages: Vec<AssetId>,
    pub material_package_descriptors: Vec<CityCellMaterialPackageDescriptor>,
    pub physics_state: PhysicsCellStateHandle,
    pub ai_summary: AiCellSummaryHandle,
    pub audio_zone: AudioZoneHandle,
    pub nav_data: NavDataHandle,
    pub light_probe_data: LightProbeHandle,
    pub event_history: EventHistoryHandle,
    pub infrastructure_state: InfrastructureSlice,
    pub population: CityCellPopulationProfile,
    pub population_summary: PopulationSummary,
    pub streaming_priority: CityCellStreamingPriorityRules,
    pub quality_tier: QualityTier,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CityCellMaterialPackageDescriptor {
    pub package_asset: AssetId,
    pub material: MaterialId,
    pub material_name: String,
    pub material_graph: Option<AssetId>,
    pub schema_version: u32,
    pub source_summary: String,
    pub validation_passed: bool,
    pub capture_confidence: f32,
    pub calibrated_photo_count: usize,
    pub scan_set_count: usize,
    pub state_response_channels: Vec<CityMaterialStateResponse>,
    pub generated_cache_texture_count: usize,
    pub generated_virtual_page_count: usize,
    pub generated_cache_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CityMaterialStateResponse {
    Wetness,
    Cracks,
    Soot,
    Corrosion,
    Heat,
    Oil,
    Blood,
    Dust,
    BiologicalContamination,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CityCellPopulationProfile {
    pub crowd_spawn_rules: Vec<CrowdSpawnRule>,
    pub traffic_rules: Vec<TrafficFlowRule>,
    pub expected_active_npcs: usize,
    pub expected_background_crowd: usize,
    pub expected_vehicle_or_transit_count: usize,
    pub validation_report: CrowdTrafficValidationReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CrowdSpawnRule {
    pub rule_id: u64,
    pub archetype_label: String,
    pub density: f32,
    pub schedule: Vec<CrowdScheduleWindow>,
    pub faction_affinity: Option<FactionId>,
    pub culture_tags: Vec<String>,
    pub danger_reaction: CrowdDangerReaction,
    pub avoidance_radius_meters: f32,
    pub conversation_weight: f32,
    pub commerce_weight: f32,
    pub observation_weight: f32,
    pub lod_tier: PopulationLodTier,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CrowdScheduleWindow {
    pub start_hour: u8,
    pub end_hour: u8,
    pub density_multiplier: f32,
    pub behavior: CrowdBehaviorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CrowdBehaviorKind {
    Commute,
    Shop,
    Work,
    Loiter,
    Patrol,
    Evacuate,
    SeekClinic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CrowdDangerReaction {
    Avoid,
    Observe,
    Report,
    Evacuate,
    Ignore,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrafficFlowRule {
    pub rule_id: u64,
    pub mode: TrafficMode,
    pub density: f32,
    pub average_speed_mps: f32,
    pub crossing_locations: Vec<LocationId>,
    pub blocked_route_response: TrafficBlockedRouteResponse,
    pub emergency_response: TrafficEmergencyResponse,
    pub sound_lighting_profile: String,
    pub lod_tier: PopulationLodTier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrafficMode {
    Pedestrian,
    DeliveryBike,
    Drone,
    Tram,
    EmergencyVehicle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrafficBlockedRouteResponse {
    Reroute,
    Queue,
    DespawnAtPortal,
    EmergencyOverride,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrafficEmergencyResponse {
    SlowAndObserve,
    ClearLane,
    Flee,
    MaintainSchedule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PopulationLodTier {
    Hero,
    Simulated,
    Background,
    Summary,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CrowdTrafficValidationReport {
    pub passed: bool,
    pub issues: Vec<WorldValidationIssue>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CityCellStreamingPriorityRules {
    pub base_priority: f32,
    pub distance_weight: f32,
    pub story_weight: f32,
    pub renderer_feedback_weight: f32,
    pub predicted_route_weight: f32,
    pub consequence_weight: f32,
}

impl Default for CityCellStreamingPriorityRules {
    fn default() -> Self {
        Self {
            base_priority: 0.4,
            distance_weight: 1.0,
            story_weight: 0.7,
            renderer_feedback_weight: 0.5,
            predicted_route_weight: 0.35,
            consequence_weight: 0.8,
        }
    }
}

impl CityCellStreamingPriorityRules {
    pub fn aggregate_score(self) -> f32 {
        (self.base_priority
            + self.story_weight * 0.2
            + self.renderer_feedback_weight * 0.2
            + self.predicted_route_weight * 0.1
            + self.consequence_weight * 0.1)
            .clamp(0.0, 2.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CityStreamingRequest {
    pub player_position: Vec3,
    pub camera_forward: Vec3,
    pub player_velocity: Vec3,
    pub story_focus_locations: Vec<LocationId>,
    pub active_event_locations: Vec<Vec3>,
    pub renderer_visible_chunks: Vec<WorldChunkId>,
    pub renderer_feedback: Vec<CityRendererStreamingFeedback>,
    pub predicted_route: Vec<LocationId>,
    pub max_hero_cells: usize,
    pub max_render_high_detail_cells: usize,
    pub max_gameplay_cells: usize,
    pub max_loaded_bytes: u64,
}

impl Default for CityStreamingRequest {
    fn default() -> Self {
        Self {
            player_position: Vec3::ZERO,
            camera_forward: Vec3::new(1.0, 0.0, 0.0),
            player_velocity: Vec3::ZERO,
            story_focus_locations: Vec::new(),
            active_event_locations: Vec::new(),
            renderer_visible_chunks: Vec::new(),
            renderer_feedback: Vec::new(),
            predicted_route: Vec::new(),
            max_hero_cells: 1,
            max_render_high_detail_cells: 2,
            max_gameplay_cells: 4,
            max_loaded_bytes: 256 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CityRendererStreamingFeedback {
    pub chunk_id: WorldChunkId,
    pub screen_coverage: f32,
    pub visible_cluster_count: u32,
    pub material_cache_pressure: f32,
    pub micro_geometry_pressure: f32,
}

impl CityRendererStreamingFeedback {
    pub fn new(chunk_id: WorldChunkId) -> Self {
        Self {
            chunk_id,
            ..Self::default()
        }
    }

    pub fn with_visibility(mut self, screen_coverage: f32, visible_cluster_count: usize) -> Self {
        self.screen_coverage = screen_coverage.clamp(0.0, 1.0);
        self.visible_cluster_count = visible_cluster_count.min(u32::MAX as usize) as u32;
        self
    }

    pub fn with_streaming_pressure(
        mut self,
        material_cache_pressure: f32,
        micro_geometry_pressure: f32,
    ) -> Self {
        self.material_cache_pressure = material_cache_pressure.clamp(0.0, 1.0);
        self.micro_geometry_pressure = micro_geometry_pressure.clamp(0.0, 1.0);
        self
    }

    pub fn importance_score(self) -> f32 {
        let cluster_pressure = (self.visible_cluster_count as f32 / 256.0)
            .sqrt()
            .clamp(0.0, 1.0);
        (self.screen_coverage * 0.44
            + cluster_pressure * 0.24
            + self.material_cache_pressure * 0.18
            + self.micro_geometry_pressure * 0.14)
            .clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CityStreamingPlan {
    pub cells: Vec<CityCellStreamingDecision>,
    pub cell_count: usize,
    pub loaded_cell_count: usize,
    pub hero_loaded_count: usize,
    pub render_high_detail_count: usize,
    pub gameplay_loaded_count: usize,
    pub summary_loaded_count: usize,
    pub unloaded_count: usize,
    pub requested_asset_count: usize,
    pub requested_streaming_bytes: u64,
    pub max_loaded_bytes: u64,
    pub issues: Vec<WorldValidationIssue>,
}

impl CityStreamingPlan {
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn passed(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error)
    }

    pub fn highest_priority_cell(&self) -> Option<&CityCellStreamingDecision> {
        self.cells.iter().max_by(|left, right| {
            left.priority_score
                .partial_cmp(&right.priority_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CityCellStreamingDecision {
    pub cell_id: WorldChunkId,
    pub district_id: DistrictId,
    pub package_asset: AssetId,
    pub package_requested: bool,
    pub state: CityCellStreamingState,
    pub priority_score: f32,
    pub distance_meters: f32,
    pub camera_alignment: f32,
    pub predicted_route_match: bool,
    pub story_focus: bool,
    pub active_event: bool,
    pub renderer_visible: bool,
    pub renderer_feedback_score: f32,
    pub renderer_visible_cluster_count: u32,
    pub dependency_count: usize,
    pub estimated_streaming_bytes: u64,
    pub requested_assets: Vec<AssetId>,
    pub reasons: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum CityCellStreamingState {
    #[default]
    Unloaded,
    SummaryLoaded,
    GameplayLoaded,
    RenderHighDetail,
    HeroLoaded,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CityDistrict {
    pub id: DistrictId,
    pub name: String,
    pub kind: DistrictKind,
    pub socioeconomic_profile: SocioProfile,
    pub controlling_factions: Vec<FactionInfluence>,
    pub infrastructure: InfrastructureState,
    pub surveillance_level: f32,
    pub crime_pressure: f32,
    pub pollution: f32,
    pub visual_theme: DistrictVisualTheme,
    pub gameplay_tags: TagSet,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InfrastructureGraphSet {
    pub power: InfrastructureGraph,
    pub water: InfrastructureGraph,
    pub data: InfrastructureGraph,
    pub surveillance: InfrastructureGraph,
    pub transit: InfrastructureGraph,
    pub drainage: InfrastructureGraph,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InfrastructureGraph {
    pub nodes: Vec<InfrastructureNode>,
    pub edges: Vec<InfrastructureEdge>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InfrastructureNode {
    pub id: InfrastructureLinkId,
    pub location: LocationId,
    pub kind: InfrastructureNodeKind,
    pub health: f32,
    pub tags: TagSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfrastructureNodeKind {
    PowerTransformer,
    WaterPipe,
    DataRelay,
    SurveillanceCamera,
    TransitStop,
    DrainagePump,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InfrastructureEdge {
    pub from: InfrastructureLinkId,
    pub to: InfrastructureLinkId,
    pub kind: InfrastructureEdgeKind,
    pub capacity: f32,
    pub vulnerable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfrastructureEdgeKind {
    PowerCable,
    WaterMain,
    FiberLine,
    CameraFeed,
    TransitRoute,
    DrainageFlow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InfrastructureSystem {
    Power,
    Water,
    Data,
    Surveillance,
    Transit,
    Drainage,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InfrastructureDamageRequest {
    pub damaged_node: InfrastructureLinkId,
    pub severity: f32,
    pub source_entity: Option<EntityId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InfrastructureDamageReport {
    pub passed: bool,
    pub damaged_node: Option<InfrastructureNode>,
    pub system: Option<InfrastructureSystem>,
    pub severity: f32,
    pub affected_nodes: Vec<InfrastructureLinkId>,
    pub affected_chunks: Vec<WorldChunkId>,
    pub affected_districts: Vec<DistrictId>,
    pub story_seeds: Vec<StorySeedId>,
    pub faction_pressure: Vec<FactionPressureDelta>,
    pub expected_events: Vec<WorldEventKind>,
    pub consequence_tags: TagSet,
    pub issues: Vec<WorldValidationIssue>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InfrastructureConsequenceMap {
    pub city_id: CityId,
    pub seed: u64,
    pub passed: bool,
    pub damaged_node: Option<InfrastructureNode>,
    pub system: Option<InfrastructureSystem>,
    pub severity: f32,
    pub affected_nodes: Vec<InfrastructureLinkId>,
    pub affected_chunks: Vec<WorldChunkId>,
    pub affected_districts: Vec<DistrictId>,
    pub story_seeds: Vec<StorySeedId>,
    pub faction_pressure: Vec<FactionPressureDelta>,
    pub expected_events: Vec<WorldEventKind>,
    pub consequence_tags: TagSet,
    pub cell_impacts: Vec<InfrastructureCellImpact>,
    pub affected_entity_count: usize,
    pub affected_renderable_count: usize,
    pub affected_light_count: usize,
    pub affected_door_count: usize,
    pub affected_camera_count: usize,
    pub affected_npc_count: usize,
    pub affected_audio_zone_count: usize,
    pub affected_water_or_drainage_count: usize,
    pub affected_navigation_location_count: usize,
    pub affected_streaming_dependency_count: usize,
    pub expected_event_count: usize,
    pub story_seed_count: usize,
    pub faction_pressure_count: usize,
    pub issues: Vec<WorldValidationIssue>,
}

impl InfrastructureConsequenceMap {
    pub fn is_empty(&self) -> bool {
        self.cell_impacts.is_empty() && self.affected_nodes.is_empty()
    }

    pub fn passed(&self) -> bool {
        self.passed
            && !self
                .issues
                .iter()
                .any(|issue| issue.severity == ValidationSeverity::Error)
    }

    pub fn highest_impact_cell(&self) -> Option<&InfrastructureCellImpact> {
        self.cell_impacts.iter().max_by_key(|cell| {
            (
                cell.impact_count(),
                cell.streaming_dependency_count,
                cell.chunk_id,
            )
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InfrastructureCellImpact {
    pub chunk_id: WorldChunkId,
    pub district_id: DistrictId,
    pub affected_node_count: usize,
    pub entity_count: usize,
    pub renderable_count: usize,
    pub light_count: usize,
    pub door_count: usize,
    pub camera_count: usize,
    pub npc_count: usize,
    pub audio_zone_count: usize,
    pub water_or_drainage_count: usize,
    pub navigation_location_count: usize,
    pub streaming_dependency_count: usize,
    pub story_hook_count: usize,
    pub tags: TagSet,
}

impl InfrastructureCellImpact {
    pub fn impact_count(&self) -> usize {
        self.affected_node_count
            + self.renderable_count
            + self.light_count
            + self.door_count
            + self.camera_count
            + self.npc_count
            + self.audio_zone_count
            + self.water_or_drainage_count
            + self.navigation_location_count
            + self.story_hook_count
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FactionPressureDelta {
    pub faction_id: FactionId,
    pub district: DistrictId,
    pub alertness_delta: f32,
    pub resource_pressure_delta: f32,
    pub territory_strength: f32,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CityPersistenceState {
    pub city_id: CityId,
    pub seed: u64,
    pub cell_states: Vec<PersistentCellState>,
    pub event_count: usize,
    pub persistent_event_count: usize,
    pub material_override_count: usize,
    pub damaged_entity_count: usize,
    pub destroyed_entity_count: usize,
    pub spawned_entity_count: usize,
    pub mesh_replacement_count: usize,
    pub navigation_blocker_count: usize,
    pub infrastructure_delta_count: usize,
    pub faction_delta_count: usize,
    pub story_thread_ref_count: usize,
    pub ai_memory_ref_count: usize,
    pub resident_asset_count: usize,
    pub issues: Vec<WorldValidationIssue>,
}

impl CityPersistenceState {
    pub fn is_empty(&self) -> bool {
        self.cell_states.is_empty()
    }

    pub fn passed(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error)
    }

    pub fn save_required(&self) -> bool {
        self.persistent_event_count > 0 || !self.cell_states.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PersistentCellState {
    pub cell_id: WorldChunkId,
    pub district_id: DistrictId,
    pub material_state_overrides: Vec<MaterialStateOverride>,
    pub damaged_entities: Vec<EntityId>,
    pub destroyed_entities: Vec<EntityId>,
    pub spawned_entities: Vec<EntityId>,
    pub mesh_replacements: Vec<EntityId>,
    pub navigation_blockers: Vec<EntityId>,
    pub infrastructure_delta: InfrastructureDelta,
    pub event_history: Vec<WorldEventId>,
    pub faction_state_delta: Vec<FactionPressureDelta>,
    pub story_thread_refs: Vec<StorySeedId>,
    pub ai_memory_refs: Vec<EntityId>,
    pub resident_assets: Vec<AssetId>,
    pub consequence_tags: TagSet,
}

impl PersistentCellState {
    fn new(cell_id: WorldChunkId, district_id: DistrictId) -> Self {
        Self {
            cell_id,
            district_id,
            material_state_overrides: Vec::new(),
            damaged_entities: Vec::new(),
            destroyed_entities: Vec::new(),
            spawned_entities: Vec::new(),
            mesh_replacements: Vec::new(),
            navigation_blockers: Vec::new(),
            infrastructure_delta: InfrastructureDelta::default(),
            event_history: Vec::new(),
            faction_state_delta: Vec::new(),
            story_thread_refs: Vec::new(),
            ai_memory_refs: Vec::new(),
            resident_assets: Vec::new(),
            consequence_tags: Vec::new(),
        }
    }

    pub fn change_count(&self) -> usize {
        self.material_state_overrides.len()
            + self.damaged_entities.len()
            + self.destroyed_entities.len()
            + self.spawned_entities.len()
            + self.mesh_replacements.len()
            + self.navigation_blockers.len()
            + self.infrastructure_delta.affected_nodes.len()
            + self.faction_state_delta.len()
            + self.story_thread_refs.len()
            + self.ai_memory_refs.len()
            + self.resident_assets.len()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialStateOverride {
    pub entity: EntityId,
    pub material: Option<MaterialId>,
    pub source_event: WorldEventId,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InfrastructureDelta {
    pub affected_nodes: Vec<InfrastructureLinkId>,
    pub systems: Vec<InfrastructureSystem>,
    pub severity: f32,
    pub source_events: Vec<WorldEventId>,
    pub consequence_tags: TagSet,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavigationCellState {
    pub cell_id: WorldChunkId,
    pub district_id: DistrictId,
    pub node_count: usize,
    pub edge_count: usize,
    pub important_location_count: usize,
    pub reachable_important_location_count: usize,
    pub dynamic_blockers: Vec<NavigationDynamicBlocker>,
    pub danger_fields: Vec<NavigationDangerField>,
    pub restricted_zones: Vec<NavigationRestrictedZone>,
    pub route_updates: Vec<NavigationRouteUpdate>,
    pub issues: Vec<WorldValidationIssue>,
}

impl NavigationCellState {
    pub fn is_changed(&self) -> bool {
        !self.dynamic_blockers.is_empty()
            || !self.danger_fields.is_empty()
            || !self.restricted_zones.is_empty()
            || !self.route_updates.is_empty()
    }

    pub fn passed(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationDynamicBlocker {
    pub entity: EntityId,
    pub source_event: Option<WorldEventId>,
    pub blocked_location: Option<LocationId>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavigationDangerField {
    pub field_id: u64,
    pub source_event: Option<WorldEventId>,
    pub kind: NavigationDangerKind,
    pub severity: f32,
    pub affected_locations: Vec<LocationId>,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NavigationDangerKind {
    PhysicalDamage,
    Flooding,
    SlipperyContamination,
    BiohazardContamination,
    ToxicGas,
    Surveillance,
    SecurityAlert,
    Infrastructure,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavigationRestrictedZone {
    pub location: LocationId,
    pub faction: Option<FactionId>,
    pub severity: f32,
    pub source_event: Option<WorldEventId>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavigationRouteUpdate {
    pub from: LocationId,
    pub to: LocationId,
    pub traversal: TraversalKind,
    pub status: NavigationRouteStatus,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NavigationRouteStatus {
    Blocked,
    Dangerous,
    Restricted,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavigationGraph {
    pub nodes: Vec<NavigationNode>,
    pub edges: Vec<NavigationEdge>,
    pub important_locations: Vec<LocationId>,
}

impl NavigationGraph {
    pub fn has_location(&self, location: LocationId) -> bool {
        self.nodes.iter().any(|node| node.location == location)
    }

    pub fn reachable(&self, start: LocationId, target: LocationId) -> bool {
        if start == target {
            return self.has_location(start);
        }

        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::from([start]);
        while let Some(location) = queue.pop_front() {
            if !visited.insert(location) {
                continue;
            }
            for edge in &self.edges {
                if edge.from == location {
                    if edge.to == target {
                        return true;
                    }
                    queue.push_back(edge.to);
                }
                if edge.bidirectional && edge.to == location {
                    if edge.from == target {
                        return true;
                    }
                    queue.push_back(edge.from);
                }
            }
        }

        false
    }

    pub fn all_important_locations_reachable(&self) -> bool {
        let Some(start) = self.important_locations.first().copied() else {
            return false;
        };
        self.important_locations
            .iter()
            .copied()
            .all(|location| self.reachable(start, location))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavigationNode {
    pub location: LocationId,
    pub position: Vec3,
    pub tags: TagSet,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavigationEdge {
    pub from: LocationId,
    pub to: LocationId,
    pub traversal: TraversalKind,
    pub bidirectional: bool,
    pub length_meters: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraversalKind {
    Walk,
    CoverDash,
    StealthPath,
    ServiceLadder,
    Transit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FactionSeed {
    pub faction_id: FactionId,
    pub name: String,
    pub home_district: DistrictId,
    pub home_districts: Vec<DistrictId>,
    pub influence: f32,
    pub resources: ResourceState,
    pub territory_claims: Vec<TerritoryClaim>,
    pub allies: Vec<FactionId>,
    pub enemies: Vec<FactionId>,
    pub secrets: Vec<SecretRef>,
    pub visual_identity: FactionVisualProfile,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResourceState {
    pub credits: u32,
    pub water_access: f32,
    pub data_access: f32,
    pub muscle: f32,
    pub medical_supply: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerritoryClaim {
    pub district: DistrictId,
    pub strength: f32,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FactionVisualProfile {
    pub primary_color: [f32; 3],
    pub symbol: String,
    pub material_preference: MaterialId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StorySeed {
    pub seed_id: StorySeedId,
    pub label: String,
    pub location: LocationId,
    pub involved_factions: Vec<FactionId>,
    pub involved_npcs: Vec<AgentPersonaId>,
    pub secret_refs: Vec<SecretRef>,
    pub physical_hooks: Vec<EntityId>,
    pub trigger_conditions: Vec<TriggerCondition>,
    pub consequence_tags: TagSet,
    pub tags: TagSet,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TriggerCondition {
    InfrastructureDamaged(InfrastructureLinkId),
    FactionReputationBelow { faction: FactionId, threshold: f32 },
    PlayerEntered(LocationId),
    WitnessPresent(EntityId),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ValidationReport {
    pub passed: bool,
    pub issues: Vec<WorldValidationIssue>,
    pub budget: WorldBudgetUsage,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldValidationIssue {
    pub severity: ValidationSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldBudgetUsage {
    pub entity_count: usize,
    pub unique_material_count: usize,
    pub dynamic_light_count: usize,
    pub physics_active_object_count: usize,
    pub destructible_count: usize,
    pub liquid_zones: usize,
    pub npc_count: usize,
    pub crowd_spawn_rule_count: usize,
    pub traffic_rule_count: usize,
    pub expected_background_crowd_count: usize,
    pub expected_vehicle_or_transit_count: usize,
    pub streaming_dependency_count: usize,
    pub streaming_memory_bytes: u64,
    pub target_platform_budget: PlatformBudget,
}

pub fn generate_world_template(request: &WorldGenerationRequest) -> WorldTemplate {
    let requested_districts = if request.district_requests.is_empty() {
        vec!["rain alley slum".to_string()]
    } else {
        request.district_requests.clone()
    };

    let mut districts = Vec::new();
    let mut chunks = Vec::new();
    let mut materials = Vec::new();
    for (index, district_request) in requested_districts.iter().enumerate() {
        let kind = DistrictKind::from_request(district_request);
        let district = build_district(request.seed, index, kind);
        let chunk =
            build_chunk_for_district(request.seed, &district, index, request.material_palette);
        materials.extend(build_district_materials(
            request.seed,
            &district,
            request.material_palette,
        ));
        districts.push(district);
        chunks.push(chunk);
    }

    for index in 0..chunks.len() {
        if let Some(previous_id) = index
            .checked_sub(1)
            .and_then(|previous| chunks.get(previous).map(|chunk| chunk.chunk_id))
        {
            chunks[index].neighbor_links.push(previous_id);
        }
        if let Some(next_id) = chunks.get(index + 1).map(|chunk| chunk.chunk_id) {
            chunks[index].neighbor_links.push(next_id);
        }
    }
    let city_cell_packages = build_city_cell_packages(&chunks, &materials);

    let infrastructure = build_infrastructure_graphs(request.seed, &districts);
    let navigation_data = merge_navigation_graphs(&chunks);
    let factions = build_factions(request.seed, &districts);
    let story_seeds = build_story_seeds(request.seed, &districts, &factions, &chunks);
    let spawn_sets = chunks
        .iter()
        .map(|chunk| chunk.entities.clone())
        .collect::<Vec<_>>();
    let material_assignments = collect_material_assignments(&spawn_sets);

    let mut template = WorldTemplate {
        city_id: stable_u64(request.seed, 0xC17E),
        seed: request.seed,
        districts,
        materials,
        infrastructure,
        factions,
        chunks,
        city_cell_packages,
        spawn_sets,
        material_assignments,
        story_seeds,
        navigation_data,
        validation_report: ValidationReport::default(),
    };
    template.validation_report = validate_world_template(&template, request.target_platform_budget);
    template
}

pub fn worldgen_material_cache_asset_id(material_id: MaterialId) -> AssetId {
    WORLDGEN_MATERIAL_CACHE_ASSET_BASE
        + (stable_u64(material_id, 0xCA7C_EA55_EED5_0001) as AssetId
            % WORLDGEN_MATERIAL_CACHE_ASSET_SPAN)
}

pub fn is_worldgen_material_cache_asset(asset_id: AssetId) -> bool {
    (WORLDGEN_MATERIAL_CACHE_ASSET_BASE
        ..WORLDGEN_MATERIAL_CACHE_ASSET_BASE + WORLDGEN_MATERIAL_CACHE_ASSET_SPAN)
        .contains(&asset_id)
}

pub fn worldgen_city_cell_asset_id(chunk_id: WorldChunkId) -> AssetId {
    WORLDGEN_CITY_CELL_ASSET_BASE
        + (stable_u64(chunk_id, 0xC17E_11A5_5E7D_0001) as AssetId % WORLDGEN_CITY_CELL_ASSET_SPAN)
}

pub fn is_worldgen_city_cell_asset(asset_id: AssetId) -> bool {
    (WORLDGEN_CITY_CELL_ASSET_BASE..WORLDGEN_CITY_CELL_ASSET_BASE + WORLDGEN_CITY_CELL_ASSET_SPAN)
        .contains(&asset_id)
}

pub fn build_city_cell_packages(
    chunks: &[WorldChunk],
    materials: &[MaterialDescriptor],
) -> Vec<CityCellPackage> {
    let materials_by_cache_asset = materials
        .iter()
        .map(|material| (worldgen_material_cache_asset_id(material.id), material))
        .collect::<BTreeMap<_, _>>();
    chunks
        .iter()
        .map(|chunk| build_city_cell_package(chunk, &materials_by_cache_asset))
        .collect::<Vec<_>>()
}

fn build_city_cell_package(
    chunk: &WorldChunk,
    materials_by_cache_asset: &BTreeMap<AssetId, &MaterialDescriptor>,
) -> CityCellPackage {
    let geometry_chunks = chunk
        .streaming_dependencies
        .iter()
        .copied()
        .filter(|asset| !is_worldgen_material_cache_asset(*asset))
        .collect::<Vec<_>>();
    let material_packages = chunk
        .streaming_dependencies
        .iter()
        .copied()
        .filter(|asset| is_worldgen_material_cache_asset(*asset))
        .collect::<Vec<_>>();
    let material_package_descriptors = material_packages
        .iter()
        .filter_map(|asset| {
            materials_by_cache_asset
                .get(asset)
                .map(|material| city_cell_material_package_descriptor(chunk, *asset, material))
        })
        .collect::<Vec<_>>();

    CityCellPackage {
        cell_id: chunk.chunk_id,
        package_asset: worldgen_city_cell_asset_id(chunk.chunk_id),
        schema_version: CITY_CELL_PACKAGE_SCHEMA_VERSION,
        bounds: chunk.bounds,
        district_id: chunk.district_id,
        geometry_chunks,
        material_packages,
        material_package_descriptors,
        physics_state: city_cell_auxiliary_asset_id(chunk.chunk_id, 0xF151_C5A7_E001),
        ai_summary: city_cell_auxiliary_asset_id(chunk.chunk_id, 0xA155_A110_0001),
        audio_zone: city_cell_auxiliary_asset_id(chunk.chunk_id, 0xA0D1_020A_E001),
        nav_data: city_cell_auxiliary_asset_id(chunk.chunk_id, 0x0A7A_DA7A_0001),
        light_probe_data: city_cell_auxiliary_asset_id(chunk.chunk_id, 0x117E_900B_E000),
        event_history: city_cell_auxiliary_asset_id(chunk.chunk_id, 0xE7E1_0157_0A11),
        infrastructure_state: chunk.local_infrastructure.clone(),
        population: chunk.population.clone(),
        population_summary: chunk.population_summary.clone(),
        streaming_priority: city_cell_streaming_priority_rules(chunk),
        quality_tier: chunk.quality_tier,
    }
}

fn city_cell_material_package_descriptor(
    chunk: &WorldChunk,
    package_asset: AssetId,
    material: &MaterialDescriptor,
) -> CityCellMaterialPackageDescriptor {
    let generation_request = city_material_generation_request(chunk, material);
    let generated = generate_material(&generation_request);
    let state_response_channels = city_material_state_response_channels(&generation_request);
    let generated_cache_bytes = generated
        .cache_outputs
        .iter()
        .map(|cache| cache.bytes)
        .sum::<u64>();

    CityCellMaterialPackageDescriptor {
        package_asset,
        material: material.id,
        material_name: material.name.clone(),
        material_graph: material
            .procedural_source
            .map(|source| source.0)
            .or(Some(generated.graph.graph_id.0)),
        schema_version: CITY_MATERIAL_PACKAGE_SCHEMA_VERSION,
        source_summary: generated.provenance.source_summary,
        validation_passed: generated.validation_report.passed,
        capture_confidence: generated.provenance.capture_confidence,
        calibrated_photo_count: generated.provenance.calibrated_photo_count,
        scan_set_count: generated.provenance.scan_set_count,
        state_response_channels,
        generated_cache_texture_count: generated.cache_outputs.len(),
        generated_virtual_page_count: generated.virtual_texture_pages.len(),
        generated_cache_bytes,
    }
}

fn city_material_generation_request(
    chunk: &WorldChunk,
    material: &MaterialDescriptor,
) -> MaterialGenerationRequest {
    let material_graph = material
        .procedural_source
        .map(|source| source.0)
        .unwrap_or_else(|| stable_u64(chunk.chunk_id, material.id ^ 0xDA7A_3000) as AssetId);
    let quality = match chunk.quality_tier {
        QualityTier::ReferenceOfflineValidation | QualityTier::HeroHighFidelityRuntime => {
            GenerationQuality::Hero
        }
        QualityTier::Disabled => GenerationQuality::Draft,
        _ => GenerationQuality::Runtime,
    };
    let texture_size = match quality {
        GenerationQuality::Draft => 512,
        GenerationQuality::Runtime => 1024,
        GenerationQuality::Hero | GenerationQuality::Reference => 2048,
    };

    MaterialGenerationRequest {
        request_id: material_graph,
        source_photos: Vec::new(),
        capture_evidence: city_material_capture_evidence(chunk, material, quality),
        semantic_label: format!("city cell {} {}", chunk.chunk_id, material.name),
        scale_meters: city_material_scale_meters(material),
        target_model: city_material_model(material),
        required_states: city_material_state_channels(material),
        quality,
        output_budget: MaterialOutputBudget {
            max_texture_size: texture_size,
            max_graph_nodes: 24,
            max_runtime_microseconds: 32.0,
            max_cache_textures: 8,
            max_memory_bytes: 128 * 1024 * 1024,
            allow_gpu_compute: true,
        },
    }
}

fn city_material_capture_evidence(
    chunk: &WorldChunk,
    material: &MaterialDescriptor,
    quality: GenerationQuality,
) -> MaterialCaptureEvidence {
    let seed = stable_u64(chunk.chunk_id, material.id);
    let hero_grade = quality >= GenerationQuality::Hero;
    MaterialCaptureEvidence {
        photo_sets: vec![CalibratedPhotoSet {
            photos: vec![
                PhotoAssetId(city_cell_auxiliary_asset_id(
                    chunk.chunk_id,
                    seed ^ 0xF070_0001,
                )),
                PhotoAssetId(city_cell_auxiliary_asset_id(
                    chunk.chunk_id,
                    seed ^ 0xF070_0002,
                )),
            ],
            color_chart: true,
            scale_reference_meters: Some(city_material_scale_meters(material)),
            known_lighting: true,
            lighting_samples: if hero_grade { 5 } else { 3 },
            cross_polarized: true,
            view_angle_count: if hero_grade { 5 } else { 3 },
            license_ok: true,
        }],
        scan_sets: vec![MaterialScanSet {
            scan_asset: city_cell_auxiliary_asset_id(chunk.chunk_id, seed ^ 0x5CA1_0001),
            measured_area_m2: (city_material_scale_meters(material).powi(2) * 0.64).max(0.01),
            height_sample_count: if hero_grade { 1024 } else { 512 },
            normal_sample_count: if hero_grade { 1024 } else { 512 },
            roughness_sample_count: if hero_grade { 512 } else { 256 },
            hero_grade,
            license_ok: true,
        }],
        surface_hints: vec![
            MaterialSurfaceHint {
                channel: MaterialOutputChannel::Height,
                confidence: if hero_grade { 0.88 } else { 0.76 },
                source_label: "city material scan height hint".to_string(),
            },
            MaterialSurfaceHint {
                channel: MaterialOutputChannel::Roughness,
                confidence: if hero_grade { 0.84 } else { 0.72 },
                source_label: "district roughness calibration hint".to_string(),
            },
        ],
    }
}

fn city_material_model(material: &MaterialDescriptor) -> MaterialModelKind {
    let label = material.name.to_ascii_lowercase();
    if label.contains("skin") {
        MaterialModelKind::Skin
    } else if label.contains("water")
        || label.contains("glass")
        || material.visual.transparency > 0.0
    {
        MaterialModelKind::TransparentSurface
    } else {
        MaterialModelKind::PhysicallyBasedSurface
    }
}

fn city_material_state_channels(material: &MaterialDescriptor) -> MaterialStateChannels {
    let label = material.name.to_ascii_lowercase();
    MaterialStateChannels {
        wetness: label.contains("wet")
            || label.contains("asphalt")
            || label.contains("water")
            || material.acoustic.wetness_muffle > 0.2,
        cracks: label.contains("glass")
            || label.contains("concrete")
            || material.physical.fracture_toughness < 0.2,
        soot: label.contains("alley")
            || label.contains("market")
            || material.visual.roughness > 0.65,
        corrosion: label.contains("steel")
            || label.contains("metal")
            || label.contains("neon")
            || material.visual.metallic > 0.4,
        heat: material
            .visual
            .emission_linear
            .iter()
            .any(|channel| *channel > 0.1),
        oil: label.contains("asphalt"),
        blood: false,
        dust: material.visual.roughness > 0.55,
        biological_contamination: label.contains("skin") || label.contains("clinic"),
    }
}

fn city_material_state_response_channels(
    request: &MaterialGenerationRequest,
) -> Vec<CityMaterialStateResponse> {
    let mut channels = Vec::new();
    if request.required_states.wetness {
        channels.push(CityMaterialStateResponse::Wetness);
    }
    if request.required_states.cracks {
        channels.push(CityMaterialStateResponse::Cracks);
    }
    if request.required_states.soot {
        channels.push(CityMaterialStateResponse::Soot);
    }
    if request.required_states.corrosion {
        channels.push(CityMaterialStateResponse::Corrosion);
    }
    if request.required_states.heat {
        channels.push(CityMaterialStateResponse::Heat);
    }
    if request.required_states.oil {
        channels.push(CityMaterialStateResponse::Oil);
    }
    if request.required_states.blood {
        channels.push(CityMaterialStateResponse::Blood);
    }
    if request.required_states.dust {
        channels.push(CityMaterialStateResponse::Dust);
    }
    if request.required_states.biological_contamination {
        channels.push(CityMaterialStateResponse::BiologicalContamination);
    }
    channels
}

fn city_material_scale_meters(material: &MaterialDescriptor) -> f32 {
    let label = material.name.to_ascii_lowercase();
    if label.contains("skin") {
        0.08
    } else if label.contains("glass") {
        1.2
    } else if label.contains("water") || label.contains("asphalt") {
        2.0
    } else {
        1.0
    }
}

fn city_cell_auxiliary_asset_id(chunk_id: WorldChunkId, salt: u64) -> AssetId {
    42_000_000_000_000 + stable_u64(chunk_id, salt) as AssetId
}

fn city_cell_streaming_priority_rules(chunk: &WorldChunk) -> CityCellStreamingPriorityRules {
    let quality_bias = match chunk.quality_tier {
        QualityTier::ReferenceOfflineValidation => 1.0,
        QualityTier::HeroHighFidelityRuntime => 0.82,
        QualityTier::NormalRuntime => 0.56,
        QualityTier::BackgroundApproximation => 0.28,
        QualityTier::Disabled => 0.0,
    };
    CityCellStreamingPriorityRules {
        base_priority: (quality_bias
            + chunk.active_npc_seeds.len() as f32 * 0.05
            + chunk.population.expected_background_crowd as f32 * 0.002
            + chunk.population.expected_vehicle_or_transit_count as f32 * 0.01
            + chunk.local_story_hooks.len() as f32 * 0.04)
            .clamp(0.0, 1.25),
        distance_weight: 1.0,
        story_weight: (0.55 + chunk.local_story_hooks.len() as f32 * 0.1).clamp(0.0, 1.0),
        renderer_feedback_weight: (0.42 + chunk.streaming_dependencies.len() as f32 * 0.025)
            .clamp(0.0, 1.0),
        predicted_route_weight: (0.35 + chunk.population.traffic_rules.len() as f32 * 0.04)
            .clamp(0.0, 1.0),
        consequence_weight: (0.45
            + chunk.physics_activation_hints.len() as f32 * 0.08
            + chunk.local_infrastructure.len() as f32 * 0.015)
            .clamp(0.0, 1.0),
    }
}

pub fn build_world_template_asset_records(template: &WorldTemplate) -> Vec<AssetRecord> {
    let material_cache_dependencies = template
        .chunks
        .iter()
        .flat_map(|chunk| chunk.streaming_dependencies.iter().copied())
        .filter(|asset| is_worldgen_material_cache_asset(*asset))
        .collect::<BTreeSet<_>>();
    let procedural_materials = template
        .materials
        .iter()
        .filter_map(|material| {
            material
                .procedural_source
                .map(|source| (source.0, material))
        })
        .collect::<BTreeMap<_, _>>();
    let materials_by_cache_asset = template
        .materials
        .iter()
        .map(|material| (worldgen_material_cache_asset_id(material.id), material))
        .collect::<BTreeMap<_, _>>();
    let material_package_descriptors = template
        .city_cell_packages
        .iter()
        .flat_map(|package| package.material_package_descriptors.iter())
        .map(|descriptor| (descriptor.package_asset, descriptor))
        .collect::<BTreeMap<_, _>>();

    let mut records = Vec::new();
    for (graph_id, material) in procedural_materials {
        records.push(AssetRecord {
            id: graph_id,
            kind: AssetKind::MaterialGraph,
            label: format!("{} procedural material graph", material.name),
            provenance: format!(
                "ashfall worldgen material graph city:{} material:{}",
                template.city_id, material.id
            ),
            dependencies: Vec::new(),
            byte_len: Some(WORLDGEN_MATERIAL_GRAPH_ASSET_BYTES),
            generated: true,
            quality_tier: QualityTier::NormalRuntime,
            load_state: AssetLoadState::Unloaded,
        });
    }

    for cache_asset in material_cache_dependencies {
        let Some(material) = materials_by_cache_asset.get(&cache_asset) else {
            continue;
        };
        let dependencies = material
            .procedural_source
            .map(|source| vec![source.0])
            .unwrap_or_default();
        let package_descriptor = material_package_descriptors.get(&cache_asset);
        records.push(AssetRecord {
            id: cache_asset,
            kind: AssetKind::Texture,
            label: format!("{} generated material cache page", material.name),
            provenance: format!(
                "ashfall worldgen material cache city:{} material:{} source:{} confidence:{:.2}",
                template.city_id,
                material.id,
                package_descriptor
                    .map(|descriptor| descriptor.source_summary.as_str())
                    .unwrap_or("procedural descriptor"),
                package_descriptor
                    .map(|descriptor| descriptor.capture_confidence)
                    .unwrap_or_default()
            ),
            dependencies,
            byte_len: Some(WORLDGEN_MATERIAL_CACHE_ASSET_BYTES),
            generated: true,
            quality_tier: QualityTier::NormalRuntime,
            load_state: AssetLoadState::Unloaded,
        });
    }

    for package in &template.city_cell_packages {
        let mut dependencies = package.geometry_chunks.clone();
        dependencies.extend(package.material_packages.iter().copied());
        dependencies.sort_unstable();
        dependencies.dedup();
        records.push(AssetRecord {
            id: package.package_asset,
            kind: AssetKind::WorldChunk,
            label: format!(
                "generated city cell {} district {}",
                package.cell_id, package.district_id
            ),
            provenance: format!(
                "ashfall worldgen city cell city:{} chunk:{} district:{}",
                template.city_id, package.cell_id, package.district_id
            ),
            dependencies,
            byte_len: Some(WORLDGEN_CITY_CELL_ASSET_BYTES),
            generated: true,
            quality_tier: package.quality_tier,
            load_state: AssetLoadState::Unloaded,
        });
    }

    records.sort_by_key(|record| record.id);
    records
}

pub fn build_world_template_package_manifests(
    template: &WorldTemplate,
) -> Vec<AssetPackageManifest> {
    let mut manifests = template
        .city_cell_packages
        .iter()
        .map(city_cell_package_manifest)
        .collect::<Vec<_>>();
    manifests.sort_by_key(|manifest| manifest.package_asset);
    manifests
}

pub fn city_cell_package_manifest(package: &CityCellPackage) -> AssetPackageManifest {
    let mut dependencies = package.geometry_chunks.clone();
    dependencies.extend(package.material_packages.iter().copied());
    dependencies.sort_unstable();
    dependencies.dedup();

    AssetPackageManifest {
        package_asset: package.package_asset,
        manifest_schema_version: ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION,
        asset_kind: AssetKind::WorldChunk,
        label: format!(
            "generated city cell {} district {}",
            package.cell_id, package.district_id
        ),
        provenance: format!(
            "ashfall worldgen city cell chunk:{} district:{}",
            package.cell_id, package.district_id
        ),
        schema_name: "CityCellPackage".to_string(),
        schema_version: CITY_CELL_PACKAGE_SCHEMA_VERSION,
        dependencies,
        chunks: vec![AssetPackageChunkManifest {
            chunk_id: package.cell_id,
            usage_label: "world_cell".to_string(),
            uncompressed_size: WORLDGEN_CITY_CELL_ASSET_BYTES,
            compressed_size: WORLDGEN_CITY_CELL_ASSET_BYTES,
            content_hash: city_cell_package_manifest_hash(package),
        }],
        total_uncompressed_bytes: WORLDGEN_CITY_CELL_ASSET_BYTES,
        generated: true,
        quality_tier: package.quality_tier,
        validation: city_cell_package_validation_report(package),
    }
}

pub fn city_cell_package_validation_report(package: &CityCellPackage) -> SharedValidationReport {
    let mut report = SharedValidationReport::for_asset(
        package.package_asset,
        WORLDGEN_SYSTEM_ID,
        VALIDATION_REPORT_SCHEMA,
    );
    let subject = format!("city_cell:{}", package.cell_id);

    if package.package_asset != worldgen_city_cell_asset_id(package.cell_id) {
        report.add_error(
            "city_cell_package_asset_mismatch",
            "CityCellPackage asset id must be deterministic from the cell id",
            subject.clone(),
        );
    }
    if package.schema_version != CITY_CELL_PACKAGE_SCHEMA_VERSION {
        report.add_error(
            "city_cell_package_schema_mismatch",
            "CityCellPackage schema version must match the worldgen contract",
            subject.clone(),
        );
    }
    if package.geometry_chunks.is_empty() {
        report.add_error(
            "city_cell_package_missing_geometry",
            "CityCellPackage must declare geometry chunks",
            subject.clone(),
        );
    }
    if package.material_packages.is_empty() {
        report.add_error(
            "city_cell_package_missing_materials",
            "CityCellPackage must declare generated material packages",
            subject.clone(),
        );
    }
    if package.material_package_descriptors.len() != package.material_packages.len() {
        report.add_error(
            "city_cell_package_material_descriptor_mismatch",
            "CityCellPackage must describe every generated material package",
            subject.clone(),
        );
    }
    let material_package_assets = package
        .material_packages
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for material_descriptor in &package.material_package_descriptors {
        let descriptor_subject = format!(
            "city_cell:{} material:{}",
            package.cell_id, material_descriptor.material
        );
        if material_descriptor.schema_version != CITY_MATERIAL_PACKAGE_SCHEMA_VERSION {
            report.add_error(
                "city_material_package_schema_mismatch",
                "generated material package descriptor schema must match the worldgen contract",
                descriptor_subject.clone(),
            );
        }
        if !material_package_assets.contains(&material_descriptor.package_asset) {
            report.add_error(
                "city_material_package_unknown_asset",
                "material package descriptor must reference a declared material package asset",
                descriptor_subject.clone(),
            );
        }
        if !material_descriptor.validation_passed {
            report.add_error(
                "city_material_package_validation_failed",
                "generated material package validation must pass before it can be streamed",
                descriptor_subject.clone(),
            );
        }
        if material_descriptor.material_graph.is_none() {
            report.add_error(
                "city_material_package_missing_graph",
                "generated material package must retain its procedural graph asset",
                descriptor_subject.clone(),
            );
        }
        if material_descriptor.capture_confidence < 0.65 {
            report.add_warning(
                "city_material_package_low_capture_confidence",
                "generated material package should retain calibrated capture or scan evidence",
                descriptor_subject.clone(),
            );
        }
        if material_descriptor.state_response_channels.is_empty() {
            report.add_warning(
                "city_material_package_missing_state_response",
                "generated material package should expose material state response channels",
                descriptor_subject,
            );
        }
    }
    if package.quality_tier == QualityTier::Disabled {
        report.add_warning(
            "city_cell_package_disabled_quality",
            "CityCellPackage is present but marked disabled",
            subject.clone(),
        );
    }
    if [
        package.physics_state,
        package.ai_summary,
        package.audio_zone,
        package.nav_data,
        package.light_probe_data,
        package.event_history,
    ]
    .contains(&0)
    {
        report.add_error(
            "city_cell_package_missing_state_handle",
            "CityCellPackage must expose physics, AI, audio, navigation, lighting, and event handles",
            subject.clone(),
        );
    }
    if !package.population.validation_report.passed {
        report.add_error(
            "city_cell_package_invalid_population",
            "CityCellPackage population profile must include valid crowd and traffic rules",
            subject.clone(),
        );
    }
    let population_summary_validation = validate_population_summary(&package.population_summary);
    if !population_summary_validation.passed {
        report.add_error(
            "city_cell_package_invalid_population_summary",
            "CityCellPackage population summary must satisfy the shared v5 schema contract",
            subject.clone(),
        );
    }

    report.add_metric(
        "geometry_chunk_count",
        package.geometry_chunks.len() as f64,
        "count",
        Some(1.0),
    );
    report.add_metric(
        "material_package_count",
        package.material_packages.len() as f64,
        "count",
        Some(1.0),
    );
    report.add_metric(
        "material_package_descriptor_count",
        package.material_package_descriptors.len() as f64,
        "count",
        Some(package.material_packages.len() as f64),
    );
    report.add_metric(
        "material_package_capture_confidence",
        average_material_package_capture_confidence(package) as f64,
        "ratio",
        Some(0.65),
    );
    report.add_metric(
        "material_package_state_response_count",
        package
            .material_package_descriptors
            .iter()
            .map(|descriptor| descriptor.state_response_channels.len())
            .sum::<usize>() as f64,
        "count",
        Some(1.0),
    );
    report.add_metric("state_handle_count", 6.0, "count", Some(6.0));
    report.add_metric(
        "streaming_priority_score",
        package.streaming_priority.aggregate_score() as f64,
        "score",
        None,
    );
    report.add_metric(
        "crowd_spawn_rule_count",
        package.population.crowd_spawn_rules.len() as f64,
        "count",
        Some(1.0),
    );
    report.add_metric(
        "traffic_rule_count",
        package.population.traffic_rules.len() as f64,
        "count",
        Some(1.0),
    );
    report.add_metric(
        "population_summary_activity",
        package.population_summary.normalized_activity() as f64,
        "ratio",
        None,
    );
    report.add_metric(
        "population_summary_issue_count",
        population_summary_validation.issue_count() as f64,
        "count",
        Some(0.0),
    );

    report
}

fn average_material_package_capture_confidence(package: &CityCellPackage) -> f32 {
    if package.material_package_descriptors.is_empty() {
        return 0.0;
    }
    package
        .material_package_descriptors
        .iter()
        .map(|descriptor| descriptor.capture_confidence)
        .sum::<f32>()
        / package.material_package_descriptors.len() as f32
}

fn city_cell_package_manifest_hash(package: &CityCellPackage) -> u128 {
    let summary = &package.population_summary;
    let material_truth_hash =
        package
            .material_package_descriptors
            .iter()
            .fold(0u64, |hash, descriptor| {
                hash ^ stable_u64(
                    descriptor.material
                        ^ descriptor.package_asset as u64
                        ^ ((descriptor.capture_confidence * 10_000.0) as u64)
                        ^ ((descriptor.state_response_channels.len() as u64) << 32),
                    descriptor
                        .material_graph
                        .map(|graph| graph as u64)
                        .unwrap_or(0xDA7A_3000),
                )
            });
    let high = stable_u64(
        package.cell_id
            ^ ((summary.expected_background_crowd as u64) << 8)
            ^ ((summary.expected_vehicle_or_transit_count as u64) << 24)
            ^ material_truth_hash,
        0xC17E_CE11_5EED,
    );
    let low = stable_u64(
        package.district_id
            ^ ((summary.crowd_spawn_rule_count as u64) << 16)
            ^ ((summary.traffic_rule_count as u64) << 32),
        0xC17E_D157_51C7,
    );
    ((high as u128) << 64) | low as u128
}

pub fn plan_city_cell_streaming(
    template: &WorldTemplate,
    request: &CityStreamingRequest,
) -> CityStreamingPlan {
    let packages_by_cell = template
        .city_cell_packages
        .iter()
        .map(|package| (package.cell_id, package))
        .collect::<BTreeMap<_, _>>();
    let mut decisions = template
        .chunks
        .iter()
        .map(|chunk| {
            city_cell_streaming_decision(
                chunk,
                packages_by_cell.get(&chunk.chunk_id).copied(),
                request,
            )
        })
        .collect::<Vec<_>>();
    decisions.sort_by(|left, right| {
        right
            .priority_score
            .partial_cmp(&left.priority_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.cell_id.cmp(&right.cell_id))
    });

    let mut loaded_bytes = 0u64;
    let mut hero_loaded = 0usize;
    let mut render_loaded = 0usize;
    let mut gameplay_loaded = 0usize;
    for decision in &mut decisions {
        let desired_state = desired_city_cell_streaming_state(
            decision,
            request,
            hero_loaded,
            render_loaded,
            gameplay_loaded,
        );
        let requested_bytes = if city_streaming_state_requests_assets(desired_state) {
            decision.estimated_streaming_bytes
        } else {
            0
        };

        decision.state = if requested_bytes > 0
            && loaded_bytes.saturating_add(requested_bytes) > request.max_loaded_bytes
        {
            decision
                .reasons
                .push("demoted to summary by city streaming memory budget".to_string());
            CityCellStreamingState::SummaryLoaded
        } else {
            desired_state
        };

        if city_streaming_state_requests_assets(decision.state) {
            loaded_bytes = loaded_bytes.saturating_add(decision.estimated_streaming_bytes);
            if let Some(package) = packages_by_cell.get(&decision.cell_id) {
                decision.requested_assets = city_cell_package_requested_assets(package);
                decision.package_requested = true;
            } else {
                decision.requested_assets = template
                    .chunks
                    .iter()
                    .find(|chunk| chunk.chunk_id == decision.cell_id)
                    .map(|chunk| chunk.streaming_dependencies.clone())
                    .unwrap_or_default();
            }
        }

        match decision.state {
            CityCellStreamingState::HeroLoaded => {
                hero_loaded += 1;
                render_loaded += 1;
                gameplay_loaded += 1;
            }
            CityCellStreamingState::RenderHighDetail => {
                render_loaded += 1;
                gameplay_loaded += 1;
            }
            CityCellStreamingState::GameplayLoaded => gameplay_loaded += 1,
            CityCellStreamingState::SummaryLoaded | CityCellStreamingState::Unloaded => {}
        }
    }

    let mut plan = CityStreamingPlan {
        cell_count: decisions.len(),
        max_loaded_bytes: request.max_loaded_bytes,
        ..CityStreamingPlan::default()
    };
    for decision in &decisions {
        match decision.state {
            CityCellStreamingState::HeroLoaded => {
                plan.loaded_cell_count += 1;
                plan.hero_loaded_count += 1;
            }
            CityCellStreamingState::RenderHighDetail => {
                plan.loaded_cell_count += 1;
                plan.render_high_detail_count += 1;
            }
            CityCellStreamingState::GameplayLoaded => {
                plan.loaded_cell_count += 1;
                plan.gameplay_loaded_count += 1;
            }
            CityCellStreamingState::SummaryLoaded => plan.summary_loaded_count += 1,
            CityCellStreamingState::Unloaded => plan.unloaded_count += 1,
        }
        plan.requested_asset_count = plan
            .requested_asset_count
            .saturating_add(decision.requested_assets.len());
        if city_streaming_state_requests_assets(decision.state) {
            plan.requested_streaming_bytes = plan
                .requested_streaming_bytes
                .saturating_add(decision.estimated_streaming_bytes);
        }
    }
    if template.chunks.is_empty() {
        plan.issues.push(validation_issue(
            ValidationSeverity::Error,
            "missing_streaming_cells",
            "city streaming planner requires at least one generated world chunk",
        ));
    }
    if !decisions.is_empty() && plan.loaded_cell_count == 0 {
        plan.issues.push(validation_issue(
            ValidationSeverity::Warning,
            "no_loaded_streaming_cells",
            "city streaming planner did not promote any cells to gameplay or render detail",
        ));
    }
    plan.cells = decisions;
    plan
}

fn city_cell_streaming_decision(
    chunk: &WorldChunk,
    package: Option<&CityCellPackage>,
    request: &CityStreamingRequest,
) -> CityCellStreamingDecision {
    let center = chunk_center(chunk);
    let distance_meters = vec3_distance_squared(center, request.player_position).sqrt();
    let to_cell = vec3_normalize(Vec3::new(
        center.x - request.player_position.x,
        center.y - request.player_position.y,
        center.z - request.player_position.z,
    ));
    let camera_forward = vec3_normalize(request.camera_forward);
    let camera_alignment = vec3_dot(camera_forward, to_cell).clamp(-1.0, 1.0);
    let velocity_alignment = vec3_dot(vec3_normalize(request.player_velocity), to_cell).max(0.0);
    let renderer_feedback = renderer_feedback_for_chunk(request, chunk.chunk_id);
    let renderer_feedback_score = renderer_feedback
        .map(|feedback| feedback.importance_score())
        .unwrap_or_default();
    let renderer_visible = request.renderer_visible_chunks.contains(&chunk.chunk_id)
        || renderer_feedback_score > 0.02
        || renderer_feedback.is_some_and(|feedback| feedback.visible_cluster_count > 0);
    let renderer_visible_cluster_count = renderer_feedback
        .map(|feedback| feedback.visible_cluster_count)
        .unwrap_or_default();
    let story_focus = chunk_has_any_location(chunk, &request.story_focus_locations);
    let predicted_route_match = chunk_has_any_location(chunk, &request.predicted_route);
    let active_event = request.active_event_locations.iter().any(|location| {
        chunk_contains_point(chunk, *location)
            || vec3_distance_squared(center, *location) <= 64.0 * 64.0
    });
    let unique_dependencies = package
        .map(city_cell_package_requested_assets)
        .unwrap_or_else(|| chunk.streaming_dependencies.clone())
        .into_iter()
        .collect::<BTreeSet<_>>();
    let estimated_streaming_bytes = unique_dependencies
        .iter()
        .map(|asset| estimated_streaming_asset_bytes(*asset))
        .sum();

    let distance_score = (1.0 / (1.0 + distance_meters / 48.0)).clamp(0.0, 1.0);
    let mut priority_score = distance_score;
    let mut reasons = vec![format!("distance {:.1}m", distance_meters)];
    if camera_alignment > 0.0 {
        priority_score += camera_alignment * 0.25;
        reasons.push("inside camera-forward streaming cone".to_string());
    }
    if velocity_alignment > 0.0 {
        priority_score += velocity_alignment * 0.2;
        reasons.push("predicted by player velocity".to_string());
    }
    if renderer_visible {
        priority_score += 0.5;
        reasons.push("renderer visibility feedback requested detail".to_string());
    }
    if renderer_feedback_score > 0.0 {
        priority_score += renderer_feedback_score * 0.55;
        reasons.push(format!(
            "renderer streaming feedback score {:.2} from {} visible cluster(s)",
            renderer_feedback_score, renderer_visible_cluster_count
        ));
    }
    if story_focus {
        priority_score += 0.7;
        reasons.push("contains active story focus".to_string());
    }
    if active_event {
        priority_score += 0.8;
        reasons.push("contains recent active world event".to_string());
    }
    if predicted_route_match {
        priority_score += 0.35;
        reasons.push("matches predicted route".to_string());
    }

    CityCellStreamingDecision {
        cell_id: chunk.chunk_id,
        district_id: chunk.district_id,
        package_asset: package
            .map(|package| package.package_asset)
            .unwrap_or_default(),
        package_requested: false,
        state: CityCellStreamingState::Unloaded,
        priority_score,
        distance_meters,
        camera_alignment,
        predicted_route_match,
        story_focus,
        active_event,
        renderer_visible,
        renderer_feedback_score,
        renderer_visible_cluster_count,
        dependency_count: unique_dependencies.len(),
        estimated_streaming_bytes,
        requested_assets: Vec::new(),
        reasons,
    }
}

fn city_cell_package_requested_assets(package: &CityCellPackage) -> Vec<AssetId> {
    let mut assets =
        Vec::with_capacity(1 + package.geometry_chunks.len() + package.material_packages.len());
    assets.push(package.package_asset);
    assets.extend(package.geometry_chunks.iter().copied());
    assets.extend(package.material_packages.iter().copied());
    assets.sort_unstable();
    assets.dedup();
    assets
}

fn renderer_feedback_for_chunk(
    request: &CityStreamingRequest,
    chunk_id: WorldChunkId,
) -> Option<CityRendererStreamingFeedback> {
    request
        .renderer_feedback
        .iter()
        .copied()
        .find(|feedback| feedback.chunk_id == chunk_id)
}

fn desired_city_cell_streaming_state(
    decision: &CityCellStreamingDecision,
    request: &CityStreamingRequest,
    hero_loaded: usize,
    render_loaded: usize,
    gameplay_loaded: usize,
) -> CityCellStreamingState {
    if hero_loaded < request.max_hero_cells && (decision.active_event || decision.story_focus) {
        CityCellStreamingState::HeroLoaded
    } else if render_loaded < request.max_render_high_detail_cells
        && (decision.renderer_visible || decision.priority_score >= 1.25)
    {
        CityCellStreamingState::RenderHighDetail
    } else if gameplay_loaded < request.max_gameplay_cells
        && (decision.predicted_route_match || decision.priority_score >= 0.55)
    {
        CityCellStreamingState::GameplayLoaded
    } else if decision.priority_score >= 0.18 {
        CityCellStreamingState::SummaryLoaded
    } else {
        CityCellStreamingState::Unloaded
    }
}

fn city_streaming_state_requests_assets(state: CityCellStreamingState) -> bool {
    matches!(
        state,
        CityCellStreamingState::GameplayLoaded
            | CityCellStreamingState::RenderHighDetail
            | CityCellStreamingState::HeroLoaded
    )
}

fn chunk_has_any_location(chunk: &WorldChunk, locations: &[LocationId]) -> bool {
    if locations.is_empty() {
        return false;
    }
    let locations = locations.iter().copied().collect::<BTreeSet<_>>();
    chunk
        .local_navigation
        .nodes
        .iter()
        .any(|node| locations.contains(&node.location))
        || chunk
            .local_navigation
            .important_locations
            .iter()
            .any(|location| locations.contains(location))
}

pub fn validate_world_template(
    template: &WorldTemplate,
    target_platform_budget: PlatformBudget,
) -> ValidationReport {
    let mut issues = Vec::new();
    let budget = calculate_budget_usage(template, target_platform_budget);

    if template.districts.is_empty() {
        issues.push(validation_issue(
            ValidationSeverity::Error,
            "missing_district",
            "world template must contain at least one district",
        ));
    }
    if template.factions.len() < 3 {
        issues.push(validation_issue(
            ValidationSeverity::Error,
            "missing_factions",
            "world template must seed at least three factions",
        ));
    }
    if template.materials.is_empty() {
        issues.push(validation_issue(
            ValidationSeverity::Error,
            "missing_material_variants",
            "world template must contain generated district material variants",
        ));
    }
    let material_ids = template
        .materials
        .iter()
        .map(|material| material.id)
        .collect::<BTreeSet<_>>();
    if material_ids.len() != template.materials.len() {
        issues.push(validation_issue(
            ValidationSeverity::Error,
            "duplicate_material_variant",
            "district material variants must have unique material ids",
        ));
    }
    for (_, material) in &template.material_assignments {
        if !material_ids.contains(material) {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "missing_material_variant_descriptor",
                "material assignments must reference generated district material variants",
            ));
            break;
        }
    }
    if !template.navigation_data.all_important_locations_reachable() {
        issues.push(validation_issue(
            ValidationSeverity::Error,
            "navigation_unreachable",
            "important locations must be connected through navigation edges",
        ));
    }

    let infrastructure_ids = all_infrastructure_ids(&template.infrastructure);
    let chunks_by_id = template
        .chunks
        .iter()
        .map(|chunk| (chunk.chunk_id, chunk))
        .collect::<BTreeMap<_, _>>();
    let packages_by_cell = template
        .city_cell_packages
        .iter()
        .map(|package| (package.cell_id, package))
        .collect::<BTreeMap<_, _>>();
    if template.city_cell_packages.len() != template.chunks.len()
        || packages_by_cell.len() != template.chunks.len()
    {
        issues.push(validation_issue(
            ValidationSeverity::Error,
            "city_cell_package_mismatch",
            "world template must contain exactly one versioned CityCellPackage per chunk",
        ));
    }
    for chunk in &template.chunks {
        if chunk.streaming_dependencies.is_empty() {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "missing_streaming_dependencies",
                "streamed chunks must list required asset ids",
            ));
        }
        if !chunk.population.validation_report.passed {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "invalid_chunk_population_profile",
                "streamed chunks must include valid crowd and traffic population rules",
            ));
        }
        if !validate_population_summary(&chunk.population_summary).passed {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "invalid_chunk_population_summary",
                "streamed chunks must include a valid shared PopulationSummary",
            ));
        }
        for infrastructure_id in &chunk.local_infrastructure {
            if !infrastructure_ids.contains(infrastructure_id) {
                issues.push(validation_issue(
                    ValidationSeverity::Error,
                    "chunk_unknown_infrastructure",
                    "chunk references an infrastructure node that is not present",
                ));
            }
        }
        if !packages_by_cell.contains_key(&chunk.chunk_id) {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "missing_city_cell_package",
                "each streamed chunk must have a CityCellPackage",
            ));
        }
        for neighbor in &chunk.neighbor_links {
            if !template
                .chunks
                .iter()
                .any(|candidate| candidate.chunk_id == *neighbor)
            {
                issues.push(validation_issue(
                    ValidationSeverity::Error,
                    "broken_chunk_neighbor",
                    "chunk references a neighbor that is not present in the template",
                ));
            }
        }
    }

    for package in &template.city_cell_packages {
        let Some(chunk) = chunks_by_id.get(&package.cell_id) else {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "orphan_city_cell_package",
                "CityCellPackage references a chunk that is not present",
            ));
            continue;
        };
        if package.schema_version != CITY_CELL_PACKAGE_SCHEMA_VERSION {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "city_cell_package_schema_mismatch",
                "CityCellPackage schema version must match the worldgen v4 contract",
            ));
        }
        if package.package_asset != worldgen_city_cell_asset_id(package.cell_id) {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "city_cell_package_asset_mismatch",
                "CityCellPackage asset id must be deterministic from the cell id",
            ));
        }
        if package.bounds != chunk.bounds || package.district_id != chunk.district_id {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "city_cell_package_identity_mismatch",
                "CityCellPackage bounds and district must mirror the generated chunk",
            ));
        }
        if package.population != chunk.population {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "city_cell_package_population_mismatch",
                "CityCellPackage population profile must mirror the generated chunk",
            ));
        }
        if package.population_summary != chunk.population_summary {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "city_cell_package_population_summary_mismatch",
                "CityCellPackage population summary must mirror the generated chunk",
            ));
        }
        if !validate_population_summary(&package.population_summary).passed {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "city_cell_package_invalid_population_summary",
                "CityCellPackage population summary must satisfy the shared v5 schema contract",
            ));
        }
        if package.geometry_chunks.is_empty() {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "city_cell_package_missing_geometry",
                "CityCellPackage must declare geometry chunks",
            ));
        }
        if package.material_packages.is_empty() {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "city_cell_package_missing_materials",
                "CityCellPackage must declare generated material packages",
            ));
        }
        if package.material_package_descriptors.len() != package.material_packages.len() {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "city_cell_package_material_descriptor_mismatch",
                "CityCellPackage must describe every generated material package",
            ));
        }
        let material_package_assets = package
            .material_packages
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for material_descriptor in &package.material_package_descriptors {
            if material_descriptor.schema_version != CITY_MATERIAL_PACKAGE_SCHEMA_VERSION {
                issues.push(validation_issue(
                    ValidationSeverity::Error,
                    "city_material_package_schema_mismatch",
                    "generated material package descriptors must use the current schema",
                ));
            }
            if !material_package_assets.contains(&material_descriptor.package_asset) {
                issues.push(validation_issue(
                    ValidationSeverity::Error,
                    "city_material_package_unknown_asset",
                    "material package descriptors must reference declared material package assets",
                ));
            }
            if !material_descriptor.validation_passed {
                issues.push(validation_issue(
                    ValidationSeverity::Error,
                    "city_material_package_validation_failed",
                    "generated material package validation must pass",
                ));
            }
            if material_descriptor.capture_confidence < 0.65 {
                issues.push(validation_issue(
                    ValidationSeverity::Warning,
                    "city_material_package_low_capture_confidence",
                    "generated material packages should retain calibrated capture or scan evidence",
                ));
            }
            if material_descriptor.state_response_channels.is_empty() {
                issues.push(validation_issue(
                    ValidationSeverity::Warning,
                    "city_material_package_missing_state_response",
                    "generated material packages should expose live material state responses",
                ));
            }
        }
        let chunk_dependencies = chunk
            .streaming_dependencies
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let package_dependencies = package
            .geometry_chunks
            .iter()
            .chain(package.material_packages.iter())
            .copied()
            .collect::<BTreeSet<_>>();
        if chunk_dependencies != package_dependencies {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "city_cell_package_dependency_mismatch",
                "CityCellPackage dependencies must match chunk streaming dependencies",
            ));
        }
        if package
            .geometry_chunks
            .iter()
            .any(|asset| is_worldgen_material_cache_asset(*asset))
            || package
                .material_packages
                .iter()
                .any(|asset| !is_worldgen_material_cache_asset(*asset))
        {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "city_cell_package_dependency_class_mismatch",
                "CityCellPackage must split geometry chunks from material packages",
            ));
        }
    }

    for story_seed in &template.story_seeds {
        if !template.navigation_data.has_location(story_seed.location) {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "story_seed_unreachable",
                "story seeds must be grounded in generated navigation locations",
            ));
        }
        if story_seed.physical_hooks.is_empty() {
            issues.push(validation_issue(
                ValidationSeverity::Warning,
                "story_seed_no_physical_hook",
                "story seeds are stronger when they reference physical hooks",
            ));
        }
        for condition in &story_seed.trigger_conditions {
            let TriggerCondition::InfrastructureDamaged(infrastructure_id) = condition else {
                continue;
            };
            if !infrastructure_ids.contains(infrastructure_id) {
                issues.push(validation_issue(
                    ValidationSeverity::Error,
                    "story_seed_unknown_infrastructure",
                    "story seed trigger references an infrastructure node that is not present",
                ));
            }
        }
    }

    if target_platform_budget > 0 && budget.entity_count > target_platform_budget as usize {
        issues.push(validation_issue(
            ValidationSeverity::Error,
            "entity_budget_exceeded",
            "generated entity count exceeds the requested platform budget",
        ));
    }
    if let Some(limits) = world_budget_limits(target_platform_budget) {
        if budget.unique_material_count > limits.max_unique_materials {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "unique_material_budget_exceeded",
                "unique material count exceeds the requested platform budget",
            ));
        }
        if budget.dynamic_light_count > limits.max_dynamic_lights {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "dynamic_light_budget_exceeded",
                "dynamic light count exceeds the requested platform budget",
            ));
        }
        if budget.physics_active_object_count > limits.max_physics_active_objects {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "physics_active_budget_exceeded",
                "physics-active object count exceeds the requested platform budget",
            ));
        }
        if budget.destructible_count > limits.max_destructibles {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "destructible_budget_exceeded",
                "destructible object count exceeds the requested platform budget",
            ));
        }
        if budget.liquid_zones > limits.max_liquid_zones {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "liquid_zone_budget_exceeded",
                "liquid and flood-prone zone count exceeds the requested platform budget",
            ));
        }
        if budget.npc_count > limits.max_npcs {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "npc_budget_exceeded",
                "active NPC seed count exceeds the requested platform budget",
            ));
        }
        if budget.expected_background_crowd_count > limits.max_background_crowd {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "background_crowd_budget_exceeded",
                "expected background crowd count exceeds the requested platform budget",
            ));
        }
        if budget.expected_vehicle_or_transit_count > limits.max_vehicle_or_transit {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "traffic_budget_exceeded",
                "expected vehicle or transit count exceeds the requested platform budget",
            ));
        }
        if budget.streaming_dependency_count > limits.max_streaming_dependencies {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "streaming_dependency_budget_exceeded",
                "streaming dependency count exceeds the requested platform budget",
            ));
        }
        if budget.streaming_memory_bytes > limits.max_streaming_memory_bytes {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "streaming_memory_budget_exceeded",
                "estimated streaming memory exceeds the requested platform budget",
            ));
        }
    }

    ValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error),
        issues,
        budget,
    }
}

pub fn evaluate_infrastructure_damage(
    template: &WorldTemplate,
    request: &InfrastructureDamageRequest,
) -> InfrastructureDamageReport {
    let severity = request.severity.clamp(0.0, 1.0);
    let Some((system, graph, node)) =
        find_infrastructure_node(&template.infrastructure, request.damaged_node)
    else {
        return InfrastructureDamageReport {
            passed: false,
            damaged_node: None,
            system: None,
            severity,
            affected_nodes: Vec::new(),
            affected_chunks: Vec::new(),
            affected_districts: Vec::new(),
            story_seeds: Vec::new(),
            faction_pressure: Vec::new(),
            expected_events: Vec::new(),
            consequence_tags: Vec::new(),
            issues: vec![validation_issue(
                ValidationSeverity::Error,
                "unknown_infrastructure_node",
                "damage request references an infrastructure node that is not present",
            )],
        };
    };

    let affected_nodes = collect_affected_infrastructure_nodes(graph, node.id, severity);
    let affected_chunks = affected_chunks_for_nodes(template, &affected_nodes);
    let affected_districts = affected_districts_for_chunks(template, &affected_chunks);
    let story_seeds = story_seeds_for_infrastructure_damage(template, &affected_nodes);
    let faction_pressure =
        faction_pressure_for_infrastructure_damage(template, system, &affected_districts, severity);
    let expected_events = expected_events_for_infrastructure_damage(system, node, severity);
    let consequence_tags = consequence_tags_for_infrastructure_damage(system, severity);

    let mut issues = Vec::new();
    if affected_chunks.is_empty() {
        issues.push(validation_issue(
            ValidationSeverity::Warning,
            "damage_without_chunk",
            "infrastructure damage did not map to a streamed world chunk",
        ));
    }
    if story_seeds.is_empty() {
        issues.push(validation_issue(
            ValidationSeverity::Warning,
            "damage_without_story_seed",
            "infrastructure damage did not activate a generated story seed",
        ));
    }

    InfrastructureDamageReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error),
        damaged_node: Some(node.clone()),
        system: Some(system),
        severity,
        affected_nodes,
        affected_chunks,
        affected_districts,
        story_seeds,
        faction_pressure,
        expected_events,
        consequence_tags,
        issues,
    }
}

pub fn infrastructure_consequence_map_for_damage(
    template: &WorldTemplate,
    request: &InfrastructureDamageRequest,
) -> InfrastructureConsequenceMap {
    let report = evaluate_infrastructure_damage(template, request);
    let affected_chunks = report
        .affected_chunks
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut cell_impacts = template
        .chunks
        .iter()
        .filter(|chunk| affected_chunks.contains(&chunk.chunk_id))
        .map(|chunk| infrastructure_cell_impact_for_damage(template, chunk, &report))
        .collect::<Vec<_>>();
    cell_impacts.sort_by_key(|cell| cell.chunk_id);

    let unique_streaming_dependencies = template
        .chunks
        .iter()
        .filter(|chunk| affected_chunks.contains(&chunk.chunk_id))
        .flat_map(|chunk| chunk.streaming_dependencies.iter().copied())
        .collect::<BTreeSet<_>>();
    let mut consequence_tags = report.consequence_tags.clone();
    for cell in &cell_impacts {
        for tag in &cell.tags {
            push_unique_tag(&mut consequence_tags, tag);
        }
    }

    let mut issues = report.issues.clone();
    if report.passed && report.affected_nodes.is_empty() {
        issues.push(validation_issue(
            ValidationSeverity::Warning,
            "consequence_without_nodes",
            "infrastructure consequence map has no affected infrastructure nodes",
        ));
    }
    if report.passed && cell_impacts.is_empty() {
        issues.push(validation_issue(
            ValidationSeverity::Warning,
            "consequence_without_cells",
            "infrastructure consequence map has no affected streamed city cells",
        ));
    }

    InfrastructureConsequenceMap {
        city_id: template.city_id,
        seed: template.seed,
        passed: report.passed,
        damaged_node: report.damaged_node.clone(),
        system: report.system,
        severity: report.severity,
        affected_nodes: report.affected_nodes.clone(),
        affected_chunks: report.affected_chunks.clone(),
        affected_districts: report.affected_districts.clone(),
        story_seeds: report.story_seeds.clone(),
        faction_pressure: report.faction_pressure.clone(),
        expected_events: report.expected_events.clone(),
        consequence_tags,
        affected_entity_count: cell_impacts.iter().map(|cell| cell.entity_count).sum(),
        affected_renderable_count: cell_impacts.iter().map(|cell| cell.renderable_count).sum(),
        affected_light_count: cell_impacts.iter().map(|cell| cell.light_count).sum(),
        affected_door_count: cell_impacts.iter().map(|cell| cell.door_count).sum(),
        affected_camera_count: cell_impacts.iter().map(|cell| cell.camera_count).sum(),
        affected_npc_count: cell_impacts.iter().map(|cell| cell.npc_count).sum(),
        affected_audio_zone_count: cell_impacts.iter().map(|cell| cell.audio_zone_count).sum(),
        affected_water_or_drainage_count: cell_impacts
            .iter()
            .map(|cell| cell.water_or_drainage_count)
            .sum(),
        affected_navigation_location_count: cell_impacts
            .iter()
            .map(|cell| cell.navigation_location_count)
            .sum(),
        affected_streaming_dependency_count: unique_streaming_dependencies.len(),
        expected_event_count: report.expected_events.len(),
        story_seed_count: report.story_seeds.len(),
        faction_pressure_count: report.faction_pressure.len(),
        cell_impacts,
        issues,
    }
}

fn infrastructure_cell_impact_for_damage(
    template: &WorldTemplate,
    chunk: &WorldChunk,
    report: &InfrastructureDamageReport,
) -> InfrastructureCellImpact {
    let affected_nodes = report
        .affected_nodes
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let system = report.system;
    let affected_node_count = chunk
        .local_infrastructure
        .iter()
        .filter(|node| affected_nodes.contains(*node))
        .count();
    let renderable_count = chunk
        .entities
        .iter()
        .filter(|entity| entity.renderable.is_some())
        .count();
    let light_count = if matches!(system, Some(InfrastructureSystem::Power)) {
        chunk
            .entities
            .iter()
            .filter(|entity| has_tag(&entity.tags, "light") || has_tag(&entity.tags, "neon"))
            .count()
    } else {
        0
    };
    let door_count = if matches!(
        system,
        Some(
            InfrastructureSystem::Power
                | InfrastructureSystem::Data
                | InfrastructureSystem::Transit
        )
    ) {
        chunk
            .entities
            .iter()
            .filter(|entity| {
                has_tag(&entity.tags, "door")
                    || has_tag(&entity.tags, "security_door")
                    || has_tag(&entity.tags, "elevator")
            })
            .count()
    } else {
        0
    };
    let camera_count = infrastructure_camera_impact_count(template, chunk, report);
    let npc_count = chunk
        .entities
        .iter()
        .filter(|entity| {
            entity.agent.is_some() || entity.human.is_some() || has_tag(&entity.tags, "npc")
        })
        .count();
    let audio_zone_count = chunk
        .entities
        .iter()
        .filter(|entity| tag_prefix_exists(&entity.tags, "audio_zone:"))
        .count();
    let water_or_drainage_count =
        infrastructure_water_or_drainage_impact_count(template, chunk, report);
    let navigation_location_count = chunk.local_navigation.important_locations.len();
    let streaming_dependency_count = chunk
        .streaming_dependencies
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .len();
    let story_hook_count =
        infrastructure_story_hook_count(&chunk.local_story_hooks, &affected_nodes);
    let entity_count = chunk
        .entities
        .iter()
        .filter(|entity| infrastructure_entity_impacted_by_system(entity, system))
        .count();
    let mut tags = vec![
        "infrastructure_consequence".to_string(),
        format!("cell:{}", chunk.chunk_id),
    ];
    if let Some(system) = system {
        tags.push(format!("system:{}", infrastructure_system_label(system)));
    }
    if affected_node_count > 0 {
        tags.push("affected_node".to_string());
    }
    if light_count > 0 {
        tags.push("lighting".to_string());
    }
    if door_count > 0 {
        tags.push("door_control".to_string());
    }
    if camera_count > 0 {
        tags.push("camera_feed".to_string());
    }
    if npc_count > 0 {
        tags.push("npc_reaction".to_string());
    }
    if audio_zone_count > 0 {
        tags.push("audio_loop".to_string());
    }
    if water_or_drainage_count > 0 {
        tags.push("water_or_drainage".to_string());
    }
    if navigation_location_count > 0 {
        tags.push("route_impact".to_string());
    }
    if story_hook_count > 0 {
        tags.push("story_opportunity".to_string());
    }

    InfrastructureCellImpact {
        chunk_id: chunk.chunk_id,
        district_id: chunk.district_id,
        affected_node_count,
        entity_count,
        renderable_count,
        light_count,
        door_count,
        camera_count,
        npc_count,
        audio_zone_count,
        water_or_drainage_count,
        navigation_location_count,
        streaming_dependency_count,
        story_hook_count,
        tags,
    }
}

fn infrastructure_camera_impact_count(
    template: &WorldTemplate,
    chunk: &WorldChunk,
    report: &InfrastructureDamageReport,
) -> usize {
    let affected_nodes = report
        .affected_nodes
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let system_impacts_camera_feed = matches!(
        report.system,
        Some(
            InfrastructureSystem::Power
                | InfrastructureSystem::Data
                | InfrastructureSystem::Surveillance
        )
    );
    chunk
        .local_infrastructure
        .iter()
        .filter(|node_id| {
            find_infrastructure_node(&template.infrastructure, **node_id).is_some_and(
                |(node_system, _, node)| {
                    node.kind == InfrastructureNodeKind::SurveillanceCamera
                        && (affected_nodes.contains(*node_id)
                            || (system_impacts_camera_feed
                                && node_system == InfrastructureSystem::Surveillance))
                },
            )
        })
        .count()
}

fn infrastructure_water_or_drainage_impact_count(
    template: &WorldTemplate,
    chunk: &WorldChunk,
    report: &InfrastructureDamageReport,
) -> usize {
    let affected_nodes = report
        .affected_nodes
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let system_impacts_water = matches!(
        report.system,
        Some(InfrastructureSystem::Water | InfrastructureSystem::Drainage)
    );
    let node_count = chunk
        .local_infrastructure
        .iter()
        .filter(|node_id| {
            find_infrastructure_node(&template.infrastructure, **node_id).is_some_and(
                |(node_system, _, _)| {
                    matches!(
                        node_system,
                        InfrastructureSystem::Water | InfrastructureSystem::Drainage
                    ) && (affected_nodes.contains(*node_id) || system_impacts_water)
                },
            )
        })
        .count();
    let entity_count = if system_impacts_water {
        chunk
            .entities
            .iter()
            .filter(|entity| {
                has_tag(&entity.tags, "water_leak")
                    || has_tag(&entity.tags, "hazard")
                    || has_tag(&entity.tags, "wettable")
            })
            .count()
    } else {
        0
    };
    node_count + entity_count
}

fn infrastructure_story_hook_count(
    hooks: &[StorySeed],
    affected_nodes: &BTreeSet<InfrastructureLinkId>,
) -> usize {
    hooks
        .iter()
        .filter(|hook| {
            hook.trigger_conditions.iter().any(|condition| {
                matches!(
                    condition,
                    TriggerCondition::InfrastructureDamaged(node)
                        if affected_nodes.contains(node)
                )
            })
        })
        .count()
}

fn infrastructure_entity_impacted_by_system(
    entity: &EntityTemplate,
    system: Option<InfrastructureSystem>,
) -> bool {
    match system {
        Some(InfrastructureSystem::Power) => {
            entity.renderable.is_some()
                || entity.agent.is_some()
                || has_tag(&entity.tags, "light")
                || has_tag(&entity.tags, "neon")
                || tag_prefix_exists(&entity.tags, "audio_zone:")
        }
        Some(InfrastructureSystem::Water | InfrastructureSystem::Drainage) => {
            entity.agent.is_some()
                || has_tag(&entity.tags, "water_leak")
                || has_tag(&entity.tags, "hazard")
                || has_tag(&entity.tags, "wettable")
        }
        Some(InfrastructureSystem::Data | InfrastructureSystem::Surveillance) => {
            entity.agent.is_some()
                || has_tag(&entity.tags, "npc")
                || tag_prefix_exists(&entity.tags, "audio_zone:")
        }
        Some(InfrastructureSystem::Transit) => {
            entity.agent.is_some()
                || has_tag(&entity.tags, "npc")
                || has_tag(&entity.tags, "ground")
        }
        None => false,
    }
}

fn tag_prefix_exists(tags: &[String], prefix: &str) -> bool {
    tags.iter().any(|tag| tag.starts_with(prefix))
}

fn push_unique_tag(tags: &mut TagSet, tag: &str) {
    if !tags.iter().any(|existing| existing == tag) {
        tags.push(tag.to_string());
    }
}

pub fn persistent_city_state_from_events(
    template: &WorldTemplate,
    events: &[WorldEvent],
) -> CityPersistenceState {
    let mut state = CityPersistenceState {
        city_id: template.city_id,
        seed: template.seed,
        event_count: events.len(),
        ..CityPersistenceState::default()
    };
    let mut cells = BTreeMap::<WorldChunkId, PersistentCellState>::new();

    for event in events {
        if !event_creates_persistent_city_state(&event.kind) {
            continue;
        }
        let Some(chunk) = persistent_chunk_for_event(template, event) else {
            state.issues.push(validation_issue(
                ValidationSeverity::Error,
                "persistent_event_without_cell",
                "persistent world event could not be mapped to a streamable city cell",
            ));
            continue;
        };

        state.persistent_event_count += 1;
        let material = event_primary_entity(&event.kind)
            .and_then(|entity| material_for_entity(template, entity));
        let cell = cells
            .entry(chunk.chunk_id)
            .or_insert_with(|| PersistentCellState::new(chunk.chunk_id, chunk.district_id));
        push_unique_copy(&mut cell.event_history, event.event_id);
        apply_persistent_event_to_cell(template, chunk, event, material, cell);
    }

    let mut cell_states = cells.into_values().collect::<Vec<_>>();
    for cell in &mut cell_states {
        cell.event_history.sort_unstable();
        cell.material_state_overrides
            .sort_by_key(|override_state| (override_state.entity, override_state.source_event));
        cell.damaged_entities.sort_unstable();
        cell.destroyed_entities.sort_unstable();
        cell.spawned_entities.sort_unstable();
        cell.mesh_replacements.sort_unstable();
        cell.navigation_blockers.sort_unstable();
        cell.infrastructure_delta.affected_nodes.sort_unstable();
        cell.infrastructure_delta.systems.sort_unstable();
        cell.infrastructure_delta.source_events.sort_unstable();
        cell.faction_state_delta.sort_by(|left, right| {
            left.district
                .cmp(&right.district)
                .then(left.faction_id.cmp(&right.faction_id))
        });
        cell.story_thread_refs.sort_unstable();
        cell.ai_memory_refs.sort_unstable();
        cell.resident_assets.sort_unstable();
        cell.consequence_tags.sort();
    }
    cell_states.sort_by_key(|cell| cell.cell_id);

    state.material_override_count = cell_states
        .iter()
        .map(|cell| cell.material_state_overrides.len())
        .sum();
    state.damaged_entity_count = cell_states
        .iter()
        .map(|cell| cell.damaged_entities.len())
        .sum();
    state.destroyed_entity_count = cell_states
        .iter()
        .map(|cell| cell.destroyed_entities.len())
        .sum();
    state.spawned_entity_count = cell_states
        .iter()
        .map(|cell| cell.spawned_entities.len())
        .sum();
    state.mesh_replacement_count = cell_states
        .iter()
        .map(|cell| cell.mesh_replacements.len())
        .sum();
    state.navigation_blocker_count = cell_states
        .iter()
        .map(|cell| cell.navigation_blockers.len())
        .sum();
    state.infrastructure_delta_count = cell_states
        .iter()
        .filter(|cell| !cell.infrastructure_delta.affected_nodes.is_empty())
        .count();
    state.faction_delta_count = cell_states
        .iter()
        .map(|cell| cell.faction_state_delta.len())
        .sum();
    state.story_thread_ref_count = cell_states
        .iter()
        .map(|cell| cell.story_thread_refs.len())
        .sum();
    state.ai_memory_ref_count = cell_states
        .iter()
        .map(|cell| cell.ai_memory_refs.len())
        .sum();
    state.resident_asset_count = cell_states
        .iter()
        .map(|cell| cell.resident_assets.len())
        .sum();
    state.cell_states = cell_states;

    if state.persistent_event_count > 0 && state.cell_states.is_empty() {
        state.issues.push(validation_issue(
            ValidationSeverity::Error,
            "persistent_events_without_cells",
            "persistent events were found, but no city cell state was produced",
        ));
    }

    state
}

pub fn navigation_cell_states_from_persistence(
    template: &WorldTemplate,
    persistence: &CityPersistenceState,
) -> Vec<NavigationCellState> {
    let mut states = template
        .chunks
        .iter()
        .map(|chunk| {
            let persisted = persistence
                .cell_states
                .iter()
                .find(|cell| cell.cell_id == chunk.chunk_id);
            navigation_cell_state_from_persistent_cell(chunk, persisted)
        })
        .collect::<Vec<_>>();
    states.sort_by_key(|state| state.cell_id);
    states
}

fn navigation_cell_state_from_persistent_cell(
    chunk: &WorldChunk,
    persisted: Option<&PersistentCellState>,
) -> NavigationCellState {
    let mut state = NavigationCellState {
        cell_id: chunk.chunk_id,
        district_id: chunk.district_id,
        node_count: chunk.local_navigation.nodes.len(),
        edge_count: chunk.local_navigation.edges.len(),
        important_location_count: chunk.local_navigation.important_locations.len(),
        reachable_important_location_count: chunk.local_navigation.important_locations.len(),
        ..NavigationCellState::default()
    };

    if let Some(persisted) = persisted {
        state.dynamic_blockers = navigation_dynamic_blockers_for_cell(chunk, persisted);
        state.danger_fields = navigation_danger_fields_for_cell(chunk, persisted);
        state.restricted_zones = navigation_restricted_zones_for_cell(chunk, persisted);
        state.route_updates = navigation_route_updates_for_cell(chunk, &state);
        state.reachable_important_location_count =
            reachable_important_locations_after_navigation_updates(
                &chunk.local_navigation,
                &state.route_updates,
            );

        if state.reachable_important_location_count < state.important_location_count {
            state.issues.push(validation_issue(
                ValidationSeverity::Warning,
                "important_navigation_partially_blocked",
                "one or more important locations are no longer reachable without rerouting",
            ));
        }
        if !state.dynamic_blockers.is_empty()
            && !state
                .route_updates
                .iter()
                .any(|route| route.status == NavigationRouteStatus::Blocked)
        {
            state.issues.push(validation_issue(
                ValidationSeverity::Warning,
                "dynamic_blocker_without_route_update",
                "dynamic navigation blocker did not touch a generated navigation edge",
            ));
        }
        if !state.danger_fields.is_empty()
            && !state
                .route_updates
                .iter()
                .any(|route| route.status == NavigationRouteStatus::Dangerous)
        {
            state.issues.push(validation_issue(
                ValidationSeverity::Warning,
                "danger_field_without_route_update",
                "danger field did not touch a generated navigation edge",
            ));
        }
    }

    state
}

fn navigation_dynamic_blockers_for_cell(
    chunk: &WorldChunk,
    persisted: &PersistentCellState,
) -> Vec<NavigationDynamicBlocker> {
    let mut blockers = persisted
        .navigation_blockers
        .iter()
        .copied()
        .map(|entity| NavigationDynamicBlocker {
            entity,
            source_event: persisted.event_history.first().copied(),
            blocked_location: nearest_navigation_location_for_entity(chunk, entity),
            reason: "runtime movement was blocked and should force rerouting".to_string(),
        })
        .collect::<Vec<_>>();
    blockers.sort_by_key(|blocker| (blocker.blocked_location, blocker.entity));
    blockers
}

fn navigation_danger_fields_for_cell(
    chunk: &WorldChunk,
    persisted: &PersistentCellState,
) -> Vec<NavigationDangerField> {
    let mut fields = Vec::new();
    let all_locations = chunk
        .local_navigation
        .nodes
        .iter()
        .map(|node| node.location)
        .collect::<Vec<_>>();

    if persisted
        .consequence_tags
        .iter()
        .any(|tag| tag == "flooding")
    {
        fields.push(NavigationDangerField {
            field_id: stable_u64(chunk.chunk_id, 0xF100),
            source_event: persisted
                .infrastructure_delta
                .source_events
                .first()
                .copied(),
            kind: NavigationDangerKind::Flooding,
            severity: persisted.infrastructure_delta.severity.max(0.55),
            affected_locations: all_locations.clone(),
            reason: "flooding makes low routes slower and riskier".to_string(),
        });
    }

    if persisted
        .consequence_tags
        .iter()
        .any(|tag| tag == "oil_slick")
    {
        fields.push(NavigationDangerField {
            field_id: stable_u64(chunk.chunk_id, 0x0110),
            source_event: persisted
                .infrastructure_delta
                .source_events
                .first()
                .copied(),
            kind: NavigationDangerKind::SlipperyContamination,
            severity: persisted.infrastructure_delta.severity.max(0.58),
            affected_locations: all_locations.clone(),
            reason: "oil contamination makes pedestrian and vehicle routes slippery".to_string(),
        });
    }

    if persisted
        .consequence_tags
        .iter()
        .any(|tag| tag == "biohazard")
    {
        fields.push(NavigationDangerField {
            field_id: stable_u64(chunk.chunk_id, 0xB100),
            source_event: persisted
                .infrastructure_delta
                .source_events
                .first()
                .copied(),
            kind: NavigationDangerKind::BiohazardContamination,
            severity: persisted.infrastructure_delta.severity.max(0.7),
            affected_locations: all_locations.clone(),
            reason: "biological contamination requires avoidance or protective routing".to_string(),
        });
    }

    if persisted.consequence_tags.iter().any(|tag| tag == "gas") {
        fields.push(NavigationDangerField {
            field_id: stable_u64(chunk.chunk_id, 0xA100),
            source_event: persisted
                .infrastructure_delta
                .source_events
                .first()
                .copied(),
            kind: NavigationDangerKind::ToxicGas,
            severity: persisted.infrastructure_delta.severity.max(0.65),
            affected_locations: all_locations.clone(),
            reason: "gas hazard should bias AI away from exposed routes".to_string(),
        });
    }

    for entity in persisted
        .damaged_entities
        .iter()
        .chain(persisted.mesh_replacements.iter())
        .copied()
    {
        if let Some(location) = nearest_navigation_location_for_entity(chunk, entity) {
            fields.push(NavigationDangerField {
                field_id: stable_u64(chunk.chunk_id, entity),
                source_event: persisted.event_history.first().copied(),
                kind: NavigationDangerKind::PhysicalDamage,
                severity: 0.45,
                affected_locations: vec![location],
                reason: "recent physical damage may create debris or unstable cover".to_string(),
            });
        }
    }

    if persisted
        .consequence_tags
        .iter()
        .any(|tag| tag == "surveillance")
    {
        let affected_locations = chunk
            .local_navigation
            .nodes
            .iter()
            .filter(|node| node.tags.iter().any(|tag| tag == "story_space"))
            .map(|node| node.location)
            .collect::<Vec<_>>();
        if !affected_locations.is_empty() {
            fields.push(NavigationDangerField {
                field_id: stable_u64(chunk.chunk_id, 0x5A00),
                source_event: persisted
                    .infrastructure_delta
                    .source_events
                    .first()
                    .copied(),
                kind: NavigationDangerKind::Surveillance,
                severity: persisted.infrastructure_delta.severity.max(0.35),
                affected_locations,
                reason: "surveillance increased around story routes".to_string(),
            });
        }
    }

    fields.sort_by_key(|field| (field.kind, field.field_id));
    fields
}

fn navigation_restricted_zones_for_cell(
    chunk: &WorldChunk,
    persisted: &PersistentCellState,
) -> Vec<NavigationRestrictedZone> {
    let location = chunk
        .local_navigation
        .important_locations
        .first()
        .copied()
        .or_else(|| {
            chunk
                .local_navigation
                .nodes
                .first()
                .map(|node| node.location)
        });
    let Some(location) = location else {
        return Vec::new();
    };

    let mut zones = persisted
        .faction_state_delta
        .iter()
        .filter(|delta| delta.alertness_delta > 0.0 || delta.resource_pressure_delta > 0.0)
        .map(|delta| NavigationRestrictedZone {
            location,
            faction: Some(delta.faction_id),
            severity: delta
                .alertness_delta
                .max(delta.resource_pressure_delta)
                .clamp(0.0, 1.0),
            source_event: persisted.event_history.first().copied(),
            reason: delta.reason.clone(),
        })
        .collect::<Vec<_>>();

    if persisted
        .consequence_tags
        .iter()
        .any(|tag| tag == "security_alert")
        && zones.is_empty()
    {
        zones.push(NavigationRestrictedZone {
            location,
            faction: None,
            severity: 0.5,
            source_event: persisted.event_history.first().copied(),
            reason: "security alert restricts normal pedestrian routing".to_string(),
        });
    }

    zones.sort_by(|left, right| {
        right
            .severity
            .partial_cmp(&left.severity)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.location.cmp(&right.location))
            .then(left.faction.cmp(&right.faction))
    });
    zones
}

fn navigation_route_updates_for_cell(
    chunk: &WorldChunk,
    state: &NavigationCellState,
) -> Vec<NavigationRouteUpdate> {
    let blocked_locations = state
        .dynamic_blockers
        .iter()
        .filter_map(|blocker| blocker.blocked_location)
        .collect::<BTreeSet<_>>();
    let dangerous_locations = state
        .danger_fields
        .iter()
        .flat_map(|field| field.affected_locations.iter().copied())
        .collect::<BTreeSet<_>>();
    let restricted_locations = state
        .restricted_zones
        .iter()
        .map(|zone| zone.location)
        .collect::<BTreeSet<_>>();

    let mut updates = Vec::new();
    for edge in &chunk.local_navigation.edges {
        let status_and_reason = if blocked_locations.contains(&edge.from)
            || blocked_locations.contains(&edge.to)
        {
            Some((
                NavigationRouteStatus::Blocked,
                "dynamic blocker touches this navigation edge",
            ))
        } else if restricted_locations.contains(&edge.from)
            || restricted_locations.contains(&edge.to)
        {
            Some((
                NavigationRouteStatus::Restricted,
                "faction or security state restricts this route",
            ))
        } else if dangerous_locations.contains(&edge.from) || dangerous_locations.contains(&edge.to)
        {
            Some((
                NavigationRouteStatus::Dangerous,
                "persistent physical or infrastructure consequence affects this route",
            ))
        } else {
            None
        };

        if let Some((status, reason)) = status_and_reason {
            updates.push(NavigationRouteUpdate {
                from: edge.from,
                to: edge.to,
                traversal: edge.traversal,
                status,
                reason: reason.to_string(),
            });
        }
    }
    updates.sort_by_key(|update| (update.status, update.from, update.to));
    updates
}

fn reachable_important_locations_after_navigation_updates(
    navigation: &NavigationGraph,
    route_updates: &[NavigationRouteUpdate],
) -> usize {
    let Some(start) = navigation.important_locations.first().copied() else {
        return 0;
    };
    let blocked_edges = route_updates
        .iter()
        .filter(|update| update.status == NavigationRouteStatus::Blocked)
        .map(|update| (update.from, update.to))
        .collect::<BTreeSet<_>>();

    navigation
        .important_locations
        .iter()
        .copied()
        .filter(|target| {
            navigation_reachable_without_edges(navigation, start, *target, &blocked_edges)
        })
        .count()
}

fn navigation_reachable_without_edges(
    navigation: &NavigationGraph,
    start: LocationId,
    target: LocationId,
    blocked_edges: &BTreeSet<(LocationId, LocationId)>,
) -> bool {
    if start == target {
        return navigation.has_location(start);
    }

    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::from([start]);
    while let Some(location) = queue.pop_front() {
        if !visited.insert(location) {
            continue;
        }
        for edge in &navigation.edges {
            if edge.from == location && !blocked_edges.contains(&(edge.from, edge.to)) {
                if edge.to == target {
                    return true;
                }
                queue.push_back(edge.to);
            }
            if edge.bidirectional
                && edge.to == location
                && !blocked_edges.contains(&(edge.from, edge.to))
            {
                if edge.from == target {
                    return true;
                }
                queue.push_back(edge.from);
            }
        }
    }

    false
}

fn nearest_navigation_location_for_entity(
    chunk: &WorldChunk,
    entity: EntityId,
) -> Option<LocationId> {
    let entity_position = chunk
        .entities
        .iter()
        .find(|template| template.entity_id == Some(entity))
        .map(|template| template.transform.translation_meters)?;
    nearest_navigation_location(chunk, entity_position)
}

fn nearest_navigation_location(chunk: &WorldChunk, position: Vec3) -> Option<LocationId> {
    chunk
        .local_navigation
        .nodes
        .iter()
        .min_by(|left, right| {
            vec3_distance_squared(left.position, position)
                .partial_cmp(&vec3_distance_squared(right.position, position))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|node| node.location)
}

fn chunk_center(chunk: &WorldChunk) -> Vec3 {
    Vec3::new(
        (chunk.bounds.min.x + chunk.bounds.max.x) * 0.5,
        (chunk.bounds.min.y + chunk.bounds.max.y) * 0.5,
        (chunk.bounds.min.z + chunk.bounds.max.z) * 0.5,
    )
}

fn vec3_dot(left: Vec3, right: Vec3) -> f32 {
    left.x
        .mul_add(right.x, left.y.mul_add(right.y, left.z * right.z))
}

fn vec3_normalize(value: Vec3) -> Vec3 {
    let length_squared = vec3_dot(value, value);
    if length_squared <= f32::EPSILON {
        Vec3::ZERO
    } else {
        let inv_length = length_squared.sqrt().recip();
        Vec3::new(
            value.x * inv_length,
            value.y * inv_length,
            value.z * inv_length,
        )
    }
}

fn vec3_distance_squared(left: Vec3, right: Vec3) -> f32 {
    let dx = left.x - right.x;
    let dy = left.y - right.y;
    let dz = left.z - right.z;
    dx.mul_add(dx, dy.mul_add(dy, dz * dz))
}

fn apply_persistent_event_to_cell(
    template: &WorldTemplate,
    chunk: &WorldChunk,
    event: &WorldEvent,
    material: Option<MaterialId>,
    cell: &mut PersistentCellState,
) {
    match &event.kind {
        WorldEventKind::EntitySpawned { entity, .. } => {
            push_unique_copy(&mut cell.spawned_entities, *entity);
            push_unique_string(&mut cell.consequence_tags, "spawned_entity");
        }
        WorldEventKind::EntityDespawned { entity } => {
            push_unique_copy(&mut cell.destroyed_entities, *entity);
            push_unique_string(&mut cell.consequence_tags, "destroyed_entity");
        }
        WorldEventKind::NavigationMoveBlocked { entity } => {
            push_unique_copy(&mut cell.navigation_blockers, *entity);
            push_unique_string(&mut cell.consequence_tags, "navigation_blocker");
        }
        WorldEventKind::DamageApplied { entity, .. }
        | WorldEventKind::GlassWallFractured { entity }
        | WorldEventKind::MetalBent { entity, .. }
        | WorldEventKind::PowerTransformerOverheated { entity } => {
            push_unique_copy(&mut cell.damaged_entities, *entity);
            push_unique_string(&mut cell.consequence_tags, "physical_damage");
            if matches!(
                event.kind,
                WorldEventKind::PowerTransformerOverheated { .. }
            ) {
                add_infrastructure_delta(
                    template,
                    chunk,
                    cell,
                    event.event_id,
                    &[InfrastructureSystem::Power],
                    0.85,
                    &["infrastructure", "power", "blackout_risk"],
                );
            }
        }
        WorldEventKind::MeshReplaced { entity } => {
            push_unique_copy(&mut cell.mesh_replacements, *entity);
            push_unique_copy(&mut cell.damaged_entities, *entity);
            push_unique_string(&mut cell.consequence_tags, "mesh_replacement");
        }
        WorldEventKind::MaterialStateChanged { entity } => {
            if !cell.material_state_overrides.iter().any(|override_state| {
                override_state.entity == *entity && override_state.source_event == event.event_id
            }) {
                cell.material_state_overrides.push(MaterialStateOverride {
                    entity: *entity,
                    material,
                    source_event: event.event_id,
                    reason: "runtime material state changed".to_string(),
                });
            }
            push_unique_string(&mut cell.consequence_tags, "material_state");
        }
        WorldEventKind::StreetFlooded => {
            let liquid_kind = persistent_liquid_kind(event);
            let severity = liquid_kind.persistent_severity();
            add_infrastructure_delta(
                template,
                chunk,
                cell,
                event.event_id,
                liquid_kind.infrastructure_systems(),
                severity,
                liquid_kind.infrastructure_tags(),
            );
            for tag in liquid_kind.consequence_tags() {
                push_unique_string(&mut cell.consequence_tags, *tag);
            }
        }
        WorldEventKind::ToxicGasReleased => {
            add_infrastructure_delta(
                template,
                chunk,
                cell,
                event.event_id,
                &[InfrastructureSystem::Drainage],
                0.65,
                &["infrastructure", "air_quality"],
            );
            push_unique_string(&mut cell.consequence_tags, "gas");
        }
        WorldEventKind::AgentMemoryUpdated { agent, .. } => {
            push_unique_copy(&mut cell.ai_memory_refs, *agent);
            push_unique_string(&mut cell.consequence_tags, "ai_memory");
        }
        WorldEventKind::FactionReputationChanged {
            faction,
            delta,
            reason,
            ..
        } => {
            cell.faction_state_delta.push(FactionPressureDelta {
                faction_id: *faction,
                district: chunk.district_id,
                alertness_delta: delta.abs().min(1.0) * 0.15,
                resource_pressure_delta: delta.abs().min(1.0),
                territory_strength: faction_territory_strength(
                    template,
                    *faction,
                    chunk.district_id,
                ),
                reason: reason.clone(),
            });
            push_unique_string(&mut cell.consequence_tags, "faction_state");
        }
        WorldEventKind::SecurityAlertRaised {
            faction, severity, ..
        } => {
            cell.faction_state_delta.push(FactionPressureDelta {
                faction_id: *faction,
                district: chunk.district_id,
                alertness_delta: severity.clamp(0.0, 1.0),
                resource_pressure_delta: (severity * 0.55).clamp(0.0, 1.0),
                territory_strength: faction_territory_strength(
                    template,
                    *faction,
                    chunk.district_id,
                ),
                reason: "security alert persists in district state".to_string(),
            });
            push_unique_string(&mut cell.consequence_tags, "security_alert");
        }
        WorldEventKind::SurveillanceIncreased { amount, .. } => {
            add_infrastructure_delta(
                template,
                chunk,
                cell,
                event.event_id,
                &[InfrastructureSystem::Surveillance],
                amount.clamp(0.0, 1.0),
                &["infrastructure", "surveillance"],
            );
            push_unique_string(&mut cell.consequence_tags, "surveillance");
        }
        WorldEventKind::StoryEventEmitted { .. } | WorldEventKind::PlayerIdentityExposed => {
            for story in &chunk.local_story_hooks {
                push_unique_copy(&mut cell.story_thread_refs, story.seed_id);
            }
            push_unique_string(&mut cell.consequence_tags, "story_state");
        }
        WorldEventKind::AssetBecameResident { asset_id, .. } => {
            push_unique_copy(&mut cell.resident_assets, *asset_id);
            push_unique_string(&mut cell.consequence_tags, "resident_asset");
        }
        _ => {}
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PersistentLiquidKind {
    Water,
    Oil,
    Biological,
}

impl PersistentLiquidKind {
    fn persistent_severity(self) -> f32 {
        match self {
            Self::Water => 0.7,
            Self::Oil => 0.62,
            Self::Biological => 0.74,
        }
    }

    fn infrastructure_systems(self) -> &'static [InfrastructureSystem] {
        match self {
            Self::Water => &[InfrastructureSystem::Water, InfrastructureSystem::Drainage],
            Self::Oil | Self::Biological => &[InfrastructureSystem::Drainage],
        }
    }

    fn infrastructure_tags(self) -> &'static [&'static str] {
        match self {
            Self::Water => &["infrastructure", "flooding"],
            Self::Oil => &[
                "infrastructure",
                "contamination",
                "oil_slick",
                "slippery_surface",
            ],
            Self::Biological => &[
                "infrastructure",
                "contamination",
                "biohazard",
                "slippery_surface",
            ],
        }
    }

    fn consequence_tags(self) -> &'static [&'static str] {
        match self {
            Self::Water => &["flooding"],
            Self::Oil => &["contamination", "oil_slick", "slippery_surface"],
            Self::Biological => &["contamination", "biohazard", "slippery_surface"],
        }
    }
}

fn persistent_liquid_kind(event: &WorldEvent) -> PersistentLiquidKind {
    if event_text_contains_any(
        event,
        &[
            "biohazard",
            "biological",
            "biological_contamination",
            "biological_trace",
            "blood",
            "contaminated_surface",
        ],
    ) {
        PersistentLiquidKind::Biological
    } else if event_text_contains_any(
        event,
        &[
            "fuel",
            "grease",
            "hydraulic",
            "oil",
            "oil_contamination",
            "oil_flow",
            "slick",
            "slick_surface",
        ],
    ) {
        PersistentLiquidKind::Oil
    } else {
        PersistentLiquidKind::Water
    }
}

fn event_text_contains_any(event: &WorldEvent, needles: &[&str]) -> bool {
    event
        .physical_evidence
        .iter()
        .chain(event.narrative_tags.iter())
        .any(|text| {
            let normalized = text.to_ascii_lowercase();
            needles.iter().any(|needle| normalized.contains(needle))
        })
}

fn add_infrastructure_delta(
    template: &WorldTemplate,
    chunk: &WorldChunk,
    cell: &mut PersistentCellState,
    source_event: WorldEventId,
    systems: &[InfrastructureSystem],
    severity: f32,
    tags: &[&str],
) {
    for system in systems {
        push_unique_copy(&mut cell.infrastructure_delta.systems, *system);
        for node in infrastructure_nodes_for_system(template, chunk, *system) {
            push_unique_copy(&mut cell.infrastructure_delta.affected_nodes, node);
        }
    }
    push_unique_copy(&mut cell.infrastructure_delta.source_events, source_event);
    cell.infrastructure_delta.severity = cell.infrastructure_delta.severity.max(severity);
    for tag in tags {
        push_unique_string(&mut cell.infrastructure_delta.consequence_tags, *tag);
        push_unique_string(&mut cell.consequence_tags, *tag);
    }
}

fn event_creates_persistent_city_state(kind: &WorldEventKind) -> bool {
    matches!(
        kind,
        WorldEventKind::EntitySpawned { .. }
            | WorldEventKind::EntityDespawned { .. }
            | WorldEventKind::NavigationMoveBlocked { .. }
            | WorldEventKind::DamageApplied { .. }
            | WorldEventKind::MeshReplaced { .. }
            | WorldEventKind::MaterialStateChanged { .. }
            | WorldEventKind::GlassWallFractured { .. }
            | WorldEventKind::MetalBent { .. }
            | WorldEventKind::PowerTransformerOverheated { .. }
            | WorldEventKind::StreetFlooded
            | WorldEventKind::ToxicGasReleased
            | WorldEventKind::AgentMemoryUpdated { .. }
            | WorldEventKind::FactionReputationChanged { .. }
            | WorldEventKind::SecurityAlertRaised { .. }
            | WorldEventKind::SurveillanceIncreased { .. }
            | WorldEventKind::PlayerIdentityExposed
            | WorldEventKind::StoryEventEmitted { .. }
            | WorldEventKind::AssetBecameResident { .. }
    )
}

fn persistent_chunk_for_event<'a>(
    template: &'a WorldTemplate,
    event: &WorldEvent,
) -> Option<&'a WorldChunk> {
    template
        .chunks
        .iter()
        .find(|chunk| chunk_contains_point(chunk, event.location_meters))
        .or_else(|| {
            event_primary_entity(&event.kind).and_then(|entity| {
                template.chunks.iter().find(|chunk| {
                    chunk
                        .entities
                        .iter()
                        .any(|template| template.entity_id == Some(entity))
                })
            })
        })
        .or_else(|| {
            template.chunks.iter().min_by(|left, right| {
                distance_to_chunk_center(left, event.location_meters)
                    .partial_cmp(&distance_to_chunk_center(right, event.location_meters))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        })
}

fn event_primary_entity(kind: &WorldEventKind) -> Option<EntityId> {
    match kind {
        WorldEventKind::EntitySpawned { entity, .. }
        | WorldEventKind::EntityDespawned { entity }
        | WorldEventKind::TransformChanged { entity }
        | WorldEventKind::NavigationMoveBlocked { entity }
        | WorldEventKind::AgentStateChanged { entity }
        | WorldEventKind::ForceApplied { entity }
        | WorldEventKind::DamageApplied { entity, .. }
        | WorldEventKind::MeshReplaced { entity }
        | WorldEventKind::MaterialStateChanged { entity }
        | WorldEventKind::GlassWallFractured { entity }
        | WorldEventKind::MetalBent { entity, .. }
        | WorldEventKind::FlexibleConstraintResolved { entity, .. }
        | WorldEventKind::PowerTransformerOverheated { entity }
        | WorldEventKind::AgentMemoryUpdated { agent: entity, .. }
        | WorldEventKind::AgentIntentProposed { agent: entity, .. }
        | WorldEventKind::AgentDecisionExplained { agent: entity, .. }
        | WorldEventKind::VoiceLineSpoken { speaker: entity }
        | WorldEventKind::SpeechSynthesized {
            speaker: entity, ..
        }
        | WorldEventKind::FacialAnimationApplied { entity, .. }
        | WorldEventKind::HumanAppearanceUpdated { entity, .. }
        | WorldEventKind::DialogueEmitted {
            speaker: entity, ..
        } => Some(*entity),
        WorldEventKind::NpcWitnessedCrime { witness, .. } => Some(*witness),
        WorldEventKind::NpcHeardSound { listener, .. } => Some(*listener),
        WorldEventKind::SoundEmitted { source_entity, .. } => *source_entity,
        WorldEventKind::SecurityAlertRaised { threat, .. } => Some(*threat),
        WorldEventKind::FactionReputationChanged { subject, .. } => Some(*subject),
        _ => None,
    }
}

fn chunk_contains_point(chunk: &WorldChunk, point: Vec3) -> bool {
    point.x >= chunk.bounds.min.x
        && point.x <= chunk.bounds.max.x
        && point.y >= chunk.bounds.min.y
        && point.y <= chunk.bounds.max.y
        && point.z >= chunk.bounds.min.z
        && point.z <= chunk.bounds.max.z
}

fn distance_to_chunk_center(chunk: &WorldChunk, point: Vec3) -> f32 {
    let center = Vec3::new(
        (chunk.bounds.min.x + chunk.bounds.max.x) * 0.5,
        (chunk.bounds.min.y + chunk.bounds.max.y) * 0.5,
        (chunk.bounds.min.z + chunk.bounds.max.z) * 0.5,
    );
    let dx = center.x - point.x;
    let dy = center.y - point.y;
    let dz = center.z - point.z;
    dx.mul_add(dx, dy.mul_add(dy, dz * dz))
}

fn material_for_entity(template: &WorldTemplate, entity: EntityId) -> Option<MaterialId> {
    template
        .chunks
        .iter()
        .flat_map(|chunk| chunk.entities.iter())
        .find(|template| template.entity_id == Some(entity))
        .and_then(|template| {
            template
                .renderable
                .as_ref()
                .map(|renderable| renderable.material)
                .or_else(|| template.physical_body.as_ref().map(|body| body.material_id))
        })
}

fn infrastructure_nodes_for_system(
    template: &WorldTemplate,
    chunk: &WorldChunk,
    system: InfrastructureSystem,
) -> Vec<InfrastructureLinkId> {
    let system_nodes = infrastructure_graph_for_system(&template.infrastructure, system)
        .nodes
        .iter()
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    chunk
        .local_infrastructure
        .iter()
        .copied()
        .filter(|node| system_nodes.contains(node))
        .collect()
}

fn infrastructure_graph_for_system(
    graphs: &InfrastructureGraphSet,
    system: InfrastructureSystem,
) -> &InfrastructureGraph {
    match system {
        InfrastructureSystem::Power => &graphs.power,
        InfrastructureSystem::Water => &graphs.water,
        InfrastructureSystem::Data => &graphs.data,
        InfrastructureSystem::Surveillance => &graphs.surveillance,
        InfrastructureSystem::Transit => &graphs.transit,
        InfrastructureSystem::Drainage => &graphs.drainage,
    }
}

fn faction_territory_strength(
    template: &WorldTemplate,
    faction_id: FactionId,
    district: DistrictId,
) -> f32 {
    template
        .factions
        .iter()
        .find(|faction| faction.faction_id == faction_id)
        .and_then(|faction| {
            faction
                .territory_claims
                .iter()
                .find(|claim| claim.district == district)
        })
        .map(|claim| claim.strength)
        .unwrap_or_default()
}

fn push_unique_copy<T>(values: &mut Vec<T>, value: T)
where
    T: Copy + PartialEq,
{
    if !values.contains(&value) {
        values.push(value);
    }
}

fn push_unique_string(values: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if !values.contains(&value) {
        values.push(value);
    }
}

fn build_district(seed: u64, index: usize, kind: DistrictKind) -> CityDistrict {
    let district_id = 1_000 + index as u64;
    let variation = stable_unit(seed, district_id);
    let (surveillance, crime, pollution, socioeconomic_profile, theme): (
        f32,
        f32,
        f32,
        &'static str,
        &'static str,
    ) = match kind {
        DistrictKind::CorporateCore => (
            0.92,
            0.22,
            0.28,
            "high wealth, corporate-controlled, low street trust",
            "clean glass towers with white security light",
        ),
        DistrictKind::RainAlleySlum => (
            0.46,
            0.78,
            0.66,
            "low wealth, dense tenancy, informal protection rackets",
            "wet asphalt, stained concrete, cracked glass, neon haze",
        ),
        DistrictKind::IndustrialDock => (
            0.55,
            0.62,
            0.84,
            "working dock blocks, smuggling pressure, heavy machinery",
            "rusted steel, sodium lamps, chemical puddles",
        ),
        DistrictKind::BlackMarket => (
            0.33,
            0.72,
            0.51,
            "cash-heavy interiors, hidden doors, illegal augment trade",
            "crowded signs, synthetic leather, hacked screen glow",
        ),
        DistrictKind::ClinicDistrict => (
            0.68,
            0.48,
            0.39,
            "medical debt, biotech waste, guarded treatment rooms",
            "sterile panels, red warning strips, surgical neon",
        ),
    };

    CityDistrict {
        id: district_id,
        name: format!("{} {}", title_case(kind.label()), index + 1),
        kind,
        socioeconomic_profile: socioeconomic_profile.to_string(),
        controlling_factions: vec![
            (700, (0.35 + variation * 0.25).clamp(0.0, 1.0)),
            (701, (0.4 + crime * 0.2).clamp(0.0, 1.0)),
        ],
        infrastructure: "power, water, data, surveillance, drainage, and transit nodes".to_string(),
        surveillance_level: (surveillance + variation * 0.04).clamp(0.0, 1.0),
        crime_pressure: (crime + variation * 0.05).clamp(0.0, 1.0),
        pollution: (pollution + variation * 0.04).clamp(0.0, 1.0),
        visual_theme: theme.to_string(),
        gameplay_tags: vec![
            "district".to_string(),
            kind.label().replace(' ', "_"),
            quality_tag(kind).to_string(),
        ],
    }
}

fn build_chunk_for_district(
    seed: u64,
    district: &CityDistrict,
    index: usize,
    material_palette: MaterialPaletteRef,
) -> WorldChunk {
    let chunk_id = (district.id << 16) | index as u64;
    let x_offset = index as f32 * 64.0;
    let entities = build_chunk_entities(district, x_offset, material_palette);
    let local_navigation = build_local_navigation(district, index, x_offset);
    let mut streaming_dependencies = collect_streaming_dependencies(&entities);
    streaming_dependencies.extend(district_material_cache_dependencies(
        district,
        material_palette,
    ));
    streaming_dependencies.sort_unstable();
    streaming_dependencies.dedup();
    let active_npc_seeds = entities
        .iter()
        .filter_map(|template| template.agent.as_ref().map(|agent| agent.persona))
        .collect::<Vec<_>>();
    let physical_hooks = entities
        .iter()
        .filter(|template| has_tag(&template.tags, "destructible"))
        .filter_map(|template| template.entity_id)
        .collect::<Vec<_>>();
    let local_infrastructure = infrastructure_ids_for_district(seed, district.id);
    let location = local_navigation
        .important_locations
        .first()
        .copied()
        .unwrap_or(ALLEY_LOCATION_ID);
    let population = build_population_profile_for_chunk(district, index, &local_navigation);
    let population_summary = population_summary_from_profile(district, &population);

    WorldChunk {
        chunk_id,
        district_id: district.id,
        bounds: Aabb::new(
            Vec3::new(x_offset - 8.0, -12.0, -2.0),
            Vec3::new(x_offset + 48.0, 12.0, 18.0),
        ),
        entities,
        population,
        population_summary,
        local_navigation,
        local_infrastructure: local_infrastructure.clone(),
        local_story_hooks: vec![StorySeed {
            seed_id: stable_u64(seed, chunk_id),
            label: format!("{} pressure hook", district.name),
            location,
            involved_factions: vec![700, 701],
            involved_npcs: active_npc_seeds.clone(),
            secret_refs: vec![stable_u64(seed, district.id + 77)],
            physical_hooks,
            trigger_conditions: local_infrastructure
                .first()
                .copied()
                .map(TriggerCondition::InfrastructureDamaged)
                .into_iter()
                .collect(),
            consequence_tags: vec![
                "district_pressure".to_string(),
                "infrastructure".to_string(),
            ],
            tags: vec!["story_seed".to_string(), "worldgen".to_string()],
        }],
        streaming_dependencies,
        neighbor_links: Vec::new(),
        navigation_links: Vec::new(),
        active_npc_seeds,
        material_palette,
        physics_activation_hints: vec![
            "activate fragile glass near player".to_string(),
            "activate water leak when chunk is visible".to_string(),
        ],
        quality_tier: match district.kind {
            DistrictKind::CorporateCore | DistrictKind::ClinicDistrict => {
                QualityTier::HeroHighFidelityRuntime
            }
            _ => QualityTier::NormalRuntime,
        },
    }
}

fn build_chunk_entities(
    district: &CityDistrict,
    x_offset: f32,
    material_palette: MaterialPaletteRef,
) -> Vec<EntityTemplate> {
    let entity_base = district.id * 100;
    build_alley_scene()
        .into_iter()
        .filter(|template| template.entity_id != Some(PLAYER_ID))
        .map(|mut template| {
            let old_entity = template.entity_id.unwrap_or(0);
            let new_entity = entity_base + old_entity;
            template.entity_id = Some(new_entity);
            template.name = format!("{} {}", district.name, template.name);
            template.transform.translation_meters.x += x_offset;
            template.tags.push("worldgen_chunk".to_string());
            template.tags.push(district.kind.label().replace(' ', "_"));
            remap_entity_materials_for_district(&mut template, district.id, material_palette);
            if let Some(human) = &mut template.human {
                human.human_id += entity_base;
            }
            if let Some(agent) = &mut template.agent {
                agent.persona += entity_base;
            }
            template
        })
        .collect()
}

fn build_population_profile_for_chunk(
    district: &CityDistrict,
    index: usize,
    navigation: &NavigationGraph,
) -> CityCellPopulationProfile {
    let location_seed = navigation
        .important_locations
        .first()
        .copied()
        .unwrap_or(ALLEY_LOCATION_ID);
    let (crowd_density, traffic_density, expected_crowd, expected_transit, primary_behavior, mode) =
        match district.kind {
            DistrictKind::CorporateCore => (
                0.58,
                0.7,
                42,
                9,
                CrowdBehaviorKind::Commute,
                TrafficMode::Drone,
            ),
            DistrictKind::RainAlleySlum => (
                0.78,
                0.42,
                64,
                5,
                CrowdBehaviorKind::Loiter,
                TrafficMode::Pedestrian,
            ),
            DistrictKind::IndustrialDock => (
                0.34,
                0.76,
                24,
                12,
                CrowdBehaviorKind::Work,
                TrafficMode::DeliveryBike,
            ),
            DistrictKind::BlackMarket => (
                0.86,
                0.5,
                72,
                6,
                CrowdBehaviorKind::Shop,
                TrafficMode::Pedestrian,
            ),
            DistrictKind::ClinicDistrict => (
                0.46,
                0.38,
                36,
                4,
                CrowdBehaviorKind::SeekClinic,
                TrafficMode::EmergencyVehicle,
            ),
        };
    let faction_affinity = district
        .controlling_factions
        .iter()
        .max_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(faction, _)| *faction);
    let mut crowd_spawn_rules = vec![
        CrowdSpawnRule {
            rule_id: stable_u64(district.id, index as u64 ^ 0xC20D_0001),
            archetype_label: format!("{} regulars", district.kind.label()),
            density: crowd_density,
            schedule: vec![
                CrowdScheduleWindow {
                    start_hour: 6,
                    end_hour: 10,
                    density_multiplier: 0.78,
                    behavior: CrowdBehaviorKind::Commute,
                },
                CrowdScheduleWindow {
                    start_hour: 10,
                    end_hour: 21,
                    density_multiplier: 1.0,
                    behavior: primary_behavior,
                },
                CrowdScheduleWindow {
                    start_hour: 21,
                    end_hour: 2,
                    density_multiplier: 0.62,
                    behavior: if district.kind == DistrictKind::CorporateCore {
                        CrowdBehaviorKind::Patrol
                    } else {
                        CrowdBehaviorKind::Loiter
                    },
                },
            ],
            faction_affinity,
            culture_tags: vec![
                district.kind.label().replace(' ', "_"),
                quality_tag(district.kind).to_string(),
            ],
            danger_reaction: match district.kind {
                DistrictKind::CorporateCore | DistrictKind::ClinicDistrict => {
                    CrowdDangerReaction::Report
                }
                DistrictKind::IndustrialDock => CrowdDangerReaction::Evacuate,
                DistrictKind::BlackMarket => CrowdDangerReaction::Observe,
                DistrictKind::RainAlleySlum => CrowdDangerReaction::Avoid,
            },
            avoidance_radius_meters: (1.2 + district.surveillance_level * 0.8).clamp(0.8, 2.4),
            conversation_weight: (0.24 + crowd_density * 0.28).clamp(0.0, 1.0),
            commerce_weight: match district.kind {
                DistrictKind::BlackMarket => 0.92,
                DistrictKind::RainAlleySlum => 0.48,
                DistrictKind::ClinicDistrict => 0.34,
                _ => 0.18,
            },
            observation_weight: (district.surveillance_level * 0.45
                + district.crime_pressure * 0.35)
                .clamp(0.0, 1.0),
            lod_tier: if crowd_density > 0.75 {
                PopulationLodTier::Simulated
            } else {
                PopulationLodTier::Background
            },
        },
        CrowdSpawnRule {
            rule_id: stable_u64(district.id, index as u64 ^ 0xC20D_0002),
            archetype_label: format!("{} emergency response", district.kind.label()),
            density: (district.crime_pressure * 0.32 + district.surveillance_level * 0.22)
                .clamp(0.05, 0.55),
            schedule: vec![CrowdScheduleWindow {
                start_hour: 0,
                end_hour: 24,
                density_multiplier: 1.0,
                behavior: CrowdBehaviorKind::Patrol,
            }],
            faction_affinity: Some(ALLEY_SECURITY_FACTION_ID),
            culture_tags: vec!["security_response".to_string(), "witness_rules".to_string()],
            danger_reaction: CrowdDangerReaction::Report,
            avoidance_radius_meters: 1.8,
            conversation_weight: 0.12,
            commerce_weight: 0.0,
            observation_weight: 0.82,
            lod_tier: PopulationLodTier::Simulated,
        },
    ];
    if district.kind == DistrictKind::BlackMarket {
        crowd_spawn_rules.push(CrowdSpawnRule {
            rule_id: stable_u64(district.id, index as u64 ^ 0xC20D_0003),
            archetype_label: "black market vendors".to_string(),
            density: 0.48,
            schedule: vec![CrowdScheduleWindow {
                start_hour: 17,
                end_hour: 4,
                density_multiplier: 1.0,
                behavior: CrowdBehaviorKind::Shop,
            }],
            faction_affinity: Some(ALLEY_OPPOSITION_FACTION_ID),
            culture_tags: vec!["commerce".to_string(), "secret_trade".to_string()],
            danger_reaction: CrowdDangerReaction::Observe,
            avoidance_radius_meters: 1.1,
            conversation_weight: 0.54,
            commerce_weight: 1.0,
            observation_weight: 0.62,
            lod_tier: PopulationLodTier::Hero,
        });
    }

    let crossing_locations = navigation
        .important_locations
        .iter()
        .copied()
        .take(3)
        .collect::<Vec<_>>();
    let mut traffic_rules = vec![TrafficFlowRule {
        rule_id: stable_u64(district.id, index as u64 ^ 0x7AFC_0001),
        mode,
        density: traffic_density,
        average_speed_mps: match mode {
            TrafficMode::Pedestrian => 1.35,
            TrafficMode::DeliveryBike => 5.5,
            TrafficMode::Drone => 8.0,
            TrafficMode::Tram => 7.0,
            TrafficMode::EmergencyVehicle => 6.5,
        },
        crossing_locations: crossing_locations.clone(),
        blocked_route_response: if district.kind == DistrictKind::CorporateCore {
            TrafficBlockedRouteResponse::EmergencyOverride
        } else if district.kind == DistrictKind::IndustrialDock {
            TrafficBlockedRouteResponse::Queue
        } else {
            TrafficBlockedRouteResponse::Reroute
        },
        emergency_response: match district.kind {
            DistrictKind::CorporateCore | DistrictKind::ClinicDistrict => {
                TrafficEmergencyResponse::ClearLane
            }
            DistrictKind::RainAlleySlum | DistrictKind::BlackMarket => {
                TrafficEmergencyResponse::Flee
            }
            DistrictKind::IndustrialDock => TrafficEmergencyResponse::SlowAndObserve,
        },
        sound_lighting_profile: format!("{} traffic audio/light profile", district.kind.label()),
        lod_tier: if traffic_density > 0.65 {
            PopulationLodTier::Simulated
        } else {
            PopulationLodTier::Summary
        },
    }];
    if district.kind == DistrictKind::CorporateCore || district.kind == DistrictKind::ClinicDistrict
    {
        traffic_rules.push(TrafficFlowRule {
            rule_id: stable_u64(district.id, index as u64 ^ 0x7AFC_0002),
            mode: TrafficMode::Tram,
            density: 0.42,
            average_speed_mps: 9.0,
            crossing_locations,
            blocked_route_response: TrafficBlockedRouteResponse::DespawnAtPortal,
            emergency_response: TrafficEmergencyResponse::MaintainSchedule,
            sound_lighting_profile: "elevated transit hum and platform signage".to_string(),
            lod_tier: PopulationLodTier::Summary,
        });
    }

    let mut profile = CityCellPopulationProfile {
        crowd_spawn_rules,
        traffic_rules,
        expected_active_npcs: 0,
        expected_background_crowd: expected_crowd,
        expected_vehicle_or_transit_count: expected_transit,
        validation_report: CrowdTrafficValidationReport::default(),
    };
    profile.expected_active_npcs = profile
        .crowd_spawn_rules
        .iter()
        .filter(|rule| rule.lod_tier <= PopulationLodTier::Simulated)
        .count()
        .max(1);
    profile.validation_report = validate_population_profile(&profile, location_seed);
    profile
}

fn population_summary_from_profile(
    district: &CityDistrict,
    profile: &CityCellPopulationProfile,
) -> PopulationSummary {
    let crowd_rule_count = profile.crowd_spawn_rules.len().max(1) as f32;
    let traffic_rule_count = profile.traffic_rules.len().max(1) as f32;
    let average_crowd_density = profile
        .crowd_spawn_rules
        .iter()
        .map(|rule| rule.density)
        .sum::<f32>()
        / crowd_rule_count;
    let average_traffic_density = profile
        .traffic_rules
        .iter()
        .map(|rule| rule.density)
        .sum::<f32>()
        / traffic_rule_count;
    let commerce_activity = profile
        .crowd_spawn_rules
        .iter()
        .map(|rule| rule.commerce_weight)
        .fold(0.0_f32, f32::max);
    let observation_coverage = profile
        .crowd_spawn_rules
        .iter()
        .map(|rule| rule.observation_weight)
        .fold(district.surveillance_level * 0.5, f32::max);
    let simulated_crowd_rule_count = profile
        .crowd_spawn_rules
        .iter()
        .filter(|rule| rule.lod_tier <= PopulationLodTier::Simulated)
        .count();
    let summary_only_crowd_rule_count = profile
        .crowd_spawn_rules
        .iter()
        .filter(|rule| rule.lod_tier >= PopulationLodTier::Background)
        .count();

    PopulationSummary {
        expected_active_npcs: usize_to_u32(profile.expected_active_npcs),
        expected_background_crowd: usize_to_u32(profile.expected_background_crowd),
        expected_vehicle_or_transit_count: usize_to_u32(profile.expected_vehicle_or_transit_count),
        crowd_spawn_rule_count: usize_to_u32(profile.crowd_spawn_rules.len()),
        traffic_rule_count: usize_to_u32(profile.traffic_rules.len()),
        simulated_crowd_rule_count: usize_to_u32(simulated_crowd_rule_count),
        summary_only_crowd_rule_count: usize_to_u32(summary_only_crowd_rule_count),
        average_crowd_density: average_crowd_density.clamp(0.0, 1.0),
        average_traffic_density: average_traffic_density.clamp(0.0, 1.0),
        faction_control_count: usize_to_u32(district.controlling_factions.len()),
        alertness: (district.surveillance_level * 0.55 + district.crime_pressure * 0.45)
            .clamp(0.0, 1.0),
        commerce_activity: commerce_activity.clamp(0.0, 1.0),
        observation_coverage: observation_coverage.clamp(0.0, 1.0),
        ..PopulationSummary::default()
    }
}

fn usize_to_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

pub fn validate_population_profile(
    profile: &CityCellPopulationProfile,
    required_location: LocationId,
) -> CrowdTrafficValidationReport {
    let mut issues = Vec::new();
    if profile.crowd_spawn_rules.is_empty() {
        issues.push(validation_issue(
            ValidationSeverity::Error,
            "missing_crowd_spawn_rules",
            "city cells must define crowd spawn rules for believable population",
        ));
    }
    if profile.traffic_rules.is_empty() {
        issues.push(validation_issue(
            ValidationSeverity::Error,
            "missing_traffic_rules",
            "city cells must define traffic or transit behavior",
        ));
    }
    if profile.expected_background_crowd > 256 {
        issues.push(validation_issue(
            ValidationSeverity::Error,
            "unbounded_background_crowd",
            "city cell crowd expectation exceeds runtime population budget",
        ));
    }
    for rule in &profile.crowd_spawn_rules {
        if rule.schedule.is_empty() {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "crowd_rule_missing_schedule",
                "crowd spawn rules need schedule windows",
            ));
        }
        if !rule.density.is_finite() || !(0.0..=1.0).contains(&rule.density) {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "crowd_rule_invalid_density",
                "crowd density must be normalized",
            ));
        }
        if rule.lod_tier <= PopulationLodTier::Simulated && rule.observation_weight <= 0.0 {
            issues.push(validation_issue(
                ValidationSeverity::Warning,
                "simulated_crowd_without_observation",
                "simulated crowds should contribute witness or observation behavior",
            ));
        }
    }
    for rule in &profile.traffic_rules {
        if !rule.density.is_finite() || !(0.0..=1.0).contains(&rule.density) {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "traffic_rule_invalid_density",
                "traffic density must be normalized",
            ));
        }
        if rule.average_speed_mps <= 0.0 || !rule.average_speed_mps.is_finite() {
            issues.push(validation_issue(
                ValidationSeverity::Error,
                "traffic_rule_invalid_speed",
                "traffic rules need a positive finite average speed",
            ));
        }
        if !rule.crossing_locations.contains(&required_location) {
            issues.push(validation_issue(
                ValidationSeverity::Warning,
                "traffic_rule_not_grounded_in_cell_navigation",
                "traffic crossings should include the cell's primary navigation location",
            ));
        }
    }

    CrowdTrafficValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error),
        issues,
    }
}

fn build_district_materials(
    seed: u64,
    district: &CityDistrict,
    material_palette: MaterialPaletteRef,
) -> Vec<MaterialDescriptor> {
    default_alley_materials()
        .into_iter()
        .map(|descriptor| {
            let variant_id = district_material_id(district.id, material_palette, descriptor.id);
            let mut variant = apply_instance_variation(
                &descriptor,
                &MaterialInstanceSeed {
                    material_id: variant_id,
                    seed: seed ^ district.id ^ descriptor.id,
                    age: district_material_age(district),
                    dirt_level: district.pollution.max(district.crime_pressure * 0.65),
                    damage_bias: district.crime_pressure * 0.55 + district.pollution * 0.25,
                    district_style: district.kind.label().to_string(),
                    local_variation: stable_unit(seed ^ district.id, descriptor.id),
                    initial_state: district_material_initial_state(district, descriptor.id),
                },
            );
            variant.id = variant_id;
            variant.name = format!("{} {}", district.name, descriptor.name);
            variant.procedural_source = Some(MaterialGeneratorRef(stable_u64(
                seed ^ district.id,
                descriptor.id ^ 0xDA7A_3000,
            ) as AssetId));
            variant
        })
        .collect()
}

fn remap_entity_materials_for_district(
    template: &mut EntityTemplate,
    district_id: DistrictId,
    material_palette: MaterialPaletteRef,
) {
    if let Some(renderable) = &mut template.renderable {
        tag_entity_material_family(&mut template.tags, renderable.material);
        renderable.material =
            district_material_id(district_id, material_palette, renderable.material);
    }
    if let Some(body) = &mut template.physical_body {
        tag_entity_material_family(&mut template.tags, body.material_id);
        body.material_id = district_material_id(district_id, material_palette, body.material_id);
    }
}

fn tag_entity_material_family(tags: &mut TagSet, material_id: MaterialId) {
    let tag = match material_id {
        MATERIAL_GLASS => Some("material:glass"),
        MATERIAL_WET_ASPHALT => Some("material:wet_asphalt"),
        MATERIAL_NEON_TUBE => Some("material:neon"),
        MATERIAL_HUMAN_SKIN => Some("material:human_skin"),
        MATERIAL_WATER => Some("material:water"),
        _ => None,
    };
    let Some(tag) = tag else {
        return;
    };
    if !tags.iter().any(|existing| existing == tag) {
        tags.push(tag.to_string());
    }
}

fn district_material_id(
    district_id: DistrictId,
    material_palette: MaterialPaletteRef,
    base_material: MaterialId,
) -> MaterialId {
    stable_u64(
        district_id ^ material_palette.rotate_left(13),
        base_material ^ 0xA57A_0001,
    )
}

fn district_material_cache_dependencies(
    district: &CityDistrict,
    material_palette: MaterialPaletteRef,
) -> Vec<AssetId> {
    district_material_cache_base_materials(district.kind)
        .into_iter()
        .map(|base_material| {
            district_material_cache_asset_id(district.id, material_palette, base_material)
        })
        .collect()
}

fn district_material_cache_base_materials(kind: DistrictKind) -> [MaterialId; 2] {
    match kind {
        DistrictKind::CorporateCore => [MATERIAL_GLASS, MATERIAL_WET_ASPHALT],
        DistrictKind::RainAlleySlum => [MATERIAL_WET_ASPHALT, MATERIAL_WATER],
        DistrictKind::IndustrialDock => [MATERIAL_WET_ASPHALT, MATERIAL_WATER],
        DistrictKind::BlackMarket => [MATERIAL_NEON_TUBE, MATERIAL_WET_ASPHALT],
        DistrictKind::ClinicDistrict => [MATERIAL_HUMAN_SKIN, MATERIAL_GLASS],
    }
}

fn district_material_cache_asset_id(
    district_id: DistrictId,
    material_palette: MaterialPaletteRef,
    base_material: MaterialId,
) -> AssetId {
    worldgen_material_cache_asset_id(district_material_id(
        district_id,
        material_palette,
        base_material,
    ))
}

fn district_material_age(district: &CityDistrict) -> f32 {
    match district.kind {
        DistrictKind::CorporateCore => (0.18 + district.pollution * 0.12).clamp(0.0, 1.0),
        DistrictKind::ClinicDistrict => (0.24 + district.pollution * 0.18).clamp(0.0, 1.0),
        DistrictKind::IndustrialDock => {
            (0.58 + district.pollution * 0.26 + district.crime_pressure * 0.08).clamp(0.0, 1.0)
        }
        DistrictKind::BlackMarket => {
            (0.5 + district.crime_pressure * 0.26 + district.pollution * 0.12).clamp(0.0, 1.0)
        }
        DistrictKind::RainAlleySlum => {
            (0.62 + district.pollution * 0.2 + district.crime_pressure * 0.1).clamp(0.0, 1.0)
        }
    }
}

fn district_material_initial_state(
    district: &CityDistrict,
    base_material: MaterialId,
) -> MaterialState {
    let wet_bias = if matches!(district.kind, DistrictKind::RainAlleySlum) {
        0.36
    } else {
        0.12
    };
    let corrosion_bias = if matches!(district.kind, DistrictKind::IndustrialDock) {
        0.36
    } else {
        0.12
    };
    MaterialState {
        moisture: (wet_bias + district.pollution * 0.32).clamp(0.0, 1.0),
        soot: (district.pollution * 0.62 + district.crime_pressure * 0.12).clamp(0.0, 1.0),
        corrosion: (corrosion_bias + district.pollution * 0.38).clamp(0.0, 1.0),
        crack_density: (district.crime_pressure * 0.16 + district.pollution * 0.08).clamp(0.0, 1.0),
        electrical_charge: if base_material == MATERIAL_NEON_TUBE {
            80.0 + district.surveillance_level * 64.0
        } else {
            district.surveillance_level * 24.0
        },
        ..MaterialState::default()
    }
}

fn build_local_navigation(district: &CityDistrict, index: usize, x_offset: f32) -> NavigationGraph {
    let base = ALLEY_LOCATION_ID + index as u64 * 10;
    let nodes = vec![
        NavigationNode {
            location: base,
            position: Vec3::new(x_offset - 4.0, 0.0, 0.0),
            tags: vec![
                "chunk_entry".to_string(),
                district.kind.label().replace(' ', "_"),
            ],
        },
        NavigationNode {
            location: base + 1,
            position: Vec3::new(x_offset + 8.0, 0.0, 0.0),
            tags: vec!["cover".to_string(), "story_space".to_string()],
        },
        NavigationNode {
            location: base + 2,
            position: Vec3::new(x_offset + 24.0, 2.5, 0.0),
            tags: vec!["service_route".to_string(), "stealth".to_string()],
        },
        NavigationNode {
            location: base + 3,
            position: Vec3::new(x_offset + 40.0, 0.0, 0.0),
            tags: vec!["chunk_exit".to_string()],
        },
    ];
    let edges = vec![
        NavigationEdge {
            from: base,
            to: base + 1,
            traversal: TraversalKind::Walk,
            bidirectional: true,
            length_meters: 12.0,
        },
        NavigationEdge {
            from: base + 1,
            to: base + 2,
            traversal: TraversalKind::StealthPath,
            bidirectional: true,
            length_meters: 16.5,
        },
        NavigationEdge {
            from: base + 2,
            to: base + 3,
            traversal: TraversalKind::CoverDash,
            bidirectional: true,
            length_meters: 17.0,
        },
    ];

    NavigationGraph {
        nodes,
        edges,
        important_locations: vec![base, base + 1, base + 3],
    }
}

fn build_infrastructure_graphs(seed: u64, districts: &[CityDistrict]) -> InfrastructureGraphSet {
    let mut graphs = InfrastructureGraphSet::default();
    for district in districts {
        let ids = infrastructure_ids_for_district(seed, district.id);
        graphs.power.nodes.push(infrastructure_node(
            ids[0],
            district.id * 10,
            InfrastructureNodeKind::PowerTransformer,
            vec!["power".to_string(), "lights".to_string()],
        ));
        graphs.water.nodes.push(infrastructure_node(
            ids[1],
            district.id * 10 + 1,
            InfrastructureNodeKind::WaterPipe,
            vec!["water".to_string(), "floodable".to_string()],
        ));
        graphs.data.nodes.push(infrastructure_node(
            ids[2],
            district.id * 10 + 2,
            InfrastructureNodeKind::DataRelay,
            vec!["data".to_string(), "hackable".to_string()],
        ));
        graphs.surveillance.nodes.push(infrastructure_node(
            ids[3],
            district.id * 10 + 3,
            InfrastructureNodeKind::SurveillanceCamera,
            vec!["camera".to_string(), "security".to_string()],
        ));
        graphs.transit.nodes.push(infrastructure_node(
            ids[4],
            district.id * 10 + 4,
            InfrastructureNodeKind::TransitStop,
            vec!["transit".to_string(), "crowd_flow".to_string()],
        ));
        graphs.drainage.nodes.push(infrastructure_node(
            ids[5],
            district.id * 10 + 5,
            InfrastructureNodeKind::DrainagePump,
            vec!["drainage".to_string(), "water".to_string()],
        ));
    }

    connect_infrastructure(&mut graphs.power, InfrastructureEdgeKind::PowerCable);
    connect_infrastructure(&mut graphs.water, InfrastructureEdgeKind::WaterMain);
    connect_infrastructure(&mut graphs.data, InfrastructureEdgeKind::FiberLine);
    connect_infrastructure(&mut graphs.surveillance, InfrastructureEdgeKind::CameraFeed);
    connect_infrastructure(&mut graphs.transit, InfrastructureEdgeKind::TransitRoute);
    connect_infrastructure(&mut graphs.drainage, InfrastructureEdgeKind::DrainageFlow);
    graphs
}

fn merge_navigation_graphs(chunks: &[WorldChunk]) -> NavigationGraph {
    let mut graph = NavigationGraph::default();
    for chunk in chunks {
        graph.nodes.extend(chunk.local_navigation.nodes.clone());
        graph.edges.extend(chunk.local_navigation.edges.clone());
        graph
            .important_locations
            .extend(chunk.local_navigation.important_locations.clone());
    }

    for pair in chunks.windows(2) {
        let Some(from) = pair[0].local_navigation.important_locations.last().copied() else {
            continue;
        };
        let Some(to) = pair[1]
            .local_navigation
            .important_locations
            .first()
            .copied()
        else {
            continue;
        };
        graph.edges.push(NavigationEdge {
            from,
            to,
            traversal: TraversalKind::Walk,
            bidirectional: true,
            length_meters: 8.0,
        });
    }

    graph
}

fn build_factions(seed: u64, districts: &[CityDistrict]) -> Vec<FactionSeed> {
    let home_districts = districts
        .iter()
        .map(|district| district.id)
        .collect::<Vec<_>>();
    let primary_home = home_districts.first().copied().unwrap_or(1_000);
    vec![
        FactionSeed {
            faction_id: 700,
            name: "Kestrel Security".to_string(),
            home_district: primary_home,
            home_districts: home_districts.clone(),
            influence: 0.62,
            resources: ResourceState {
                credits: 90_000,
                water_access: 0.45,
                data_access: 0.7,
                muscle: 0.82,
                medical_supply: 0.25,
            },
            territory_claims: districts
                .iter()
                .map(|district| TerritoryClaim {
                    district: district.id,
                    strength: (0.42 + district.surveillance_level * 0.35).clamp(0.0, 1.0),
                    reason: "surveillance contract and protection fees".to_string(),
                })
                .collect(),
            allies: vec![702],
            enemies: vec![701],
            secrets: vec![stable_u64(seed, 700)],
            visual_identity: FactionVisualProfile {
                primary_color: [0.1, 0.45, 0.95],
                symbol: "kestrel eye".to_string(),
                material_preference: MATERIAL_NEON_TUBE,
            },
        },
        FactionSeed {
            faction_id: 701,
            name: "Vanta Circuit".to_string(),
            home_district: primary_home,
            home_districts: home_districts.clone(),
            influence: 0.58,
            resources: ResourceState {
                credits: 42_000,
                water_access: 0.58,
                data_access: 0.86,
                muscle: 0.48,
                medical_supply: 0.3,
            },
            territory_claims: districts
                .iter()
                .map(|district| TerritoryClaim {
                    district: district.id,
                    strength: (0.35 + district.crime_pressure * 0.4).clamp(0.0, 1.0),
                    reason: "data taps and black-market routing".to_string(),
                })
                .collect(),
            allies: Vec::new(),
            enemies: vec![700, 702],
            secrets: vec![stable_u64(seed, 701)],
            visual_identity: FactionVisualProfile {
                primary_color: [0.8, 0.04, 0.55],
                symbol: "black circuit halo".to_string(),
                material_preference: MATERIAL_GLASS,
            },
        },
        FactionSeed {
            faction_id: 702,
            name: "Mire Clinic Network".to_string(),
            home_district: primary_home,
            home_districts,
            influence: 0.36,
            resources: ResourceState {
                credits: 64_000,
                water_access: 0.38,
                data_access: 0.52,
                muscle: 0.24,
                medical_supply: 0.93,
            },
            territory_claims: districts
                .iter()
                .map(|district| TerritoryClaim {
                    district: district.id,
                    strength: (0.25 + (1.0 - district.pollution) * 0.2).clamp(0.0, 1.0),
                    reason: "hidden treatment rooms and supply corridors".to_string(),
                })
                .collect(),
            allies: vec![700],
            enemies: vec![701],
            secrets: vec![stable_u64(seed, 702)],
            visual_identity: FactionVisualProfile {
                primary_color: [0.05, 0.95, 0.68],
                symbol: "split medical cross".to_string(),
                material_preference: MATERIAL_HUMAN_SKIN,
            },
        },
    ]
}

fn build_story_seeds(
    seed: u64,
    districts: &[CityDistrict],
    factions: &[FactionSeed],
    chunks: &[WorldChunk],
) -> Vec<StorySeed> {
    districts
        .iter()
        .zip(chunks.iter())
        .map(|(district, chunk)| {
            let involved_factions = factions
                .iter()
                .take(2)
                .map(|faction| faction.faction_id)
                .collect::<Vec<_>>();
            let physical_hooks = chunk
                .entities
                .iter()
                .filter(|template| has_tag(&template.tags, "destructible"))
                .filter_map(|template| template.entity_id)
                .collect::<Vec<_>>();
            StorySeed {
                seed_id: stable_u64(seed, district.id + 0x570),
                label: format!("{} water-and-data leverage", district.name),
                location: chunk
                    .local_navigation
                    .important_locations
                    .get(1)
                    .copied()
                    .unwrap_or(ALLEY_LOCATION_ID),
                involved_factions,
                involved_npcs: chunk.active_npc_seeds.clone(),
                secret_refs: vec![stable_u64(seed, district.id + 0x5EC)],
                physical_hooks,
                trigger_conditions: chunk
                    .local_infrastructure
                    .first()
                    .copied()
                    .map(TriggerCondition::InfrastructureDamaged)
                    .into_iter()
                    .collect(),
                consequence_tags: vec![
                    "faction_conflict".to_string(),
                    "resource_pressure".to_string(),
                    "physical_hook".to_string(),
                ],
                tags: vec![
                    "story_seed".to_string(),
                    "faction".to_string(),
                    district.kind.label().replace(' ', "_"),
                ],
            }
        })
        .collect()
}

fn collect_material_assignments(spawn_sets: &[EntitySpawnSet]) -> Vec<MaterialAssignment> {
    let mut assignments = BTreeSet::new();
    for template in spawn_sets.iter().flatten() {
        let Some(entity) = template.entity_id else {
            continue;
        };
        if let Some(renderable) = &template.renderable {
            assignments.insert((entity, renderable.material));
        }
        if let Some(body) = &template.physical_body {
            assignments.insert((entity, body.material_id));
        }
    }
    assignments.into_iter().collect()
}

fn collect_streaming_dependencies(entities: &[EntityTemplate]) -> Vec<AssetId> {
    let mut dependencies = BTreeSet::new();
    for template in entities {
        if let Some(renderable) = &template.renderable {
            dependencies.insert(renderable.mesh.0);
        }
        if has_tag(&template.tags, "destructible") {
            dependencies.insert(ALLEY_GLASS_FRACTURED_MESH_ASSET);
        }
    }
    dependencies.into_iter().collect()
}

fn calculate_budget_usage(
    template: &WorldTemplate,
    target_platform_budget: PlatformBudget,
) -> WorldBudgetUsage {
    let entity_count = template.spawn_sets.iter().map(Vec::len).sum();
    let unique_material_count = template
        .material_assignments
        .iter()
        .map(|(_, material)| *material)
        .collect::<BTreeSet<_>>()
        .len();
    let mut dynamic_light_count = 0;
    let mut physics_active_object_count = 0;
    let mut destructible_count = 0;
    let mut liquid_zones = 0;
    let mut npc_count = 0;
    for template in template.spawn_sets.iter().flatten() {
        if has_tag(&template.tags, "light") || has_tag(&template.tags, "neon") {
            dynamic_light_count += 1;
        }
        if template
            .physical_body
            .as_ref()
            .is_some_and(|body| body.dynamic || body.fragile)
            || template
                .tags
                .iter()
                .any(|tag| is_physics_activation_tag(tag))
        {
            physics_active_object_count += 1;
        }
        if has_tag(&template.tags, "destructible") {
            destructible_count += 1;
        }
        if has_tag(&template.tags, "water_leak") {
            liquid_zones += 1;
        }
        if has_tag(&template.tags, "npc") {
            npc_count += 1;
        }
    }
    let streaming_dependencies = template
        .chunks
        .iter()
        .flat_map(|chunk| chunk.streaming_dependencies.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let streaming_dependency_count = streaming_dependencies.len();
    let streaming_memory_bytes = streaming_dependencies
        .iter()
        .map(|asset_id| estimated_streaming_asset_bytes(*asset_id))
        .sum();
    let crowd_spawn_rule_count = template
        .chunks
        .iter()
        .map(|chunk| chunk.population.crowd_spawn_rules.len())
        .sum();
    let traffic_rule_count = template
        .chunks
        .iter()
        .map(|chunk| chunk.population.traffic_rules.len())
        .sum();
    let expected_background_crowd_count = template
        .chunks
        .iter()
        .map(|chunk| chunk.population.expected_background_crowd)
        .sum();
    let expected_vehicle_or_transit_count = template
        .chunks
        .iter()
        .map(|chunk| chunk.population.expected_vehicle_or_transit_count)
        .sum();

    WorldBudgetUsage {
        entity_count,
        unique_material_count,
        dynamic_light_count,
        physics_active_object_count,
        destructible_count,
        liquid_zones,
        npc_count,
        crowd_spawn_rule_count,
        traffic_rule_count,
        expected_background_crowd_count,
        expected_vehicle_or_transit_count,
        streaming_dependency_count,
        streaming_memory_bytes,
        target_platform_budget,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorldBudgetLimits {
    max_unique_materials: usize,
    max_dynamic_lights: usize,
    max_physics_active_objects: usize,
    max_destructibles: usize,
    max_liquid_zones: usize,
    max_npcs: usize,
    max_background_crowd: usize,
    max_vehicle_or_transit: usize,
    max_streaming_dependencies: usize,
    max_streaming_memory_bytes: u64,
}

fn world_budget_limits(target_platform_budget: PlatformBudget) -> Option<WorldBudgetLimits> {
    if target_platform_budget == 0 {
        return None;
    }
    let units = target_platform_budget as usize;
    Some(WorldBudgetLimits {
        max_unique_materials: (units / 4).max(1),
        max_dynamic_lights: (units / 8).max(1),
        max_physics_active_objects: (units / 2).max(1),
        max_destructibles: (units / 4).max(1),
        max_liquid_zones: (units / 12).max(1),
        max_npcs: (units / 8).max(1),
        max_background_crowd: units.saturating_mul(4).max(16),
        max_vehicle_or_transit: (units / 2).max(2),
        max_streaming_dependencies: (units / 4).max(1),
        max_streaming_memory_bytes: target_platform_budget.saturating_mul(4 * 1024 * 1024),
    })
}

fn is_physics_activation_tag(tag: &str) -> bool {
    matches!(tag, "destructible" | "hazard" | "water_leak") || tag.starts_with("audio_occluder:")
}

fn estimated_streaming_asset_bytes(asset_id: AssetId) -> u64 {
    if is_worldgen_city_cell_asset(asset_id) {
        return WORLDGEN_CITY_CELL_ASSET_BYTES;
    }
    if is_worldgen_material_cache_asset(asset_id) {
        return WORLDGEN_MATERIAL_CACHE_ASSET_BYTES;
    }
    match asset_id {
        ALLEY_GLASS_FRACTURED_MESH_ASSET => 16 * 1024 * 1024,
        ALLEY_MARA_HUMAN_BUNDLE_ASSET => 24 * 1024 * 1024,
        ALLEY_GLASS_INTACT_MESH_ASSET
        | ALLEY_SERVICE_PIPE_MESH_ASSET
        | ALLEY_WET_ASPHALT_MESH_ASSET => 8 * 1024 * 1024,
        ALLEY_NEON_SIGN_MESH_ASSET => 4 * 1024 * 1024,
        ALLEY_SECURITY_CAMERA_MESH_ASSET => 2 * 1024 * 1024,
        ALLEY_SERVICE_DOOR_MESH_ASSET | ALLEY_MARKET_STALL_MESH_ASSET => 3 * 1024 * 1024,
        _ => 6 * 1024 * 1024,
    }
}

fn find_infrastructure_node(
    graphs: &InfrastructureGraphSet,
    node_id: InfrastructureLinkId,
) -> Option<(
    InfrastructureSystem,
    &InfrastructureGraph,
    &InfrastructureNode,
)> {
    for (system, graph) in [
        (InfrastructureSystem::Power, &graphs.power),
        (InfrastructureSystem::Water, &graphs.water),
        (InfrastructureSystem::Data, &graphs.data),
        (InfrastructureSystem::Surveillance, &graphs.surveillance),
        (InfrastructureSystem::Transit, &graphs.transit),
        (InfrastructureSystem::Drainage, &graphs.drainage),
    ] {
        if let Some(node) = graph.nodes.iter().find(|node| node.id == node_id) {
            return Some((system, graph, node));
        }
    }

    None
}

fn collect_affected_infrastructure_nodes(
    graph: &InfrastructureGraph,
    damaged_node: InfrastructureLinkId,
    severity: f32,
) -> Vec<InfrastructureLinkId> {
    let max_hops = if severity >= 0.85 {
        2
    } else if severity >= 0.35 {
        1
    } else {
        0
    };
    let mut affected = BTreeSet::from([damaged_node]);
    let mut frontier = BTreeSet::from([damaged_node]);

    for _ in 0..max_hops {
        let mut next = BTreeSet::new();
        for node in &frontier {
            for edge in &graph.edges {
                if !edge.vulnerable {
                    continue;
                }
                if edge.from == *node {
                    next.insert(edge.to);
                }
                if edge.to == *node {
                    next.insert(edge.from);
                }
            }
        }
        next.retain(|node| !affected.contains(node));
        if next.is_empty() {
            break;
        }
        affected.extend(next.iter().copied());
        frontier = next;
    }

    affected.into_iter().collect()
}

fn affected_chunks_for_nodes(
    template: &WorldTemplate,
    affected_nodes: &[InfrastructureLinkId],
) -> Vec<WorldChunkId> {
    let affected = affected_nodes.iter().copied().collect::<BTreeSet<_>>();
    template
        .chunks
        .iter()
        .filter(|chunk| {
            chunk
                .local_infrastructure
                .iter()
                .any(|node| affected.contains(node))
        })
        .map(|chunk| chunk.chunk_id)
        .collect()
}

fn affected_districts_for_chunks(
    template: &WorldTemplate,
    affected_chunks: &[WorldChunkId],
) -> Vec<DistrictId> {
    let affected = affected_chunks.iter().copied().collect::<BTreeSet<_>>();
    template
        .chunks
        .iter()
        .filter(|chunk| affected.contains(&chunk.chunk_id))
        .map(|chunk| chunk.district_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn story_seeds_for_infrastructure_damage(
    template: &WorldTemplate,
    affected_nodes: &[InfrastructureLinkId],
) -> Vec<StorySeedId> {
    let affected = affected_nodes.iter().copied().collect::<BTreeSet<_>>();
    template
        .story_seeds
        .iter()
        .chain(
            template
                .chunks
                .iter()
                .flat_map(|chunk| chunk.local_story_hooks.iter()),
        )
        .filter(|seed| {
            seed.trigger_conditions.iter().any(|condition| {
                matches!(
                    condition,
                    TriggerCondition::InfrastructureDamaged(node) if affected.contains(node)
                )
            })
        })
        .map(|seed| seed.seed_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn faction_pressure_for_infrastructure_damage(
    template: &WorldTemplate,
    system: InfrastructureSystem,
    affected_districts: &[DistrictId],
    severity: f32,
) -> Vec<FactionPressureDelta> {
    let affected = affected_districts.iter().copied().collect::<BTreeSet<_>>();
    let resource_weight = match system {
        InfrastructureSystem::Power => 0.9,
        InfrastructureSystem::Water | InfrastructureSystem::Drainage => 1.0,
        InfrastructureSystem::Data | InfrastructureSystem::Surveillance => 0.75,
        InfrastructureSystem::Transit => 0.55,
    };

    let mut deltas = Vec::new();
    for faction in &template.factions {
        for claim in &faction.territory_claims {
            if !affected.contains(&claim.district) {
                continue;
            }
            deltas.push(FactionPressureDelta {
                faction_id: faction.faction_id,
                district: claim.district,
                alertness_delta: (severity * claim.strength * 0.35).clamp(0.0, 1.0),
                resource_pressure_delta: (severity * claim.strength * resource_weight)
                    .clamp(0.0, 1.0),
                territory_strength: claim.strength,
                reason: infrastructure_pressure_reason(system),
            });
        }
    }
    deltas.sort_by(|left, right| {
        left.district
            .cmp(&right.district)
            .then(left.faction_id.cmp(&right.faction_id))
    });
    deltas
}

fn expected_events_for_infrastructure_damage(
    system: InfrastructureSystem,
    node: &InfrastructureNode,
    severity: f32,
) -> Vec<WorldEventKind> {
    let mut events = match system {
        InfrastructureSystem::Power => {
            vec![WorldEventKind::PowerTransformerOverheated { entity: node.id }]
        }
        InfrastructureSystem::Water => vec![WorldEventKind::StreetFlooded],
        InfrastructureSystem::Data => vec![WorldEventKind::Custom(format!(
            "data relay {} compromised",
            node.id
        ))],
        InfrastructureSystem::Surveillance => vec![WorldEventKind::Custom(format!(
            "surveillance blind spot around node {}",
            node.id
        ))],
        InfrastructureSystem::Transit => vec![WorldEventKind::Custom(format!(
            "transit route disrupted at node {}",
            node.id
        ))],
        InfrastructureSystem::Drainage => vec![WorldEventKind::StreetFlooded],
    };

    if severity >= 0.9 {
        events.push(WorldEventKind::Custom(format!(
            "{} cascade risk at infrastructure node {}",
            infrastructure_system_label(system),
            node.id
        )));
    }
    events
}

fn consequence_tags_for_infrastructure_damage(
    system: InfrastructureSystem,
    severity: f32,
) -> TagSet {
    let mut tags = vec![
        "infrastructure".to_string(),
        infrastructure_system_label(system).to_string(),
    ];
    if severity >= 0.75 {
        tags.push("district_wide".to_string());
    }
    match system {
        InfrastructureSystem::Power => tags.push("blackout_risk".to_string()),
        InfrastructureSystem::Water | InfrastructureSystem::Drainage => {
            tags.push("flooding".to_string())
        }
        InfrastructureSystem::Data | InfrastructureSystem::Surveillance => {
            tags.push("security_gap".to_string())
        }
        InfrastructureSystem::Transit => tags.push("movement_disruption".to_string()),
    }
    tags
}

fn infrastructure_pressure_reason(system: InfrastructureSystem) -> String {
    match system {
        InfrastructureSystem::Power => "power control and lighting reliability changed",
        InfrastructureSystem::Water => "water access and flood risk changed",
        InfrastructureSystem::Data => "data access and faction communication changed",
        InfrastructureSystem::Surveillance => "surveillance coverage and security leverage changed",
        InfrastructureSystem::Transit => "movement routes and patrol timing changed",
        InfrastructureSystem::Drainage => "drainage capacity and street flooding changed",
    }
    .to_string()
}

fn infrastructure_system_label(system: InfrastructureSystem) -> &'static str {
    match system {
        InfrastructureSystem::Power => "power",
        InfrastructureSystem::Water => "water",
        InfrastructureSystem::Data => "data",
        InfrastructureSystem::Surveillance => "surveillance",
        InfrastructureSystem::Transit => "transit",
        InfrastructureSystem::Drainage => "drainage",
    }
}

fn all_infrastructure_ids(graphs: &InfrastructureGraphSet) -> BTreeSet<InfrastructureLinkId> {
    [
        &graphs.power,
        &graphs.water,
        &graphs.data,
        &graphs.surveillance,
        &graphs.transit,
        &graphs.drainage,
    ]
    .into_iter()
    .flat_map(|graph| graph.nodes.iter().map(|node| node.id))
    .collect()
}

fn infrastructure_ids_for_district(seed: u64, district: DistrictId) -> Vec<InfrastructureLinkId> {
    (0..6)
        .map(|offset| stable_u64(seed, district * 10 + offset) & 0x000F_FFFF_FFFF_FFFF)
        .collect()
}

fn infrastructure_node(
    id: InfrastructureLinkId,
    location: LocationId,
    kind: InfrastructureNodeKind,
    tags: TagSet,
) -> InfrastructureNode {
    InfrastructureNode {
        id,
        location,
        kind,
        health: 1.0,
        tags,
    }
}

fn connect_infrastructure(graph: &mut InfrastructureGraph, kind: InfrastructureEdgeKind) {
    for pair in graph.nodes.windows(2) {
        graph.edges.push(InfrastructureEdge {
            from: pair[0].id,
            to: pair[1].id,
            kind,
            capacity: 1.0,
            vulnerable: true,
        });
    }
}

fn validation_issue(
    severity: ValidationSeverity,
    code: &'static str,
    message: &'static str,
) -> WorldValidationIssue {
    WorldValidationIssue {
        severity,
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn title_case(label: &str) -> String {
    label
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn quality_tag(kind: DistrictKind) -> &'static str {
    match kind {
        DistrictKind::CorporateCore | DistrictKind::ClinicDistrict => "hero_story_area",
        DistrictKind::RainAlleySlum | DistrictKind::BlackMarket => "normal_gameplay_area",
        DistrictKind::IndustrialDock => "background_industrial_area",
    }
}

fn has_tag(tags: &[String], expected: &str) -> bool {
    tags.iter().any(|tag| tag == expected)
}

fn stable_unit(seed: u64, salt: u64) -> f32 {
    (stable_u64(seed, salt) % 10_000) as f32 / 10_000.0
}

fn stable_u64(seed: u64, salt: u64) -> u64 {
    let mut value = seed
        .wrapping_add(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn worldgen_gpu_workload_from_frame(frame: &FrameContext) -> WorldgenGpuWorkload {
    let entity_count = frame
        .snapshot
        .names
        .len()
        .max(frame.snapshot.transforms.len());
    let renderable_count = frame.snapshot.renderables.len();
    let mut material_ids = BTreeSet::new();
    let mut streaming_dependencies = BTreeSet::new();

    for (_, renderable) in frame.snapshot.renderables.iter() {
        material_ids.insert(renderable.material);
        streaming_dependencies.insert(renderable.mesh.0);
    }
    for (_, body) in frame.snapshot.physical_bodies.iter() {
        material_ids.insert(body.material_id);
    }
    for (_, tags) in frame.snapshot.tags.iter() {
        if has_tag(tags, "destructible") {
            streaming_dependencies.insert(ALLEY_GLASS_FRACTURED_MESH_ASSET);
        }
    }

    let explicit_chunk_count = frame
        .snapshot
        .tags
        .iter()
        .filter(|(_, tags)| has_tag(tags, "worldgen_chunk"))
        .count();
    let infrastructure_hint_count = frame
        .snapshot
        .tags
        .iter()
        .filter(|(_, tags)| tags.iter().any(|tag| is_infrastructure_hint_tag(tag)))
        .count();
    let story_hook_count = frame
        .recent_events
        .len()
        .saturating_add(frame.snapshot.agents.len());

    WorldgenGpuWorkload {
        chunk_count: explicit_chunk_count.max(entity_count.div_ceil(16)).max(1),
        entity_count,
        renderable_count,
        material_count: material_ids.len().max(frame.snapshot.materials.len()),
        streaming_dependency_count: streaming_dependencies.len(),
        navigation_node_count: frame.snapshot.transforms.len().max(1),
        navigation_edge_count: frame.snapshot.transforms.len().saturating_sub(1),
        infrastructure_hint_count,
        story_hook_count,
        npc_count: frame.snapshot.agents.len(),
    }
}

fn is_infrastructure_hint_tag(tag: &str) -> bool {
    tag == "light"
        || tag == "neon"
        || tag == "hazard"
        || tag == "water_leak"
        || tag.starts_with("audio_zone:")
        || tag.starts_with("audio_occluder:")
}

fn worldgen_gpu_resource(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    kind: GpuResourceKind,
    byte_len: u64,
    lifetime: GpuResourceLifetime,
    bindless: bool,
) -> GpuResourceHandle {
    let desc = GpuResourceDesc::new(label, kind, byte_len)
        .owned_by(70)
        .with_lifetime(lifetime);
    graph.declare_resource(if bindless { desc.bindless() } else { desc })
}

fn worldgen_compute_pipeline_with_permutation(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    shader_key: &'static str,
    quality_tier: QualityTier,
    permutation: GpuShaderPermutation,
) -> ComputePipelineHandle {
    graph
        .create_compute_pipeline(
            GpuPipelineDesc::new(label, shader_key, quality_tier).with_permutation(permutation),
        )
        .handle
}

fn worldgen_shader_permutation(
    defines: impl IntoIterator<Item = impl Into<String>>,
) -> GpuShaderPermutation {
    GpuShaderPermutation::new(defines)
}

fn worldgen_resource_bytes(item_count: u64, bytes_per_item: u64) -> u64 {
    item_count
        .saturating_mul(bytes_per_item)
        .max(bytes_per_item)
}

fn estimate_worldgen_gpu_milliseconds(workload: &WorldgenGpuWorkload) -> f32 {
    if workload.entity_count == 0 {
        return 0.0;
    }
    (0.04
        + workload.chunk_count as f32 * 0.012
        + workload.navigation_node_count as f32 * 0.002
        + workload.infrastructure_hint_count as f32 * 0.01
        + workload.story_hook_count as f32 * 0.008
        + workload.streaming_dependency_count as f32 * 0.006)
        .min(0.65)
}

#[derive(Clone, Debug, Default, PartialEq)]
struct WorldgenGpuWorkload {
    chunk_count: usize,
    entity_count: usize,
    renderable_count: usize,
    material_count: usize,
    streaming_dependency_count: usize,
    navigation_node_count: usize,
    navigation_edge_count: usize,
    infrastructure_hint_count: usize,
    story_hook_count: usize,
    npc_count: usize,
}

#[derive(Default)]
pub struct CyberpunkWorldGenerationModule {
    last_workload: Option<WorldgenGpuWorkload>,
}

impl CyberpunkWorldGenerationModule {
    pub fn generate(&self, request: WorldGenerationRequest) -> WorldTemplate {
        generate_world_template(&request)
    }
}

impl EngineModule for CyberpunkWorldGenerationModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            module_id: 70,
            name: "cyberpunk_world_generation",
            schema: SchemaVersion {
                name: "WorldTemplate",
                version: 1,
            },
            default_quality: QualityTier::NormalRuntime,
        }
    }

    fn tick(&mut self, frame: &FrameContext, _out: &mut CommandSink) {
        self.last_workload = Some(worldgen_gpu_workload_from_frame(frame));
    }

    fn schedule_gpu(&mut self, graph: &mut GpuGraphBuilder) {
        let Some(workload) = self.last_workload.as_ref() else {
            return;
        };
        if workload.entity_count == 0 {
            return;
        }

        let chunk_count = u64::try_from(workload.chunk_count.max(1)).unwrap_or(u64::MAX);
        let entity_count = u64::try_from(workload.entity_count.max(1)).unwrap_or(u64::MAX);
        let renderable_count = u64::try_from(workload.renderable_count.max(1)).unwrap_or(u64::MAX);
        let material_count = u64::try_from(workload.material_count.max(1)).unwrap_or(u64::MAX);
        let nav_node_count =
            u64::try_from(workload.navigation_node_count.max(1)).unwrap_or(u64::MAX);
        let nav_edge_count =
            u64::try_from(workload.navigation_edge_count.max(1)).unwrap_or(u64::MAX);
        let infrastructure_hint_count =
            u64::try_from(workload.infrastructure_hint_count.max(1)).unwrap_or(u64::MAX);
        let story_hook_count = u64::try_from(workload.story_hook_count.max(1)).unwrap_or(u64::MAX);
        let streaming_dependency_count =
            u64::try_from(workload.streaming_dependency_count.max(1)).unwrap_or(u64::MAX);

        let chunk_metadata = worldgen_gpu_resource(
            graph,
            "worldgen chunk metadata buffer",
            GpuResourceKind::Buffer,
            worldgen_resource_bytes(chunk_count, 256),
            GpuResourceLifetime::Imported,
            true,
        );
        let entity_placement = worldgen_gpu_resource(
            graph,
            "worldgen entity placement buffer",
            GpuResourceKind::Buffer,
            worldgen_resource_bytes(entity_count, 128)
                .saturating_add(worldgen_resource_bytes(renderable_count, 64)),
            GpuResourceLifetime::Imported,
            false,
        );
        let material_palette = worldgen_gpu_resource(
            graph,
            "worldgen material palette buffer",
            GpuResourceKind::Buffer,
            worldgen_resource_bytes(material_count, 96),
            GpuResourceLifetime::Imported,
            true,
        );
        let visible_chunk_list = worldgen_gpu_resource(
            graph,
            "worldgen visible chunk list",
            GpuResourceKind::Buffer,
            worldgen_resource_bytes(chunk_count, 64),
            GpuResourceLifetime::Transient,
            false,
        );
        let chunk_visibility_pipeline = worldgen_compute_pipeline_with_permutation(
            graph,
            "worldgen_chunk_visibility",
            "worldgen/chunk_visibility.comp",
            QualityTier::BackgroundApproximation,
            worldgen_shader_permutation(["WORLD_STREAMING_CHUNKS", "LOD_HINTS"]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "worldgen_chunk_visibility",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(chunk_visibility_pipeline),
                QualityTier::BackgroundApproximation,
            )
            .reads([chunk_metadata, entity_placement, material_palette])
            .writes([visible_chunk_list]),
        );

        let navigation_nodes = worldgen_gpu_resource(
            graph,
            "worldgen navigation node buffer",
            GpuResourceKind::Buffer,
            worldgen_resource_bytes(nav_node_count, 96)
                .saturating_add(worldgen_resource_bytes(nav_edge_count, 64)),
            GpuResourceLifetime::Imported,
            false,
        );
        let navigation_validation = worldgen_gpu_resource(
            graph,
            "worldgen navigation validation output",
            GpuResourceKind::Buffer,
            worldgen_resource_bytes(nav_node_count, 32),
            GpuResourceLifetime::Transient,
            false,
        );
        let navigation_pipeline = worldgen_compute_pipeline_with_permutation(
            graph,
            "worldgen_navigation_validation",
            "worldgen/navigation_validation.comp",
            QualityTier::BackgroundApproximation,
            worldgen_shader_permutation(["REACHABILITY", "PLAYABILITY_VALIDATION"]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "worldgen_navigation_validation",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(navigation_pipeline),
                QualityTier::BackgroundApproximation,
            )
            .reads([chunk_metadata, navigation_nodes])
            .writes([navigation_validation]),
        );

        if workload.infrastructure_hint_count > 0 {
            let infrastructure_graph = worldgen_gpu_resource(
                graph,
                "worldgen infrastructure graph buffer",
                GpuResourceKind::Buffer,
                worldgen_resource_bytes(infrastructure_hint_count, 160),
                GpuResourceLifetime::Imported,
                false,
            );
            let infrastructure_output = worldgen_gpu_resource(
                graph,
                "worldgen infrastructure propagation output",
                GpuResourceKind::Buffer,
                worldgen_resource_bytes(infrastructure_hint_count, 96),
                GpuResourceLifetime::Transient,
                false,
            );
            let infrastructure_pipeline = worldgen_compute_pipeline_with_permutation(
                graph,
                "worldgen_infrastructure_propagation",
                "worldgen/infrastructure_propagation.comp",
                QualityTier::NormalRuntime,
                worldgen_shader_permutation(["INFRASTRUCTURE_GRAPH", "CONSEQUENCE_PROPAGATION"]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "worldgen_infrastructure_propagation",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(infrastructure_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([chunk_metadata, infrastructure_graph])
                .writes([infrastructure_output]),
            );
        }

        if workload.story_hook_count > 0 || workload.npc_count > 0 {
            let story_seed_buffer = worldgen_gpu_resource(
                graph,
                "worldgen story hook seed buffer",
                GpuResourceKind::Buffer,
                worldgen_resource_bytes(story_hook_count.max(1), 192),
                GpuResourceLifetime::Imported,
                false,
            );
            let story_hook_index = worldgen_gpu_resource(
                graph,
                "worldgen story hook index output",
                GpuResourceKind::Buffer,
                worldgen_resource_bytes(story_hook_count.max(1), 128),
                GpuResourceLifetime::Transient,
                false,
            );
            let story_pipeline = worldgen_compute_pipeline_with_permutation(
                graph,
                "worldgen_story_hook_indexing",
                "worldgen/story_hook_indexing.comp",
                QualityTier::BackgroundApproximation,
                worldgen_shader_permutation(["STORY_SEEDS", "FACTION_HOOKS", "PHYSICAL_HOOKS"]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "worldgen_story_hook_indexing",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(story_pipeline),
                    QualityTier::BackgroundApproximation,
                )
                .reads([chunk_metadata, story_seed_buffer, navigation_validation])
                .writes([story_hook_index]),
            );
        }

        if workload.streaming_dependency_count > 0 {
            let dependency_buffer = worldgen_gpu_resource(
                graph,
                "worldgen streaming dependency buffer",
                GpuResourceKind::Buffer,
                worldgen_resource_bytes(streaming_dependency_count, 64),
                GpuResourceLifetime::Imported,
                false,
            );
            let compacted_dependencies = worldgen_gpu_resource(
                graph,
                "worldgen compacted streaming dependencies",
                GpuResourceKind::Buffer,
                worldgen_resource_bytes(streaming_dependency_count, 48),
                GpuResourceLifetime::Transient,
                false,
            );
            let streaming_pipeline = worldgen_compute_pipeline_with_permutation(
                graph,
                "worldgen_streaming_dependency_compaction",
                "worldgen/streaming_dependency_compaction.comp",
                QualityTier::BackgroundApproximation,
                worldgen_shader_permutation(["STREAMING_DEPENDENCIES", "ASSET_DEDUP"]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "worldgen_streaming_dependency_compaction",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(streaming_pipeline),
                    QualityTier::BackgroundApproximation,
                )
                .reads([visible_chunk_list, dependency_buffer])
                .writes([compacted_dependencies]),
            );
        }
    }

    fn performance_counters(&self) -> PerformanceCounters {
        self.last_workload
            .as_ref()
            .map(|workload| PerformanceCounters {
                cpu_milliseconds: 0.05 + workload.entity_count as f32 * 0.002,
                gpu_milliseconds: estimate_worldgen_gpu_milliseconds(workload),
                memory_bytes: 12 * 1024 * 1024
                    + workload.entity_count as u64 * 128 * 1024
                    + workload.streaming_dependency_count as u64 * 64 * 1024,
            })
            .unwrap_or(PerformanceCounters {
                cpu_milliseconds: 0.05,
                gpu_milliseconds: 0.0,
                memory_bytes: 12 * 1024 * 1024,
            })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlleySceneSeed {
    pub materials: Vec<MaterialDescriptor>,
    pub entities: Vec<EntityTemplate>,
    pub assets: Vec<AssetRecord>,
    pub player: AlleyPlayerRuntimeSeed,
    pub ai: AlleyAiRuntimeSeed,
}

impl AlleySceneSeed {
    pub fn new() -> Self {
        Self {
            materials: default_alley_materials(),
            entities: build_alley_scene(),
            assets: build_alley_asset_records(),
            player: AlleyPlayerRuntimeSeed::default(),
            ai: AlleyAiRuntimeSeed::default(),
        }
    }

    pub fn build_world_state(&self) -> Result<WorldState, CommandError> {
        let mut world = WorldState::default();
        for material in &self.materials {
            world.register_material(material.clone());
        }
        for template in &self.entities {
            world.spawn_entity_template(template.clone())?;
        }
        Ok(world)
    }

    pub fn register_assets(&self, registry: &mut AssetRegistry) {
        for asset in &self.assets {
            registry.register_known(asset.clone());
        }
    }

    pub fn scripted_glass_break_command(&self) -> WorldCommand {
        WorldCommand::ApplyForce(self.player.scripted_glass_break_force)
    }
}

impl Default for AlleySceneSeed {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build_alley_scene_seed() -> AlleySceneSeed {
    AlleySceneSeed::new()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlleyAiRuntimeSeed {
    pub security_faction: FactionId,
    pub opposition_faction: FactionId,
    pub story_location: LocationId,
}

impl AlleyAiRuntimeSeed {
    pub const fn new(
        security_faction: FactionId,
        opposition_faction: FactionId,
        story_location: LocationId,
    ) -> Self {
        Self {
            security_faction,
            opposition_faction,
            story_location,
        }
    }
}

impl Default for AlleyAiRuntimeSeed {
    fn default() -> Self {
        Self::new(
            ALLEY_SECURITY_FACTION_ID,
            ALLEY_OPPOSITION_FACTION_ID,
            ALLEY_LOCATION_ID,
        )
    }
}

pub fn alley_ai_runtime_seed() -> AlleyAiRuntimeSeed {
    AlleyAiRuntimeSeed::default()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlleyPlayerRuntimeSeed {
    pub player_entity: EntityId,
    pub initial_camera_yaw_radians: f32,
    pub camera_eye_height_meters: f32,
    pub walkable_min: [f32; 2],
    pub walkable_max: [f32; 2],
    pub walk_speed_meters_per_second: f32,
    pub fast_walk_speed_meters_per_second: f32,
    pub primary_interaction: PrimaryInteractionSettings,
    pub scripted_glass_break_force: ForceCommand,
}

impl AlleyPlayerRuntimeSeed {
    pub fn new(player_entity: EntityId) -> Self {
        let primary_interaction = PrimaryInteractionSettings::new(player_entity);
        Self {
            player_entity,
            initial_camera_yaw_radians: ALLEY_CAMERA_INITIAL_YAW_RADIANS,
            camera_eye_height_meters: ALLEY_CAMERA_EYE_HEIGHT_METERS,
            walkable_min: ALLEY_WALKABLE_MIN,
            walkable_max: ALLEY_WALKABLE_MAX,
            walk_speed_meters_per_second: ALLEY_WALK_SPEED_METERS_PER_SECOND,
            fast_walk_speed_meters_per_second: ALLEY_FAST_WALK_SPEED_METERS_PER_SECOND,
            primary_interaction,
            scripted_glass_break_force: ForceCommand {
                entity: GLASS_WALL_ID,
                vector_newtons: Vec3::new(primary_interaction.force_newtons, 0.0, 0.0),
                impulse_newton_seconds: primary_interaction.impulse_seconds,
                source: Some(player_entity),
            },
        }
    }

    pub fn scripted_glass_break_command(self) -> WorldCommand {
        WorldCommand::ApplyForce(self.scripted_glass_break_force)
    }
}

impl Default for AlleyPlayerRuntimeSeed {
    fn default() -> Self {
        Self::new(PLAYER_ID)
    }
}

pub fn alley_player_runtime_seed() -> AlleyPlayerRuntimeSeed {
    AlleyPlayerRuntimeSeed::default()
}

pub fn alley_scripted_glass_break_command() -> WorldCommand {
    alley_player_runtime_seed().scripted_glass_break_command()
}

pub fn build_alley_asset_records() -> Vec<AssetRecord> {
    vec![
        alley_asset_record(
            ALLEY_GLASS_INTACT_MESH_ASSET,
            AssetKind::Mesh,
            "intact alley glass wall mesh",
            QualityTier::NormalRuntime,
            AssetLoadState::Resident,
            false,
            Vec::new(),
        ),
        alley_asset_record(
            ALLEY_GLASS_FRACTURED_MESH_ASSET,
            AssetKind::Mesh,
            "fractured alley glass wall mesh",
            QualityTier::NormalRuntime,
            AssetLoadState::Unloaded,
            false,
            vec![ALLEY_GLASS_INTACT_MESH_ASSET],
        ),
        alley_asset_record(
            ALLEY_MARA_HUMAN_BUNDLE_ASSET,
            AssetKind::GeneratedBundle,
            "Mara hero human bundle",
            QualityTier::HeroHighFidelityRuntime,
            AssetLoadState::Resident,
            true,
            Vec::new(),
        ),
        alley_asset_record(
            ALLEY_NEON_SIGN_MESH_ASSET,
            AssetKind::Mesh,
            "faulty magenta neon sign mesh",
            QualityTier::NormalRuntime,
            AssetLoadState::Resident,
            false,
            Vec::new(),
        ),
        alley_asset_record(
            ALLEY_SERVICE_PIPE_MESH_ASSET,
            AssetKind::Mesh,
            "leaking service pipe mesh",
            QualityTier::NormalRuntime,
            AssetLoadState::Resident,
            false,
            Vec::new(),
        ),
        alley_asset_record(
            ALLEY_WET_ASPHALT_MESH_ASSET,
            AssetKind::Mesh,
            "rain slick alley asphalt mesh",
            QualityTier::NormalRuntime,
            AssetLoadState::Resident,
            false,
            Vec::new(),
        ),
        alley_asset_record(
            ALLEY_SECURITY_CAMERA_MESH_ASSET,
            AssetKind::Mesh,
            "wall mounted surveillance camera mesh",
            QualityTier::NormalRuntime,
            AssetLoadState::Resident,
            false,
            Vec::new(),
        ),
        alley_asset_record(
            ALLEY_SERVICE_DOOR_MESH_ASSET,
            AssetKind::Mesh,
            "locked utility service door mesh",
            QualityTier::NormalRuntime,
            AssetLoadState::Resident,
            false,
            Vec::new(),
        ),
        alley_asset_record(
            ALLEY_MARKET_STALL_MESH_ASSET,
            AssetKind::Mesh,
            "covered black market utility stall mesh",
            QualityTier::NormalRuntime,
            AssetLoadState::Resident,
            false,
            Vec::new(),
        ),
    ]
}

fn alley_asset_record(
    id: AssetId,
    kind: AssetKind,
    label: &'static str,
    quality_tier: QualityTier,
    load_state: AssetLoadState,
    generated: bool,
    dependencies: Vec<AssetId>,
) -> AssetRecord {
    AssetRecord {
        id,
        kind,
        label: label.to_string(),
        provenance: "ashfall vertical slice seed".to_string(),
        dependencies,
        byte_len: Some(estimated_streaming_asset_bytes(id)),
        generated,
        quality_tier,
        load_state,
    }
}

pub fn build_alley_scene() -> Vec<EntityTemplate> {
    vec![
        EntityTemplate {
            entity_id: Some(PLAYER_ID),
            name: "player proxy".to_string(),
            transform: Transform::at(Vec3::new(0.0, 0.0, 0.0)),
            renderable: None,
            physical_body: Some(PhysicalBody {
                mass_kg: 85.0,
                material_id: MATERIAL_HUMAN_SKIN,
                dynamic: true,
                fragile: false,
            }),
            material_state: None,
            human: None,
            agent: None,
            tags: vec!["player".to_string()],
        },
        EntityTemplate {
            entity_id: Some(GLASS_WALL_ID),
            name: "alley glass wall".to_string(),
            transform: Transform::at(Vec3::new(2.0, 0.0, 0.0)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(ALLEY_GLASS_INTACT_MESH_ASSET),
                material: MATERIAL_GLASS,
                visible: true,
                fracture_replacement_mesh: Some(MeshAssetHandle(ALLEY_GLASS_FRACTURED_MESH_ASSET)),
            }),
            physical_body: Some(PhysicalBody {
                mass_kg: 140.0,
                material_id: MATERIAL_GLASS,
                dynamic: false,
                fragile: true,
            }),
            material_state: Some(MaterialState::default()),
            human: None,
            agent: None,
            tags: vec![
                "glass".to_string(),
                "destructible".to_string(),
                "wall".to_string(),
                "audio_occluder:glass".to_string(),
            ],
        },
        EntityTemplate {
            entity_id: Some(NPC_MARA_ID),
            name: "Mara".to_string(),
            transform: Transform::at(Vec3::new(5.5, 0.0, 0.0)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(ALLEY_MARA_HUMAN_BUNDLE_ASSET),
                material: MATERIAL_HUMAN_SKIN,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: Some(PhysicalBody {
                mass_kg: 68.0,
                material_id: MATERIAL_HUMAN_SKIN,
                dynamic: true,
                fragile: false,
            }),
            material_state: Some(MaterialState {
                moisture: 0.42,
                soot: 0.08,
                temperature: 300.0,
                ..MaterialState::default()
            }),
            human: Some(HumanState {
                human_id: 300,
                quality_tier: QualityTier::HeroHighFidelityRuntime,
            }),
            agent: Some(AgentState {
                persona: 300,
                emotional_state: EmotionState::default(),
                ai_lod: QualityTier::NormalRuntime,
            }),
            tags: vec!["npc".to_string(), "witness".to_string()],
        },
        EntityTemplate {
            entity_id: Some(NEON_SIGN_ID),
            name: "faulty magenta neon sign".to_string(),
            transform: Transform::at(Vec3::new(1.0, 3.0, 2.5)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(ALLEY_NEON_SIGN_MESH_ASSET),
                material: MATERIAL_NEON_TUBE,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: Some(PhysicalBody {
                mass_kg: 20.0,
                material_id: MATERIAL_NEON_TUBE,
                dynamic: false,
                fragile: true,
            }),
            material_state: Some(MaterialState {
                electrical_charge: 120.0,
                ..MaterialState::default()
            }),
            human: None,
            agent: None,
            tags: vec!["light".to_string(), "neon".to_string()],
        },
        EntityTemplate {
            entity_id: Some(WATER_LEAK_ID),
            name: "leaking service pipe".to_string(),
            transform: Transform::at(Vec3::new(-1.5, 0.5, 0.4)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(ALLEY_SERVICE_PIPE_MESH_ASSET),
                material: MATERIAL_WATER,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: Some(PhysicalBody {
                mass_kg: 12.0,
                material_id: MATERIAL_WET_ASPHALT,
                dynamic: false,
                fragile: false,
            }),
            material_state: Some(MaterialState {
                moisture: 1.0,
                ..MaterialState::default()
            }),
            human: None,
            agent: None,
            tags: vec![
                "water_leak".to_string(),
                "hazard".to_string(),
                "steam_leak".to_string(),
            ],
        },
        EntityTemplate {
            entity_id: Some(ALLEY_ASPHALT_ID),
            name: "rain slick alley asphalt".to_string(),
            transform: Transform::at(Vec3::new(-0.8, 0.0, -0.02)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(ALLEY_WET_ASPHALT_MESH_ASSET),
                material: MATERIAL_WET_ASPHALT,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: Some(PhysicalBody {
                mass_kg: 10_000.0,
                material_id: MATERIAL_WET_ASPHALT,
                dynamic: false,
                fragile: false,
            }),
            material_state: Some(MaterialState {
                moisture: 0.25,
                ..MaterialState::default()
            }),
            human: None,
            agent: None,
            tags: vec![
                "ground".to_string(),
                "asphalt".to_string(),
                "wettable".to_string(),
            ],
        },
        EntityTemplate {
            entity_id: Some(ALLEY_SECURITY_CAMERA_ID),
            name: "wall mounted surveillance camera".to_string(),
            transform: Transform::at(Vec3::new(-4.72, -4.35, 2.36)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(ALLEY_SECURITY_CAMERA_MESH_ASSET),
                material: MATERIAL_GLASS,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: Some(PhysicalBody {
                mass_kg: 8.5,
                material_id: MATERIAL_GLASS,
                dynamic: false,
                fragile: true,
            }),
            material_state: Some(MaterialState {
                moisture: 0.28,
                soot: 0.18,
                corrosion: 0.22,
                electrical_charge: 64.0,
                ..MaterialState::default()
            }),
            human: None,
            agent: None,
            tags: vec![
                "camera".to_string(),
                "security".to_string(),
                "surveillance".to_string(),
                "hackable".to_string(),
            ],
        },
        EntityTemplate {
            entity_id: Some(ALLEY_SERVICE_DOOR_ID),
            name: "locked utility service door".to_string(),
            transform: Transform::at(Vec3::new(-5.05, 6.65, 0.08)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(ALLEY_SERVICE_DOOR_MESH_ASSET),
                material: MATERIAL_WET_ASPHALT,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: Some(PhysicalBody {
                mass_kg: 240.0,
                material_id: MATERIAL_WET_ASPHALT,
                dynamic: false,
                fragile: false,
            }),
            material_state: Some(MaterialState {
                moisture: 0.34,
                soot: 0.44,
                corrosion: 0.58,
                crack_density: 0.16,
                electrical_charge: 18.0,
                ..MaterialState::default()
            }),
            human: None,
            agent: None,
            tags: vec![
                "door".to_string(),
                "security_door".to_string(),
                "lockable".to_string(),
                "infrastructure".to_string(),
                "cover".to_string(),
            ],
        },
        EntityTemplate {
            entity_id: Some(ALLEY_MARKET_STALL_ID),
            name: "covered black market utility stall".to_string(),
            transform: Transform::at(Vec3::new(3.62, -4.85, 0.06)),
            renderable: Some(Renderable {
                mesh: MeshAssetHandle(ALLEY_MARKET_STALL_MESH_ASSET),
                material: MATERIAL_NEON_TUBE,
                visible: true,
                fracture_replacement_mesh: None,
            }),
            physical_body: Some(PhysicalBody {
                mass_kg: 92.0,
                material_id: MATERIAL_WET_ASPHALT,
                dynamic: false,
                fragile: false,
            }),
            material_state: Some(MaterialState {
                moisture: 0.48,
                soot: 0.26,
                corrosion: 0.31,
                crack_density: 0.08,
                electrical_charge: 32.0,
                ..MaterialState::default()
            }),
            human: None,
            agent: None,
            tags: vec![
                "market".to_string(),
                "stall".to_string(),
                "cover".to_string(),
                "story_space".to_string(),
            ],
        },
        EntityTemplate {
            entity_id: Some(ALLEY_ACOUSTIC_ZONE_ID),
            name: "rain alley acoustic zone".to_string(),
            transform: Transform {
                translation_meters: Vec3::new(1.8, 0.0, 1.2),
                scale: Vec3::new(12.0, 5.0, 4.0),
                ..Transform::default()
            },
            renderable: None,
            physical_body: None,
            material_state: None,
            human: None,
            agent: None,
            tags: vec![
                "audio_zone:alley".to_string(),
                "rain alley slum".to_string(),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ashfall_core::world::{CommandSink, WorldState};

    fn request_with_budget(target_platform_budget: PlatformBudget) -> WorldGenerationRequest {
        WorldGenerationRequest {
            request_id: 42,
            seed: 12_345,
            city_profile: "humid corporate-fringe arcology".to_string(),
            district_requests: vec![
                "rain alley slum".to_string(),
                "black market".to_string(),
                "clinic district".to_string(),
            ],
            target_platform_budget,
            story_requirements: vec!["physical hooks".to_string(), "faction pressure".to_string()],
            material_palette: 900,
            faction_templates: vec![
                "security group".to_string(),
                "hacker collective".to_string(),
                "clinic network".to_string(),
            ],
            validation_level: QualityTier::NormalRuntime,
        }
    }

    #[test]
    fn deterministic_generation_produces_valid_streaming_chunks() {
        let generator = CyberpunkWorldGenerationModule::default();
        let request = request_with_budget(64);

        let first = generator.generate(request.clone());
        let second = generator.generate(request);

        assert_eq!(first, second);
        assert!(first.validation_report.passed);
        assert_eq!(first.districts.len(), 3);
        assert_eq!(first.chunks.len(), 3);
        assert_eq!(first.city_cell_packages.len(), first.chunks.len());
        assert_eq!(first.factions.len(), 3);
        assert_eq!(first.materials.len(), first.districts.len() * 5);
        let generated_asset_records = build_world_template_asset_records(&first);
        let generated_asset_ids = generated_asset_records
            .iter()
            .map(|asset| asset.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(generated_asset_ids.len(), generated_asset_records.len());
        assert_eq!(
            generated_asset_records
                .iter()
                .filter(|asset| asset.kind == AssetKind::MaterialGraph)
                .count(),
            first.materials.len()
        );
        assert_eq!(
            generated_asset_records
                .iter()
                .filter(|asset| is_worldgen_material_cache_asset(asset.id))
                .count(),
            first
                .chunks
                .iter()
                .flat_map(|chunk| chunk.streaming_dependencies.iter().copied())
                .filter(|asset| is_worldgen_material_cache_asset(*asset))
                .collect::<BTreeSet<_>>()
                .len()
        );
        assert_eq!(
            generated_asset_records
                .iter()
                .filter(|asset| is_worldgen_city_cell_asset(asset.id))
                .count(),
            first.chunks.len()
        );
        assert!(generated_asset_records.iter().all(|asset| asset.generated));
        assert!(
            generated_asset_records
                .iter()
                .filter(|asset| is_worldgen_material_cache_asset(asset.id))
                .all(|asset| {
                    asset.kind == AssetKind::Texture
                        && asset.dependencies.len() == 1
                        && generated_asset_ids.contains(&asset.dependencies[0])
                })
        );
        assert!(generated_asset_records.iter().all(|asset| {
            asset
                .dependencies
                .iter()
                .filter(|dependency| is_worldgen_material_cache_asset(**dependency))
                .all(|dependency| generated_asset_ids.contains(dependency))
        }));
        assert!(first.navigation_data.all_important_locations_reachable());
        assert_eq!(
            first.infrastructure.water.nodes.len(),
            first.districts.len()
        );
        assert_eq!(
            first.infrastructure.data.edges.len(),
            first.districts.len() - 1
        );
        assert!(first.validation_report.budget.entity_count <= 64);
        assert!(first.validation_report.budget.npc_count >= 3);
        assert!(first.validation_report.budget.unique_material_count <= 16);
        assert!(first.validation_report.budget.dynamic_light_count <= 8);
        assert!(first.validation_report.budget.physics_active_object_count <= 32);
        assert!(first.validation_report.budget.streaming_dependency_count <= 16);
        assert!(first.validation_report.budget.streaming_memory_bytes <= 256 * 1024 * 1024);
        assert!(first.validation_report.budget.crowd_spawn_rule_count >= first.chunks.len() * 2);
        assert!(first.validation_report.budget.traffic_rule_count >= first.chunks.len());
        assert!(
            first
                .validation_report
                .budget
                .expected_background_crowd_count
                > 96
        );
        assert!(
            first
                .validation_report
                .budget
                .expected_vehicle_or_transit_count
                > 8
        );

        for chunk in &first.chunks {
            let district = first
                .districts
                .iter()
                .find(|district| district.id == chunk.district_id)
                .expect("chunk should reference a generated district");
            let glass_variant =
                district_material_id(district.id, chunk.material_palette, MATERIAL_GLASS);
            let wet_road_variant =
                district_material_id(district.id, chunk.material_palette, MATERIAL_WET_ASPHALT);
            assert!(first.materials.iter().any(|material| {
                material.id == glass_variant
                    && material.name.starts_with(&district.name)
                    && material.procedural_source.is_some()
            }));
            assert!(first.materials.iter().any(|material| {
                material.id == wet_road_variant
                    && material.visual.roughness != 0.18
                    && material.acoustic.wetness_muffle > 0.4
            }));
            assert!(!chunk.streaming_dependencies.is_empty());
            assert!(
                chunk
                    .streaming_dependencies
                    .contains(&ALLEY_GLASS_FRACTURED_MESH_ASSET)
            );
            let material_cache_dependencies =
                district_material_cache_dependencies(district, chunk.material_palette);
            assert_eq!(
                chunk
                    .streaming_dependencies
                    .iter()
                    .filter(|asset| is_worldgen_material_cache_asset(**asset))
                    .count(),
                material_cache_dependencies.len()
            );
            assert!(material_cache_dependencies.iter().all(|asset| {
                chunk.streaming_dependencies.contains(asset)
                    && is_worldgen_material_cache_asset(*asset)
            }));
            assert!(chunk.population.validation_report.passed);
            assert!(validate_population_summary(&chunk.population_summary).passed);
            assert!(!chunk.population.crowd_spawn_rules.is_empty());
            assert!(!chunk.population.traffic_rules.is_empty());
            assert!(chunk.population.expected_background_crowd > 0);
            assert!(chunk.population.expected_vehicle_or_transit_count > 0);
            assert_eq!(
                chunk.population_summary.expected_active_npcs as usize,
                chunk.population.expected_active_npcs
            );
            assert_eq!(
                chunk.population_summary.expected_background_crowd as usize,
                chunk.population.expected_background_crowd
            );
            assert_eq!(
                chunk.population_summary.expected_vehicle_or_transit_count as usize,
                chunk.population.expected_vehicle_or_transit_count
            );
            assert_eq!(
                chunk.population_summary.crowd_spawn_rule_count as usize,
                chunk.population.crowd_spawn_rules.len()
            );
            assert_eq!(
                chunk.population_summary.traffic_rule_count as usize,
                chunk.population.traffic_rules.len()
            );
            assert!(chunk.population_summary.normalized_activity() > 0.0);
            assert!(chunk.population.crowd_spawn_rules.iter().all(
                |rule| !rule.schedule.is_empty() && rule.density > 0.0 && rule.density <= 1.0
            ));
            assert!(chunk
                .population
                .traffic_rules
                .iter()
                .all(|rule| !rule.crossing_locations.is_empty()
                    && rule.average_speed_mps > 0.0));
            let package = first
                .city_cell_packages
                .iter()
                .find(|package| package.cell_id == chunk.chunk_id)
                .expect("chunk should have a v4 CityCellPackage");
            assert_eq!(package.schema_version, CITY_CELL_PACKAGE_SCHEMA_VERSION);
            assert_eq!(
                package.package_asset,
                worldgen_city_cell_asset_id(chunk.chunk_id)
            );
            assert_eq!(package.bounds, chunk.bounds);
            assert_eq!(package.district_id, chunk.district_id);
            assert_eq!(package.infrastructure_state, chunk.local_infrastructure);
            assert_eq!(package.population, chunk.population);
            assert_eq!(package.population_summary, chunk.population_summary);
            assert!(validate_population_summary(&package.population_summary).passed);
            assert_eq!(package.quality_tier, chunk.quality_tier);
            assert_eq!(
                package.material_packages.len(),
                material_cache_dependencies.len()
            );
            assert_eq!(
                package.material_package_descriptors.len(),
                package.material_packages.len()
            );
            assert!(
                package
                    .material_package_descriptors
                    .iter()
                    .all(|descriptor| {
                        descriptor.schema_version == CITY_MATERIAL_PACKAGE_SCHEMA_VERSION
                            && package
                                .material_packages
                                .contains(&descriptor.package_asset)
                            && descriptor.material_graph.is_some()
                            && descriptor.validation_passed
                            && descriptor.capture_confidence >= 0.65
                            && descriptor.calibrated_photo_count > 0
                            && descriptor.scan_set_count > 0
                            && !descriptor.state_response_channels.is_empty()
                            && descriptor.generated_cache_texture_count > 0
                            && descriptor.generated_virtual_page_count > 0
                            && descriptor.generated_cache_bytes > 0
                            && descriptor.source_summary.contains("calibrated photo")
                    })
            );
            assert!(
                package
                    .geometry_chunks
                    .iter()
                    .all(|asset| !is_worldgen_material_cache_asset(*asset))
            );
            assert!(
                package
                    .material_packages
                    .iter()
                    .all(|asset| is_worldgen_material_cache_asset(*asset))
            );
            assert!(
                package.physics_state != 0
                    && package.ai_summary != 0
                    && package.audio_zone != 0
                    && package.nav_data != 0
                    && package.light_probe_data != 0
                    && package.event_history != 0
            );
            assert!(package.streaming_priority.aggregate_score() > 0.0);
            assert!(generated_asset_records.iter().any(|asset| {
                asset.id == package.package_asset
                    && asset.kind == AssetKind::WorldChunk
                    && asset.dependencies == chunk.streaming_dependencies
            }));
            assert!(!chunk.local_infrastructure.is_empty());
            assert!(!chunk.active_npc_seeds.is_empty());
            assert!(chunk.local_navigation.all_important_locations_reachable());
            assert!(chunk.entities.iter().any(|template| {
                template.tags.iter().any(|tag| tag == "material:glass")
                    && template
                        .renderable
                        .as_ref()
                        .is_some_and(|renderable| renderable.material == glass_variant)
            }));
            assert!(chunk.entities.iter().any(|template| {
                template
                    .tags
                    .iter()
                    .any(|tag| tag == "material:wet_asphalt")
                    && template
                        .physical_body
                        .as_ref()
                        .is_some_and(|body| body.material_id == wet_road_variant)
            }));
        }

        let middle = &first.chunks[1];
        assert_eq!(middle.neighbor_links.len(), 2);
        assert!(first.story_seeds.iter().all(|seed| {
            !seed.involved_factions.is_empty()
                && !seed.physical_hooks.is_empty()
                && first.navigation_data.has_location(seed.location)
        }));
        assert!(first.spawn_sets.iter().flatten().any(|template| {
            template.renderable.is_some()
                && template.physical_body.is_some()
                && template.tags.iter().any(|tag| tag == "worldgen_chunk")
        }));
        assert!(
            first
                .spawn_sets
                .iter()
                .flatten()
                .any(|template| template.human.is_some() && template.agent.is_some())
        );
    }

    #[test]
    fn city_cell_packages_emit_shared_manifests_and_validation_reports() {
        let template = generate_world_template(&request_with_budget(64));
        let manifests = build_world_template_package_manifests(&template);

        assert_eq!(manifests.len(), template.city_cell_packages.len());
        for package in &template.city_cell_packages {
            let manifest = manifests
                .iter()
                .find(|manifest| manifest.package_asset == package.package_asset)
                .expect("each city cell package should have a shared manifest");
            let package_dependencies = package
                .geometry_chunks
                .iter()
                .chain(package.material_packages.iter())
                .copied()
                .collect::<BTreeSet<_>>();
            let manifest_dependencies = manifest
                .dependencies
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();

            assert_eq!(manifest.asset_kind, AssetKind::WorldChunk);
            assert_eq!(manifest.schema_name, "CityCellPackage");
            assert_eq!(manifest.schema_version, CITY_CELL_PACKAGE_SCHEMA_VERSION);
            assert_eq!(manifest_dependencies, package_dependencies);
            assert_eq!(manifest.chunks.len(), 1);
            assert_eq!(manifest.chunks[0].usage_label, "world_cell");
            assert_eq!(
                manifest.total_uncompressed_bytes,
                WORLDGEN_CITY_CELL_ASSET_BYTES
            );
            assert_eq!(manifest.validation.asset, Some(package.package_asset));
            assert!(manifest.validation.passed);
            assert!(manifest.validate(WORLDGEN_SYSTEM_ID).passed);
            assert!(manifest.validation.metrics.iter().any(|metric| {
                metric.name == "material_package_descriptor_count"
                    && metric.value == package.material_package_descriptors.len() as f64
            }));
            assert!(manifest.validation.metrics.iter().any(|metric| {
                metric.name == "material_package_capture_confidence" && metric.value >= 0.65
            }));
        }
    }

    #[test]
    fn city_streaming_plan_prioritizes_story_visibility_and_predicted_routes() {
        let template = generate_world_template(&request_with_budget(64));
        let first_chunk = &template.chunks[0];
        let second_chunk = &template.chunks[1];
        let third_chunk = &template.chunks[2];
        let request = CityStreamingRequest {
            player_position: Vec3::ZERO,
            camera_forward: Vec3::new(1.0, 0.0, 0.0),
            player_velocity: Vec3::new(4.0, 0.0, 0.0),
            story_focus_locations: first_chunk.local_navigation.important_locations.clone(),
            active_event_locations: vec![chunk_center(first_chunk)],
            renderer_visible_chunks: vec![second_chunk.chunk_id],
            renderer_feedback: vec![
                CityRendererStreamingFeedback::new(second_chunk.chunk_id)
                    .with_visibility(0.64, 192)
                    .with_streaming_pressure(0.7, 0.82),
            ],
            predicted_route: third_chunk.local_navigation.important_locations.clone(),
            ..CityStreamingRequest::default()
        };

        let plan = plan_city_cell_streaming(&template, &request);

        assert!(plan.passed());
        assert_eq!(plan.cell_count, template.chunks.len());
        assert_eq!(plan.hero_loaded_count, 1);
        assert!(plan.render_high_detail_count >= 1);
        assert!(plan.gameplay_loaded_count >= 1);
        assert!(plan.loaded_cell_count >= 3);
        assert!(plan.requested_asset_count >= 6 + template.city_cell_packages.len());
        assert!(plan.requested_streaming_bytes > 64 * 1024 * 1024);
        assert!(plan.highest_priority_cell().is_some_and(|cell| cell.cell_id
            == first_chunk.chunk_id
            && cell.package_asset == worldgen_city_cell_asset_id(first_chunk.chunk_id)
            && cell.package_requested
            && cell.requested_assets.contains(&cell.package_asset)
            && cell.state == CityCellStreamingState::HeroLoaded
            && cell.story_focus
            && cell.active_event));
        assert!(plan.cells.iter().any(|cell| {
            cell.cell_id == second_chunk.chunk_id
                && cell.package_asset == worldgen_city_cell_asset_id(second_chunk.chunk_id)
                && cell.package_requested
                && cell.requested_assets.contains(&cell.package_asset)
                && cell.renderer_visible
                && cell.renderer_feedback_score > 0.4
                && cell.renderer_visible_cluster_count >= 192
                && cell.state >= CityCellStreamingState::RenderHighDetail
        }));
        assert!(plan.cells.iter().any(|cell| {
            cell.cell_id == third_chunk.chunk_id
                && cell.package_asset == worldgen_city_cell_asset_id(third_chunk.chunk_id)
                && cell.package_requested
                && cell.requested_assets.contains(&cell.package_asset)
                && cell.predicted_route_match
                && cell.state >= CityCellStreamingState::GameplayLoaded
        }));
    }

    #[test]
    fn alley_scene_seed_owns_entities_materials_and_assets() {
        let seed = build_alley_scene_seed();

        assert_eq!(seed.materials.len(), 5);
        assert_eq!(seed.entities.len(), 10);
        assert_eq!(seed.assets.len(), 9);
        assert_eq!(seed.player.player_entity, PLAYER_ID);
        assert_eq!(seed.player.walkable_min, ALLEY_WALKABLE_MIN);
        assert_eq!(seed.player.walkable_max, ALLEY_WALKABLE_MAX);
        assert_eq!(seed.ai, alley_ai_runtime_seed());
        assert_eq!(seed.ai.security_faction, ALLEY_SECURITY_FACTION_ID);
        assert_eq!(seed.ai.opposition_faction, ALLEY_OPPOSITION_FACTION_ID);
        assert_eq!(seed.ai.story_location, ALLEY_LOCATION_ID);
        assert_eq!(
            seed.player.primary_interaction,
            PrimaryInteractionSettings::new(PLAYER_ID)
        );
        assert_eq!(
            seed.scripted_glass_break_command(),
            alley_scripted_glass_break_command()
        );
        assert!(seed.assets.iter().any(|asset| {
            asset.id == ALLEY_GLASS_FRACTURED_MESH_ASSET
                && asset.load_state == AssetLoadState::Unloaded
                && asset.dependencies == vec![ALLEY_GLASS_INTACT_MESH_ASSET]
        }));
        assert!(seed.entities.iter().any(|template| {
            template.entity_id == Some(GLASS_WALL_ID)
                && template.renderable.as_ref().is_some_and(|renderable| {
                    renderable.mesh.0 == ALLEY_GLASS_INTACT_MESH_ASSET
                        && renderable
                            .fracture_replacement_mesh
                            .is_some_and(|mesh| mesh.0 == ALLEY_GLASS_FRACTURED_MESH_ASSET)
                })
        }));
        assert!(seed.entities.iter().any(|template| {
            template.entity_id == Some(WATER_LEAK_ID)
                && template.tags.iter().any(|tag| tag == "steam_leak")
        }));
        assert!(
            [
                ALLEY_SECURITY_CAMERA_MESH_ASSET,
                ALLEY_SERVICE_DOOR_MESH_ASSET,
                ALLEY_MARKET_STALL_MESH_ASSET,
            ]
            .iter()
            .all(|asset_id| seed.assets.iter().any(|asset| {
                asset.id == *asset_id
                    && asset.kind == AssetKind::Mesh
                    && asset.load_state == AssetLoadState::Resident
            }))
        );
        assert!(seed.entities.iter().any(|template| {
            template.entity_id == Some(ALLEY_SECURITY_CAMERA_ID)
                && template.renderable.as_ref().is_some_and(|renderable| {
                    renderable.mesh.0 == ALLEY_SECURITY_CAMERA_MESH_ASSET
                        && renderable.material == MATERIAL_GLASS
                })
                && template.tags.iter().any(|tag| tag == "surveillance")
                && template.tags.iter().any(|tag| tag == "hackable")
        }));
        assert!(seed.entities.iter().any(|template| {
            template.entity_id == Some(ALLEY_SERVICE_DOOR_ID)
                && template
                    .renderable
                    .as_ref()
                    .is_some_and(|renderable| renderable.mesh.0 == ALLEY_SERVICE_DOOR_MESH_ASSET)
                && template.tags.iter().any(|tag| tag == "security_door")
                && template.tags.iter().any(|tag| tag == "cover")
        }));
        assert!(seed.entities.iter().any(|template| {
            template.entity_id == Some(ALLEY_MARKET_STALL_ID)
                && template
                    .renderable
                    .as_ref()
                    .is_some_and(|renderable| renderable.mesh.0 == ALLEY_MARKET_STALL_MESH_ASSET)
                && template.tags.iter().any(|tag| tag == "story_space")
        }));

        let world = seed
            .build_world_state()
            .expect("vertical slice seed should spawn");
        let snapshot = world.snapshot(1, SimTime::new(0.0, 1));
        assert_eq!(snapshot.transforms.len(), seed.entities.len());
        assert!(snapshot.materials.find(MATERIAL_GLASS).is_some());
        assert!(
            snapshot
                .names
                .find(NPC_MARA_ID)
                .is_some_and(|name| name == "Mara")
        );
        assert!(
            snapshot
                .material_states
                .find(NPC_MARA_ID)
                .is_some_and(|state| {
                    state.moisture >= 0.4 && state.soot > 0.0 && state.temperature >= 300.0
                })
        );

        let mut registry = AssetRegistry::default();
        seed.register_assets(&mut registry);
        assert_eq!(registry.len(), seed.assets.len());
        assert_eq!(
            registry.load_state(ALLEY_GLASS_FRACTURED_MESH_ASSET),
            Some(AssetLoadState::Unloaded)
        );
        assert_eq!(
            registry.load_state(ALLEY_MARA_HUMAN_BUNDLE_ASSET),
            Some(AssetLoadState::Resident)
        );
        assert_eq!(
            registry.load_state(ALLEY_SECURITY_CAMERA_MESH_ASSET),
            Some(AssetLoadState::Resident)
        );
        assert_eq!(
            registry.load_state(ALLEY_SERVICE_DOOR_MESH_ASSET),
            Some(AssetLoadState::Resident)
        );
        assert_eq!(
            registry.load_state(ALLEY_MARKET_STALL_MESH_ASSET),
            Some(AssetLoadState::Resident)
        );
    }

    #[test]
    fn gpu_schedule_prepares_chunks_navigation_infrastructure_and_streaming() {
        let mut world = WorldState::default();
        for template in build_alley_scene() {
            world
                .spawn_entity_template(template)
                .expect("alley entity should spawn");
        }
        let frame = FrameContext {
            frame_id: 9,
            sim_time: SimTime::new(9.0 / 60.0, 9),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(9, SimTime::new(9.0 / 60.0, 9)),
            recent_events: Vec::new(),
            forces: Vec::new(),
        };
        let mut module = CyberpunkWorldGenerationModule::default();
        let mut sink = CommandSink::default();
        module.tick(&frame, &mut sink);
        assert!(module.performance_counters().gpu_milliseconds > 0.0);

        let services = ashfall_core::gpu::GpuServices::new_vulkan();
        let mut graph = services.begin_frame(70);
        module.schedule_gpu(&mut graph);

        for expected in [
            "worldgen_chunk_visibility",
            "worldgen_navigation_validation",
            "worldgen_infrastructure_propagation",
            "worldgen_story_hook_indexing",
            "worldgen_streaming_dependency_compaction",
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
            usage.owner == Some(70)
                && usage.label == "worldgen chunk metadata buffer"
                && usage.bindless_index.is_some()
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "worldgen_chunk_visibility")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(70)
                && usage.label == "worldgen navigation validation output"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "worldgen_navigation_validation")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(70)
                && usage.label == "worldgen compacted streaming dependencies"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "worldgen_streaming_dependency_compaction")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "worldgen_infrastructure_propagation"
                && pipeline.shader_key == "worldgen/infrastructure_propagation.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "INFRASTRUCTURE_GRAPH")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "worldgen_streaming_dependency_compaction"
                && pipeline.shader_key == "worldgen/streaming_dependency_compaction.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "STREAMING_DEPENDENCIES")
        }));
    }

    #[test]
    fn infrastructure_damage_produces_grounded_consequences() {
        let template = generate_world_template(&request_with_budget(64));
        let damaged_node = template.infrastructure.power.nodes[0].id;

        let report = evaluate_infrastructure_damage(
            &template,
            &InfrastructureDamageRequest {
                damaged_node,
                severity: 0.92,
                source_entity: Some(PLAYER_ID),
            },
        );
        let repeat = evaluate_infrastructure_damage(
            &template,
            &InfrastructureDamageRequest {
                damaged_node,
                severity: 0.92,
                source_entity: Some(PLAYER_ID),
            },
        );

        assert_eq!(report, repeat);
        assert!(report.passed);
        assert_eq!(report.system, Some(InfrastructureSystem::Power));
        assert!(report.affected_nodes.contains(&damaged_node));
        assert!(report.affected_chunks.len() >= 2);
        assert!(report.affected_districts.len() >= 2);
        assert!(!report.story_seeds.is_empty());
        assert!(report.expected_events.iter().any(|event| {
            matches!(
                event,
                WorldEventKind::PowerTransformerOverheated { entity } if *entity == damaged_node
            )
        }));
        assert!(
            report
                .consequence_tags
                .iter()
                .any(|tag| tag == "blackout_risk")
        );
        assert!(
            report
                .faction_pressure
                .iter()
                .any(|delta| delta.faction_id == 700 && delta.resource_pressure_delta > 0.2)
        );
    }

    #[test]
    fn infrastructure_consequence_map_surfaces_cell_level_outage_impacts() {
        let template = generate_world_template(&request_with_budget(64));
        let damaged_node = template.infrastructure.power.nodes[0].id;

        let map = infrastructure_consequence_map_for_damage(
            &template,
            &InfrastructureDamageRequest {
                damaged_node,
                severity: 0.92,
                source_entity: Some(PLAYER_ID),
            },
        );

        assert!(map.passed());
        assert_eq!(map.system, Some(InfrastructureSystem::Power));
        assert!(map.affected_nodes.contains(&damaged_node));
        assert!(map.affected_chunks.len() >= 2);
        assert_eq!(map.cell_impacts.len(), map.affected_chunks.len());
        assert!(map.affected_entity_count > 0);
        assert!(map.affected_renderable_count > 0);
        assert!(map.affected_light_count > 0);
        assert!(map.affected_camera_count > 0);
        assert!(map.affected_npc_count > 0);
        assert!(map.affected_audio_zone_count > 0);
        assert!(map.affected_navigation_location_count > 0);
        assert!(map.affected_streaming_dependency_count >= 6);
        assert!(map.expected_event_count > 0);
        assert!(map.story_seed_count > 0);
        assert!(map.faction_pressure_count > 0);
        assert!(map.consequence_tags.iter().any(|tag| tag == "lighting"));
        assert!(map.consequence_tags.iter().any(|tag| tag == "camera_feed"));
        assert!(
            map.consequence_tags
                .iter()
                .any(|tag| tag == "story_opportunity")
        );
        assert!(map.highest_impact_cell().is_some_and(|cell| {
            cell.light_count > 0
                && cell.camera_count > 0
                && cell.npc_count > 0
                && cell.navigation_location_count > 0
                && cell.story_hook_count > 0
        }));
    }

    #[test]
    fn infrastructure_damage_reports_unknown_nodes() {
        let template = generate_world_template(&request_with_budget(64));

        let report = evaluate_infrastructure_damage(
            &template,
            &InfrastructureDamageRequest {
                damaged_node: u64::MAX,
                severity: 4.0,
                source_entity: None,
            },
        );

        assert!(!report.passed);
        assert_eq!(report.severity, 1.0);
        assert_eq!(report.damaged_node, None);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == ValidationSeverity::Error
                && issue.code == "unknown_infrastructure_node"
        }));
    }

    #[test]
    fn persistent_city_state_records_consequence_deltas_per_cell() {
        let template = generate_world_template(&request_with_budget(64));
        let chunk = &template.chunks[0];
        let location = chunk.local_navigation.nodes[0].position;
        let fractured_entity = chunk
            .entities
            .iter()
            .find(|entity| entity.tags.iter().any(|tag| tag == "destructible"))
            .and_then(|entity| entity.entity_id)
            .expect("generated chunk should contain destructible geometry");
        let agent = chunk
            .entities
            .iter()
            .find(|entity| entity.agent.is_some())
            .and_then(|entity| entity.entity_id)
            .expect("generated chunk should contain an agent");

        let events = vec![
            WorldEvent {
                event_id: 1,
                tick: 1,
                location_meters: location,
                actors: vec![fractured_entity],
                kind: WorldEventKind::GlassWallFractured {
                    entity: fractured_entity,
                },
                physical_evidence: vec!["fractured_glass".to_string()],
                narrative_tags: vec!["physics".to_string()],
            },
            WorldEvent {
                event_id: 2,
                tick: 1,
                location_meters: location,
                actors: vec![fractured_entity],
                kind: WorldEventKind::MaterialStateChanged {
                    entity: fractured_entity,
                },
                physical_evidence: Vec::new(),
                narrative_tags: vec!["material".to_string()],
            },
            WorldEvent {
                event_id: 3,
                tick: 1,
                location_meters: location,
                actors: vec![fractured_entity],
                kind: WorldEventKind::MeshReplaced {
                    entity: fractured_entity,
                },
                physical_evidence: Vec::new(),
                narrative_tags: vec!["rendering".to_string()],
            },
            WorldEvent {
                event_id: 4,
                tick: 1,
                location_meters: location,
                actors: Vec::new(),
                kind: WorldEventKind::StreetFlooded,
                physical_evidence: Vec::new(),
                narrative_tags: vec!["physics".to_string()],
            },
            WorldEvent {
                event_id: 5,
                tick: 1,
                location_meters: location,
                actors: vec![agent],
                kind: WorldEventKind::AgentMemoryUpdated {
                    agent,
                    source_event: Some(1),
                    memory_kind: "evidence".to_string(),
                    content: "saw fractured glass".to_string(),
                    importance: 0.8,
                    confidence: 0.9,
                },
                physical_evidence: Vec::new(),
                narrative_tags: vec!["ai".to_string()],
            },
            WorldEvent {
                event_id: 6,
                tick: 1,
                location_meters: location,
                actors: vec![agent],
                kind: WorldEventKind::SecurityAlertRaised {
                    faction: 700,
                    source_event: 1,
                    threat: fractured_entity,
                    severity: 0.75,
                },
                physical_evidence: Vec::new(),
                narrative_tags: vec!["story".to_string()],
            },
            WorldEvent {
                event_id: 7,
                tick: 1,
                location_meters: location,
                actors: Vec::new(),
                kind: WorldEventKind::StoryEventEmitted {
                    label: "district_pressure_changed".to_string(),
                },
                physical_evidence: Vec::new(),
                narrative_tags: vec!["story".to_string()],
            },
            WorldEvent {
                event_id: 8,
                tick: 1,
                location_meters: location,
                actors: Vec::new(),
                kind: WorldEventKind::AssetBecameResident {
                    asset_id: ALLEY_GLASS_FRACTURED_MESH_ASSET,
                    quality_tier: QualityTier::HeroHighFidelityRuntime,
                },
                physical_evidence: Vec::new(),
                narrative_tags: vec!["asset_streaming".to_string()],
            },
        ];

        let state = persistent_city_state_from_events(&template, &events);

        assert!(state.passed());
        assert!(state.save_required());
        assert_eq!(state.city_id, template.city_id);
        assert_eq!(state.event_count, events.len());
        assert_eq!(state.persistent_event_count, events.len());
        assert_eq!(state.cell_states.len(), 1);
        assert_eq!(state.material_override_count, 1);
        assert_eq!(state.damaged_entity_count, 1);
        assert_eq!(state.mesh_replacement_count, 1);
        assert_eq!(state.infrastructure_delta_count, 1);
        assert_eq!(state.faction_delta_count, 1);
        assert_eq!(state.story_thread_ref_count, 1);
        assert_eq!(state.ai_memory_ref_count, 1);
        assert_eq!(state.resident_asset_count, 1);

        let cell = &state.cell_states[0];
        assert_eq!(cell.cell_id, chunk.chunk_id);
        assert!(cell.damaged_entities.contains(&fractured_entity));
        assert!(cell.mesh_replacements.contains(&fractured_entity));
        assert!(cell.material_state_overrides.iter().any(|override_state| {
            override_state.entity == fractured_entity && override_state.material.is_some()
        }));
        assert!(
            cell.infrastructure_delta
                .systems
                .contains(&InfrastructureSystem::Water)
        );
        assert!(
            cell.infrastructure_delta
                .systems
                .contains(&InfrastructureSystem::Drainage)
        );
        assert!(
            cell.story_thread_refs
                .contains(&chunk.local_story_hooks[0].seed_id)
        );
        assert!(cell.ai_memory_refs.contains(&agent));
        assert!(
            cell.resident_assets
                .contains(&ALLEY_GLASS_FRACTURED_MESH_ASSET)
        );
    }

    #[test]
    fn navigation_cell_state_updates_routes_from_persistent_consequences() {
        let template = generate_world_template(&request_with_budget(64));
        let chunk = &template.chunks[0];
        let location = chunk.local_navigation.nodes[0].position;
        let agent = chunk
            .entities
            .iter()
            .find(|entity| entity.agent.is_some())
            .and_then(|entity| entity.entity_id)
            .expect("generated chunk should contain an agent");

        let events = vec![
            WorldEvent {
                event_id: 10,
                tick: 1,
                location_meters: location,
                actors: vec![agent],
                kind: WorldEventKind::NavigationMoveBlocked { entity: agent },
                physical_evidence: vec!["navigation_blocked".to_string()],
                narrative_tags: vec!["navigation".to_string()],
            },
            WorldEvent {
                event_id: 11,
                tick: 1,
                location_meters: location,
                actors: Vec::new(),
                kind: WorldEventKind::StreetFlooded,
                physical_evidence: Vec::new(),
                narrative_tags: vec!["physics".to_string()],
            },
            WorldEvent {
                event_id: 12,
                tick: 1,
                location_meters: location,
                actors: vec![agent],
                kind: WorldEventKind::SecurityAlertRaised {
                    faction: 700,
                    source_event: 10,
                    threat: agent,
                    severity: 0.82,
                },
                physical_evidence: Vec::new(),
                narrative_tags: vec!["security".to_string()],
            },
        ];
        let persistence = persistent_city_state_from_events(&template, &events);

        let states = navigation_cell_states_from_persistence(&template, &persistence);

        assert_eq!(states.len(), template.chunks.len());
        let state = states
            .iter()
            .find(|state| state.cell_id == chunk.chunk_id)
            .expect("changed chunk should have a navigation state");
        assert!(state.is_changed());
        assert!(state.passed());
        assert_eq!(state.dynamic_blockers.len(), 1);
        assert!(
            state
                .danger_fields
                .iter()
                .any(|field| field.kind == NavigationDangerKind::Flooding)
        );
        assert_eq!(state.restricted_zones.len(), 1);
        assert!(
            state
                .route_updates
                .iter()
                .any(|route| route.status == NavigationRouteStatus::Blocked)
        );
        assert!(
            state
                .route_updates
                .iter()
                .any(|route| route.status == NavigationRouteStatus::Dangerous)
        );
        assert!(
            state.reachable_important_location_count < state.important_location_count,
            "blocked route should reduce reachability until a reroute is generated"
        );
    }

    #[test]
    fn navigation_cell_state_tracks_contamination_specific_route_hazards() {
        let template = generate_world_template(&request_with_budget(64));
        let chunk = &template.chunks[0];
        let location = chunk.local_navigation.nodes[0].position;

        let events = vec![
            WorldEvent {
                event_id: 20,
                tick: 2,
                location_meters: location,
                actors: Vec::new(),
                kind: WorldEventKind::StreetFlooded,
                physical_evidence: vec![
                    "oil_flow".to_string(),
                    "oil_contamination".to_string(),
                    "slick_surface".to_string(),
                ],
                narrative_tags: vec!["liquid".to_string(), "hazard".to_string()],
            },
            WorldEvent {
                event_id: 21,
                tick: 2,
                location_meters: location,
                actors: Vec::new(),
                kind: WorldEventKind::StreetFlooded,
                physical_evidence: vec![
                    "biological_trace".to_string(),
                    "biological_contamination".to_string(),
                    "contaminated_surface".to_string(),
                ],
                narrative_tags: vec!["liquid".to_string(), "hazard".to_string()],
            },
        ];

        let persistence = persistent_city_state_from_events(&template, &events);

        assert!(persistence.passed());
        assert_eq!(persistence.persistent_event_count, events.len());
        assert_eq!(persistence.infrastructure_delta_count, 1);
        let cell = persistence
            .cell_states
            .iter()
            .find(|cell| cell.cell_id == chunk.chunk_id)
            .expect("contamination should persist in the affected cell");
        assert!(
            cell.infrastructure_delta
                .systems
                .contains(&InfrastructureSystem::Drainage)
        );
        assert!(
            !cell
                .infrastructure_delta
                .systems
                .contains(&InfrastructureSystem::Water),
            "oil and biological spills should not be reclassified as a water flood"
        );
        assert!(cell.consequence_tags.iter().any(|tag| tag == "oil_slick"));
        assert!(cell.consequence_tags.iter().any(|tag| tag == "biohazard"));
        assert!(
            cell.consequence_tags
                .iter()
                .any(|tag| tag == "slippery_surface")
        );

        let states = navigation_cell_states_from_persistence(&template, &persistence);
        let state = states
            .iter()
            .find(|state| state.cell_id == chunk.chunk_id)
            .expect("contamination should alter navigation in the affected cell");

        assert!(state.is_changed());
        assert!(state.passed());
        assert!(state.danger_fields.iter().any(|field| {
            field.kind == NavigationDangerKind::SlipperyContamination
                && field.reason.contains("oil")
                && field.severity >= 0.58
        }));
        assert!(state.danger_fields.iter().any(|field| {
            field.kind == NavigationDangerKind::BiohazardContamination
                && field.reason.contains("biological")
                && field.severity >= 0.7
        }));
        assert!(
            state
                .route_updates
                .iter()
                .any(|route| route.status == NavigationRouteStatus::Dangerous)
        );
    }

    #[test]
    fn validation_reports_budget_errors_without_panicking() {
        let template = generate_world_template(&request_with_budget(1));

        assert!(!template.validation_report.passed);
        for expected in [
            "entity_budget_exceeded",
            "unique_material_budget_exceeded",
            "dynamic_light_budget_exceeded",
            "physics_active_budget_exceeded",
            "destructible_budget_exceeded",
            "liquid_zone_budget_exceeded",
            "npc_budget_exceeded",
            "streaming_dependency_budget_exceeded",
            "streaming_memory_budget_exceeded",
        ] {
            assert!(
                template.validation_report.issues.iter().any(|issue| {
                    issue.severity == ValidationSeverity::Error && issue.code == expected
                }),
                "{expected} should be reported"
            );
        }
        assert!(template.validation_report.budget.streaming_memory_bytes > 4 * 1024 * 1024);
    }
}

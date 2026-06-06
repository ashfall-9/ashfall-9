use std::collections::{BTreeMap, BTreeSet};

pub mod beauty;
pub mod beauty_contract;
pub mod beauty_v16;
pub mod beauty_v17;
pub mod windowed;

use ashfall_core::assets::AssetPriority;
use ashfall_core::core::*;
use ashfall_core::gpu::{
    ComputePipelineHandle, GpuDispatchKind, GpuGraphBuilder, GpuPassDesc, GpuPipelineDesc,
    GpuQueueKind, GpuResourceDesc, GpuResourceHandle, GpuResourceKind, GpuResourceLifetime,
    GpuResourceResidencyPolicy, GpuShaderArtifactKind, GpuShaderPermutation,
};
use ashfall_core::runtime::{EngineModule, FrameContext, ModuleDescriptor, ModuleStateRecord};
use ashfall_core::world::{CommandSink, Renderable, WorldEvent, WorldEventKind, WorldSnapshot};

pub type LightSet = Vec<LightRecord>;
pub type GpuSceneInstanceBuffer = GpuResourceHandle;
pub type GpuMeshClusterBuffer = GpuResourceHandle;
pub type GpuMaterialTable = GpuResourceHandle;
pub type GpuMaterialStateTable = GpuResourceHandle;
pub type MaterialRuntimeProgramSet = Vec<MaterialRuntimeProgramRecord>;
pub type VolumeSet = Vec<VolumeRenderInput>;
pub type VirtualShadowPageSet = Vec<VirtualShadowPageRequest>;
pub type FluidSurfaceSet = Vec<FluidSurfaceRenderInput>;
pub type DecalSet = Vec<DecalRenderInput>;
pub type WeatherState = RenderWeatherState;
pub type WorldCellVisibility = Vec<WorldCellVisibilityRecord>;
pub type DebugRenderRequest = DebugRenderFlags;
pub type HumanRenderSet = Vec<HumanRenderRecord>;
pub type AiDebugOverlaySet = Vec<AiIntentDebugOverlay>;
pub type DebugRenderFlags = Vec<DebugRenderView>;
pub type VisibilityResults = Vec<EntityId>;
pub type PickingData = Vec<PickingRecord>;
pub type RenderCaptureSet = Vec<RenderFrameCapture>;
pub type OptionalPerceptionBuffers = Option<PerceptionBufferSet>;
pub type GpuTimingReport = PerformanceCounters;
pub type RenderDebugData = Vec<RenderDebugEntry>;
pub type GpuVolumeHandle = GpuResourceHandle;
pub type GpuBufferHandle = GpuResourceHandle;
pub type MeshHandle = MeshAssetHandle;
const RENDERING_MODULE_STATE_VERSION: u32 = 1;
const TEXTURE_GLASS_CRACK_DETAIL_CACHE: TextureHandle = TextureHandle(30_001);
const TEXTURE_WET_ASPHALT_REFLECTION_CACHE: TextureHandle = TextureHandle(30_002);
const TEXTURE_HUMAN_SKIN_DETAIL_CACHE: TextureHandle = TextureHandle(30_003);
const TEXTURE_AGED_SURFACE_CACHE: TextureHandle = TextureHandle(30_004);

#[derive(Clone, Debug, Default, PartialEq)]
struct RenderEventHints {
    deformed_entities: BTreeSet<EntityId>,
    constraint_entities: BTreeSet<EntityId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderPacket {
    pub frame_id: FrameId,
    pub sim_time: SimTime,
    pub cameras: Vec<Camera>,
    pub lights: LightSet,
    pub instances: GpuSceneInstanceBuffer,
    pub instance_records: Vec<SceneInstanceRecord>,
    pub mesh_clusters: GpuMeshClusterBuffer,
    pub mesh_cluster_records: Vec<MeshClusterRecord>,
    pub materials: GpuMaterialTable,
    pub material_records: Vec<MaterialGpuRecord>,
    pub material_states: GpuMaterialStateTable,
    pub material_state_records: Vec<MaterialStateGpuRecord>,
    pub material_programs: MaterialRuntimeProgramSet,
    pub volumes: VolumeSet,
    pub virtual_shadow_pages: VirtualShadowPageSet,
    pub fluids: FluidSurfaceSet,
    pub decals: DecalSet,
    pub weather: WeatherState,
    pub world_cell_visibility: WorldCellVisibility,
    pub surface_effects: Vec<SurfaceEffectRecord>,
    pub humans: HumanRenderSet,
    pub ai_intent_overlays: AiDebugOverlaySet,
    pub visible_entities: Vec<EntityId>,
    pub wet_material_entities: Vec<EntityId>,
    pub cracked_material_entities: Vec<EntityId>,
    pub deformed_material_entities: Vec<EntityId>,
    pub constraint_debug_entities: Vec<EntityId>,
    pub debug_request: DebugRenderRequest,
    pub debug_views: DebugRenderFlags,
    pub quality_profile: RenderQualityProfile,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderOutput {
    pub frame_id: FrameId,
    pub final_image: TextureHandle,
    pub depth_buffer: TextureHandle,
    pub motion_vectors: GpuBufferHandle,
    pub visibility: VisibilityResults,
    pub visibility_records: Vec<VisibilityRecord>,
    pub picking: PickingData,
    pub reference_comparison: Option<ReferenceComparisonImages>,
    pub capture_frames: RenderCaptureSet,
    pub optional_perception_buffers: OptionalPerceptionBuffers,
    pub draw_plan: GpuDrivenDrawPlan,
    pub gpu_timing: GpuTimingReport,
    pub debug_data: RenderDebugData,
    pub optional_ray_tracing_feedback: Option<RayTracingFeedback>,
    pub validation_report: RenderValidationReport,
    pub budget_usage: RenderBudgetUsage,
    pub quality_profile: RenderQualityProfile,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuDrivenDrawPlan {
    pub batches: Vec<RenderDrawBatch>,
    pub visible_instance_count: usize,
    pub selected_cluster_count: u32,
    pub triangle_budget: u32,
    pub indirect_draw_count: usize,
    pub cpu_draw_call_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderDrawBatch {
    pub material: MaterialId,
    pub lod: RenderLodTier,
    pub instance_count: usize,
    pub cluster_count: u32,
    pub triangle_budget: u32,
    pub human: bool,
    pub transparent: bool,
    pub emissive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub entity: Option<EntityId>,
    pub label: String,
    pub position: Vec3,
    pub forward: Vec3,
    pub vertical_fov_radians: f32,
    pub focal_length_mm: f32,
    pub sensor_width_mm: f32,
    pub sensor_height_mm: f32,
    pub aperture_f_stop: f32,
    pub shutter_seconds: f32,
    pub iso: f32,
    pub focus_distance_meters: f32,
    pub exposure_bias: f32,
    pub motion_blur_enabled: bool,
    pub depth_of_field_enabled: bool,
    pub lens_distortion_amount: f32,
    pub chromatic_aberration_amount: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightRecord {
    pub entity: EntityId,
    pub kind: LightKind,
    pub position: Vec3,
    pub color_linear: [f32; 3],
    pub intensity_lumens: f32,
    pub radius_meters: f32,
    pub casts_shadows: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    Sun,
    Neon,
    Screen,
    Fire,
    AmbientProbe,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VirtualShadowPageRequest {
    pub page_id: u64,
    pub light_entity: Option<EntityId>,
    pub caster_entity: Option<EntityId>,
    pub source_event: Option<WorldEventId>,
    pub bounds: Aabb,
    pub reason: ShadowPageInvalidationReason,
    pub estimated_page_count: u32,
    pub priority: QualityTier,
    pub dirty: bool,
    pub contact_sharpness: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShadowPageInvalidationReason {
    ShadowCastingLight,
    DestructibleGeometry,
    AnimatedHuman,
    VolumeScattering,
    MaterialStateChange,
    Deformation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneInstanceRecord {
    pub entity: EntityId,
    pub name: String,
    pub transform: Transform,
    pub mesh: MeshAssetHandle,
    pub material: MaterialId,
    pub material_state: MaterialState,
    pub flags: InstanceRenderFlags,
    pub lod: RenderLodTier,
    pub distance_to_primary_camera: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InstanceRenderFlags {
    pub visible: bool,
    pub human: bool,
    pub wet: bool,
    pub cracked: bool,
    pub deformed: bool,
    pub constraint_corrected: bool,
    pub emissive: bool,
    pub transparent: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderLodTier {
    Hero,
    Near,
    Mid,
    Far,
    Impostor,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshClusterRecord {
    pub entity: EntityId,
    pub mesh: MeshAssetHandle,
    pub cluster_count: u32,
    pub triangle_budget: u32,
    pub lod: RenderLodTier,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialGpuRecord {
    pub entity: EntityId,
    pub material_id: MaterialId,
    pub base_color_linear: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub emission_linear: [f32; 3],
    pub transparency: f32,
    pub subsurface: f32,
    pub anisotropy: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialStateGpuRecord {
    pub entity: EntityId,
    pub material_id: MaterialId,
    pub wetness: f32,
    pub soot: f32,
    pub corrosion: f32,
    pub crack_density: f32,
    pub plastic_strain: f32,
    pub biological_contamination: f32,
    pub oil_contamination: f32,
    pub heat: f32,
    pub electrical_charge: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialRuntimeProgramRecord {
    pub material_id: MaterialId,
    pub graph_handle: MaterialGraphHandle,
    pub generator: Option<MaterialGeneratorRef>,
    pub shader_model: MaterialProgramShaderModel,
    pub required_state_channels: Vec<MaterialStateChannel>,
    pub cache_policy: MaterialCachePolicyRecord,
    pub displacement_policy: MaterialDisplacementPolicy,
    pub cache_textures: Vec<TextureHandle>,
    pub instance_count: usize,
    pub deterministic_key: u128,
    pub fallback_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaterialProgramShaderModel {
    OpaquePbr,
    TransparentPbr,
    Subsurface,
    Emissive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaterialStateChannel {
    Wetness,
    Cracks,
    Soot,
    Corrosion,
    Heat,
    PlasticStrain,
    Electrical,
    Biological,
    Oil,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialCachePolicyRecord {
    pub max_resolution_pixels: u32,
    pub update_frequency: MaterialCacheUpdateFrequency,
    pub invalidation_channels: Vec<MaterialStateChannel>,
    pub streaming_priority: f32,
    pub cache_required: bool,
    pub cache_miss_fallback: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialCacheUpdateFrequency {
    Static,
    OnStateChange,
    PerFrameHero,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialDisplacementPolicy {
    None,
    ParallaxDetail,
    MicroDisplacement,
    FractureInterior,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VolumeRenderInput {
    pub density_field: GpuVolumeHandle,
    pub temperature_field: Option<GpuVolumeHandle>,
    pub velocity_field: Option<GpuVolumeHandle>,
    pub source_event: WorldEventId,
    pub source_entities: Vec<EntityId>,
    pub medium: VolumeMedium,
    pub material_id: MaterialId,
    pub bounds: Aabb,
    pub quality_hint: QualityTier,
    pub density: f32,
    pub hazard_level: f32,
    pub density_cell_count: u64,
    pub temperature_cell_count: u64,
    pub pressure_cell_count: u64,
    pub scattering: f32,
    pub visibility_blocking: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VolumeMedium {
    Steam,
    ToxicGas,
    Smoke,
    Unknown,
}

impl VolumeMedium {
    pub fn label(self) -> &'static str {
        match self {
            Self::Steam => "steam",
            Self::ToxicGas => "toxic_gas",
            Self::Smoke => "smoke",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FluidSurfaceRenderInput {
    pub surface_mesh: MeshHandle,
    pub particle_spray_buffer: Option<GpuBufferHandle>,
    pub foam_buffer: Option<GpuBufferHandle>,
    pub material_id: MaterialId,
    pub bounds: Aabb,
    pub wetness_targets: Vec<EntityId>,
    pub flow_velocity_meters_per_second: Vec3,
    pub screen_space_reflection: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecalRenderInput {
    pub source_event: Option<WorldEventId>,
    pub entity: EntityId,
    pub kind: DecalKind,
    pub bounds: Aabb,
    pub material_id: MaterialId,
    pub intensity: f32,
    pub quality_hint: QualityTier,
    pub generated_atlas_page: u32,
    pub state_channels: Vec<MaterialStateChannel>,
    pub generated_from_material_state: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DecalKind {
    CrackField,
    WetnessFilm,
    SootStain,
    CorrosionBloom,
    PlasticDeformation,
    ConstraintStress,
}

impl DecalKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::CrackField => "crack_field",
            Self::WetnessFilm => "wetness_film",
            Self::SootStain => "soot_stain",
            Self::CorrosionBloom => "corrosion_bloom",
            Self::PlasticDeformation => "plastic_deformation",
            Self::ConstraintStress => "constraint_stress",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceEffectRecord {
    pub event_id: WorldEventId,
    pub entity: EntityId,
    pub kind: SurfaceEffectKind,
    pub bounds: Aabb,
    pub material_id: MaterialId,
    pub intensity: f32,
    pub quality_hint: QualityTier,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceEffectKind {
    MetalDent { plastic_strain: f32 },
    ConstraintCorrection { correction_meters: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanRenderRecord {
    pub entity: EntityId,
    pub human_id: HumanId,
    pub mesh: Option<MeshAssetHandle>,
    pub material: Option<MaterialId>,
    pub lod: HumanRenderLod,
    pub skin_detail: QualityTier,
    pub hair_strand_buffer: Option<GpuBufferHandle>,
    pub face_morph_buffer: Option<GpuBufferHandle>,
    pub eye_reflection_probe: Option<GpuResourceHandle>,
    pub skin_wetness: f32,
    pub injury_overlay: f32,
    pub hair_wetness: f32,
    pub clothing_wetness: f32,
    pub subsurface_strength: f32,
    pub pore_microdetail_strength: f32,
    pub blood_redness: f32,
    pub oil_sweat_mix: f32,
    pub eye_wetness: f32,
    pub eye_redness: f32,
    pub iris_depth: f32,
    pub tearline_strength: f32,
    pub hair_lod: HumanHairRenderLod,
    pub hair_motion_response: f32,
    pub clothing_motion_response: f32,
    pub clothing_damage: f32,
    pub cybernetic_surface_ratio: f32,
    pub closeup_quality_score: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanRenderLod {
    HeroFace,
    NearbyNpc,
    Crowd,
    DistantImpostor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanHairRenderLod {
    Strand,
    Hybrid,
    Cards,
    Impostor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugRenderView {
    MaterialState,
    Lighting,
    Overdraw,
    Lod,
    GpuCost,
    Visibility,
    AiDecisions,
    Luminance,
    Exposure,
    Normals,
    Roughness,
    MaterialIds,
    GlobalIllumination,
    Shadows,
    Velocity,
    Depth,
    HumanRendering,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherCondition {
    Clear,
    Rain,
    Flooding,
    ToxicGas,
    Dust,
    MixedHazard,
}

impl WeatherCondition {
    pub fn label(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Rain => "rain",
            Self::Flooding => "flooding",
            Self::ToxicGas => "toxic_gas",
            Self::Dust => "dust",
            Self::MixedHazard => "mixed_hazard",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderWeatherState {
    pub condition: WeatherCondition,
    pub precipitation_intensity: f32,
    pub surface_wetness: f32,
    pub flood_depth_meters: f32,
    pub toxic_gas_density: f32,
    pub fog_density: f32,
    pub dust_density: f32,
    pub neon_haze: f32,
    pub wind_velocity_mps: Vec3,
    pub temperature_celsius: f32,
    pub humidity: f32,
    pub source_event_count: usize,
}

impl Default for RenderWeatherState {
    fn default() -> Self {
        Self {
            condition: WeatherCondition::Clear,
            precipitation_intensity: 0.0,
            surface_wetness: 0.0,
            flood_depth_meters: 0.0,
            toxic_gas_density: 0.0,
            fog_density: 0.0,
            dust_density: 0.0,
            neon_haze: 0.0,
            wind_velocity_mps: Vec3::ZERO,
            temperature_celsius: 20.0,
            humidity: 0.35,
            source_event_count: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorldCellRenderState {
    Hero,
    Gameplay,
    RenderHighDetail,
    Background,
    Summary,
    Dormant,
}

impl WorldCellRenderState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hero => "hero",
            Self::Gameplay => "gameplay",
            Self::RenderHighDetail => "render_high_detail",
            Self::Background => "background",
            Self::Summary => "summary",
            Self::Dormant => "dormant",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldCellVisibilityRecord {
    pub cell_id: u64,
    pub district_id: u64,
    pub loaded: bool,
    pub quality_tier: QualityTier,
    pub render_state: WorldCellRenderState,
    pub visible_entity_count: usize,
    pub population_activity: f32,
    pub visibility_weight: f32,
    pub atmospheric_density: f32,
    pub resident_asset_count: u32,
    pub active_story_thread_count: u32,
    pub streaming_priority: AssetPriority,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisibilityRecord {
    pub entity: EntityId,
    pub camera_index: usize,
    pub visible_fraction: f32,
    pub distance_meters: f32,
    pub lod: RenderLodTier,
    pub reason: VisibilityReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibilityReason {
    FrustumAccepted,
    HeroPinned,
    LightEmitter,
    DebugForced,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PickingRecord {
    pub entity: EntityId,
    pub camera_index: usize,
    pub screen_position: [f32; 2],
    pub depth_meters: f32,
    pub material: MaterialId,
    pub lod: RenderLodTier,
    pub visible_fraction: f32,
    pub source_instance_index: usize,
    pub editor_selectable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceComparisonImages {
    pub gameplay_image: TextureHandle,
    pub reference_image: TextureHandle,
    pub error_heatmap_image: TextureHandle,
    pub metrics_buffer: GpuBufferHandle,
    pub compared_frame_id: FrameId,
    pub sample_budget: u32,
    pub rms_luminance_error: f32,
    pub max_luminance_error: f32,
    pub same_scene_hash: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderCaptureKind {
    Screenshot,
    CinematicFrame,
}

impl RenderCaptureKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Screenshot => "screenshot",
            Self::CinematicFrame => "cinematic_frame",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderFrameCapture {
    pub kind: RenderCaptureKind,
    pub frame_id: FrameId,
    pub image: TextureHandle,
    pub depth: TextureHandle,
    pub motion_vectors: GpuBufferHandle,
    pub camera_label: String,
    pub exposure_bias: f32,
    pub story_entity_count: usize,
    pub weather_condition: WeatherCondition,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PerceptionBufferSet {
    pub entity_id_buffer: GpuBufferHandle,
    pub material_id_buffer: GpuBufferHandle,
    pub depth_pyramid: TextureHandle,
    pub motion_vectors: GpuBufferHandle,
    pub visible_entity_count: usize,
    pub hazard_volume_count: usize,
    pub ai_overlay_count: usize,
    pub confidence: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RayTracingFeedback {
    pub reflection_candidate_count: usize,
    pub shadow_candidate_count: usize,
    pub fallback_used: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderDebugEntry {
    pub view: DebugRenderView,
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AiIntentDebugOverlay {
    pub event_id: WorldEventId,
    pub agent: EntityId,
    pub action: String,
    pub source_event: Option<WorldEventId>,
    pub position: Vec3,
    pub source_position: Option<Vec3>,
    pub validation_passed: bool,
    pub color_linear: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderQualityProfile {
    pub quality_tier: QualityTier,
    pub capability_tier: RenderCapabilityTier,
    pub ray_traced_reflections: bool,
    pub ray_traced_shadows: bool,
    pub volumetric_resolution_scale: f32,
    pub material_cache_quality: QualityTier,
    pub human_skin_quality: QualityTier,
    pub temporal_reconstruction: bool,
}

impl RenderQualityProfile {
    pub fn for_quality(quality_tier: QualityTier) -> Self {
        match quality_tier {
            QualityTier::Disabled => Self {
                quality_tier,
                capability_tier: RenderCapabilityTier::RasterOnly,
                ray_traced_reflections: false,
                ray_traced_shadows: false,
                volumetric_resolution_scale: 0.0,
                material_cache_quality: QualityTier::Disabled,
                human_skin_quality: QualityTier::Disabled,
                temporal_reconstruction: false,
            },
            QualityTier::BackgroundApproximation => Self {
                quality_tier,
                capability_tier: RenderCapabilityTier::RasterOnly,
                ray_traced_reflections: false,
                ray_traced_shadows: false,
                volumetric_resolution_scale: 0.35,
                material_cache_quality: QualityTier::BackgroundApproximation,
                human_skin_quality: QualityTier::BackgroundApproximation,
                temporal_reconstruction: true,
            },
            QualityTier::NormalRuntime => Self {
                quality_tier,
                capability_tier: RenderCapabilityTier::ScreenSpaceEffects,
                ray_traced_reflections: false,
                ray_traced_shadows: false,
                volumetric_resolution_scale: 0.75,
                material_cache_quality: QualityTier::NormalRuntime,
                human_skin_quality: QualityTier::NormalRuntime,
                temporal_reconstruction: true,
            },
            QualityTier::HeroHighFidelityRuntime => Self {
                quality_tier,
                capability_tier: RenderCapabilityTier::SelectiveRayTracing,
                ray_traced_reflections: true,
                ray_traced_shadows: true,
                volumetric_resolution_scale: 1.0,
                material_cache_quality: QualityTier::HeroHighFidelityRuntime,
                human_skin_quality: QualityTier::HeroHighFidelityRuntime,
                temporal_reconstruction: true,
            },
            QualityTier::ReferenceOfflineValidation => Self {
                quality_tier,
                capability_tier: RenderCapabilityTier::ReferencePathTracing,
                ray_traced_reflections: true,
                ray_traced_shadows: true,
                volumetric_resolution_scale: 2.0,
                material_cache_quality: QualityTier::ReferenceOfflineValidation,
                human_skin_quality: QualityTier::ReferenceOfflineValidation,
                temporal_reconstruction: false,
            },
        }
    }

    fn degraded_for_budget(&self) -> Self {
        Self {
            quality_tier: QualityTier::BackgroundApproximation,
            capability_tier: RenderCapabilityTier::RasterOnly,
            ray_traced_reflections: false,
            ray_traced_shadows: false,
            volumetric_resolution_scale: self.volumetric_resolution_scale.min(0.5),
            material_cache_quality: QualityTier::BackgroundApproximation,
            human_skin_quality: QualityTier::BackgroundApproximation,
            temporal_reconstruction: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderCapabilityTier {
    RasterOnly,
    ScreenSpaceEffects,
    SelectiveRayTracing,
    RayTracedGlobalIllumination,
    ReferencePathTracing,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderBudget {
    pub quality_tier: QualityTier,
    pub max_gpu_milliseconds: f32,
    pub max_visible_instances: usize,
    pub max_dynamic_lights: usize,
    pub max_volume_regions: usize,
    pub max_fluid_surfaces: usize,
    pub max_decals: usize,
    pub max_world_cell_visibility_records: usize,
    pub max_surface_effects: usize,
    pub max_memory_bytes: u64,
    pub allow_ray_tracing: bool,
}

impl RenderBudget {
    pub fn for_quality(quality_tier: QualityTier) -> Self {
        match quality_tier {
            QualityTier::Disabled => Self {
                quality_tier,
                max_gpu_milliseconds: 0.0,
                max_visible_instances: 0,
                max_dynamic_lights: 0,
                max_volume_regions: 0,
                max_fluid_surfaces: 0,
                max_decals: 0,
                max_world_cell_visibility_records: 0,
                max_surface_effects: 0,
                max_memory_bytes: 0,
                allow_ray_tracing: false,
            },
            QualityTier::BackgroundApproximation => Self {
                quality_tier,
                max_gpu_milliseconds: 1.0,
                max_visible_instances: 64,
                max_dynamic_lights: 8,
                max_volume_regions: 1,
                max_fluid_surfaces: 1,
                max_decals: 12,
                max_world_cell_visibility_records: 4,
                max_surface_effects: 4,
                max_memory_bytes: 48 * 1024 * 1024,
                allow_ray_tracing: false,
            },
            QualityTier::NormalRuntime => Self {
                quality_tier,
                max_gpu_milliseconds: 4.0,
                max_visible_instances: 8_192,
                max_dynamic_lights: 128,
                max_volume_regions: 8,
                max_fluid_surfaces: 8,
                max_decals: 512,
                max_world_cell_visibility_records: 128,
                max_surface_effects: 64,
                max_memory_bytes: 128 * 1024 * 1024,
                allow_ray_tracing: false,
            },
            QualityTier::HeroHighFidelityRuntime => Self {
                quality_tier,
                max_gpu_milliseconds: 8.0,
                max_visible_instances: 16_384,
                max_dynamic_lights: 256,
                max_volume_regions: 16,
                max_fluid_surfaces: 16,
                max_decals: 2_048,
                max_world_cell_visibility_records: 512,
                max_surface_effects: 256,
                max_memory_bytes: 384 * 1024 * 1024,
                allow_ray_tracing: true,
            },
            QualityTier::ReferenceOfflineValidation => Self {
                quality_tier,
                max_gpu_milliseconds: f32::INFINITY,
                max_visible_instances: usize::MAX,
                max_dynamic_lights: usize::MAX,
                max_volume_regions: usize::MAX,
                max_fluid_surfaces: usize::MAX,
                max_decals: usize::MAX,
                max_world_cell_visibility_records: usize::MAX,
                max_surface_effects: usize::MAX,
                max_memory_bytes: u64::MAX,
                allow_ray_tracing: true,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderBudgetUsage {
    pub visible_instances: usize,
    pub dynamic_lights: usize,
    pub volume_regions: usize,
    pub fluid_surfaces: usize,
    pub decals: usize,
    pub world_cell_visibility_records: usize,
    pub surface_effects: usize,
    pub human_records: usize,
    pub estimated_gpu_milliseconds: f32,
    pub memory_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderValidationReport {
    pub passed: bool,
    pub issues: Vec<RenderValidationIssue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderValidationIssue {
    pub severity: RenderValidationSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderValidationSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Default)]
pub struct RenderingModule {
    requested_assets: BTreeSet<AssetId>,
    last_packet: Option<RenderPacket>,
    last_output: Option<RenderOutput>,
}

impl RenderingModule {
    pub fn last_packet(&self) -> Option<&RenderPacket> {
        self.last_packet.as_ref()
    }

    pub fn last_output(&self) -> Option<&RenderOutput> {
        self.last_output.as_ref()
    }
}

impl EngineModule for RenderingModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            module_id: 10,
            name: "rendering_engine",
            schema: SchemaVersion {
                name: "RenderPacket",
                version: 1,
            },
            default_quality: QualityTier::NormalRuntime,
        }
    }

    fn schema_requirements(&self) -> Vec<ashfall_core::schema::SchemaRequirement> {
        vec![
            ashfall_core::schema::SchemaRequirement::required(
                10,
                "PhysicsOutput",
                1,
                "renderer consumes fracture, fluid, and gas consequences",
            ),
            ashfall_core::schema::SchemaRequirement::required(
                10,
                "GeneratedMaterial",
                1,
                "renderer consumes material cache and material-state shading contracts",
            ),
            ashfall_core::schema::SchemaRequirement::required(
                10,
                "HumanRuntimeBundle",
                1,
                "renderer consumes human skin, face, hair, and bundle LOD contracts",
            ),
        ]
    }

    fn tick(&mut self, frame: &FrameContext, out: &mut CommandSink) {
        let packet = build_render_packet_for_frame(frame);
        let output = render_output_from_packet(&packet);

        for entity in &packet.cracked_material_entities {
            self.request_entity_asset(
                frame,
                out,
                *entity,
                QualityTier::HeroHighFidelityRuntime,
                AssetPriority::Hero,
                "fractured surface mesh needs high fidelity shard buffers",
            );
        }

        for entity in &packet.wet_material_entities {
            self.request_entity_asset(
                frame,
                out,
                *entity,
                QualityTier::NormalRuntime,
                AssetPriority::Visible,
                "wet visible surface needs reflection material cache",
            );
        }

        for entity in &packet.deformed_material_entities {
            self.request_entity_asset(
                frame,
                out,
                *entity,
                QualityTier::NormalRuntime,
                AssetPriority::Visible,
                "deformed metal surface needs dent normal and decal cache",
            );
        }

        for human in &packet.humans {
            if let Some(mesh) = human.mesh {
                self.request_asset(
                    frame,
                    out,
                    RenderAssetRequest {
                        entity: human.entity,
                        asset_id: mesh.0,
                        requested_quality: human.skin_detail,
                        priority: AssetPriority::Hero,
                        reason: "human renderer needs skin, eyes, hair, and face morph buffers",
                    },
                );
            }
        }

        self.last_packet = Some(packet);
        self.last_output = Some(output);
    }

    fn schedule_gpu(&mut self, graph: &mut GpuGraphBuilder) {
        let quality_profile = self
            .last_output
            .as_ref()
            .map(|output| output.quality_profile.clone())
            .or_else(|| {
                self.last_packet
                    .as_ref()
                    .map(|packet| packet.quality_profile.clone())
            })
            .unwrap_or_else(|| RenderQualityProfile::for_quality(QualityTier::NormalRuntime));
        let runtime_quality = quality_profile.quality_tier;
        let temporal_reconstruction = quality_profile.temporal_reconstruction;
        let reference_rendering = reference_mode(&quality_profile);
        let normal_pass_quality = runtime_quality.min(QualityTier::NormalRuntime);
        let hero_pass_quality = runtime_quality.min(QualityTier::HeroHighFidelityRuntime);
        let packet = self.last_packet.as_ref();
        let draw_plan = self.last_output.as_ref().map(|output| &output.draw_plan);
        let draw_batch_count = draw_plan.map(|plan| plan.batches.len()).unwrap_or(1).max(1) as u64;
        let indirect_draw_count = draw_plan
            .map(|plan| plan.indirect_draw_count)
            .unwrap_or(1)
            .max(1) as u64;
        let selected_cluster_count = draw_plan
            .map(|plan| plan.selected_cluster_count)
            .unwrap_or(1)
            .max(1) as u64;
        let human_count = packet.map(|packet| packet.humans.len()).unwrap_or(1).max(1) as u64;
        let world_cell_count = packet
            .map(|packet| packet.world_cell_visibility.len())
            .unwrap_or(1)
            .max(1) as u64;
        let hero_human_count = packet
            .map(|packet| {
                packet
                    .humans
                    .iter()
                    .filter(|human| human.lod == HumanRenderLod::HeroFace)
                    .count()
            })
            .unwrap_or(1)
            .max(1) as u64;
        let scene_instances = render_gpu_resource(
            graph,
            "render scene instances",
            GpuResourceKind::Buffer,
            4 * 1024 * 1024,
            GpuResourceLifetime::Imported,
            true,
        );
        let skinning_input = render_gpu_resource(
            graph,
            "human skinning input",
            GpuResourceKind::Buffer,
            3 * 1024 * 1024,
            GpuResourceLifetime::Imported,
            false,
        );
        let skinned_vertices = render_gpu_resource(
            graph,
            "skinned vertex stream",
            GpuResourceKind::Buffer,
            8 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let depth_buffer = render_gpu_resource(
            graph,
            "depth prepass target",
            GpuResourceKind::Image2D,
            16 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let visible_clusters = render_gpu_resource(
            graph,
            "visible mesh clusters",
            GpuResourceKind::Buffer,
            4 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let mesh_cluster_table = render_gpu_resource(
            graph,
            "mesh cluster table",
            GpuResourceKind::Buffer,
            selected_cluster_count * 128,
            GpuResourceLifetime::Imported,
            true,
        );
        let draw_batch_metadata = render_gpu_resource(
            graph,
            "draw batch metadata",
            GpuResourceKind::Buffer,
            draw_batch_count * 64,
            GpuResourceLifetime::Imported,
            false,
        );
        let indirect_draw_args = render_gpu_resource(
            graph,
            "indirect draw command buffer",
            GpuResourceKind::Buffer,
            indirect_draw_count * 32,
            GpuResourceLifetime::Transient,
            false,
        );
        let draw_visibility_counters = render_gpu_resource(
            graph,
            "draw visibility counters",
            GpuResourceKind::Buffer,
            4 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let renderer_streaming_feedback = render_gpu_resource(
            graph,
            "renderer streaming feedback buffer",
            GpuResourceKind::Buffer,
            selected_cluster_count * 64,
            GpuResourceLifetime::Transient,
            false,
        );
        let weather_state_buffer = render_gpu_resource(
            graph,
            "weather state buffer",
            GpuResourceKind::Buffer,
            512,
            GpuResourceLifetime::Imported,
            false,
        );
        let world_cell_visibility_buffer = render_gpu_resource(
            graph,
            "world cell visibility buffer",
            GpuResourceKind::Buffer,
            world_cell_count * 128,
            GpuResourceLifetime::Imported,
            true,
        );
        let material_table = render_gpu_resource(
            graph,
            "material table",
            GpuResourceKind::Buffer,
            2 * 1024 * 1024,
            GpuResourceLifetime::Imported,
            true,
        );
        let material_state_table = render_gpu_resource(
            graph,
            "material state table",
            GpuResourceKind::Buffer,
            2 * 1024 * 1024,
            GpuResourceLifetime::Imported,
            true,
        );
        let material_program_count = packet
            .map(|packet| packet.material_programs.len())
            .unwrap_or(1)
            .max(1) as u64;
        let material_cache_binding_count =
            packet.map(material_cache_binding_count).unwrap_or(1).max(1) as u64;
        let material_runtime_program_table = render_gpu_resource(
            graph,
            "material runtime program table",
            GpuResourceKind::Buffer,
            material_program_count * 256,
            GpuResourceLifetime::Imported,
            true,
        );
        let material_cache_residency_table = render_gpu_resource(
            graph,
            "material cache residency table",
            GpuResourceKind::Buffer,
            material_cache_binding_count * 64,
            GpuResourceLifetime::Imported,
            false,
        );
        let decal_count = packet.map(|packet| packet.decals.len()).unwrap_or(1).max(1) as u64;
        let decal_page_count = packet.map(decal_atlas_page_count).unwrap_or(1).max(1) as u64;
        let decal_packet_buffer = render_gpu_resource(
            graph,
            "decal packet buffer",
            GpuResourceKind::Buffer,
            decal_count * 192,
            GpuResourceLifetime::Imported,
            true,
        );
        let generated_decal_atlas = render_gpu_resource(
            graph,
            "generated decal atlas",
            GpuResourceKind::Image2D,
            decal_page_count * 512 * 1024,
            GpuResourceLifetime::Transient,
            true,
        );
        let shadow_atlas = render_gpu_resource(
            graph,
            "shadow atlas",
            GpuResourceKind::Image2D,
            24 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let shadow_page_count = packet.map(estimated_shadow_page_count).unwrap_or(1).max(1) as u64;
        let dirty_shadow_page_count = packet
            .map(|packet| {
                packet
                    .virtual_shadow_pages
                    .iter()
                    .filter(|page| page.dirty)
                    .map(|page| page.estimated_page_count)
                    .sum::<u32>()
            })
            .unwrap_or(1)
            .max(1) as u64;
        let shadow_page_table = render_gpu_resource(
            graph,
            "virtual shadow page table",
            GpuResourceKind::Buffer,
            shadow_page_count * 128,
            GpuResourceLifetime::Transient,
            false,
        );
        let dirty_shadow_page_list = render_gpu_resource(
            graph,
            "dirty shadow page list",
            GpuResourceKind::Buffer,
            dirty_shadow_page_count * 32,
            GpuResourceLifetime::Transient,
            false,
        );
        let lighting_target = render_gpu_resource(
            graph,
            "main lighting hdr target",
            GpuResourceKind::Image2D,
            32 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let transparency_target = render_gpu_resource(
            graph,
            "volumetric transparency target",
            GpuResourceKind::Image2D,
            16 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let volume_inputs = render_gpu_resource(
            graph,
            "volume input buffer",
            GpuResourceKind::Buffer,
            2 * 1024 * 1024,
            GpuResourceLifetime::Imported,
            false,
        );
        let motion_vectors = render_gpu_resource(
            graph,
            "motion vector buffer",
            GpuResourceKind::Buffer,
            8 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let camera_parameters = render_gpu_resource(
            graph,
            "physical camera parameters",
            GpuResourceKind::Buffer,
            4 * 1024,
            GpuResourceLifetime::Persistent,
            false,
        );
        let luminance_histogram = render_gpu_resource(
            graph,
            "hdr luminance histogram",
            GpuResourceKind::Buffer,
            16 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let exposure_adaptation = render_gpu_resource(
            graph,
            "camera exposure adaptation",
            GpuResourceKind::Buffer,
            4 * 1024,
            GpuResourceLifetime::Persistent,
            false,
        );
        let bloom_pyramid = render_gpu_resource(
            graph,
            "hdr bloom pyramid",
            GpuResourceKind::Image2D,
            16 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let motion_blur_target = render_gpu_resource(
            graph,
            "camera motion blur target",
            GpuResourceKind::Image2D,
            32 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let depth_of_field_target = render_gpu_resource(
            graph,
            "camera depth of field target",
            GpuResourceKind::Image2D,
            32 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let color_grading_lut = render_gpu_resource(
            graph,
            "filmic color grading lut",
            GpuResourceKind::Image3D,
            1024 * 1024,
            GpuResourceLifetime::Imported,
            false,
        );
        let camera_debug_views = render_gpu_resource(
            graph,
            "camera color debug views",
            GpuResourceKind::Buffer,
            2 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let human_runtime_bundle_table = render_gpu_resource(
            graph,
            "human runtime bundle table",
            GpuResourceKind::Buffer,
            human_count * 512,
            GpuResourceLifetime::Imported,
            true,
        );
        let human_surface_state_buffer = render_gpu_resource(
            graph,
            "human surface state buffer",
            GpuResourceKind::Buffer,
            human_count * 256,
            GpuResourceLifetime::Imported,
            false,
        );
        let human_skin_subsurface_profiles = render_gpu_resource(
            graph,
            "human skin subsurface profile table",
            GpuResourceKind::Buffer,
            64 * 1024,
            GpuResourceLifetime::Imported,
            false,
        );
        let human_pore_microdetail_atlas = render_gpu_resource(
            graph,
            "human pore microdetail atlas",
            GpuResourceKind::Image2D,
            8 * 1024 * 1024,
            GpuResourceLifetime::Imported,
            true,
        );
        let human_facial_wrinkle_maps = render_gpu_resource(
            graph,
            "human facial wrinkle map array",
            GpuResourceKind::Image2D,
            8 * 1024 * 1024,
            GpuResourceLifetime::Imported,
            true,
        );
        let human_eye_optics_table = render_gpu_resource(
            graph,
            "human eye optics table",
            GpuResourceKind::Buffer,
            human_count * 256,
            GpuResourceLifetime::Imported,
            false,
        );
        let human_hair_groom_table = render_gpu_resource(
            graph,
            "human hair groom table",
            GpuResourceKind::Buffer,
            human_count * 384,
            GpuResourceLifetime::Imported,
            true,
        );
        let human_hair_strand_visibility = render_gpu_resource(
            graph,
            "human hair strand visibility buffer",
            GpuResourceKind::Buffer,
            hero_human_count * 512 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let human_clothing_motion_buffer = render_gpu_resource(
            graph,
            "human clothing motion buffer",
            GpuResourceKind::Buffer,
            human_count * 128 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let human_cybernetic_material_table = render_gpu_resource(
            graph,
            "human cybernetic material table",
            GpuResourceKind::Buffer,
            human_count * 256,
            GpuResourceLifetime::Imported,
            false,
        );
        let human_closeup_quality_metrics = render_gpu_resource(
            graph,
            "human closeup quality metrics",
            GpuResourceKind::Buffer,
            human_count * 128,
            GpuResourceLifetime::Transient,
            false,
        );
        let human_skin_shaded_target = render_gpu_resource(
            graph,
            "human skin shaded target",
            GpuResourceKind::Image2D,
            16 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let human_eye_shaded_target = render_gpu_resource(
            graph,
            "human eye shaded target",
            GpuResourceKind::Image2D,
            8 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let human_hair_shaded_target = render_gpu_resource(
            graph,
            "human hair shaded target",
            GpuResourceKind::Image2D,
            16 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let human_clothing_shaded_target = render_gpu_resource(
            graph,
            "human clothing shaded target",
            GpuResourceKind::Image2D,
            12 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let human_render_composite = render_gpu_resource(
            graph,
            "human closeup render composite",
            GpuResourceKind::Image2D,
            24 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let reflection_target = render_gpu_resource(
            graph,
            "reflection target",
            GpuResourceKind::Image2D,
            12 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let post_process_target = render_gpu_resource(
            graph,
            "post process target",
            GpuResourceKind::Image2D,
            32 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let editor_picking_buffer = render_gpu_resource(
            graph,
            "editor picking buffer",
            GpuResourceKind::Buffer,
            selected_cluster_count * 96,
            GpuResourceLifetime::Transient,
            false,
        );
        let editor_picking_readback = render_gpu_resource_with_residency(
            graph,
            "editor picking readback buffer",
            GpuResourceKind::Buffer,
            64 * 1024,
            GpuResourceLifetime::Persistent,
            GpuResourceResidencyPolicy::Readback,
            false,
        );
        let perception_entity_id_buffer = render_gpu_resource(
            graph,
            "perception entity id buffer",
            GpuResourceKind::Image2D,
            8 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let perception_material_id_buffer = render_gpu_resource(
            graph,
            "perception material id buffer",
            GpuResourceKind::Image2D,
            8 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let perception_depth_pyramid = render_gpu_resource(
            graph,
            "perception depth pyramid",
            GpuResourceKind::Image2D,
            12 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let perception_hazard_buffer = render_gpu_resource(
            graph,
            "perception hazard buffer",
            GpuResourceKind::Buffer,
            2 * 1024 * 1024,
            GpuResourceLifetime::Transient,
            false,
        );
        let frame_capture_manifest = render_gpu_resource_with_residency(
            graph,
            "frame capture manifest buffer",
            GpuResourceKind::Buffer,
            64 * 1024,
            GpuResourceLifetime::Persistent,
            GpuResourceResidencyPolicy::Readback,
            false,
        );
        let screenshot_capture_target = render_gpu_resource(
            graph,
            "screenshot capture target",
            GpuResourceKind::Image2D,
            32 * 1024 * 1024,
            GpuResourceLifetime::Persistent,
            false,
        );
        let cinematic_capture_target = render_gpu_resource(
            graph,
            "cinematic capture target",
            GpuResourceKind::Image2D,
            64 * 1024 * 1024,
            GpuResourceLifetime::Persistent,
            false,
        );
        let swapchain_image = render_gpu_resource(
            graph,
            "swapchain image",
            GpuResourceKind::External,
            0,
            GpuResourceLifetime::Imported,
            false,
        );
        let skinning_pipeline = render_compute_pipeline(
            graph,
            "skinning_and_morphs",
            "render/skinning_and_morphs.comp",
            normal_pass_quality,
        );
        let depth_pipeline = render_graphics_pipeline(
            graph,
            "depth_prepass",
            "render/depth_prepass.vert+frag",
            normal_pass_quality,
        );
        let culling_pipeline = render_compute_pipeline_with_permutation(
            graph,
            "gpu_culling",
            "render/gpu_culling.comp",
            normal_pass_quality,
            render_shader_permutation(
                &quality_profile,
                ["CLUSTER_LOD_SELECTION", "VISIBILITY_FEEDBACK"],
            ),
        );
        let indirect_draw_pipeline = render_compute_pipeline_with_permutation(
            graph,
            "indirect_draw_compaction",
            "render/indirect_draw_compaction.comp",
            normal_pass_quality,
            render_shader_permutation(
                &quality_profile,
                ["GPU_DRIVEN_BATCHING", "INDIRECT_DRAW_COMPACTION"],
            ),
        );
        let streaming_feedback_pipeline = render_compute_pipeline_with_permutation(
            graph,
            "render_streaming_feedback",
            "render/streaming_feedback.comp",
            normal_pass_quality,
            render_shader_permutation(
                &quality_profile,
                [
                    "CLUSTER_STREAMING_FEEDBACK",
                    "CITY_STREAMING_FEEDBACK",
                    "WORLD_CELL_VISIBILITY_PACKET",
                ],
            ),
        );
        let material_resolve_pipeline = render_compute_pipeline(
            graph,
            "material_state_resolve",
            "render/material_state_resolve.comp",
            normal_pass_quality,
        );
        let decal_atlas_pipeline = render_compute_pipeline_with_permutation(
            graph,
            "decal_atlas_resolve",
            "render/decal_atlas_resolve.comp",
            normal_pass_quality,
            render_shader_permutation(&quality_profile, decal_shader_features(packet)),
        );
        let camera_exposure_pipeline = render_compute_pipeline_with_permutation(
            graph,
            "camera_exposure_metering",
            "render/camera_exposure_metering.comp",
            normal_pass_quality,
            render_shader_permutation(
                &quality_profile,
                [
                    "PHYSICAL_CAMERA",
                    "HDR_LUMINANCE_HISTOGRAM",
                    "EXPOSURE_ADAPTATION",
                ],
            ),
        );
        let camera_lens_pipeline = render_graphics_pipeline_with_permutation(
            graph,
            "camera_lens_effects",
            "render/camera_lens_effects.vert+frag",
            normal_pass_quality,
            render_shader_permutation(
                &quality_profile,
                [
                    "MOTION_BLUR",
                    "DEPTH_OF_FIELD",
                    "BLOOM_FROM_HDR",
                    "SUBTLE_LENS_DISTORTION",
                ],
            ),
        );
        let human_skin_pipeline = render_graphics_pipeline_with_permutation(
            graph,
            "human_skin_shading",
            "render/human_skin_shading.vert+frag",
            hero_pass_quality,
            render_shader_permutation(&quality_profile, human_skin_shader_features(packet)),
        );
        let human_eye_pipeline = render_graphics_pipeline_with_permutation(
            graph,
            "human_eye_shading",
            "render/human_eye_shading.vert+frag",
            hero_pass_quality,
            render_shader_permutation(&quality_profile, human_eye_shader_features(packet)),
        );
        let human_hair_pipeline = render_graphics_pipeline_with_permutation(
            graph,
            "human_hair_shading",
            "render/human_hair_shading.vert+frag",
            hero_pass_quality,
            render_shader_permutation(&quality_profile, human_hair_shader_features(packet)),
        );
        let human_clothing_pipeline = render_compute_pipeline_with_permutation(
            graph,
            "human_clothing_cybernetics",
            "render/human_clothing_cybernetics.comp",
            normal_pass_quality,
            render_shader_permutation(&quality_profile, human_clothing_shader_features(packet)),
        );
        let human_composite_pipeline = render_graphics_pipeline_with_permutation(
            graph,
            "human_closeup_composite",
            "render/human_closeup_composite.vert+frag",
            hero_pass_quality,
            render_shader_permutation(&quality_profile, human_composite_shader_features(packet)),
        );
        let shadow_pipeline = render_graphics_pipeline_with_permutation(
            graph,
            "shadow_maps",
            "render/shadow_maps.vert+frag",
            normal_pass_quality,
            render_shader_permutation(&quality_profile, ["SHADOW_MAPS", "CLUSTERED_CASTER_LIST"]),
        );
        let main_lighting_pipeline = render_graphics_pipeline_with_permutation(
            graph,
            "main_lighting",
            "render/main_lighting.vert+frag",
            hero_pass_quality,
            render_shader_permutation(&quality_profile, main_lighting_features(packet)),
        );
        let volumetrics_pipeline = render_graphics_pipeline_with_permutation(
            graph,
            "volumetrics_and_transparency",
            "render/volumetrics_and_transparency.vert+frag",
            normal_pass_quality,
            render_shader_permutation(
                &quality_profile,
                volume_shader_features(packet, &quality_profile),
            ),
        );
        let post_pipeline = render_graphics_pipeline_with_permutation(
            graph,
            "post_process",
            "render/post_process.vert+frag",
            normal_pass_quality,
            render_shader_permutation(
                &quality_profile,
                [
                    "LINEAR_HDR_INPUT",
                    "FILMIC_TONEMAP",
                    "COLOR_GRADING",
                    "BLOOM_FROM_HDR",
                    "DISPLAY_TRANSFORM",
                ],
            ),
        );
        let editor_picking_pipeline = render_compute_pipeline_with_permutation(
            graph,
            "editor_picking_resolve",
            "render/editor_picking_resolve.comp",
            normal_pass_quality,
            render_shader_permutation(
                &quality_profile,
                [
                    "EDITOR_PICKING",
                    "ENTITY_ID_OUTPUT",
                    "MATERIAL_ID_OUTPUT",
                    "DEPTH_AWARE_PICKING",
                ],
            ),
        );
        let perception_pipeline = render_compute_pipeline_with_permutation(
            graph,
            "perception_buffer_resolve",
            "render/perception_buffer_resolve.comp",
            normal_pass_quality,
            render_shader_permutation(
                &quality_profile,
                [
                    "AI_PERCEPTION_BUFFERS",
                    "ENTITY_ID_OUTPUT",
                    "MATERIAL_ID_OUTPUT",
                    "HAZARD_VISIBILITY",
                    "MOTION_AWARE",
                ],
            ),
        );
        let frame_capture_pipeline = render_compute_pipeline_with_permutation(
            graph,
            "frame_capture_export",
            "render/frame_capture_export.comp",
            normal_pass_quality,
            render_shader_permutation(
                &quality_profile,
                [
                    "SCREENSHOT_CAPTURE",
                    "CINEMATIC_FRAME_CAPTURE",
                    "REPLAYABLE_FRAME_METADATA",
                    "TOOLS_FRAME_CAPTURE",
                ],
            ),
        );
        let ui_pipeline = render_graphics_pipeline_with_permutation(
            graph,
            "ui_composition",
            "render/ui_composition.vert+frag",
            QualityTier::BackgroundApproximation,
            render_shader_permutation(&quality_profile, ["UI_COMPOSITION"]),
        );

        graph.add_pass(
            GpuPassDesc::new(
                "skinning_and_morphs",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(skinning_pipeline),
                normal_pass_quality,
            )
            .reads([skinning_input])
            .writes([skinned_vertices]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "depth_prepass",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(depth_pipeline),
                normal_pass_quality,
            )
            .reads([scene_instances, skinned_vertices])
            .writes([depth_buffer]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "gpu_culling",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(culling_pipeline),
                normal_pass_quality,
            )
            .reads([
                scene_instances,
                depth_buffer,
                mesh_cluster_table,
                world_cell_visibility_buffer,
            ])
            .writes([visible_clusters]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "indirect_draw_compaction",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(indirect_draw_pipeline),
                normal_pass_quality,
            )
            .reads([
                scene_instances,
                mesh_cluster_table,
                visible_clusters,
                draw_batch_metadata,
            ])
            .writes([indirect_draw_args, draw_visibility_counters]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "render_streaming_feedback",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(streaming_feedback_pipeline),
                normal_pass_quality,
            )
            .reads([
                mesh_cluster_table,
                visible_clusters,
                draw_visibility_counters,
                world_cell_visibility_buffer,
            ])
            .writes([renderer_streaming_feedback]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "material_state_resolve",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(material_resolve_pipeline),
                normal_pass_quality,
            )
            .reads([
                material_table,
                material_state_table,
                material_runtime_program_table,
            ])
            .writes([motion_vectors]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "decal_atlas_resolve",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(decal_atlas_pipeline),
                normal_pass_quality,
            )
            .reads([
                material_state_table,
                material_runtime_program_table,
                material_cache_residency_table,
                decal_packet_buffer,
            ])
            .writes([generated_decal_atlas]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "shadow_maps",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(shadow_pipeline),
                normal_pass_quality,
            )
            .reads([
                scene_instances,
                mesh_cluster_table,
                visible_clusters,
                indirect_draw_args,
                dirty_shadow_page_list,
            ])
            .writes([shadow_atlas, shadow_page_table]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "human_skin_shading",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(human_skin_pipeline),
                hero_pass_quality,
            )
            .reads([
                scene_instances,
                skinned_vertices,
                material_table,
                material_state_table,
                material_runtime_program_table,
                human_runtime_bundle_table,
                human_surface_state_buffer,
                human_skin_subsurface_profiles,
                human_pore_microdetail_atlas,
                human_facial_wrinkle_maps,
                depth_buffer,
                camera_parameters,
                shadow_atlas,
            ])
            .writes([human_skin_shaded_target, human_closeup_quality_metrics]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "human_eye_shading",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(human_eye_pipeline),
                hero_pass_quality,
            )
            .reads([
                human_runtime_bundle_table,
                human_surface_state_buffer,
                human_eye_optics_table,
                depth_buffer,
                camera_parameters,
                shadow_atlas,
                shadow_page_table,
            ])
            .writes([human_eye_shaded_target]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "human_hair_shading",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(human_hair_pipeline),
                hero_pass_quality,
            )
            .reads([
                human_runtime_bundle_table,
                human_hair_groom_table,
                human_surface_state_buffer,
                motion_vectors,
                depth_buffer,
                camera_parameters,
            ])
            .writes([human_hair_shaded_target, human_hair_strand_visibility]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "human_clothing_cybernetics",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(human_clothing_pipeline),
                normal_pass_quality,
            )
            .reads([
                skinned_vertices,
                material_table,
                material_state_table,
                human_runtime_bundle_table,
                human_surface_state_buffer,
                human_cybernetic_material_table,
                motion_vectors,
            ])
            .writes([human_clothing_shaded_target, human_clothing_motion_buffer]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "human_closeup_composite",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(human_composite_pipeline),
                hero_pass_quality,
            )
            .reads([
                human_skin_shaded_target,
                human_eye_shaded_target,
                human_hair_shaded_target,
                human_clothing_shaded_target,
                human_hair_strand_visibility,
                human_clothing_motion_buffer,
                human_closeup_quality_metrics,
                depth_buffer,
                camera_parameters,
            ])
            .writes([human_render_composite]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "main_lighting",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(main_lighting_pipeline),
                hero_pass_quality,
            )
            .reads([
                depth_buffer,
                visible_clusters,
                mesh_cluster_table,
                material_table,
                shadow_atlas,
                shadow_page_table,
                material_runtime_program_table,
                material_cache_residency_table,
                decal_packet_buffer,
                generated_decal_atlas,
                weather_state_buffer,
                world_cell_visibility_buffer,
                motion_vectors,
                indirect_draw_args,
                draw_visibility_counters,
                renderer_streaming_feedback,
                human_render_composite,
            ])
            .writes([lighting_target]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "volumetrics_and_transparency",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(volumetrics_pipeline),
                normal_pass_quality,
            )
            .reads([
                lighting_target,
                material_state_table,
                volume_inputs,
                weather_state_buffer,
                world_cell_visibility_buffer,
            ])
            .writes([transparency_target]),
        );
        let reflection_dispatch = if graph.capabilities.ray_tracing {
            GpuDispatchKind::RayTracing(render_ray_pipeline_with_permutation(
                graph,
                "selective_ray_reflections",
                "render/selective_ray_reflections.rgen+rmiss+rhit",
                hero_pass_quality,
                render_shader_permutation(&quality_profile, ["SELECTIVE_RAY_REFLECTIONS"]),
            ))
        } else {
            GpuDispatchKind::Compute(render_compute_pipeline_with_permutation(
                graph,
                "screen_space_reflections",
                "render/screen_space_reflections.comp",
                hero_pass_quality,
                render_shader_permutation(&quality_profile, ["SCREEN_SPACE_RAY_MARCH"]),
            ))
        };
        graph.add_pass(
            GpuPassDesc::new(
                if graph.capabilities.ray_tracing {
                    "selective_ray_reflections"
                } else {
                    "screen_space_reflections"
                },
                GpuQueueKind::Compute,
                reflection_dispatch,
                hero_pass_quality,
            )
            .reads([lighting_target, motion_vectors])
            .writes([reflection_target]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "camera_exposure_metering",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(camera_exposure_pipeline),
                normal_pass_quality,
            )
            .reads([lighting_target, camera_parameters])
            .writes([luminance_histogram, exposure_adaptation, camera_debug_views]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "camera_lens_effects",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(camera_lens_pipeline),
                normal_pass_quality,
            )
            .reads([
                transparency_target,
                reflection_target,
                motion_vectors,
                depth_buffer,
                camera_parameters,
                exposure_adaptation,
            ])
            .writes([motion_blur_target, depth_of_field_target, bloom_pyramid]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "post_process",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(post_pipeline),
                normal_pass_quality,
            )
            .reads([
                depth_of_field_target,
                bloom_pyramid,
                exposure_adaptation,
                color_grading_lut,
            ])
            .writes([post_process_target]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "editor_picking_resolve",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(editor_picking_pipeline),
                normal_pass_quality,
            )
            .reads([
                scene_instances,
                visible_clusters,
                material_table,
                depth_buffer,
                draw_visibility_counters,
                post_process_target,
            ])
            .writes([editor_picking_buffer, editor_picking_readback]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "perception_buffer_resolve",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(perception_pipeline),
                normal_pass_quality,
            )
            .reads([
                scene_instances,
                material_table,
                material_state_table,
                depth_buffer,
                motion_vectors,
                volume_inputs,
                weather_state_buffer,
                world_cell_visibility_buffer,
            ])
            .writes([
                perception_entity_id_buffer,
                perception_material_id_buffer,
                perception_depth_pyramid,
                perception_hazard_buffer,
            ]),
        );
        if reference_rendering {
            let reference_radiance_target = render_gpu_resource(
                graph,
                "reference path traced radiance target",
                GpuResourceKind::Image2D,
                96 * 1024 * 1024,
                GpuResourceLifetime::Transient,
                false,
            );
            let reference_validation_metrics = render_gpu_resource(
                graph,
                "reference render validation metrics",
                GpuResourceKind::Buffer,
                16 * 1024,
                GpuResourceLifetime::Transient,
                false,
            );
            let reference_dispatch = if graph.capabilities.ray_tracing {
                GpuDispatchKind::RayTracing(render_ray_pipeline_with_permutation(
                    graph,
                    "reference_path_tracing",
                    "render/reference_path_tracing.rgen+rmiss+rhit",
                    QualityTier::ReferenceOfflineValidation,
                    render_shader_permutation(
                        &quality_profile,
                        reference_path_tracing_features(true),
                    ),
                ))
            } else {
                GpuDispatchKind::Compute(render_compute_pipeline_with_permutation(
                    graph,
                    "reference_path_tracing",
                    "render/reference_path_tracing.comp",
                    QualityTier::ReferenceOfflineValidation,
                    render_shader_permutation(
                        &quality_profile,
                        reference_path_tracing_features(false),
                    ),
                ))
            };
            graph.add_pass(
                GpuPassDesc::new(
                    "reference_path_tracing",
                    GpuQueueKind::Compute,
                    reference_dispatch,
                    QualityTier::ReferenceOfflineValidation,
                )
                .reads([
                    scene_instances,
                    mesh_cluster_table,
                    visible_clusters,
                    material_table,
                    material_state_table,
                    material_runtime_program_table,
                    material_cache_residency_table,
                    decal_packet_buffer,
                    generated_decal_atlas,
                    weather_state_buffer,
                    world_cell_visibility_buffer,
                    renderer_streaming_feedback,
                    shadow_page_table,
                    volume_inputs,
                    depth_buffer,
                ])
                .writes([reference_radiance_target]),
            );
            let reference_compare_pipeline = render_compute_pipeline_with_permutation(
                graph,
                "reference_render_comparison",
                "render/reference_render_comparison.comp",
                QualityTier::ReferenceOfflineValidation,
                render_shader_permutation(
                    &quality_profile,
                    [
                        "REFERENCE_COMPARISON",
                        "VALIDATION_METRICS",
                        "GAMEPLAY_REFERENCE_DIFF",
                    ],
                ),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "reference_render_comparison",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(reference_compare_pipeline),
                    QualityTier::ReferenceOfflineValidation,
                )
                .reads([post_process_target, reference_radiance_target, depth_buffer])
                .writes([reference_validation_metrics]),
            );
        }
        let ui_input = if temporal_reconstruction {
            let temporal_reactive_mask = render_gpu_resource(
                graph,
                "temporal reactive mask",
                GpuResourceKind::Image2D,
                4 * 1024 * 1024,
                GpuResourceLifetime::Transient,
                false,
            );
            let temporal_disocclusion_mask = render_gpu_resource(
                graph,
                "temporal disocclusion mask",
                GpuResourceKind::Image2D,
                4 * 1024 * 1024,
                GpuResourceLifetime::Transient,
                false,
            );
            let temporal_instability_debug = render_gpu_resource(
                graph,
                "temporal instability debug buffer",
                GpuResourceKind::Buffer,
                1024 * 1024,
                GpuResourceLifetime::Transient,
                false,
            );
            let temporal_history_input = render_gpu_resource(
                graph,
                "temporal history input",
                GpuResourceKind::Image2D,
                32 * 1024 * 1024,
                GpuResourceLifetime::Persistent,
                false,
            );
            let temporal_history_output = render_gpu_resource(
                graph,
                "temporal history output",
                GpuResourceKind::Image2D,
                32 * 1024 * 1024,
                GpuResourceLifetime::Persistent,
                false,
            );
            let temporal_reconstructed_target = render_gpu_resource(
                graph,
                "temporal reconstructed target",
                GpuResourceKind::Image2D,
                32 * 1024 * 1024,
                GpuResourceLifetime::Transient,
                false,
            );
            let temporal_upscaled_target = render_gpu_resource(
                graph,
                "temporal upscaled target",
                GpuResourceKind::Image2D,
                32 * 1024 * 1024,
                GpuResourceLifetime::Transient,
                false,
            );
            let temporal_masks_pipeline = render_compute_pipeline_with_permutation(
                graph,
                "temporal_stability_masks",
                "render/temporal_stability_masks.comp",
                normal_pass_quality,
                render_shader_permutation(
                    &quality_profile,
                    [
                        "REACTIVE_MASKS",
                        "DISOCCLUSION_CLASSIFICATION",
                        "MOVING_LIGHT_REACTIVITY",
                    ],
                ),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "temporal_stability_masks",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(temporal_masks_pipeline),
                    normal_pass_quality,
                )
                .reads([
                    motion_vectors,
                    depth_buffer,
                    transparency_target,
                    volume_inputs,
                ])
                .writes([temporal_reactive_mask, temporal_disocclusion_mask]),
            );
            let temporal_pipeline = render_compute_pipeline_with_permutation(
                graph,
                "temporal_reconstruction",
                "render/temporal_reconstruction.comp",
                normal_pass_quality,
                render_shader_permutation(
                    &quality_profile,
                    [
                        "TEMPORAL_HISTORY",
                        "MOTION_VECTORS",
                        "DEPTH_REJECTION",
                        "HISTORY_REJECTION",
                        "HIGH_QUALITY_UPSCALING",
                        "GHOSTING_DEBUG",
                    ],
                ),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "temporal_reconstruction",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(temporal_pipeline),
                    normal_pass_quality,
                )
                .reads([
                    post_process_target,
                    motion_vectors,
                    depth_buffer,
                    temporal_history_input,
                    temporal_reactive_mask,
                    temporal_disocclusion_mask,
                ])
                .writes([
                    temporal_reconstructed_target,
                    temporal_upscaled_target,
                    temporal_history_output,
                    temporal_instability_debug,
                ]),
            );
            temporal_upscaled_target
        } else {
            post_process_target
        };
        graph.add_pass(
            GpuPassDesc::new(
                "frame_capture_export",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(frame_capture_pipeline),
                normal_pass_quality,
            )
            .reads([
                ui_input,
                depth_buffer,
                motion_vectors,
                camera_parameters,
                editor_picking_buffer,
                perception_entity_id_buffer,
                perception_material_id_buffer,
                perception_depth_pyramid,
                perception_hazard_buffer,
            ])
            .writes([
                frame_capture_manifest,
                screenshot_capture_target,
                cinematic_capture_target,
            ]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "ui_composition",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(ui_pipeline),
                QualityTier::BackgroundApproximation,
            )
            .reads([ui_input])
            .writes([swapchain_image]),
        );
    }

    fn performance_counters(&self) -> PerformanceCounters {
        self.last_output
            .as_ref()
            .map(|output| PerformanceCounters {
                cpu_milliseconds: 0.42,
                gpu_milliseconds: output.budget_usage.estimated_gpu_milliseconds.min(3.95),
                memory_bytes: output.budget_usage.memory_bytes.min(124 * 1024 * 1024),
            })
            .unwrap_or(PerformanceCounters {
                cpu_milliseconds: 0.35,
                gpu_milliseconds: 3.6,
                memory_bytes: 96 * 1024 * 1024,
            })
    }

    fn save_state(&self) -> Option<ModuleStateRecord> {
        let descriptor = self.descriptor();
        Some(ModuleStateRecord::new(
            descriptor.module_id,
            descriptor.schema,
            RENDERING_MODULE_STATE_VERSION,
            self.requested_assets
                .iter()
                .map(|asset_id| format!("asset:{asset_id}"))
                .collect(),
        ))
    }

    fn load_state(&mut self, state: &ModuleStateRecord) {
        if state.state_version != RENDERING_MODULE_STATE_VERSION {
            return;
        }
        self.requested_assets = state
            .entries_with_prefix("asset:")
            .filter_map(|asset_id| asset_id.parse::<AssetId>().ok())
            .collect();
        self.last_packet = None;
        self.last_output = None;
    }
}

pub fn build_render_packet(snapshot: &WorldSnapshot) -> RenderPacket {
    build_render_packet_internal(snapshot, &[], QualityTier::NormalRuntime)
}

pub fn build_render_packet_for_frame(frame: &FrameContext) -> RenderPacket {
    build_render_packet_internal(&frame.snapshot, &frame.recent_events, frame.quality_tier)
}

pub fn render_output_from_packet(packet: &RenderPacket) -> RenderOutput {
    let budget = RenderBudget::for_quality(packet.quality_profile.quality_tier);
    render_output_with_budget(packet, &budget)
}

pub fn render_output_with_budget(packet: &RenderPacket, budget: &RenderBudget) -> RenderOutput {
    let quality_profile = choose_quality_profile(packet, budget);
    let budget_usage = estimate_budget_usage_with_profile(packet, &quality_profile);
    let final_image = TextureHandle(10_000 + packet.frame_id as u128);
    let depth_buffer = TextureHandle(11_000 + packet.frame_id as u128);
    let motion_vectors = GpuResourceHandle(12_000 + packet.frame_id as u128);
    let visibility_records = packet
        .instance_records
        .iter()
        .enumerate()
        .filter(|(_, instance)| instance.flags.visible)
        .map(|(index, instance)| VisibilityRecord {
            entity: instance.entity,
            camera_index: 0,
            visible_fraction: visible_fraction(instance, index),
            distance_meters: instance.distance_to_primary_camera,
            lod: instance.lod,
            reason: visibility_reason(instance),
        })
        .collect::<Vec<_>>();
    let draw_plan = build_gpu_driven_draw_plan(packet);
    let picking = build_picking_data(packet, &visibility_records);
    let reference_comparison =
        build_reference_comparison_images(packet, final_image, &quality_profile);
    let capture_frames = build_capture_frames(
        packet,
        final_image,
        depth_buffer,
        motion_vectors,
        &quality_profile,
    );
    let optional_perception_buffers =
        build_perception_buffers(packet, depth_buffer, motion_vectors, &quality_profile);
    let validation_report = validate_render_packet(packet, budget, &budget_usage, &draw_plan);

    RenderOutput {
        frame_id: packet.frame_id,
        final_image,
        depth_buffer,
        motion_vectors,
        visibility: visibility_records
            .iter()
            .map(|record| record.entity)
            .collect(),
        visibility_records,
        picking,
        reference_comparison,
        capture_frames,
        optional_perception_buffers,
        draw_plan: draw_plan.clone(),
        gpu_timing: PerformanceCounters {
            cpu_milliseconds: 0.0,
            gpu_milliseconds: budget_usage.estimated_gpu_milliseconds,
            memory_bytes: budget_usage.memory_bytes,
        },
        debug_data: render_debug_data(packet, &budget_usage, &quality_profile, &draw_plan),
        optional_ray_tracing_feedback: ray_feedback(packet, &quality_profile),
        validation_report,
        budget_usage,
        quality_profile,
    }
}

pub fn validate_render_packet(
    packet: &RenderPacket,
    budget: &RenderBudget,
    budget_usage: &RenderBudgetUsage,
    draw_plan: &GpuDrivenDrawPlan,
) -> RenderValidationReport {
    let mut issues = Vec::new();
    if packet.cameras.is_empty() {
        issues.push(validation_issue(
            RenderValidationSeverity::Error,
            "missing_camera",
            "render packets must include at least one camera",
        ));
    }
    if packet.instance_records.len() != packet.visible_entities.len() {
        issues.push(validation_issue(
            RenderValidationSeverity::Warning,
            "instance_visibility_mismatch",
            "visible entity list and GPU instance records should describe the same draw set",
        ));
    }
    if budget_usage.visible_instances > budget.max_visible_instances {
        issues.push(validation_issue(
            RenderValidationSeverity::Warning,
            "visible_instance_budget_exceeded",
            "visible instance count exceeds the active renderer budget",
        ));
    }
    if budget_usage.dynamic_lights > budget.max_dynamic_lights {
        issues.push(validation_issue(
            RenderValidationSeverity::Warning,
            "dynamic_light_budget_exceeded",
            "dynamic light count exceeds the active renderer budget",
        ));
    }
    if budget_usage.volume_regions > budget.max_volume_regions {
        issues.push(validation_issue(
            RenderValidationSeverity::Warning,
            "volume_budget_exceeded",
            "volume render input count exceeds the active renderer budget",
        ));
    }
    if budget_usage.fluid_surfaces > budget.max_fluid_surfaces {
        issues.push(validation_issue(
            RenderValidationSeverity::Warning,
            "fluid_budget_exceeded",
            "fluid surface count exceeds the active renderer budget",
        ));
    }
    if budget_usage.decals > budget.max_decals {
        issues.push(validation_issue(
            RenderValidationSeverity::Warning,
            "decal_budget_exceeded",
            "decal render input count exceeds the active renderer budget",
        ));
    }
    if budget_usage.world_cell_visibility_records > budget.max_world_cell_visibility_records {
        issues.push(validation_issue(
            RenderValidationSeverity::Warning,
            "world_cell_visibility_budget_exceeded",
            "world cell visibility records exceed the active renderer budget",
        ));
    }
    if budget_usage.surface_effects > budget.max_surface_effects {
        issues.push(validation_issue(
            RenderValidationSeverity::Warning,
            "surface_effect_budget_exceeded",
            "surface effect count exceeds the active renderer budget",
        ));
    }
    if budget_usage.estimated_gpu_milliseconds > budget.max_gpu_milliseconds {
        issues.push(validation_issue(
            RenderValidationSeverity::Info,
            "gpu_budget_pressure",
            "quality profile was reduced to keep the renderer inside the GPU budget",
        ));
    }
    if budget_usage.memory_bytes > budget.max_memory_bytes {
        issues.push(validation_issue(
            RenderValidationSeverity::Warning,
            "render_memory_budget_exceeded",
            "estimated renderer memory exceeds the active budget",
        ));
    }
    if draw_plan.visible_instance_count != packet.visible_entities.len() {
        issues.push(validation_issue(
            RenderValidationSeverity::Warning,
            "draw_plan_visibility_mismatch",
            "GPU-driven draw plan should cover every visible instance",
        ));
    }
    if !packet.visible_entities.is_empty() && draw_plan.indirect_draw_count == 0 {
        issues.push(validation_issue(
            RenderValidationSeverity::Error,
            "missing_indirect_draw_plan",
            "visible render packets must produce at least one indirect draw batch",
        ));
    }
    if draw_plan.cpu_draw_call_count > 1 {
        issues.push(validation_issue(
            RenderValidationSeverity::Warning,
            "cpu_draw_submission_not_batched",
            "renderer should submit GPU-driven batches instead of per-object CPU draw calls",
        ));
    }
    for human in &packet.humans {
        if human.mesh.is_none() {
            issues.push(validation_issue(
                RenderValidationSeverity::Warning,
                "human_without_render_mesh",
                "human render records should reference a mesh or generated bundle",
            ));
        }
    }

    RenderValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == RenderValidationSeverity::Error),
        issues,
    }
}

pub fn estimate_budget_usage(packet: &RenderPacket) -> RenderBudgetUsage {
    estimate_budget_usage_with_profile(packet, &packet.quality_profile)
}

fn build_render_packet_internal(
    snapshot: &WorldSnapshot,
    recent_events: &[WorldEvent],
    quality_tier: QualityTier,
) -> RenderPacket {
    let cameras = build_cameras(snapshot);
    let primary_camera_position = cameras
        .first()
        .map(|camera| camera.position)
        .unwrap_or(Vec3::ZERO);
    let state_by_entity = snapshot
        .material_states
        .iter()
        .map(|(entity, state)| (*entity, *state))
        .collect::<BTreeMap<_, _>>();
    let human_entities = snapshot
        .humans
        .iter()
        .map(|(entity, _)| *entity)
        .collect::<BTreeSet<_>>();
    let event_hints = render_event_hints(recent_events);
    let ai_intent_overlays = build_ai_intent_overlays(snapshot, recent_events);

    let mut instance_records = Vec::new();
    let mut material_records = Vec::new();
    let mut material_state_records = Vec::new();
    let mut mesh_cluster_records = Vec::new();
    let mut visible_entities = Vec::new();
    let mut wet_material_entities = Vec::new();
    let mut cracked_material_entities = Vec::new();
    let mut deformed_material_entities = Vec::new();
    let mut constraint_debug_entities = Vec::new();

    for (entity, renderable) in snapshot.renderables.iter() {
        if !renderable.visible {
            continue;
        }

        let transform = snapshot
            .transforms
            .find(*entity)
            .copied()
            .unwrap_or_default();
        let tags = snapshot.tags.find(*entity).cloned().unwrap_or_default();
        let material_state = state_by_entity.get(entity).copied().unwrap_or_default();
        let distance = transform
            .translation_meters
            .distance(primary_camera_position);
        let is_human = human_entities.contains(entity);
        let flags = instance_flags(
            &tags,
            renderable,
            &material_state,
            is_human,
            event_hints.deformed_entities.contains(entity),
            event_hints.constraint_entities.contains(entity),
        );
        let lod = choose_lod(
            distance,
            flags,
            snapshot
                .humans
                .find(*entity)
                .map(|human| human.quality_tier),
        );
        let name = snapshot
            .names
            .find(*entity)
            .cloned()
            .unwrap_or_else(|| format!("entity {entity}"));

        visible_entities.push(*entity);
        if flags.wet {
            wet_material_entities.push(*entity);
        }
        if flags.cracked {
            cracked_material_entities.push(*entity);
        }
        if flags.deformed {
            deformed_material_entities.push(*entity);
        }
        if flags.constraint_corrected {
            constraint_debug_entities.push(*entity);
        }

        instance_records.push(SceneInstanceRecord {
            entity: *entity,
            name,
            transform,
            mesh: renderable.mesh,
            material: renderable.material,
            material_state,
            flags,
            lod,
            distance_to_primary_camera: distance,
        });
        mesh_cluster_records.push(mesh_cluster_record(*entity, renderable.mesh, lod, flags));
        material_records.push(material_gpu_record(
            *entity,
            renderable,
            &tags,
            &material_state,
            is_human,
        ));
        material_state_records.push(material_state_gpu_record(
            *entity,
            renderable.material,
            &material_state,
        ));
    }

    let lights = build_lights(snapshot);
    let volumes = build_volume_inputs(snapshot, recent_events);
    let humans = build_human_render_set(snapshot, primary_camera_position);
    let virtual_shadow_pages =
        build_virtual_shadow_pages(&lights, &instance_records, &volumes, &humans, recent_events);
    let material_programs = build_material_runtime_programs(snapshot, &instance_records);
    let fluids = build_fluid_inputs(snapshot, recent_events);
    let surface_effects = build_surface_effects(snapshot, recent_events);
    let decals = build_decal_inputs(snapshot, &instance_records, recent_events, &surface_effects);
    let weather = build_weather_state(snapshot, recent_events);
    let world_cell_visibility = build_world_cell_visibility(snapshot, &visible_entities, &weather);
    let debug_request = default_debug_request();

    RenderPacket {
        frame_id: snapshot.frame_id,
        sim_time: snapshot.sim_time,
        cameras,
        lights,
        instances: GpuResourceHandle(1),
        instance_records,
        mesh_clusters: GpuResourceHandle(3),
        mesh_cluster_records,
        materials: GpuResourceHandle(4),
        material_records,
        material_states: GpuResourceHandle(6),
        material_state_records,
        material_programs,
        volumes,
        virtual_shadow_pages,
        fluids,
        decals,
        weather,
        world_cell_visibility,
        surface_effects,
        humans,
        ai_intent_overlays,
        visible_entities,
        wet_material_entities,
        cracked_material_entities,
        deformed_material_entities,
        constraint_debug_entities,
        debug_request: debug_request.clone(),
        debug_views: debug_request,
        quality_profile: RenderQualityProfile::for_quality(quality_tier),
    }
}

fn default_debug_request() -> DebugRenderRequest {
    vec![
        DebugRenderView::MaterialState,
        DebugRenderView::Lighting,
        DebugRenderView::Overdraw,
        DebugRenderView::Lod,
        DebugRenderView::GpuCost,
        DebugRenderView::Visibility,
        DebugRenderView::AiDecisions,
        DebugRenderView::Luminance,
        DebugRenderView::Exposure,
        DebugRenderView::Normals,
        DebugRenderView::Roughness,
        DebugRenderView::MaterialIds,
        DebugRenderView::GlobalIllumination,
        DebugRenderView::Shadows,
        DebugRenderView::Velocity,
        DebugRenderView::Depth,
        DebugRenderView::HumanRendering,
    ]
}

fn build_weather_state(snapshot: &WorldSnapshot, recent_events: &[WorldEvent]) -> WeatherState {
    let material_count = snapshot.material_states.len().max(1) as f32;
    let mut moisture_sum = 0.0;
    let mut soot_sum = 0.0;
    let mut temperature_sum = 0.0;
    let mut high_pressure_count = 0usize;

    for (_, state) in snapshot.material_states.iter() {
        moisture_sum += state.moisture.clamp(0.0, 1.0);
        soot_sum += state.soot.clamp(0.0, 1.0);
        temperature_sum += state.temperature;
        if state.pressure > 125_000.0 {
            high_pressure_count += 1;
        }
    }

    let flood_event_count = recent_events
        .iter()
        .filter(|event| matches!(event.kind, WorldEventKind::StreetFlooded))
        .count();
    let gas_event_count = recent_events
        .iter()
        .filter(|event| matches!(event.kind, WorldEventKind::ToxicGasReleased))
        .count();
    let fracture_event_count = recent_events
        .iter()
        .filter(|event| matches!(event.kind, WorldEventKind::GlassWallFractured { .. }))
        .count();

    let average_moisture = moisture_sum / material_count;
    let average_soot = soot_sum / material_count;
    let average_temperature_celsius = temperature_sum / material_count - 273.15;
    let precipitation_intensity =
        (average_moisture * 0.72 + flood_event_count as f32 * 0.35).clamp(0.0, 1.0);
    let surface_wetness =
        (average_moisture + flood_event_count as f32 * 0.5 + precipitation_intensity * 0.2)
            .clamp(0.0, 1.0);
    let flood_depth_meters = if flood_event_count > 0 {
        (0.08 + surface_wetness * 0.28).clamp(0.0, 0.45)
    } else {
        (surface_wetness * 0.035).clamp(0.0, 0.08)
    };
    let toxic_gas_density = (gas_event_count as f32 * 0.65
        + high_pressure_count as f32 * 0.08
        + snapshot
            .material_states
            .iter()
            .map(|(_, state)| (state.pressure - 101_325.0).max(0.0) / 80_000.0)
            .sum::<f32>()
            / material_count
            * 0.12)
        .clamp(0.0, 1.0);
    let dust_density = (average_soot * 0.7 + fracture_event_count as f32 * 0.12).clamp(0.0, 1.0);
    let fog_density =
        (surface_wetness * 0.2 + toxic_gas_density * 0.16 + dust_density * 0.08).clamp(0.0, 1.0);
    let neon_haze = (snapshot
        .tags
        .iter()
        .filter(|(_, tags)| {
            tags.iter()
                .any(|tag| tag.contains("neon") || tag.contains("sign"))
        })
        .count() as f32
        * 0.08
        + fog_density * 0.35
        + surface_wetness * 0.18)
        .clamp(0.0, 1.0);
    let humidity = (0.35 + surface_wetness * 0.5 + fog_density * 0.25).clamp(0.0, 1.0);
    let condition = if toxic_gas_density >= 0.45 && flood_depth_meters >= 0.08 {
        WeatherCondition::MixedHazard
    } else if toxic_gas_density >= 0.45 {
        WeatherCondition::ToxicGas
    } else if flood_depth_meters >= 0.08 {
        WeatherCondition::Flooding
    } else if precipitation_intensity >= 0.28 {
        WeatherCondition::Rain
    } else if dust_density >= 0.28 {
        WeatherCondition::Dust
    } else {
        WeatherCondition::Clear
    };
    let gust = (precipitation_intensity * 3.2 + toxic_gas_density * 1.4 + dust_density * 1.1)
        .clamp(0.0, 7.5);

    RenderWeatherState {
        condition,
        precipitation_intensity,
        surface_wetness,
        flood_depth_meters,
        toxic_gas_density,
        fog_density,
        dust_density,
        neon_haze,
        wind_velocity_mps: Vec3::new(0.85 + gust, 0.22 + fog_density, 0.0),
        temperature_celsius: average_temperature_celsius,
        humidity,
        source_event_count: flood_event_count + gas_event_count + fracture_event_count,
    }
}

fn build_world_cell_visibility(
    snapshot: &WorldSnapshot,
    visible_entities: &[EntityId],
    weather: &WeatherState,
) -> WorldCellVisibility {
    let cell_count = snapshot.city_cells.len();
    if cell_count == 0 {
        return Vec::new();
    }

    snapshot
        .city_cells
        .iter()
        .enumerate()
        .map(|(index, (_, cell))| {
            let visible_entity_count = visible_entities
                .iter()
                .filter(|entity| (**entity as usize) % cell_count == index)
                .count();
            let population_activity = cell.population_summary.normalized_activity();
            let atmospheric_density = (weather.fog_density
                + weather.toxic_gas_density * 0.6
                + weather.dust_density * 0.4
                + weather.neon_haze * 0.3)
                .clamp(0.0, 1.0);
            let visibility_weight = world_cell_visibility_weight(
                cell.loaded,
                cell.quality_tier,
                visible_entity_count,
                population_activity,
                atmospheric_density,
            );

            WorldCellVisibilityRecord {
                cell_id: cell.cell_id,
                district_id: cell.district_id,
                loaded: cell.loaded,
                quality_tier: cell.quality_tier,
                render_state: world_cell_render_state(cell.loaded, cell.quality_tier),
                visible_entity_count,
                population_activity,
                visibility_weight,
                atmospheric_density,
                resident_asset_count: cell.resident_asset_count,
                active_story_thread_count: cell.active_story_thread_count,
                streaming_priority: world_cell_streaming_priority(
                    cell.loaded,
                    cell.quality_tier,
                    visible_entity_count,
                    cell.active_story_thread_count,
                ),
            }
        })
        .collect()
}

fn world_cell_render_state(loaded: bool, quality: QualityTier) -> WorldCellRenderState {
    if !loaded {
        return WorldCellRenderState::Dormant;
    }

    match quality {
        QualityTier::ReferenceOfflineValidation | QualityTier::HeroHighFidelityRuntime => {
            WorldCellRenderState::Hero
        }
        QualityTier::NormalRuntime => WorldCellRenderState::Gameplay,
        QualityTier::BackgroundApproximation => WorldCellRenderState::Background,
        QualityTier::Disabled => WorldCellRenderState::Summary,
    }
}

fn world_cell_visibility_weight(
    loaded: bool,
    quality: QualityTier,
    visible_entity_count: usize,
    population_activity: f32,
    atmospheric_density: f32,
) -> f32 {
    if !loaded {
        return (population_activity * 0.18).clamp(0.0, 0.25);
    }

    let quality_weight = match quality {
        QualityTier::Disabled => 0.1,
        QualityTier::BackgroundApproximation => 0.32,
        QualityTier::NormalRuntime => 0.66,
        QualityTier::HeroHighFidelityRuntime => 0.9,
        QualityTier::ReferenceOfflineValidation => 1.0,
    };
    (quality_weight + (visible_entity_count as f32 / 12.0).min(0.3) + population_activity * 0.16
        - atmospheric_density * 0.08)
        .clamp(0.0, 1.0)
}

fn world_cell_streaming_priority(
    loaded: bool,
    quality: QualityTier,
    visible_entity_count: usize,
    active_story_thread_count: u32,
) -> AssetPriority {
    if active_story_thread_count > 0 || quality >= QualityTier::HeroHighFidelityRuntime {
        AssetPriority::Hero
    } else if !loaded {
        AssetPriority::Background
    } else if visible_entity_count > 0 || quality >= QualityTier::NormalRuntime {
        AssetPriority::Visible
    } else {
        AssetPriority::Background
    }
}

fn render_event_hints(recent_events: &[WorldEvent]) -> RenderEventHints {
    let mut hints = RenderEventHints::default();
    for event in recent_events {
        match event.kind {
            WorldEventKind::MetalBent { entity, .. } => {
                hints.deformed_entities.insert(entity);
            }
            WorldEventKind::FlexibleConstraintResolved { entity, .. } => {
                hints.constraint_entities.insert(entity);
            }
            _ => {}
        }
    }
    hints
}

fn build_ai_intent_overlays(
    snapshot: &WorldSnapshot,
    recent_events: &[WorldEvent],
) -> Vec<AiIntentDebugOverlay> {
    recent_events
        .iter()
        .filter_map(|event| {
            let WorldEventKind::AgentIntentProposed {
                agent,
                action,
                source_event,
                validation_passed,
            } = &event.kind
            else {
                return None;
            };

            let position = snapshot
                .transforms
                .find(*agent)
                .map(|transform| transform.translation_meters)
                .unwrap_or(event.location_meters);
            let source_position = source_event.and_then(|source_event| {
                recent_events
                    .iter()
                    .find(|candidate| candidate.event_id == source_event)
                    .map(|candidate| candidate.location_meters)
            });

            Some(AiIntentDebugOverlay {
                event_id: event.event_id,
                agent: *agent,
                action: action.clone(),
                source_event: *source_event,
                position,
                source_position,
                validation_passed: *validation_passed,
                color_linear: ai_intent_color(action, *validation_passed),
            })
        })
        .collect()
}

fn ai_intent_color(action: &str, validation_passed: bool) -> [f32; 4] {
    if !validation_passed {
        return [1.0, 0.16, 0.12, 1.0];
    }

    match action {
        "ReportCrime" => [1.0, 0.55, 0.12, 1.0],
        "Investigate" => [0.14, 0.72, 1.0, 1.0],
        "Flee" => [0.95, 0.2, 0.16, 1.0],
        "SpeakTo" => [0.5, 0.9, 0.35, 1.0],
        _ => [0.78, 0.78, 1.0, 1.0],
    }
}

fn build_cameras(snapshot: &WorldSnapshot) -> Vec<Camera> {
    if let Some((entity, transform)) = snapshot.tags.iter().find_map(|(entity, tags)| {
        has_tag(tags, "player")
            .then(|| {
                snapshot
                    .transforms
                    .find(*entity)
                    .map(|transform| (*entity, *transform))
            })
            .flatten()
    }) {
        return vec![Camera {
            entity: Some(entity),
            label: "player_camera".to_string(),
            position: Vec3::new(
                transform.translation_meters.x,
                transform.translation_meters.y,
                transform.translation_meters.z + 1.65,
            ),
            forward: Vec3::new(1.0, 0.0, -0.08),
            vertical_fov_radians: std::f32::consts::FRAC_PI_3,
            focal_length_mm: 35.0,
            sensor_width_mm: 36.0,
            sensor_height_mm: 24.0,
            aperture_f_stop: 2.8,
            shutter_seconds: 1.0 / 96.0,
            iso: 640.0,
            focus_distance_meters: 4.5,
            exposure_bias: 0.15,
            motion_blur_enabled: true,
            depth_of_field_enabled: true,
            lens_distortion_amount: 0.018,
            chromatic_aberration_amount: 0.004,
        }];
    }

    vec![Camera {
        entity: None,
        label: "default_debug_camera".to_string(),
        position: Vec3::new(0.0, -6.0, 2.0),
        forward: Vec3::new(0.0, 1.0, -0.15),
        vertical_fov_radians: std::f32::consts::FRAC_PI_3,
        focal_length_mm: 35.0,
        sensor_width_mm: 36.0,
        sensor_height_mm: 24.0,
        aperture_f_stop: 4.0,
        shutter_seconds: 1.0 / 120.0,
        iso: 400.0,
        focus_distance_meters: 6.0,
        exposure_bias: 0.0,
        motion_blur_enabled: true,
        depth_of_field_enabled: true,
        lens_distortion_amount: 0.012,
        chromatic_aberration_amount: 0.002,
    }]
}

fn build_lights(snapshot: &WorldSnapshot) -> Vec<LightRecord> {
    let mut lights = snapshot
        .tags
        .iter()
        .filter(|(_, tags)| has_tag(tags, "light") || has_tag(tags, "neon"))
        .map(|(entity, tags)| {
            let position = snapshot
                .transforms
                .find(*entity)
                .map(|transform| transform.translation_meters)
                .unwrap_or(Vec3::ZERO);
            let neon = has_tag(tags, "neon");
            LightRecord {
                entity: *entity,
                kind: if neon {
                    LightKind::Neon
                } else {
                    LightKind::Screen
                },
                position,
                color_linear: if neon {
                    [4.2, 0.22, 3.8]
                } else {
                    [1.0, 1.0, 0.84]
                },
                intensity_lumens: if neon { 12_000.0 } else { 2_200.0 },
                radius_meters: if neon { 18.0 } else { 8.0 },
                casts_shadows: true,
            }
        })
        .collect::<Vec<_>>();

    if lights.is_empty() {
        lights.push(LightRecord {
            entity: 0,
            kind: LightKind::AmbientProbe,
            position: Vec3::ZERO,
            color_linear: [0.08, 0.1, 0.16],
            intensity_lumens: 400.0,
            radius_meters: 64.0,
            casts_shadows: false,
        });
    }

    lights
}

fn build_human_render_set(
    snapshot: &WorldSnapshot,
    primary_camera_position: Vec3,
) -> Vec<HumanRenderRecord> {
    snapshot
        .humans
        .iter()
        .map(|(entity, human)| {
            let transform = snapshot
                .transforms
                .find(*entity)
                .copied()
                .unwrap_or_default();
            let distance = transform
                .translation_meters
                .distance(primary_camera_position);
            let renderable = snapshot.renderables.find(*entity);
            let lod = human_lod(human.quality_tier, distance);
            let surface = human_surface_render_response(snapshot.material_states.find(*entity));
            let tags = snapshot.tags.find(*entity).cloned().unwrap_or_default();
            let cybernetic_surface_ratio = human_cybernetic_surface_ratio(
                human.human_id,
                renderable.map(|r| r.material),
                &tags,
            );
            HumanRenderRecord {
                entity: *entity,
                human_id: human.human_id,
                mesh: renderable.map(|renderable| renderable.mesh),
                material: renderable.map(|renderable| renderable.material),
                lod,
                skin_detail: human.quality_tier,
                hair_strand_buffer: (lod == HumanRenderLod::HeroFace)
                    .then_some(GpuResourceHandle(40_000 + *entity as u128)),
                face_morph_buffer: matches!(
                    lod,
                    HumanRenderLod::HeroFace | HumanRenderLod::NearbyNpc
                )
                .then_some(GpuResourceHandle(41_000 + *entity as u128)),
                eye_reflection_probe: matches!(
                    lod,
                    HumanRenderLod::HeroFace | HumanRenderLod::NearbyNpc
                )
                .then_some(GpuResourceHandle(42_000 + *entity as u128)),
                skin_wetness: surface.skin_wetness,
                injury_overlay: surface.injury_overlay,
                hair_wetness: surface.hair_wetness,
                clothing_wetness: surface.clothing_wetness,
                subsurface_strength: human_subsurface_strength(human.quality_tier, surface),
                pore_microdetail_strength: human_pore_microdetail_strength(human.quality_tier, lod),
                blood_redness: surface.blood_redness,
                oil_sweat_mix: surface.oil_sweat_mix,
                eye_wetness: (0.58 + surface.skin_wetness * 0.24).clamp(0.0, 1.0),
                eye_redness: surface.eye_redness,
                iris_depth: if lod == HumanRenderLod::HeroFace {
                    0.82
                } else {
                    0.55
                },
                tearline_strength: (0.5 + surface.skin_wetness * 0.35).clamp(0.0, 1.0),
                hair_lod: human_hair_lod(lod, human.quality_tier),
                hair_motion_response: human_hair_motion_response(lod, surface.hair_wetness),
                clothing_motion_response: human_clothing_motion_response(
                    lod,
                    surface.clothing_wetness,
                ),
                clothing_damage: surface.clothing_damage,
                cybernetic_surface_ratio,
                closeup_quality_score: human_closeup_quality_score(
                    lod,
                    human.quality_tier,
                    cybernetic_surface_ratio,
                ),
            }
        })
        .collect()
}

fn build_virtual_shadow_pages(
    lights: &[LightRecord],
    instances: &[SceneInstanceRecord],
    volumes: &[VolumeRenderInput],
    humans: &[HumanRenderRecord],
    recent_events: &[WorldEvent],
) -> Vec<VirtualShadowPageRequest> {
    let source_events = recent_events
        .iter()
        .filter_map(shadow_source_event)
        .collect::<BTreeMap<_, _>>();
    let human_entities = humans
        .iter()
        .map(|human| human.entity)
        .collect::<BTreeSet<_>>();
    let mut pages = Vec::new();

    for light in lights.iter().filter(|light| light.casts_shadows) {
        pages.push(VirtualShadowPageRequest {
            page_id: shadow_page_id(
                light.entity,
                0,
                ShadowPageInvalidationReason::ShadowCastingLight,
            ),
            light_entity: Some(light.entity),
            caster_entity: None,
            source_event: None,
            bounds: bounds_around(light.position, light.radius_meters.min(12.0), 4.0),
            reason: ShadowPageInvalidationReason::ShadowCastingLight,
            estimated_page_count: shadow_page_count_from_radius(light.radius_meters),
            priority: QualityTier::NormalRuntime,
            dirty: false,
            contact_sharpness: (1.0 / light.radius_meters.max(1.0)).clamp(0.04, 0.35),
        });
    }

    for instance in instances {
        let source_event = source_events.get(&instance.entity).copied();
        let reason = if human_entities.contains(&instance.entity) {
            Some(ShadowPageInvalidationReason::AnimatedHuman)
        } else if instance.flags.cracked {
            Some(ShadowPageInvalidationReason::DestructibleGeometry)
        } else if instance.flags.deformed {
            Some(ShadowPageInvalidationReason::Deformation)
        } else if instance.flags.wet && source_event.is_some() {
            Some(ShadowPageInvalidationReason::MaterialStateChange)
        } else {
            None
        };
        let Some(reason) = reason else {
            continue;
        };

        let dirty = source_event.is_some()
            || matches!(
                reason,
                ShadowPageInvalidationReason::AnimatedHuman
                    | ShadowPageInvalidationReason::DestructibleGeometry
                    | ShadowPageInvalidationReason::Deformation
            );
        pages.push(VirtualShadowPageRequest {
            page_id: shadow_page_id(instance.entity, source_event.unwrap_or(0), reason),
            light_entity: lights
                .iter()
                .find(|light| light.casts_shadows)
                .map(|light| light.entity),
            caster_entity: Some(instance.entity),
            source_event,
            bounds: shadow_page_bounds_for_instance(instance, reason),
            reason,
            estimated_page_count: shadow_page_count_for_instance(instance, reason),
            priority: shadow_page_priority(instance, reason),
            dirty,
            contact_sharpness: shadow_contact_sharpness(instance, reason),
        });
    }

    for volume in volumes {
        pages.push(VirtualShadowPageRequest {
            page_id: shadow_page_id(
                volume.source_entities.first().copied().unwrap_or(0),
                volume.source_event,
                ShadowPageInvalidationReason::VolumeScattering,
            ),
            light_entity: lights
                .iter()
                .find(|light| light.casts_shadows)
                .map(|light| light.entity),
            caster_entity: volume.source_entities.first().copied(),
            source_event: Some(volume.source_event),
            bounds: volume.bounds,
            reason: ShadowPageInvalidationReason::VolumeScattering,
            estimated_page_count: (1 + (volume.visibility_blocking * 4.0).ceil() as u32).max(1),
            priority: volume.quality_hint,
            dirty: true,
            contact_sharpness: (0.12 + volume.scattering * 0.08).clamp(0.08, 0.28),
        });
    }

    pages.sort_by_key(|page| (page.page_id, page.reason));
    pages.dedup_by_key(|page| (page.page_id, page.reason));
    pages
}

#[derive(Clone, Debug)]
struct MaterialProgramAccumulator {
    material_id: MaterialId,
    descriptor: Option<MaterialDescriptor>,
    instance_count: usize,
    max_lod: RenderLodTier,
    wetness: f32,
    crack_density: f32,
    soot: f32,
    corrosion: f32,
    heat: f32,
    plastic_strain: f32,
    electrical_charge: f32,
    biological_contamination: f32,
    oil_contamination: f32,
    human: bool,
    transparent: bool,
    emissive: bool,
    deformed: bool,
}

fn build_material_runtime_programs(
    snapshot: &WorldSnapshot,
    instances: &[SceneInstanceRecord],
) -> Vec<MaterialRuntimeProgramRecord> {
    let mut programs = BTreeMap::<MaterialId, MaterialProgramAccumulator>::new();
    for instance in instances {
        let descriptor = snapshot.materials.find(instance.material).cloned();
        let entry =
            programs
                .entry(instance.material)
                .or_insert_with(|| MaterialProgramAccumulator {
                    material_id: instance.material,
                    descriptor,
                    instance_count: 0,
                    max_lod: RenderLodTier::Impostor,
                    wetness: 0.0,
                    crack_density: 0.0,
                    soot: 0.0,
                    corrosion: 0.0,
                    heat: 0.0,
                    plastic_strain: 0.0,
                    electrical_charge: 0.0,
                    biological_contamination: 0.0,
                    oil_contamination: 0.0,
                    human: false,
                    transparent: false,
                    emissive: false,
                    deformed: false,
                });
        entry.instance_count += 1;
        entry.max_lod = entry.max_lod.min(instance.lod);
        entry.wetness = entry.wetness.max(instance.material_state.moisture);
        entry.crack_density = entry
            .crack_density
            .max(instance.material_state.crack_density);
        entry.soot = entry.soot.max(instance.material_state.soot);
        entry.corrosion = entry.corrosion.max(instance.material_state.corrosion);
        entry.heat = entry
            .heat
            .max(((instance.material_state.temperature - 293.15) / 800.0).clamp(0.0, 1.0));
        entry.plastic_strain = entry
            .plastic_strain
            .max(instance.material_state.plastic_strain);
        entry.electrical_charge = entry
            .electrical_charge
            .max((instance.material_state.electrical_charge / 240.0).clamp(0.0, 1.0));
        entry.biological_contamination = entry
            .biological_contamination
            .max(instance.material_state.biological_contamination);
        entry.oil_contamination = entry
            .oil_contamination
            .max(instance.material_state.oil_contamination);
        entry.human |= instance.flags.human;
        entry.transparent |= instance.flags.transparent;
        entry.emissive |= instance.flags.emissive;
        entry.deformed |= instance.flags.deformed;
    }

    programs
        .into_values()
        .map(material_runtime_program_from_accumulator)
        .collect()
}

fn material_runtime_program_from_accumulator(
    program: MaterialProgramAccumulator,
) -> MaterialRuntimeProgramRecord {
    let shader_model = material_program_shader_model(&program);
    let required_state_channels = material_required_state_channels(&program);
    let displacement_policy = material_displacement_policy(&program);
    let cache_textures = material_cache_textures(&program, shader_model);
    let cache_policy = material_cache_policy(
        program.max_lod,
        &required_state_channels,
        !cache_textures.is_empty(),
        shader_model,
    );
    let graph_handle = program
        .descriptor
        .as_ref()
        .and_then(|descriptor| descriptor.procedural_source)
        .map(|generator| MaterialGraphHandle(generator.0))
        .unwrap_or_else(|| MaterialGraphHandle(60_000 + program.material_id as u128));
    let generator = program
        .descriptor
        .as_ref()
        .and_then(|descriptor| descriptor.procedural_source);
    let deterministic_key = material_program_key(
        program.material_id,
        graph_handle,
        shader_model,
        displacement_policy,
        &required_state_channels,
        &cache_textures,
    );

    MaterialRuntimeProgramRecord {
        material_id: program.material_id,
        graph_handle,
        generator,
        shader_model,
        required_state_channels,
        cache_policy,
        displacement_policy,
        cache_textures,
        instance_count: program.instance_count,
        deterministic_key,
        fallback_available: true,
    }
}

fn material_program_shader_model(
    program: &MaterialProgramAccumulator,
) -> MaterialProgramShaderModel {
    let descriptor = program.descriptor.as_ref();
    if program.human {
        MaterialProgramShaderModel::Subsurface
    } else if descriptor.is_some_and(|descriptor| descriptor.visual.transparency > 0.05)
        || program.transparent
    {
        MaterialProgramShaderModel::TransparentPbr
    } else if descriptor.is_some_and(|descriptor| {
        descriptor
            .visual
            .emission_linear
            .iter()
            .any(|value| *value > 0.01)
    }) || program.emissive
    {
        MaterialProgramShaderModel::Emissive
    } else {
        MaterialProgramShaderModel::OpaquePbr
    }
}

fn material_required_state_channels(
    program: &MaterialProgramAccumulator,
) -> Vec<MaterialStateChannel> {
    let mut channels = BTreeSet::new();
    if program.wetness >= 0.05 {
        channels.insert(MaterialStateChannel::Wetness);
    }
    if program.crack_density >= 0.05 {
        channels.insert(MaterialStateChannel::Cracks);
    }
    if program.soot >= 0.05 {
        channels.insert(MaterialStateChannel::Soot);
    }
    if program.corrosion >= 0.05 {
        channels.insert(MaterialStateChannel::Corrosion);
    }
    if program.heat >= 0.05 {
        channels.insert(MaterialStateChannel::Heat);
    }
    if program.plastic_strain >= 0.05 || program.deformed {
        channels.insert(MaterialStateChannel::PlasticStrain);
    }
    if program.electrical_charge >= 0.05 || program.emissive {
        channels.insert(MaterialStateChannel::Electrical);
    }
    if program.biological_contamination >= 0.05 || program.human {
        channels.insert(MaterialStateChannel::Biological);
    }
    if program.oil_contamination >= 0.05 {
        channels.insert(MaterialStateChannel::Oil);
    }
    channels.into_iter().collect()
}

fn material_displacement_policy(
    program: &MaterialProgramAccumulator,
) -> MaterialDisplacementPolicy {
    if program.crack_density >= 0.5 {
        MaterialDisplacementPolicy::FractureInterior
    } else if program.plastic_strain >= 0.05 || program.deformed {
        MaterialDisplacementPolicy::MicroDisplacement
    } else if program.wetness >= 0.05 || program.crack_density >= 0.05 {
        MaterialDisplacementPolicy::ParallaxDetail
    } else {
        MaterialDisplacementPolicy::None
    }
}

fn material_cache_textures(
    program: &MaterialProgramAccumulator,
    shader_model: MaterialProgramShaderModel,
) -> Vec<TextureHandle> {
    let mut textures = Vec::new();
    if program.crack_density >= 0.05 {
        textures.push(TEXTURE_GLASS_CRACK_DETAIL_CACHE);
    }
    if program.wetness >= 0.05 {
        textures.push(TEXTURE_WET_ASPHALT_REFLECTION_CACHE);
    }
    if shader_model == MaterialProgramShaderModel::Subsurface {
        textures.push(TEXTURE_HUMAN_SKIN_DETAIL_CACHE);
    }
    if material_program_needs_aged_surface_cache(program) {
        textures.push(TEXTURE_AGED_SURFACE_CACHE);
    }
    textures
}

fn material_program_needs_aged_surface_cache(program: &MaterialProgramAccumulator) -> bool {
    program.corrosion >= 0.2
        || program.soot >= 0.18
        || program.heat >= 0.12
        || program.electrical_charge >= 0.1
}

fn material_cache_policy(
    lod: RenderLodTier,
    state_channels: &[MaterialStateChannel],
    cache_required: bool,
    shader_model: MaterialProgramShaderModel,
) -> MaterialCachePolicyRecord {
    let max_resolution_pixels = match lod {
        RenderLodTier::Hero => 2048,
        RenderLodTier::Near => 1024,
        RenderLodTier::Mid => 512,
        RenderLodTier::Far | RenderLodTier::Impostor => 256,
    };
    let update_frequency =
        if shader_model == MaterialProgramShaderModel::Subsurface && lod == RenderLodTier::Hero {
            MaterialCacheUpdateFrequency::PerFrameHero
        } else if state_channels.is_empty() {
            MaterialCacheUpdateFrequency::Static
        } else {
            MaterialCacheUpdateFrequency::OnStateChange
        };
    let streaming_priority = match lod {
        RenderLodTier::Hero => 1.0,
        RenderLodTier::Near => 0.72,
        RenderLodTier::Mid => 0.42,
        RenderLodTier::Far | RenderLodTier::Impostor => 0.18,
    } + state_channels.len() as f32 * 0.04;

    MaterialCachePolicyRecord {
        max_resolution_pixels,
        update_frequency,
        invalidation_channels: state_channels.to_vec(),
        streaming_priority: streaming_priority.clamp(0.0, 1.0),
        cache_required,
        cache_miss_fallback: true,
    }
}

fn material_program_key(
    material_id: MaterialId,
    graph_handle: MaterialGraphHandle,
    shader_model: MaterialProgramShaderModel,
    displacement_policy: MaterialDisplacementPolicy,
    state_channels: &[MaterialStateChannel],
    cache_textures: &[TextureHandle],
) -> u128 {
    let mut key = material_id as u128 ^ graph_handle.0.rotate_left(17);
    key ^= (material_shader_model_code(shader_model) as u128) << 96;
    key ^= (material_displacement_code(displacement_policy) as u128) << 88;
    for (index, channel) in state_channels.iter().enumerate() {
        key ^= (material_state_channel_code(*channel) as u128) << ((index % 8) * 8);
    }
    for texture in cache_textures {
        key = key.rotate_left(11) ^ texture.0;
    }
    key
}

fn material_shader_model_code(shader_model: MaterialProgramShaderModel) -> u8 {
    match shader_model {
        MaterialProgramShaderModel::OpaquePbr => 1,
        MaterialProgramShaderModel::TransparentPbr => 2,
        MaterialProgramShaderModel::Subsurface => 3,
        MaterialProgramShaderModel::Emissive => 4,
    }
}

fn material_displacement_code(displacement_policy: MaterialDisplacementPolicy) -> u8 {
    match displacement_policy {
        MaterialDisplacementPolicy::None => 0,
        MaterialDisplacementPolicy::ParallaxDetail => 1,
        MaterialDisplacementPolicy::MicroDisplacement => 2,
        MaterialDisplacementPolicy::FractureInterior => 3,
    }
}

fn material_state_channel_code(channel: MaterialStateChannel) -> u8 {
    match channel {
        MaterialStateChannel::Wetness => 1,
        MaterialStateChannel::Cracks => 2,
        MaterialStateChannel::Soot => 3,
        MaterialStateChannel::Corrosion => 4,
        MaterialStateChannel::Heat => 5,
        MaterialStateChannel::PlasticStrain => 6,
        MaterialStateChannel::Electrical => 7,
        MaterialStateChannel::Biological => 8,
        MaterialStateChannel::Oil => 9,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct HumanSurfaceRenderResponse {
    skin_wetness: f32,
    injury_overlay: f32,
    hair_wetness: f32,
    clothing_wetness: f32,
    blood_redness: f32,
    oil_sweat_mix: f32,
    eye_redness: f32,
    clothing_damage: f32,
}

fn human_surface_render_response(
    material_state: Option<&MaterialState>,
) -> HumanSurfaceRenderResponse {
    let Some(state) = material_state else {
        return HumanSurfaceRenderResponse::default();
    };

    HumanSurfaceRenderResponse {
        skin_wetness: (state.moisture * 0.82).clamp(0.0, 1.0),
        injury_overlay: (state.crack_density * 0.58
            + state.plastic_strain.min(1.0) * 0.22
            + state.biological_contamination * 0.2)
            .clamp(0.0, 1.0),
        hair_wetness: (state.moisture * 0.76).clamp(0.0, 1.0),
        clothing_wetness: (state.moisture * 0.8 + state.soot * 0.08).clamp(0.0, 1.0),
        blood_redness: (state.crack_density * 0.48
            + state.biological_contamination * 0.2
            + (state.temperature - 293.15).max(0.0) / 1200.0)
            .clamp(0.0, 1.0),
        oil_sweat_mix: (state.moisture * 0.55 + (state.temperature - 293.15).max(0.0) / 1400.0)
            .max(state.oil_contamination * 0.72)
            .clamp(0.0, 1.0),
        eye_redness: (state.moisture * 0.12 + state.crack_density * 0.22).clamp(0.0, 1.0),
        clothing_damage: (state.soot * 0.32 + state.crack_density * 0.35 + state.plastic_strain)
            .clamp(0.0, 1.0),
    }
}

fn build_fluid_inputs(
    snapshot: &WorldSnapshot,
    recent_events: &[WorldEvent],
) -> Vec<FluidSurfaceRenderInput> {
    let mut fluids = Vec::new();
    for event in recent_events {
        if !matches!(event.kind, WorldEventKind::StreetFlooded) {
            continue;
        }

        for target in event.actors.iter().skip(1) {
            let Some(renderable) = snapshot.renderables.find(*target) else {
                continue;
            };
            let transform = snapshot
                .transforms
                .find(*target)
                .copied()
                .unwrap_or_default();
            fluids.push(FluidSurfaceRenderInput {
                surface_mesh: renderable.mesh,
                particle_spray_buffer: Some(GpuResourceHandle(50_000 + *target as u128)),
                foam_buffer: Some(GpuResourceHandle(51_000 + *target as u128)),
                material_id: renderable.material,
                bounds: bounds_around(transform.translation_meters, 1.4, 0.08),
                wetness_targets: vec![*target],
                flow_velocity_meters_per_second: Vec3::new(0.4, -0.1, 0.0),
                screen_space_reflection: true,
            });
        }
    }
    fluids
}

fn build_volume_inputs(
    snapshot: &WorldSnapshot,
    recent_events: &[WorldEvent],
) -> Vec<VolumeRenderInput> {
    let mut volumes = Vec::new();
    for event in recent_events {
        if !matches!(event.kind, WorldEventKind::ToxicGasReleased) {
            continue;
        }

        for entity in &event.actors {
            let transform = snapshot
                .transforms
                .find(*entity)
                .copied()
                .unwrap_or_default();
            let medium = volume_medium_from_event(event);
            let density = evidence_f32(&event.physical_evidence, "gas_density")
                .unwrap_or_else(|| default_volume_density(medium));
            let visibility_blocking = evidence_f32(&event.physical_evidence, "visibility_blocking")
                .unwrap_or_else(|| default_volume_visibility(medium));
            let hazard_level = evidence_f32(&event.physical_evidence, "hazard_level")
                .unwrap_or_else(|| default_volume_hazard(medium));
            let radius = evidence_f32(&event.physical_evidence, "volume_radius_meters")
                .unwrap_or(default_volume_radius(medium));
            let height = evidence_f32(&event.physical_evidence, "volume_height_meters")
                .unwrap_or(default_volume_height(medium));
            let material_id = snapshot
                .physical_bodies
                .find(*entity)
                .map(|body| body.material_id)
                .or_else(|| {
                    snapshot
                        .renderables
                        .find(*entity)
                        .map(|renderable| renderable.material)
                })
                .unwrap_or(0);
            volumes.push(VolumeRenderInput {
                density_field: GpuResourceHandle(60_000 + *entity as u128),
                temperature_field: Some(GpuResourceHandle(61_000 + *entity as u128)),
                velocity_field: Some(GpuResourceHandle(62_000 + *entity as u128)),
                source_event: event.event_id,
                source_entities: event.actors.clone(),
                medium,
                material_id,
                bounds: bounds_around(transform.translation_meters, radius, height),
                quality_hint: volume_quality_hint(medium, hazard_level, visibility_blocking),
                density,
                hazard_level,
                density_cell_count: evidence_u64(&event.physical_evidence, "density_cells")
                    .unwrap_or(0),
                temperature_cell_count: evidence_u64(&event.physical_evidence, "temperature_cells")
                    .unwrap_or(0),
                pressure_cell_count: evidence_u64(&event.physical_evidence, "pressure_cells")
                    .unwrap_or(0),
                scattering: volume_scattering(medium, density, visibility_blocking, hazard_level),
                visibility_blocking,
            });
        }
    }
    volumes
}

fn build_surface_effects(
    snapshot: &WorldSnapshot,
    recent_events: &[WorldEvent],
) -> Vec<SurfaceEffectRecord> {
    let mut effects = Vec::new();
    for event in recent_events {
        match event.kind {
            WorldEventKind::MetalBent {
                entity,
                plastic_strain,
            } => {
                let Some(transform) = snapshot.transforms.find(entity).copied() else {
                    continue;
                };
                let material_id = material_for_entity(snapshot, entity);
                effects.push(SurfaceEffectRecord {
                    event_id: event.event_id,
                    entity,
                    kind: SurfaceEffectKind::MetalDent { plastic_strain },
                    bounds: bounds_around(transform.translation_meters, 0.65, 0.18),
                    material_id,
                    intensity: plastic_strain.clamp(0.0, 1.0),
                    quality_hint: if plastic_strain >= 0.2 {
                        QualityTier::HeroHighFidelityRuntime
                    } else {
                        QualityTier::NormalRuntime
                    },
                });
            }
            WorldEventKind::FlexibleConstraintResolved {
                entity,
                correction_meters,
            } => {
                let Some(transform) = snapshot.transforms.find(entity).copied() else {
                    continue;
                };
                effects.push(SurfaceEffectRecord {
                    event_id: event.event_id,
                    entity,
                    kind: SurfaceEffectKind::ConstraintCorrection { correction_meters },
                    bounds: bounds_around(transform.translation_meters, 0.35, 0.35),
                    material_id: material_for_entity(snapshot, entity),
                    intensity: (correction_meters / 2.0).clamp(0.0, 1.0),
                    quality_hint: QualityTier::BackgroundApproximation,
                });
            }
            _ => {}
        }
    }
    effects
}

fn build_decal_inputs(
    snapshot: &WorldSnapshot,
    instances: &[SceneInstanceRecord],
    recent_events: &[WorldEvent],
    surface_effects: &[SurfaceEffectRecord],
) -> Vec<DecalRenderInput> {
    let mut decals = Vec::new();
    let instances_by_entity = instances
        .iter()
        .map(|instance| (instance.entity, instance))
        .collect::<BTreeMap<_, _>>();

    for instance in instances {
        push_material_state_decals(&mut decals, instance);
    }

    for effect in surface_effects {
        let Some(instance) = instances_by_entity.get(&effect.entity).copied() else {
            continue;
        };
        let kind = match effect.kind {
            SurfaceEffectKind::MetalDent { .. } => DecalKind::PlasticDeformation,
            SurfaceEffectKind::ConstraintCorrection { .. } => DecalKind::ConstraintStress,
        };
        push_decal_if_visible(
            &mut decals,
            DecalRenderInput {
                source_event: Some(effect.event_id),
                entity: effect.entity,
                kind,
                bounds: effect.bounds,
                material_id: effect.material_id,
                intensity: effect.intensity.clamp(0.0, 1.0),
                quality_hint: effect.quality_hint,
                generated_atlas_page: decal_atlas_page(effect.entity, effect.material_id, kind),
                state_channels: decal_state_channels(kind),
                generated_from_material_state: false,
            },
            instance,
        );
    }

    for event in recent_events {
        match event.kind {
            WorldEventKind::GlassWallFractured { entity } => {
                push_event_decal(
                    &mut decals,
                    snapshot,
                    &instances_by_entity,
                    event,
                    entity,
                    DecalKind::CrackField,
                    0.94,
                );
            }
            WorldEventKind::MaterialStateChanged { entity } => {
                let Some(state) = snapshot.material_states.find(entity).copied() else {
                    continue;
                };
                for (kind, intensity) in material_state_decal_kinds(state) {
                    push_event_decal(
                        &mut decals,
                        snapshot,
                        &instances_by_entity,
                        event,
                        entity,
                        kind,
                        intensity,
                    );
                }
            }
            WorldEventKind::StreetFlooded => {
                for entity in event.actors.iter().skip(1) {
                    push_event_decal(
                        &mut decals,
                        snapshot,
                        &instances_by_entity,
                        event,
                        *entity,
                        DecalKind::WetnessFilm,
                        0.86,
                    );
                }
            }
            WorldEventKind::MetalBent {
                entity,
                plastic_strain,
            } => {
                push_event_decal(
                    &mut decals,
                    snapshot,
                    &instances_by_entity,
                    event,
                    entity,
                    DecalKind::PlasticDeformation,
                    plastic_strain,
                );
            }
            WorldEventKind::FlexibleConstraintResolved {
                entity,
                correction_meters,
            } => {
                push_event_decal(
                    &mut decals,
                    snapshot,
                    &instances_by_entity,
                    event,
                    entity,
                    DecalKind::ConstraintStress,
                    correction_meters / 2.0,
                );
            }
            _ => {}
        }
    }

    decals.sort_by_key(|decal| {
        (
            decal.entity,
            decal_kind_code(decal.kind),
            decal.source_event.unwrap_or(0),
            decal.generated_atlas_page,
        )
    });
    decals
}

fn push_material_state_decals(decals: &mut Vec<DecalRenderInput>, instance: &SceneInstanceRecord) {
    for (kind, intensity) in material_state_decal_kinds(instance.material_state) {
        if intensity <= 0.04 {
            continue;
        }
        decals.push(DecalRenderInput {
            source_event: None,
            entity: instance.entity,
            kind,
            bounds: decal_bounds_for_instance(instance, kind, intensity),
            material_id: instance.material,
            intensity,
            quality_hint: decal_quality_hint(instance, kind, intensity),
            generated_atlas_page: decal_atlas_page(instance.entity, instance.material, kind),
            state_channels: decal_state_channels(kind),
            generated_from_material_state: true,
        });
    }
}

fn material_state_decal_kinds(state: MaterialState) -> Vec<(DecalKind, f32)> {
    let heat = ((state.temperature - 293.15) / 800.0).clamp(0.0, 1.0);
    let mut decals = Vec::new();
    if state.crack_density >= 0.05 {
        decals.push((DecalKind::CrackField, state.crack_density.clamp(0.0, 1.0)));
    }
    if state.moisture >= 0.08 {
        decals.push((DecalKind::WetnessFilm, state.moisture.clamp(0.0, 1.0)));
    }
    if state.soot >= 0.05 || heat >= 0.08 {
        decals.push((
            DecalKind::SootStain,
            state.soot.max(heat * 0.65).clamp(0.0, 1.0),
        ));
    }
    if state.corrosion >= 0.05 {
        decals.push((DecalKind::CorrosionBloom, state.corrosion.clamp(0.0, 1.0)));
    }
    if state.plastic_strain >= 0.05 {
        decals.push((
            DecalKind::PlasticDeformation,
            state.plastic_strain.clamp(0.0, 1.0),
        ));
    }
    if state.biological_contamination >= 0.08 {
        decals.push((
            DecalKind::SootStain,
            state.biological_contamination.clamp(0.0, 1.0),
        ));
    }
    if state.oil_contamination >= 0.08 {
        decals.push((
            DecalKind::WetnessFilm,
            state.oil_contamination.clamp(0.0, 1.0),
        ));
    }
    decals
}

fn push_event_decal(
    decals: &mut Vec<DecalRenderInput>,
    snapshot: &WorldSnapshot,
    instances_by_entity: &BTreeMap<EntityId, &SceneInstanceRecord>,
    event: &WorldEvent,
    entity: EntityId,
    kind: DecalKind,
    intensity: f32,
) {
    let Some(instance) = instances_by_entity.get(&entity).copied() else {
        return;
    };
    let material_id = material_for_entity(snapshot, entity);
    push_decal_if_visible(
        decals,
        DecalRenderInput {
            source_event: Some(event.event_id),
            entity,
            kind,
            bounds: decal_bounds_for_instance(instance, kind, intensity),
            material_id,
            intensity: intensity.clamp(0.0, 1.0),
            quality_hint: decal_quality_hint(instance, kind, intensity),
            generated_atlas_page: decal_atlas_page(entity, material_id, kind),
            state_channels: decal_state_channels(kind),
            generated_from_material_state: false,
        },
        instance,
    );
}

fn push_decal_if_visible(
    decals: &mut Vec<DecalRenderInput>,
    decal: DecalRenderInput,
    instance: &SceneInstanceRecord,
) {
    if !instance.flags.visible || decal.intensity <= 0.02 {
        return;
    }
    decals.push(decal);
}

fn decal_bounds_for_instance(
    instance: &SceneInstanceRecord,
    kind: DecalKind,
    intensity: f32,
) -> Aabb {
    let radius = match kind {
        DecalKind::CrackField => 0.75 + intensity.clamp(0.0, 1.0) * 0.44,
        DecalKind::WetnessFilm => 1.2 + intensity.clamp(0.0, 1.0) * 0.82,
        DecalKind::SootStain | DecalKind::CorrosionBloom => 0.65 + intensity.clamp(0.0, 1.0) * 0.42,
        DecalKind::PlasticDeformation => 0.52 + intensity.clamp(0.0, 1.0) * 0.34,
        DecalKind::ConstraintStress => 0.38 + intensity.clamp(0.0, 1.0) * 0.22,
    };
    let height = if instance.flags.human {
        1.8
    } else if matches!(kind, DecalKind::WetnessFilm) {
        0.08
    } else {
        0.32 + intensity.clamp(0.0, 1.0) * 0.24
    };
    bounds_around(instance.transform.translation_meters, radius, height)
}

fn decal_quality_hint(
    instance: &SceneInstanceRecord,
    kind: DecalKind,
    intensity: f32,
) -> QualityTier {
    if instance.lod == RenderLodTier::Hero
        || intensity >= 0.72
        || matches!(kind, DecalKind::CrackField | DecalKind::PlasticDeformation)
    {
        QualityTier::HeroHighFidelityRuntime
    } else if instance.lod <= RenderLodTier::Near || intensity >= 0.24 {
        QualityTier::NormalRuntime
    } else {
        QualityTier::BackgroundApproximation
    }
}

fn decal_atlas_page(entity: EntityId, material_id: MaterialId, kind: DecalKind) -> u32 {
    let seed = entity
        .wrapping_mul(2_654_435_761)
        .wrapping_add(material_id.rotate_left(17))
        .wrapping_add(decal_kind_code(kind) as u64 * 97);
    (seed % 16_384) as u32
}

fn decal_kind_code(kind: DecalKind) -> u8 {
    match kind {
        DecalKind::CrackField => 1,
        DecalKind::WetnessFilm => 2,
        DecalKind::SootStain => 3,
        DecalKind::CorrosionBloom => 4,
        DecalKind::PlasticDeformation => 5,
        DecalKind::ConstraintStress => 6,
    }
}

fn decal_state_channels(kind: DecalKind) -> Vec<MaterialStateChannel> {
    match kind {
        DecalKind::CrackField => vec![MaterialStateChannel::Cracks],
        DecalKind::WetnessFilm => vec![MaterialStateChannel::Wetness],
        DecalKind::SootStain => vec![MaterialStateChannel::Soot, MaterialStateChannel::Heat],
        DecalKind::CorrosionBloom => vec![MaterialStateChannel::Corrosion],
        DecalKind::PlasticDeformation => vec![MaterialStateChannel::PlasticStrain],
        DecalKind::ConstraintStress => vec![MaterialStateChannel::PlasticStrain],
    }
}

fn choose_quality_profile(packet: &RenderPacket, budget: &RenderBudget) -> RenderQualityProfile {
    let mut profile = RenderQualityProfile::for_quality(budget.quality_tier);
    if !budget.allow_ray_tracing {
        profile.ray_traced_reflections = false;
        profile.ray_traced_shadows = false;
        if profile.capability_tier >= RenderCapabilityTier::SelectiveRayTracing {
            profile.capability_tier = RenderCapabilityTier::ScreenSpaceEffects;
        }
    }

    let usage = estimate_budget_usage_with_profile(packet, &profile);
    if usage.visible_instances > budget.max_visible_instances
        || usage.dynamic_lights > budget.max_dynamic_lights
        || usage.volume_regions > budget.max_volume_regions
        || usage.fluid_surfaces > budget.max_fluid_surfaces
        || usage.decals > budget.max_decals
        || usage.world_cell_visibility_records > budget.max_world_cell_visibility_records
        || usage.surface_effects > budget.max_surface_effects
        || usage.estimated_gpu_milliseconds > budget.max_gpu_milliseconds
        || usage.memory_bytes > budget.max_memory_bytes
    {
        profile = profile.degraded_for_budget();
    }

    profile
}

fn estimate_budget_usage_with_profile(
    packet: &RenderPacket,
    profile: &RenderQualityProfile,
) -> RenderBudgetUsage {
    let quality_multiplier = match profile.quality_tier {
        QualityTier::Disabled => 0.0,
        QualityTier::BackgroundApproximation => 0.45,
        QualityTier::NormalRuntime => 1.0,
        QualityTier::HeroHighFidelityRuntime => 1.55,
        QualityTier::ReferenceOfflineValidation => 5.0,
    };
    let ray_cost = f32::from(profile.ray_traced_reflections) * 0.7
        + f32::from(profile.ray_traced_shadows) * 0.45;
    let estimated_gpu_milliseconds = ((1.1
        + packet.visible_entities.len() as f32 * 0.07
        + packet.lights.len() as f32 * 0.16
        + packet.volumes.len() as f32 * 0.33 * profile.volumetric_resolution_scale.max(0.1)
        + packet.fluids.len() as f32 * 0.24
        + packet.decals.len() as f32 * 0.035
        + packet.world_cell_visibility.len() as f32 * 0.012
        + weather_render_cost(&packet.weather)
        + packet.surface_effects.len() as f32 * 0.09
        + packet.humans.len() as f32 * 0.42
        + packet.ai_intent_overlays.len() as f32 * 0.01
        + packet.cracked_material_entities.len() as f32 * 0.16
        + packet.deformed_material_entities.len() as f32 * 0.08
        + packet.wet_material_entities.len() as f32 * 0.06
        + ray_cost)
        * quality_multiplier
        * 100.0)
        .round()
        / 100.0;

    RenderBudgetUsage {
        visible_instances: packet.visible_entities.len(),
        dynamic_lights: packet.lights.len(),
        volume_regions: packet.volumes.len(),
        fluid_surfaces: packet.fluids.len(),
        decals: packet.decals.len(),
        world_cell_visibility_records: packet.world_cell_visibility.len(),
        surface_effects: packet.surface_effects.len(),
        human_records: packet.humans.len(),
        estimated_gpu_milliseconds,
        memory_bytes: 64 * 1024 * 1024
            + packet.visible_entities.len() as u64 * 192 * 1024
            + packet.material_records.len() as u64 * 32 * 1024
            + packet.humans.len() as u64 * 8 * 1024 * 1024
            + packet.volumes.len() as u64 * 4 * 1024 * 1024
            + packet.fluids.len() as u64 * 2 * 1024 * 1024
            + packet.decals.len() as u64 * 128 * 1024
            + packet.world_cell_visibility.len() as u64 * 16 * 1024
            + weather_memory_cost(&packet.weather)
            + packet.surface_effects.len() as u64 * 512 * 1024
            + packet.ai_intent_overlays.len() as u64 * 8 * 1024,
    }
}

fn weather_render_cost(weather: &WeatherState) -> f32 {
    weather.precipitation_intensity * 0.12
        + weather.flood_depth_meters * 0.35
        + weather.toxic_gas_density * 0.2
        + weather.fog_density * 0.16
        + weather.dust_density * 0.08
        + weather.neon_haze * 0.1
}

fn weather_memory_cost(weather: &WeatherState) -> u64 {
    let active_atmosphere = weather.precipitation_intensity
        + weather.surface_wetness
        + weather.toxic_gas_density
        + weather.fog_density
        + weather.dust_density
        + weather.neon_haze;
    if active_atmosphere > 0.05 {
        768 * 1024
    } else {
        128 * 1024
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DrawBatchKey {
    material: MaterialId,
    lod: RenderLodTier,
    human: bool,
    transparent: bool,
    emissive: bool,
}

fn build_gpu_driven_draw_plan(packet: &RenderPacket) -> GpuDrivenDrawPlan {
    let instances_by_entity = packet
        .instance_records
        .iter()
        .map(|instance| (instance.entity, instance))
        .collect::<BTreeMap<_, _>>();
    let mut batches_by_key: BTreeMap<DrawBatchKey, RenderDrawBatch> = BTreeMap::new();

    for cluster in &packet.mesh_cluster_records {
        let Some(instance) = instances_by_entity.get(&cluster.entity) else {
            continue;
        };
        if !instance.flags.visible {
            continue;
        }
        let key = DrawBatchKey {
            material: instance.material,
            lod: cluster.lod,
            human: instance.flags.human,
            transparent: instance.flags.transparent,
            emissive: instance.flags.emissive,
        };
        let batch = batches_by_key
            .entry(key)
            .or_insert_with(|| RenderDrawBatch {
                material: key.material,
                lod: key.lod,
                instance_count: 0,
                cluster_count: 0,
                triangle_budget: 0,
                human: key.human,
                transparent: key.transparent,
                emissive: key.emissive,
            });
        batch.instance_count += 1;
        batch.cluster_count = batch.cluster_count.saturating_add(cluster.cluster_count);
        batch.triangle_budget = batch
            .triangle_budget
            .saturating_add(cluster.triangle_budget);
    }

    let batches = batches_by_key.into_values().collect::<Vec<_>>();
    let visible_instance_count = batches.iter().map(|batch| batch.instance_count).sum();
    let selected_cluster_count = batches.iter().map(|batch| batch.cluster_count).sum();
    let triangle_budget = batches.iter().map(|batch| batch.triangle_budget).sum();
    let indirect_draw_count = batches.len();
    let cpu_draw_call_count = usize::from(!batches.is_empty());

    GpuDrivenDrawPlan {
        batches,
        visible_instance_count,
        selected_cluster_count,
        triangle_budget,
        indirect_draw_count,
        cpu_draw_call_count,
    }
}

fn build_picking_data(
    packet: &RenderPacket,
    visibility_records: &[VisibilityRecord],
) -> PickingData {
    let instance_by_entity = packet
        .instance_records
        .iter()
        .enumerate()
        .map(|(index, instance)| (instance.entity, (index, instance)))
        .collect::<BTreeMap<_, _>>();
    let primary_camera = packet.cameras.first();

    visibility_records
        .iter()
        .filter_map(|record| {
            let (source_instance_index, instance) = instance_by_entity.get(&record.entity)?;
            Some(PickingRecord {
                entity: record.entity,
                camera_index: record.camera_index,
                screen_position: picking_screen_position(primary_camera, instance),
                depth_meters: record.distance_meters,
                material: instance.material,
                lod: record.lod,
                visible_fraction: record.visible_fraction,
                source_instance_index: *source_instance_index,
                editor_selectable: !instance.flags.transparent
                    || instance.flags.cracked
                    || instance.flags.human,
            })
        })
        .collect()
}

fn picking_screen_position(camera: Option<&Camera>, instance: &SceneInstanceRecord) -> [f32; 2] {
    let Some(camera) = camera else {
        return [0.5, 0.5];
    };
    let delta = Vec3::new(
        instance.transform.translation_meters.x - camera.position.x,
        instance.transform.translation_meters.y - camera.position.y,
        instance.transform.translation_meters.z - camera.position.z,
    );
    [
        (0.5 + delta.x * 0.055).clamp(0.0, 1.0),
        (0.5 - delta.z * 0.075 + delta.y * 0.025).clamp(0.0, 1.0),
    ]
}

fn build_reference_comparison_images(
    packet: &RenderPacket,
    gameplay_image: TextureHandle,
    quality: &RenderQualityProfile,
) -> Option<ReferenceComparisonImages> {
    reference_mode(quality).then(|| ReferenceComparisonImages {
        gameplay_image,
        reference_image: TextureHandle(70_000 + packet.frame_id as u128),
        error_heatmap_image: TextureHandle(71_000 + packet.frame_id as u128),
        metrics_buffer: GpuResourceHandle(72_000 + packet.frame_id as u128),
        compared_frame_id: packet.frame_id,
        sample_budget: reference_sample_budget(quality),
        rms_luminance_error: estimated_reference_rms_error(packet, quality),
        max_luminance_error: estimated_reference_max_error(packet, quality),
        same_scene_hash: render_scene_hash(packet),
    })
}

fn build_capture_frames(
    packet: &RenderPacket,
    final_image: TextureHandle,
    depth_buffer: TextureHandle,
    motion_vectors: GpuBufferHandle,
    quality: &RenderQualityProfile,
) -> RenderCaptureSet {
    let primary_camera = packet.cameras.first();
    let mut captures = vec![RenderFrameCapture {
        kind: RenderCaptureKind::Screenshot,
        frame_id: packet.frame_id,
        image: TextureHandle(80_000 + packet.frame_id as u128),
        depth: depth_buffer,
        motion_vectors,
        camera_label: primary_camera
            .map(|camera| camera.label.clone())
            .unwrap_or_else(|| "missing".to_string()),
        exposure_bias: primary_camera
            .map(|camera| camera.exposure_bias)
            .unwrap_or(0.0),
        story_entity_count: packet
            .world_cell_visibility
            .iter()
            .map(|cell| cell.active_story_thread_count as usize)
            .sum(),
        weather_condition: packet.weather.condition,
    }];

    if !packet.ai_intent_overlays.is_empty()
        || !packet.humans.is_empty()
        || reference_mode(quality)
        || packet.weather.condition != WeatherCondition::Clear
    {
        captures.push(RenderFrameCapture {
            kind: RenderCaptureKind::CinematicFrame,
            frame_id: packet.frame_id,
            image: final_image,
            depth: depth_buffer,
            motion_vectors,
            camera_label: primary_camera
                .map(|camera| format!("{} cinematic", camera.label))
                .unwrap_or_else(|| "cinematic".to_string()),
            exposure_bias: primary_camera
                .map(|camera| camera.exposure_bias)
                .unwrap_or(0.0),
            story_entity_count: packet.ai_intent_overlays.len()
                + packet.humans.len()
                + packet
                    .world_cell_visibility
                    .iter()
                    .map(|cell| cell.active_story_thread_count as usize)
                    .sum::<usize>(),
            weather_condition: packet.weather.condition,
        });
    }

    captures
}

fn build_perception_buffers(
    packet: &RenderPacket,
    depth_buffer: TextureHandle,
    motion_vectors: GpuBufferHandle,
    quality: &RenderQualityProfile,
) -> OptionalPerceptionBuffers {
    let hazard_volume_count = packet
        .volumes
        .iter()
        .filter(|volume| volume.hazard_level > 0.0 || volume.visibility_blocking > 0.0)
        .count();
    if packet.visible_entities.is_empty()
        || (quality.quality_tier == QualityTier::Disabled && hazard_volume_count == 0)
    {
        return None;
    }

    Some(PerceptionBufferSet {
        entity_id_buffer: GpuResourceHandle(90_000 + packet.frame_id as u128),
        material_id_buffer: GpuResourceHandle(91_000 + packet.frame_id as u128),
        depth_pyramid: TextureHandle(depth_buffer.0 + 90_000),
        motion_vectors,
        visible_entity_count: packet.visible_entities.len(),
        hazard_volume_count,
        ai_overlay_count: packet.ai_intent_overlays.len(),
        confidence: perception_confidence(packet, quality),
    })
}

fn estimated_reference_rms_error(packet: &RenderPacket, quality: &RenderQualityProfile) -> f32 {
    let approximation_pressure = packet.volumes.len() as f32 * 0.006
        + packet.decals.len() as f32 * 0.002
        + packet.world_cell_visibility.len() as f32 * 0.001
        + f32::from(!quality.ray_traced_reflections) * 0.012
        + f32::from(!quality.ray_traced_shadows) * 0.008;
    approximation_pressure.clamp(0.0, 0.12)
}

fn estimated_reference_max_error(packet: &RenderPacket, quality: &RenderQualityProfile) -> f32 {
    (estimated_reference_rms_error(packet, quality) * 3.5
        + packet.weather.neon_haze * 0.03
        + packet.weather.toxic_gas_density * 0.02)
        .clamp(0.0, 0.5)
}

fn render_scene_hash(packet: &RenderPacket) -> u128 {
    let mut hash = packet.frame_id as u128 ^ ((packet.visible_entities.len() as u128) << 32);
    for entity in &packet.visible_entities {
        hash = hash
            .wrapping_mul(1_099_511_628_211)
            .wrapping_add(*entity as u128);
    }
    hash ^= (packet.material_records.len() as u128) << 48;
    hash ^= (packet.decals.len() as u128) << 64;
    hash ^= (packet.world_cell_visibility.len() as u128) << 72;
    hash
}

fn perception_confidence(packet: &RenderPacket, quality: &RenderQualityProfile) -> f32 {
    let quality_base = match quality.quality_tier {
        QualityTier::Disabled => 0.0,
        QualityTier::BackgroundApproximation => 0.48,
        QualityTier::NormalRuntime => 0.72,
        QualityTier::HeroHighFidelityRuntime => 0.88,
        QualityTier::ReferenceOfflineValidation => 0.96,
    };
    (quality_base + (packet.visible_entities.len() as f32 / 64.0).min(0.08)
        - packet.weather.toxic_gas_density * 0.08
        - packet.weather.fog_density * 0.04)
        .clamp(0.0, 1.0)
}

fn render_debug_data(
    packet: &RenderPacket,
    usage: &RenderBudgetUsage,
    quality: &RenderQualityProfile,
    draw_plan: &GpuDrivenDrawPlan,
) -> Vec<RenderDebugEntry> {
    let primary_camera = packet.cameras.first();
    let mut entries = vec![
        RenderDebugEntry {
            view: DebugRenderView::Visibility,
            label: "visible_entities".to_string(),
            value: packet.visible_entities.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Visibility,
            label: "picking_records".to_string(),
            value: packet
                .instance_records
                .iter()
                .filter(|instance| instance.flags.visible)
                .count()
                .to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Visibility,
            label: "perception_candidate_entities".to_string(),
            value: packet.visible_entities.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Visibility,
            label: "world_cell_visibility_records".to_string(),
            value: packet.world_cell_visibility.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Visibility,
            label: "world_cell_render_states".to_string(),
            value: world_cell_render_state_debug(packet),
        },
        RenderDebugEntry {
            view: DebugRenderView::Visibility,
            label: "hero_or_gameplay_cells".to_string(),
            value: hero_or_gameplay_cell_count(packet).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Visibility,
            label: "max_world_cell_visibility_weight".to_string(),
            value: format!("{:.2}", max_world_cell_visibility_weight(packet)),
        },
        RenderDebugEntry {
            view: DebugRenderView::Visibility,
            label: "hero_pinned_entities".to_string(),
            value: packet
                .instance_records
                .iter()
                .filter(|instance| instance.flags.human || instance.flags.cracked)
                .count()
                .to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "dynamic_lights".to_string(),
            value: packet.lights.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "shadow_casting_lights".to_string(),
            value: packet
                .lights
                .iter()
                .filter(|light| light.casts_shadows)
                .count()
                .to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "neon_lights".to_string(),
            value: packet
                .lights
                .iter()
                .filter(|light| light.kind == LightKind::Neon)
                .count()
                .to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "weather_condition".to_string(),
            value: packet.weather.condition.label().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "weather_surface_wetness".to_string(),
            value: format!("{:.2}", packet.weather.surface_wetness),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "weather_flood_depth_meters".to_string(),
            value: format!("{:.2}", packet.weather.flood_depth_meters),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "weather_toxic_gas_density".to_string(),
            value: format!("{:.2}", packet.weather.toxic_gas_density),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "weather_neon_haze".to_string(),
            value: format!("{:.2}", packet.weather.neon_haze),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "weather_source_events".to_string(),
            value: packet.weather.source_event_count.to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "total_light_lumens".to_string(),
            value: format!("{:.0}", total_light_lumens(packet)),
        },
        RenderDebugEntry {
            view: DebugRenderView::Luminance,
            label: "linear_hdr_until_display".to_string(),
            value: "true".to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Luminance,
            label: "hdr_histogram".to_string(),
            value: "camera_exposure_metering".to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Exposure,
            label: "camera_label".to_string(),
            value: primary_camera
                .map(|camera| camera.label.clone())
                .unwrap_or_else(|| "missing".to_string()),
        },
        RenderDebugEntry {
            view: DebugRenderView::Exposure,
            label: "focal_length_mm".to_string(),
            value: primary_camera
                .map(|camera| format!("{:.1}", camera.focal_length_mm))
                .unwrap_or_else(|| "0.0".to_string()),
        },
        RenderDebugEntry {
            view: DebugRenderView::Exposure,
            label: "sensor_size_mm".to_string(),
            value: primary_camera
                .map(|camera| {
                    format!(
                        "{:.1}x{:.1}",
                        camera.sensor_width_mm, camera.sensor_height_mm
                    )
                })
                .unwrap_or_else(|| "0.0x0.0".to_string()),
        },
        RenderDebugEntry {
            view: DebugRenderView::Exposure,
            label: "aperture_f_stop".to_string(),
            value: primary_camera
                .map(|camera| format!("{:.1}", camera.aperture_f_stop))
                .unwrap_or_else(|| "0.0".to_string()),
        },
        RenderDebugEntry {
            view: DebugRenderView::Exposure,
            label: "shutter_seconds".to_string(),
            value: primary_camera
                .map(|camera| format!("{:.5}", camera.shutter_seconds))
                .unwrap_or_else(|| "0.00000".to_string()),
        },
        RenderDebugEntry {
            view: DebugRenderView::Exposure,
            label: "iso".to_string(),
            value: primary_camera
                .map(|camera| format!("{:.0}", camera.iso))
                .unwrap_or_else(|| "0".to_string()),
        },
        RenderDebugEntry {
            view: DebugRenderView::Exposure,
            label: "exposure_bias".to_string(),
            value: primary_camera
                .map(|camera| format!("{:.2}", camera.exposure_bias))
                .unwrap_or_else(|| "0.00".to_string()),
        },
        RenderDebugEntry {
            view: DebugRenderView::Exposure,
            label: "focus_distance_meters".to_string(),
            value: primary_camera
                .map(|camera| format!("{:.2}", camera.focus_distance_meters))
                .unwrap_or_else(|| "0.00".to_string()),
        },
        RenderDebugEntry {
            view: DebugRenderView::Exposure,
            label: "motion_blur".to_string(),
            value: primary_camera
                .is_some_and(|camera| camera.motion_blur_enabled)
                .to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Exposure,
            label: "depth_of_field".to_string(),
            value: primary_camera
                .is_some_and(|camera| camera.depth_of_field_enabled)
                .to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Normals,
            label: "debug_view".to_string(),
            value: "gbuffer_normals".to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Roughness,
            label: "debug_view".to_string(),
            value: "material_roughness".to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialIds,
            label: "debug_view".to_string(),
            value: "material_ids".to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::GlobalIllumination,
            label: "debug_view".to_string(),
            value: "gi_cache".to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Shadows,
            label: "debug_view".to_string(),
            value: "virtual_shadow_pages".to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Velocity,
            label: "debug_view".to_string(),
            value: "motion_vectors".to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Depth,
            label: "debug_view".to_string(),
            value: "depth_prepass".to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "ray_traced_shadows".to_string(),
            value: quality.ray_traced_shadows.to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "virtual_shadow_requests".to_string(),
            value: packet.virtual_shadow_pages.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "dirty_shadow_pages".to_string(),
            value: dirty_shadow_page_request_count(packet).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "shadow_page_estimate".to_string(),
            value: estimated_shadow_page_count(packet).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "shadow_invalidation_reasons".to_string(),
            value: shadow_page_reason_debug(packet),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "volume_regions".to_string(),
            value: packet.volumes.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "volume_media".to_string(),
            value: volume_media_debug(packet),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "max_volume_visibility".to_string(),
            value: format!("{:.2}", max_volume_visibility(packet)),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "max_volume_hazard".to_string(),
            value: format!("{:.2}", max_volume_hazard(packet)),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "weather_fog_density".to_string(),
            value: format!("{:.2}", packet.weather.fog_density),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lighting,
            label: "weather_dust_density".to_string(),
            value: format!("{:.2}", packet.weather.dust_density),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "wet_material_entities".to_string(),
            value: packet.wet_material_entities.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "cracked_material_entities".to_string(),
            value: packet.cracked_material_entities.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "deformed_material_entities".to_string(),
            value: packet.deformed_material_entities.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "surface_effects".to_string(),
            value: packet.surface_effects.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "decal_records".to_string(),
            value: packet.decals.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "decal_kinds".to_string(),
            value: decal_kind_debug(packet),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "decal_atlas_pages".to_string(),
            value: decal_atlas_page_count(packet).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "event_backed_decals".to_string(),
            value: event_backed_decal_count(packet).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "state_backed_decals".to_string(),
            value: state_backed_decal_count(packet).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "max_decal_intensity".to_string(),
            value: format!("{:.2}", max_decal_intensity(packet)),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "material_runtime_programs".to_string(),
            value: packet.material_programs.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "material_cache_bindings".to_string(),
            value: material_cache_binding_count(packet).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "material_cache_textures".to_string(),
            value: material_cache_texture_debug(packet),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "aged_surface_materials".to_string(),
            value: aged_surface_material_count(packet).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "aged_surface_cache_bindings".to_string(),
            value: aged_surface_cache_binding_count(packet).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "material_state_channels".to_string(),
            value: material_state_channel_debug(packet),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "material_state_active_counts".to_string(),
            value: material_state_active_counts_debug(packet),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "material_state_maxima".to_string(),
            value: material_state_maxima_debug(packet),
        },
        RenderDebugEntry {
            view: DebugRenderView::MaterialState,
            label: "fracture_interior_materials".to_string(),
            value: fracture_interior_material_count(packet).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::HumanRendering,
            label: "human_records".to_string(),
            value: packet.humans.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::HumanRendering,
            label: "hero_face_humans".to_string(),
            value: human_lod_count(packet, HumanRenderLod::HeroFace).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::HumanRendering,
            label: "skin_subsurface_avg".to_string(),
            value: format!(
                "{:.2}",
                average_human_metric(packet, |human| { human.subsurface_strength })
            ),
        },
        RenderDebugEntry {
            view: DebugRenderView::HumanRendering,
            label: "pore_microdetail_avg".to_string(),
            value: format!(
                "{:.2}",
                average_human_metric(packet, |human| { human.pore_microdetail_strength })
            ),
        },
        RenderDebugEntry {
            view: DebugRenderView::HumanRendering,
            label: "eye_wetness_avg".to_string(),
            value: format!(
                "{:.2}",
                average_human_metric(packet, |human| { human.eye_wetness })
            ),
        },
        RenderDebugEntry {
            view: DebugRenderView::HumanRendering,
            label: "hair_strand_buffers".to_string(),
            value: packet
                .humans
                .iter()
                .filter(|human| human.hair_strand_buffer.is_some())
                .count()
                .to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::HumanRendering,
            label: "clothing_motion_avg".to_string(),
            value: format!(
                "{:.2}",
                average_human_metric(packet, |human| { human.clothing_motion_response })
            ),
        },
        RenderDebugEntry {
            view: DebugRenderView::HumanRendering,
            label: "cybernetic_surfaces".to_string(),
            value: packet
                .humans
                .iter()
                .filter(|human| human.cybernetic_surface_ratio > 0.0)
                .count()
                .to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::HumanRendering,
            label: "closeup_quality_avg".to_string(),
            value: format!(
                "{:.2}",
                average_human_metric(packet, |human| { human.closeup_quality_score })
            ),
        },
        RenderDebugEntry {
            view: DebugRenderView::AiDecisions,
            label: "ai_intent_overlays".to_string(),
            value: packet.ai_intent_overlays.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::AiDecisions,
            label: "ai_intent_actions".to_string(),
            value: packet
                .ai_intent_overlays
                .iter()
                .map(|overlay| overlay.action.as_str())
                .collect::<Vec<_>>()
                .join(","),
        },
        RenderDebugEntry {
            view: DebugRenderView::GpuCost,
            label: "estimated_gpu_ms".to_string(),
            value: format!("{:.2}", usage.estimated_gpu_milliseconds),
        },
        RenderDebugEntry {
            view: DebugRenderView::GpuCost,
            label: "debug_request_views".to_string(),
            value: packet.debug_request.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::GpuCost,
            label: "capture_outputs".to_string(),
            value: render_capture_count(packet, quality).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::GpuCost,
            label: "reference_comparison_images".to_string(),
            value: reference_mode(quality).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::GpuCost,
            label: "perception_buffers".to_string(),
            value: (!packet.visible_entities.is_empty()).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::GpuCost,
            label: "gpu_draw_batches".to_string(),
            value: draw_plan.batches.len().to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::GpuCost,
            label: "indirect_draws".to_string(),
            value: draw_plan.indirect_draw_count.to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::GpuCost,
            label: "cpu_draw_calls".to_string(),
            value: draw_plan.cpu_draw_call_count.to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Overdraw,
            label: "gpu_selected_clusters".to_string(),
            value: draw_plan.selected_cluster_count.to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Overdraw,
            label: "triangle_budget".to_string(),
            value: draw_plan.triangle_budget.to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Overdraw,
            label: "transparent_batches".to_string(),
            value: draw_plan
                .batches
                .iter()
                .filter(|batch| batch.transparent)
                .count()
                .to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lod,
            label: "quality_tier".to_string(),
            value: format!("{:?}", quality.quality_tier),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lod,
            label: "lod_hero".to_string(),
            value: instance_lod_count(packet, RenderLodTier::Hero).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lod,
            label: "lod_near".to_string(),
            value: instance_lod_count(packet, RenderLodTier::Near).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lod,
            label: "lod_mid".to_string(),
            value: instance_lod_count(packet, RenderLodTier::Mid).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lod,
            label: "lod_far".to_string(),
            value: instance_lod_count(packet, RenderLodTier::Far).to_string(),
        },
        RenderDebugEntry {
            view: DebugRenderView::Lod,
            label: "lod_impostor".to_string(),
            value: instance_lod_count(packet, RenderLodTier::Impostor).to_string(),
        },
    ];
    entries.push(RenderDebugEntry {
        view: DebugRenderView::GpuCost,
        label: "temporal_reconstruction".to_string(),
        value: quality.temporal_reconstruction.to_string(),
    });
    entries.push(RenderDebugEntry {
        view: DebugRenderView::GpuCost,
        label: "reference_path_tracing".to_string(),
        value: reference_mode(quality).to_string(),
    });
    entries.push(RenderDebugEntry {
        view: DebugRenderView::GpuCost,
        label: "reference_sample_budget".to_string(),
        value: reference_sample_budget(quality).to_string(),
    });
    entries.push(RenderDebugEntry {
        view: DebugRenderView::GpuCost,
        label: "reference_comparison".to_string(),
        value: reference_mode(quality).to_string(),
    });
    entries.push(RenderDebugEntry {
        view: DebugRenderView::GpuCost,
        label: "volumetric_resolution_scale".to_string(),
        value: format!("{:.2}", quality.volumetric_resolution_scale),
    });
    entries
}

fn total_light_lumens(packet: &RenderPacket) -> f32 {
    packet
        .lights
        .iter()
        .map(|light| light.intensity_lumens)
        .sum()
}

fn volume_media_debug(packet: &RenderPacket) -> String {
    packet
        .volumes
        .iter()
        .map(|volume| volume.medium.label())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",")
}

fn max_volume_visibility(packet: &RenderPacket) -> f32 {
    packet
        .volumes
        .iter()
        .map(|volume| volume.visibility_blocking)
        .fold(0.0, f32::max)
}

fn max_volume_hazard(packet: &RenderPacket) -> f32 {
    packet
        .volumes
        .iter()
        .map(|volume| volume.hazard_level)
        .fold(0.0, f32::max)
}

fn world_cell_render_state_debug(packet: &RenderPacket) -> String {
    packet
        .world_cell_visibility
        .iter()
        .fold(BTreeMap::new(), |mut counts, cell| {
            *counts.entry(cell.render_state.label()).or_insert(0usize) += 1;
            counts
        })
        .into_iter()
        .map(|(label, count)| format!("{label}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn hero_or_gameplay_cell_count(packet: &RenderPacket) -> usize {
    packet
        .world_cell_visibility
        .iter()
        .filter(|cell| {
            matches!(
                cell.render_state,
                WorldCellRenderState::Hero | WorldCellRenderState::Gameplay
            )
        })
        .count()
}

fn max_world_cell_visibility_weight(packet: &RenderPacket) -> f32 {
    packet
        .world_cell_visibility
        .iter()
        .map(|cell| cell.visibility_weight)
        .fold(0.0, f32::max)
}

fn render_capture_count(packet: &RenderPacket, quality: &RenderQualityProfile) -> usize {
    1 + usize::from(
        !packet.ai_intent_overlays.is_empty()
            || !packet.humans.is_empty()
            || reference_mode(quality)
            || packet.weather.condition != WeatherCondition::Clear,
    )
}

fn dirty_shadow_page_request_count(packet: &RenderPacket) -> usize {
    packet
        .virtual_shadow_pages
        .iter()
        .filter(|page| page.dirty)
        .count()
}

fn estimated_shadow_page_count(packet: &RenderPacket) -> u32 {
    packet
        .virtual_shadow_pages
        .iter()
        .map(|page| page.estimated_page_count)
        .sum()
}

fn shadow_page_reason_debug(packet: &RenderPacket) -> String {
    packet
        .virtual_shadow_pages
        .iter()
        .map(|page| shadow_reason_label(page.reason))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",")
}

fn shadow_reason_label(reason: ShadowPageInvalidationReason) -> &'static str {
    match reason {
        ShadowPageInvalidationReason::ShadowCastingLight => "shadow_casting_light",
        ShadowPageInvalidationReason::DestructibleGeometry => "destructible_geometry",
        ShadowPageInvalidationReason::AnimatedHuman => "animated_human",
        ShadowPageInvalidationReason::VolumeScattering => "volume_scattering",
        ShadowPageInvalidationReason::MaterialStateChange => "material_state_change",
        ShadowPageInvalidationReason::Deformation => "deformation",
    }
}

fn decal_kind_debug(packet: &RenderPacket) -> String {
    packet
        .decals
        .iter()
        .map(|decal| decal.kind.label())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",")
}

fn decal_atlas_page_count(packet: &RenderPacket) -> usize {
    packet
        .decals
        .iter()
        .map(|decal| decal.generated_atlas_page)
        .collect::<BTreeSet<_>>()
        .len()
}

fn event_backed_decal_count(packet: &RenderPacket) -> usize {
    packet
        .decals
        .iter()
        .filter(|decal| decal.source_event.is_some())
        .count()
}

fn state_backed_decal_count(packet: &RenderPacket) -> usize {
    packet
        .decals
        .iter()
        .filter(|decal| decal.generated_from_material_state)
        .count()
}

fn max_decal_intensity(packet: &RenderPacket) -> f32 {
    packet
        .decals
        .iter()
        .map(|decal| decal.intensity)
        .fold(0.0, f32::max)
}

fn material_cache_binding_count(packet: &RenderPacket) -> usize {
    packet
        .material_programs
        .iter()
        .map(|program| program.cache_textures.len())
        .sum()
}

fn aged_surface_cache_binding_count(packet: &RenderPacket) -> usize {
    packet
        .material_programs
        .iter()
        .filter(|program| program.cache_textures.contains(&TEXTURE_AGED_SURFACE_CACHE))
        .count()
}

fn aged_surface_material_count(packet: &RenderPacket) -> usize {
    packet
        .material_state_records
        .iter()
        .filter(|state| material_state_needs_aged_surface_cache(state))
        .count()
}

fn fracture_interior_material_count(packet: &RenderPacket) -> usize {
    packet
        .material_programs
        .iter()
        .filter(|program| {
            program.displacement_policy == MaterialDisplacementPolicy::FractureInterior
        })
        .count()
}

fn material_state_channel_debug(packet: &RenderPacket) -> String {
    packet
        .material_programs
        .iter()
        .flat_map(|program| program.required_state_channels.iter().copied())
        .map(material_state_channel_label)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",")
}

fn material_cache_texture_debug(packet: &RenderPacket) -> String {
    packet
        .material_programs
        .iter()
        .flat_map(|program| program.cache_textures.iter().copied())
        .map(material_cache_texture_label)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",")
}

fn material_cache_texture_label(texture: TextureHandle) -> &'static str {
    match texture {
        TEXTURE_GLASS_CRACK_DETAIL_CACHE => "crack_detail",
        TEXTURE_WET_ASPHALT_REFLECTION_CACHE => "wet_reflection",
        TEXTURE_HUMAN_SKIN_DETAIL_CACHE => "human_skin_detail",
        TEXTURE_AGED_SURFACE_CACHE => "aged_surface",
        _ => "unknown",
    }
}

fn material_state_needs_aged_surface_cache(state: &MaterialStateGpuRecord) -> bool {
    state.corrosion >= 0.2
        || state.soot >= 0.18
        || state.heat >= 0.12
        || state.electrical_charge >= 0.1
        || state.oil_contamination >= 0.12
}

fn material_state_active_counts_debug(packet: &RenderPacket) -> String {
    let mut wetness = 0;
    let mut cracks = 0;
    let mut soot = 0;
    let mut corrosion = 0;
    let mut heat = 0;
    let mut electrical = 0;
    let mut biological = 0;
    let mut oil = 0;
    let mut plastic_strain = 0;

    for state in &packet.material_state_records {
        wetness += usize::from(state.wetness >= 0.05);
        cracks += usize::from(state.crack_density >= 0.05);
        soot += usize::from(state.soot >= 0.05);
        corrosion += usize::from(state.corrosion >= 0.05);
        heat += usize::from(state.heat >= 0.05);
        electrical += usize::from(state.electrical_charge >= 0.05);
        biological += usize::from(state.biological_contamination >= 0.05);
        oil += usize::from(state.oil_contamination >= 0.05);
        plastic_strain += usize::from(state.plastic_strain >= 0.05);
    }

    format!(
        "wetness:{wetness},cracks:{cracks},soot:{soot},corrosion:{corrosion},heat:{heat},electrical:{electrical},biological:{biological},oil:{oil},plastic_strain:{plastic_strain}"
    )
}

fn material_state_maxima_debug(packet: &RenderPacket) -> String {
    let mut wetness: f32 = 0.0;
    let mut cracks: f32 = 0.0;
    let mut soot: f32 = 0.0;
    let mut corrosion: f32 = 0.0;
    let mut heat: f32 = 0.0;
    let mut electrical: f32 = 0.0;
    let mut biological: f32 = 0.0;
    let mut oil: f32 = 0.0;
    let mut plastic_strain: f32 = 0.0;

    for state in &packet.material_state_records {
        wetness = wetness.max(state.wetness);
        cracks = cracks.max(state.crack_density);
        soot = soot.max(state.soot);
        corrosion = corrosion.max(state.corrosion);
        heat = heat.max(state.heat);
        electrical = electrical.max(state.electrical_charge);
        biological = biological.max(state.biological_contamination);
        oil = oil.max(state.oil_contamination);
        plastic_strain = plastic_strain.max(state.plastic_strain);
    }

    format!(
        "wetness:{wetness:.2},cracks:{cracks:.2},soot:{soot:.2},corrosion:{corrosion:.2},heat:{heat:.2},electrical:{electrical:.2},biological:{biological:.2},oil:{oil:.2},plastic_strain:{plastic_strain:.2}"
    )
}

fn material_state_channel_label(channel: MaterialStateChannel) -> &'static str {
    match channel {
        MaterialStateChannel::Wetness => "wetness",
        MaterialStateChannel::Cracks => "cracks",
        MaterialStateChannel::Soot => "soot",
        MaterialStateChannel::Corrosion => "corrosion",
        MaterialStateChannel::Heat => "heat",
        MaterialStateChannel::PlasticStrain => "plastic_strain",
        MaterialStateChannel::Electrical => "electrical",
        MaterialStateChannel::Biological => "biological",
        MaterialStateChannel::Oil => "oil",
    }
}

fn instance_lod_count(packet: &RenderPacket, lod: RenderLodTier) -> usize {
    packet
        .instance_records
        .iter()
        .filter(|instance| instance.lod == lod)
        .count()
}

fn human_lod_count(packet: &RenderPacket, lod: HumanRenderLod) -> usize {
    packet
        .humans
        .iter()
        .filter(|human| human.lod == lod)
        .count()
}

fn average_human_metric(packet: &RenderPacket, metric: impl Fn(&HumanRenderRecord) -> f32) -> f32 {
    if packet.humans.is_empty() {
        return 0.0;
    }
    packet.humans.iter().map(metric).sum::<f32>() / packet.humans.len() as f32
}

fn ray_feedback(
    packet: &RenderPacket,
    quality: &RenderQualityProfile,
) -> Option<RayTracingFeedback> {
    (quality.ray_traced_reflections || quality.ray_traced_shadows).then(|| RayTracingFeedback {
        reflection_candidate_count: packet.wet_material_entities.len()
            + packet
                .material_records
                .iter()
                .filter(|material| material.transparency > 0.1 || material.metallic > 0.5)
                .count(),
        shadow_candidate_count: packet
            .lights
            .iter()
            .filter(|light| light.casts_shadows)
            .count(),
        fallback_used: quality.capability_tier < RenderCapabilityTier::SelectiveRayTracing,
    })
}

fn instance_flags(
    tags: &[String],
    renderable: &Renderable,
    material_state: &MaterialState,
    is_human: bool,
    event_deformed: bool,
    event_constraint_corrected: bool,
) -> InstanceRenderFlags {
    InstanceRenderFlags {
        visible: renderable.visible,
        human: is_human,
        wet: material_state.moisture >= 0.5 || has_tag(tags, "water_leak"),
        cracked: material_state.crack_density >= 0.5 || has_tag(tags, "destructible"),
        deformed: material_state.plastic_strain >= 0.05 || event_deformed,
        constraint_corrected: event_constraint_corrected,
        emissive: has_tag(tags, "light")
            || has_tag(tags, "neon")
            || material_state.electrical_charge > 10.0,
        transparent: has_tag(tags, "glass"),
    }
}

fn choose_lod(
    distance: f32,
    flags: InstanceRenderFlags,
    human_quality: Option<QualityTier>,
) -> RenderLodTier {
    if flags.human && human_quality >= Some(QualityTier::HeroHighFidelityRuntime) {
        return RenderLodTier::Hero;
    }
    if flags.cracked
        || flags.deformed
        || flags.constraint_corrected
        || flags.emissive
        || distance <= 8.0
    {
        RenderLodTier::Hero
    } else if distance <= 24.0 {
        RenderLodTier::Near
    } else if distance <= 60.0 {
        RenderLodTier::Mid
    } else {
        RenderLodTier::Impostor
    }
}

fn human_lod(quality: QualityTier, distance: f32) -> HumanRenderLod {
    if quality >= QualityTier::HeroHighFidelityRuntime && distance <= 12.0 {
        HumanRenderLod::HeroFace
    } else if distance <= 24.0 {
        HumanRenderLod::NearbyNpc
    } else if distance <= 80.0 {
        HumanRenderLod::Crowd
    } else {
        HumanRenderLod::DistantImpostor
    }
}

fn human_hair_lod(lod: HumanRenderLod, quality: QualityTier) -> HumanHairRenderLod {
    match (lod, quality) {
        (HumanRenderLod::HeroFace, QualityTier::ReferenceOfflineValidation)
        | (HumanRenderLod::HeroFace, QualityTier::HeroHighFidelityRuntime) => {
            HumanHairRenderLod::Strand
        }
        (HumanRenderLod::HeroFace, _) | (HumanRenderLod::NearbyNpc, _) => {
            HumanHairRenderLod::Hybrid
        }
        (HumanRenderLod::Crowd, _) => HumanHairRenderLod::Cards,
        (HumanRenderLod::DistantImpostor, _) => HumanHairRenderLod::Impostor,
    }
}

fn human_subsurface_strength(quality: QualityTier, surface: HumanSurfaceRenderResponse) -> f32 {
    let quality_bonus = if quality >= QualityTier::HeroHighFidelityRuntime {
        0.18
    } else {
        0.0
    };
    (0.42 + quality_bonus + surface.blood_redness * 0.12 - surface.skin_wetness * 0.06)
        .clamp(0.0, 1.0)
}

fn human_pore_microdetail_strength(quality: QualityTier, lod: HumanRenderLod) -> f32 {
    let lod_strength: f32 = match lod {
        HumanRenderLod::HeroFace => 0.9,
        HumanRenderLod::NearbyNpc => 0.68,
        HumanRenderLod::Crowd => 0.32,
        HumanRenderLod::DistantImpostor => 0.08,
    };
    let quality_scale: f32 = if quality >= QualityTier::HeroHighFidelityRuntime {
        1.0
    } else {
        0.72
    };
    (lod_strength * quality_scale).clamp(0.0, 1.0)
}

fn human_hair_motion_response(lod: HumanRenderLod, wetness: f32) -> f32 {
    let lod_response = match lod {
        HumanRenderLod::HeroFace => 0.84,
        HumanRenderLod::NearbyNpc => 0.62,
        HumanRenderLod::Crowd => 0.28,
        HumanRenderLod::DistantImpostor => 0.04,
    };
    (lod_response * (1.0 - wetness * 0.28)).clamp(0.0, 1.0)
}

fn human_clothing_motion_response(lod: HumanRenderLod, wetness: f32) -> f32 {
    let lod_response = match lod {
        HumanRenderLod::HeroFace => 0.72,
        HumanRenderLod::NearbyNpc => 0.58,
        HumanRenderLod::Crowd => 0.24,
        HumanRenderLod::DistantImpostor => 0.03,
    };
    (lod_response * (1.0 - wetness * 0.22)).clamp(0.0, 1.0)
}

fn human_cybernetic_surface_ratio(
    human_id: HumanId,
    material: Option<MaterialId>,
    tags: &[String],
) -> f32 {
    if tags
        .iter()
        .any(|tag| tag.contains("cybernetic") || tag.contains("implant"))
    {
        0.32
    } else if material.is_some_and(|material| material.is_multiple_of(7))
        || human_id.is_multiple_of(3)
    {
        0.18
    } else {
        0.0
    }
}

fn human_closeup_quality_score(
    lod: HumanRenderLod,
    quality: QualityTier,
    cybernetic_surface_ratio: f32,
) -> f32 {
    let lod_score = match lod {
        HumanRenderLod::HeroFace => 0.88,
        HumanRenderLod::NearbyNpc => 0.7,
        HumanRenderLod::Crowd => 0.38,
        HumanRenderLod::DistantImpostor => 0.12,
    };
    let quality_bonus = if quality >= QualityTier::HeroHighFidelityRuntime {
        0.08
    } else {
        0.0
    };
    (lod_score + quality_bonus + cybernetic_surface_ratio * 0.08).clamp(0.0, 1.0)
}

fn mesh_cluster_record(
    entity: EntityId,
    mesh: MeshAssetHandle,
    lod: RenderLodTier,
    flags: InstanceRenderFlags,
) -> MeshClusterRecord {
    let base = match lod {
        RenderLodTier::Hero => 96,
        RenderLodTier::Near => 48,
        RenderLodTier::Mid => 20,
        RenderLodTier::Far => 8,
        RenderLodTier::Impostor => 2,
    };
    let material_bonus = u32::from(flags.human) * 24
        + u32::from(flags.cracked) * 16
        + u32::from(flags.deformed) * 10
        + u32::from(flags.constraint_corrected) * 4
        + u32::from(flags.transparent) * 8;
    let cluster_count = base + material_bonus;

    MeshClusterRecord {
        entity,
        mesh,
        cluster_count,
        triangle_budget: cluster_count * 96,
        lod,
    }
}

fn material_gpu_record(
    entity: EntityId,
    renderable: &Renderable,
    tags: &[String],
    state: &MaterialState,
    is_human: bool,
) -> MaterialGpuRecord {
    let mut base_color = pseudo_material_color(renderable.material, tags, is_human);
    let wetness = state.moisture.clamp(0.0, 1.0);
    let soot = state.soot.clamp(0.0, 1.0);
    let corrosion = state.corrosion.clamp(0.0, 1.0);
    let plastic_strain = state.plastic_strain.clamp(0.0, 1.0);
    let heat = ((state.temperature - 293.15) / 800.0).clamp(0.0, 1.0);
    base_color[0] = (base_color[0] * (1.0 - wetness * 0.12 - soot * 0.3)).clamp(0.0, 16.0);
    base_color[1] = (base_color[1] * (1.0 - wetness * 0.1 - soot * 0.3)).clamp(0.0, 16.0);
    base_color[2] = (base_color[2] * (1.0 - wetness * 0.08 - soot * 0.3)).clamp(0.0, 16.0);
    base_color[0] = (base_color[0] * (1.0 - corrosion * 0.15) + corrosion * 0.42).clamp(0.0, 16.0);
    base_color[1] = (base_color[1] * (1.0 - corrosion * 0.3) + corrosion * 0.18).clamp(0.0, 16.0);
    base_color[2] = (base_color[2] * (1.0 - corrosion * 0.36) + corrosion * 0.06).clamp(0.0, 16.0);
    base_color[0] = (base_color[0] * (1.0 - plastic_strain * 0.08)).clamp(0.0, 16.0);
    base_color[1] = (base_color[1] * (1.0 - plastic_strain * 0.08)).clamp(0.0, 16.0);
    base_color[2] = (base_color[2] * (1.0 - plastic_strain * 0.06)).clamp(0.0, 16.0);
    base_color[0] = (base_color[0] + heat * 0.18).clamp(0.0, 16.0);
    base_color[1] = (base_color[1] + heat * 0.04).clamp(0.0, 16.0);

    let emissive =
        has_tag(tags, "light") || has_tag(tags, "neon") || state.electrical_charge > 10.0;
    let glass = has_tag(tags, "glass");
    let metal = has_tag(tags, "metal") || renderable.material == 3;
    let heat_emission = (heat - 0.45).max(0.0) * 3.0;

    MaterialGpuRecord {
        entity,
        material_id: renderable.material,
        base_color_linear: base_color,
        roughness: (0.62 - wetness * 0.35
            + state.crack_density * 0.08
            + plastic_strain * 0.12
            + soot * 0.2
            + corrosion * 0.18
            + heat * 0.06)
            .clamp(0.02, 1.0),
        metallic: if metal {
            (0.85 - corrosion * 0.28).clamp(0.0, 1.0)
        } else {
            0.0
        },
        emission_linear: if emissive {
            [2.8, 0.22, 2.1]
        } else if heat_emission > 0.0 {
            [heat_emission, heat_emission * 0.22, heat_emission * 0.04]
        } else {
            [0.0; 3]
        },
        transparency: if glass { 0.58 } else { 0.0 },
        subsurface: if is_human { 0.42 } else { 0.0 },
        anisotropy: if is_human || metal {
            (0.35 + plastic_strain * 0.25 + corrosion * 0.08).clamp(0.0, 1.0)
        } else {
            0.05
        },
    }
}

fn material_state_gpu_record(
    entity: EntityId,
    material_id: MaterialId,
    state: &MaterialState,
) -> MaterialStateGpuRecord {
    MaterialStateGpuRecord {
        entity,
        material_id,
        wetness: state.moisture.clamp(0.0, 1.0),
        soot: state.soot.clamp(0.0, 1.0),
        corrosion: state.corrosion.clamp(0.0, 1.0),
        crack_density: state.crack_density.clamp(0.0, 1.0),
        plastic_strain: state.plastic_strain.clamp(0.0, 1.0),
        biological_contamination: state.biological_contamination.clamp(0.0, 1.0),
        oil_contamination: state.oil_contamination.clamp(0.0, 1.0),
        heat: ((state.temperature - 293.15) / 800.0).clamp(0.0, 1.0),
        electrical_charge: (state.electrical_charge / 240.0).clamp(0.0, 1.0),
    }
}

fn pseudo_material_color(material_id: MaterialId, tags: &[String], is_human: bool) -> [f32; 4] {
    if is_human {
        return [0.74, 0.48, 0.38, 1.0];
    }
    if has_tag(tags, "glass") {
        return [0.62, 0.82, 1.0, 0.46];
    }
    if has_tag(tags, "neon") {
        return [1.6, 0.18, 1.35, 1.0];
    }
    if has_tag(tags, "asphalt") {
        return [0.08, 0.09, 0.1, 1.0];
    }
    if has_tag(tags, "water_leak") {
        return [0.13, 0.25, 0.34, 0.82];
    }

    let seed = (material_id as f32 % 11.0) / 11.0;
    [
        0.24 + seed * 0.18,
        0.26 + seed * 0.12,
        0.3 + seed * 0.1,
        1.0,
    ]
}

fn visible_fraction(instance: &SceneInstanceRecord, index: usize) -> f32 {
    let base = match instance.lod {
        RenderLodTier::Hero => 0.95,
        RenderLodTier::Near => 0.78,
        RenderLodTier::Mid => 0.48,
        RenderLodTier::Far => 0.25,
        RenderLodTier::Impostor => 0.12,
    };
    (base - (index % 5) as f32 * 0.015).clamp(0.05, 1.0)
}

fn visibility_reason(instance: &SceneInstanceRecord) -> VisibilityReason {
    if instance.flags.human || instance.flags.cracked || instance.flags.deformed {
        VisibilityReason::HeroPinned
    } else if instance.flags.emissive {
        VisibilityReason::LightEmitter
    } else {
        VisibilityReason::FrustumAccepted
    }
}

fn bounds_around(center: Vec3, horizontal_radius: f32, height: f32) -> Aabb {
    Aabb::new(
        Vec3::new(
            center.x - horizontal_radius,
            center.y - horizontal_radius,
            center.z,
        ),
        Vec3::new(
            center.x + horizontal_radius,
            center.y + horizontal_radius,
            center.z + height,
        ),
    )
}

fn shadow_source_event(event: &WorldEvent) -> Option<(EntityId, WorldEventId)> {
    match event.kind {
        WorldEventKind::GlassWallFractured { entity }
        | WorldEventKind::MeshReplaced { entity }
        | WorldEventKind::MaterialStateChanged { entity }
        | WorldEventKind::TransformChanged { entity }
        | WorldEventKind::HumanAppearanceUpdated { entity, .. }
        | WorldEventKind::MetalBent { entity, .. }
        | WorldEventKind::FlexibleConstraintResolved { entity, .. } => {
            Some((entity, event.event_id))
        }
        _ => None,
    }
}

fn shadow_page_id(
    entity: EntityId,
    source_event: WorldEventId,
    reason: ShadowPageInvalidationReason,
) -> u64 {
    let event_hash = (source_event as u64) ^ ((source_event >> 64) as u64);
    entity
        .wrapping_mul(1_099_511_628_211)
        .wrapping_add(event_hash.rotate_left(13))
        .wrapping_add(shadow_reason_code(reason))
}

fn shadow_reason_code(reason: ShadowPageInvalidationReason) -> u64 {
    match reason {
        ShadowPageInvalidationReason::ShadowCastingLight => 1,
        ShadowPageInvalidationReason::DestructibleGeometry => 2,
        ShadowPageInvalidationReason::AnimatedHuman => 3,
        ShadowPageInvalidationReason::VolumeScattering => 4,
        ShadowPageInvalidationReason::MaterialStateChange => 5,
        ShadowPageInvalidationReason::Deformation => 6,
    }
}

fn shadow_page_bounds_for_instance(
    instance: &SceneInstanceRecord,
    reason: ShadowPageInvalidationReason,
) -> Aabb {
    let radius = match reason {
        ShadowPageInvalidationReason::AnimatedHuman => 0.9,
        ShadowPageInvalidationReason::DestructibleGeometry => 1.35,
        ShadowPageInvalidationReason::MaterialStateChange => 1.1,
        ShadowPageInvalidationReason::Deformation
        | ShadowPageInvalidationReason::VolumeScattering
        | ShadowPageInvalidationReason::ShadowCastingLight => 1.0,
    };
    let height = if instance.flags.human { 2.2 } else { 1.4 };
    bounds_around(instance.transform.translation_meters, radius, height)
}

fn shadow_page_count_from_radius(radius_meters: f32) -> u32 {
    (radius_meters / 4.0).ceil().clamp(1.0, 12.0) as u32
}

fn shadow_page_count_for_instance(
    instance: &SceneInstanceRecord,
    reason: ShadowPageInvalidationReason,
) -> u32 {
    let lod_pages = match instance.lod {
        RenderLodTier::Hero => 4,
        RenderLodTier::Near => 3,
        RenderLodTier::Mid => 2,
        RenderLodTier::Far | RenderLodTier::Impostor => 1,
    };
    lod_pages
        + u32::from(matches!(
            reason,
            ShadowPageInvalidationReason::DestructibleGeometry
                | ShadowPageInvalidationReason::AnimatedHuman
        ))
}

fn shadow_page_priority(
    instance: &SceneInstanceRecord,
    reason: ShadowPageInvalidationReason,
) -> QualityTier {
    if instance.lod == RenderLodTier::Hero
        || matches!(
            reason,
            ShadowPageInvalidationReason::AnimatedHuman
                | ShadowPageInvalidationReason::DestructibleGeometry
        )
    {
        QualityTier::HeroHighFidelityRuntime
    } else if instance.lod <= RenderLodTier::Near {
        QualityTier::NormalRuntime
    } else {
        QualityTier::BackgroundApproximation
    }
}

fn shadow_contact_sharpness(
    instance: &SceneInstanceRecord,
    reason: ShadowPageInvalidationReason,
) -> f32 {
    let lod_base: f32 = match instance.lod {
        RenderLodTier::Hero => 0.9,
        RenderLodTier::Near => 0.72,
        RenderLodTier::Mid => 0.52,
        RenderLodTier::Far | RenderLodTier::Impostor => 0.34,
    };
    let reason_boost: f32 = match reason {
        ShadowPageInvalidationReason::AnimatedHuman => 0.12,
        ShadowPageInvalidationReason::DestructibleGeometry => 0.1,
        ShadowPageInvalidationReason::Deformation => 0.06,
        ShadowPageInvalidationReason::MaterialStateChange => 0.04,
        ShadowPageInvalidationReason::VolumeScattering
        | ShadowPageInvalidationReason::ShadowCastingLight => 0.0,
    };
    (lod_base + reason_boost).clamp(0.1, 1.0)
}

fn material_for_entity(snapshot: &WorldSnapshot, entity: EntityId) -> MaterialId {
    snapshot
        .renderables
        .find(entity)
        .map(|renderable| renderable.material)
        .or_else(|| {
            snapshot
                .physical_bodies
                .find(entity)
                .map(|body| body.material_id)
        })
        .unwrap_or(0)
}

impl RenderingModule {
    fn request_entity_asset(
        &mut self,
        frame: &FrameContext,
        out: &mut CommandSink,
        entity: EntityId,
        requested_quality: QualityTier,
        priority: AssetPriority,
        reason: &'static str,
    ) {
        let Some(renderable) = frame.snapshot.renderables.find(entity) else {
            return;
        };
        self.request_asset(
            frame,
            out,
            RenderAssetRequest {
                entity,
                asset_id: renderable.mesh.0,
                requested_quality,
                priority,
                reason,
            },
        );
    }

    fn request_asset(
        &mut self,
        frame: &FrameContext,
        out: &mut CommandSink,
        request: RenderAssetRequest,
    ) {
        if !self.requested_assets.insert(request.asset_id) {
            return;
        }

        let location = frame
            .snapshot
            .transforms
            .find(request.entity)
            .map(|transform| transform.translation_meters)
            .unwrap_or(Vec3::ZERO);
        out.event(WorldEvent {
            event_id: deterministic_event_id(frame.sim_time.tick, 10, request.asset_id),
            tick: frame.sim_time.tick,
            location_meters: location,
            actors: vec![request.entity],
            kind: WorldEventKind::AssetStreamingRequested {
                asset_id: request.asset_id,
                requester: 10,
                requested_quality: if frame.quality_tier < QualityTier::NormalRuntime {
                    request.requested_quality.min(frame.quality_tier)
                } else {
                    request.requested_quality
                },
                priority: request.priority,
                reason: request.reason.to_string(),
            },
            physical_evidence: vec!["render_dependency".to_string()],
            narrative_tags: vec!["asset_streaming".to_string(), "rendering".to_string()],
        });
    }
}

struct RenderAssetRequest {
    entity: EntityId,
    asset_id: AssetId,
    requested_quality: QualityTier,
    priority: AssetPriority,
    reason: &'static str,
}

fn render_gpu_resource(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    kind: GpuResourceKind,
    byte_len: u64,
    lifetime: GpuResourceLifetime,
    bindless: bool,
) -> GpuResourceHandle {
    let mut desc = GpuResourceDesc::new(label, kind, byte_len)
        .owned_by(10)
        .with_lifetime(lifetime);
    if bindless {
        desc = desc.bindless();
    }
    graph.declare_resource(desc)
}

fn render_gpu_resource_with_residency(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    kind: GpuResourceKind,
    byte_len: u64,
    lifetime: GpuResourceLifetime,
    residency_policy: GpuResourceResidencyPolicy,
    bindless: bool,
) -> GpuResourceHandle {
    let mut desc = GpuResourceDesc::new(label, kind, byte_len)
        .owned_by(10)
        .with_lifetime(lifetime)
        .with_residency_policy(residency_policy);
    if bindless {
        desc = desc.bindless();
    }
    graph.declare_resource(desc)
}

fn render_graphics_pipeline(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    shader_key: &'static str,
    quality_tier: QualityTier,
) -> ashfall_core::gpu::RenderPipelineHandle {
    render_graphics_pipeline_with_permutation(
        graph,
        label,
        shader_key,
        quality_tier,
        GpuShaderPermutation::default(),
    )
}

fn render_graphics_pipeline_with_permutation(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    shader_key: &'static str,
    quality_tier: QualityTier,
    permutation: GpuShaderPermutation,
) -> ashfall_core::gpu::RenderPipelineHandle {
    graph
        .create_render_pipeline(render_pipeline_desc(
            label,
            shader_key,
            quality_tier,
            permutation,
        ))
        .handle
}

fn render_compute_pipeline(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    shader_key: &'static str,
    quality_tier: QualityTier,
) -> ComputePipelineHandle {
    render_compute_pipeline_with_permutation(
        graph,
        label,
        shader_key,
        quality_tier,
        GpuShaderPermutation::default(),
    )
}

fn render_compute_pipeline_with_permutation(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    shader_key: &'static str,
    quality_tier: QualityTier,
    permutation: GpuShaderPermutation,
) -> ComputePipelineHandle {
    graph
        .create_compute_pipeline(render_pipeline_desc(
            label,
            shader_key,
            quality_tier,
            permutation,
        ))
        .handle
}

fn render_ray_pipeline_with_permutation(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    shader_key: &'static str,
    quality_tier: QualityTier,
    permutation: GpuShaderPermutation,
) -> ashfall_core::gpu::RayPipelineHandle {
    graph
        .create_ray_pipeline(render_pipeline_desc(
            label,
            shader_key,
            quality_tier,
            permutation,
        ))
        .handle
}

fn render_pipeline_desc(
    label: &'static str,
    shader_key: &'static str,
    quality_tier: QualityTier,
    permutation: GpuShaderPermutation,
) -> GpuPipelineDesc {
    let mut desc =
        GpuPipelineDesc::new(label, shader_key, quality_tier).with_permutation(permutation);
    desc.shader_contract.artifact = GpuShaderArtifactKind::SpirV;
    desc
}

fn render_shader_permutation(
    quality: &RenderQualityProfile,
    features: impl IntoIterator<Item = &'static str>,
) -> GpuShaderPermutation {
    let mut defines = features.into_iter().map(str::to_string).collect::<Vec<_>>();
    defines.push(quality_define(quality.quality_tier).to_string());
    defines.push(
        if quality.ray_traced_reflections {
            "RAY_TRACED_REFLECTIONS"
        } else {
            "SCREEN_SPACE_REFLECTIONS"
        }
        .to_string(),
    );
    defines.push(
        if quality.ray_traced_shadows {
            "RAY_TRACED_SHADOWS"
        } else {
            "RASTER_SHADOWS"
        }
        .to_string(),
    );
    if quality.temporal_reconstruction {
        defines.push("TEMPORAL_RECONSTRUCTION".to_string());
    }
    GpuShaderPermutation::new(defines)
}

fn reference_mode(quality: &RenderQualityProfile) -> bool {
    quality.capability_tier == RenderCapabilityTier::ReferencePathTracing
        || quality.quality_tier == QualityTier::ReferenceOfflineValidation
}

fn reference_sample_budget(quality: &RenderQualityProfile) -> u32 {
    if reference_mode(quality) { 512 } else { 0 }
}

fn reference_path_tracing_features(ray_tracing_supported: bool) -> Vec<&'static str> {
    let mut features = vec![
        "REFERENCE_PATH_TRACING",
        "OFFLINE_VALIDATION",
        "PROGRESSIVE_ACCUMULATION",
    ];
    if ray_tracing_supported {
        features.push("HARDWARE_RAY_TRACING_REFERENCE");
    } else {
        features.push("COMPUTE_REFERENCE_FALLBACK");
    }
    features
}

fn quality_define(quality: QualityTier) -> &'static str {
    match quality {
        QualityTier::Disabled => "QUALITY_DISABLED",
        QualityTier::BackgroundApproximation => "QUALITY_BACKGROUND",
        QualityTier::NormalRuntime => "QUALITY_NORMAL",
        QualityTier::HeroHighFidelityRuntime => "QUALITY_HERO",
        QualityTier::ReferenceOfflineValidation => "QUALITY_REFERENCE",
    }
}

fn main_lighting_features(packet: Option<&RenderPacket>) -> Vec<&'static str> {
    let mut features = vec!["PBR_MATERIALS", "BINDLESS_MATERIALS", "NEON_LIGHTS"];
    let Some(packet) = packet else {
        features.push("HUMAN_RENDERING_COMPOSITE");
        return features;
    };

    if !packet.humans.is_empty() {
        features.push("HUMAN_RENDERING_COMPOSITE");
    }
    if !packet.material_programs.is_empty() {
        features.push("PROCEDURAL_MATERIAL_PROGRAMS");
    }
    if material_cache_binding_count(packet) > 0 {
        features.push("MATERIAL_CACHE_TEXTURES");
        features.push("CACHE_MISS_FALLBACK");
    }
    if packet
        .material_programs
        .iter()
        .any(|program| !program.required_state_channels.is_empty())
    {
        features.push("MATERIAL_STATE_CHANNELS");
    }
    if packet.material_programs.iter().any(|program| {
        program
            .required_state_channels
            .contains(&MaterialStateChannel::Wetness)
    }) {
        features.push("WETNESS_RESPONSE");
    }
    if packet.material_programs.iter().any(|program| {
        program
            .required_state_channels
            .contains(&MaterialStateChannel::Cracks)
    }) {
        features.push("DAMAGE_RESPONSE");
    }
    if packet
        .material_programs
        .iter()
        .any(|program| program.shader_model == MaterialProgramShaderModel::Subsurface)
    {
        features.push("SUBSURFACE_MATERIALS");
    }
    if fracture_interior_material_count(packet) > 0 {
        features.push("FRACTURE_INTERIOR_DISPLACEMENT");
    }
    if packet
        .material_programs
        .iter()
        .any(|program| program.displacement_policy == MaterialDisplacementPolicy::MicroDisplacement)
    {
        features.push("MATERIAL_MICRO_DISPLACEMENT");
    }
    if !packet.decals.is_empty() {
        features.push("PROCEDURAL_DECAL_PACKET");
        features.push("GENERATED_DECAL_ATLAS");
    }
    if !packet.world_cell_visibility.is_empty() {
        features.push("WORLD_CELL_VISIBILITY_PACKET");
    }
    if packet.weather.surface_wetness > 0.05 || packet.weather.precipitation_intensity > 0.05 {
        features.push("WEATHER_SURFACE_WETNESS");
    }
    if packet.weather.flood_depth_meters > 0.05 {
        features.push("WEATHER_FLOOD_REFLECTIONS");
    }
    if packet.weather.neon_haze > 0.05 {
        features.push("WEATHER_NEON_HAZE");
    }
    if packet
        .decals
        .iter()
        .any(|decal| decal.kind == DecalKind::CrackField)
    {
        features.push("DECAL_CRACK_FIELDS");
    }
    if packet
        .decals
        .iter()
        .any(|decal| decal.kind == DecalKind::WetnessFilm)
    {
        features.push("DECAL_WETNESS_FILMS");
    }
    if packet
        .decals
        .iter()
        .any(|decal| matches!(decal.kind, DecalKind::SootStain | DecalKind::CorrosionBloom))
    {
        features.push("DECAL_AGED_SURFACES");
    }
    features
}

fn decal_shader_features(packet: Option<&RenderPacket>) -> Vec<&'static str> {
    let mut features = vec!["GENERATED_DECAL_ATLAS", "MATERIAL_STATE_DECALS"];
    let Some(packet) = packet else {
        return features;
    };

    if event_backed_decal_count(packet) > 0 {
        features.push("EVENT_BACKED_DECALS");
    }
    if state_backed_decal_count(packet) > 0 {
        features.push("STATE_BACKED_DECALS");
    }
    if packet
        .decals
        .iter()
        .any(|decal| decal.quality_hint >= QualityTier::HeroHighFidelityRuntime)
    {
        features.push("HERO_DECAL_PAGES");
    }
    features
}

fn human_skin_shader_features(_packet: Option<&RenderPacket>) -> Vec<&'static str> {
    vec![
        "HUMAN_RUNTIME_BUNDLE_V4",
        "SKIN_SUBSURFACE_SCATTERING",
        "PORE_MICRODETAIL",
        "BLOOD_REDNESS_STATE",
        "SWEAT_OIL_WETNESS",
        "FACIAL_WRINKLE_MAPS",
    ]
}

fn human_eye_shader_features(_packet: Option<&RenderPacket>) -> Vec<&'static str> {
    vec![
        "HUMAN_RUNTIME_BUNDLE_V4",
        "EYE_CORNEA",
        "EYE_SCLERA",
        "IRIS_DEPTH",
        "TEARLINE_WET_MENISCUS",
    ]
}

fn human_hair_shader_features(_packet: Option<&RenderPacket>) -> Vec<&'static str> {
    vec![
        "HUMAN_RUNTIME_BUNDLE_V4",
        "HAIR_STRAND_CARD_HYBRID",
        "HAIR_WETNESS_RESPONSE",
        "HAIR_MOTION_RESPONSE",
    ]
}

fn human_clothing_shader_features(_packet: Option<&RenderPacket>) -> Vec<&'static str> {
    vec![
        "HUMAN_RUNTIME_BUNDLE_V4",
        "CLOTHING_MATERIAL_MOTION",
        "CLOTHING_WET_DIRT_DAMAGE",
        "CYBERNETIC_SEAM_MATERIAL",
    ]
}

fn human_composite_shader_features(_packet: Option<&RenderPacket>) -> Vec<&'static str> {
    vec!["HUMAN_RENDERING_COMPOSITE", "CLOSEUP_QUALITY"]
}

fn volume_scale_define(quality: &RenderQualityProfile) -> &'static str {
    if quality.volumetric_resolution_scale >= 1.5 {
        "VOLUME_SCALE_REFERENCE"
    } else if quality.volumetric_resolution_scale >= 1.0 {
        "VOLUME_SCALE_FULL"
    } else if quality.volumetric_resolution_scale >= 0.5 {
        "VOLUME_SCALE_HALF"
    } else {
        "VOLUME_SCALE_BACKGROUND"
    }
}

fn volume_shader_features(
    packet: Option<&RenderPacket>,
    quality: &RenderQualityProfile,
) -> Vec<&'static str> {
    let mut features = vec![
        "VOLUMETRICS",
        "TRANSPARENCY_COMPOSITE",
        volume_scale_define(quality),
    ];
    let Some(packet) = packet else {
        return features;
    };

    if packet
        .volumes
        .iter()
        .any(|volume| volume.medium == VolumeMedium::Steam)
    {
        features.push("VOLUME_STEAM");
    }
    if packet
        .volumes
        .iter()
        .any(|volume| volume.medium == VolumeMedium::ToxicGas)
    {
        features.push("VOLUME_TOXIC_GAS");
    }
    if packet
        .volumes
        .iter()
        .any(|volume| volume.medium == VolumeMedium::Smoke)
    {
        features.push("VOLUME_SMOKE");
    }
    if max_volume_hazard(packet) >= 0.7 {
        features.push("VOLUME_HAZARD_GAMEPLAY");
    }
    if max_volume_visibility(packet) >= 0.65 {
        features.push("VOLUME_VISIBILITY_OCCLUSION");
    }
    if packet.weather.toxic_gas_density > 0.05 {
        features.push("WEATHER_TOXIC_GAS");
    }
    if packet.weather.fog_density > 0.05 {
        features.push("WEATHER_FOG");
    }
    if packet.weather.dust_density > 0.05 {
        features.push("WEATHER_DUST");
    }
    if !packet.world_cell_visibility.is_empty() {
        features.push("WORLD_CELL_VISIBILITY_PACKET");
    }

    features
}

fn validation_issue(
    severity: RenderValidationSeverity,
    code: &'static str,
    message: &'static str,
) -> RenderValidationIssue {
    RenderValidationIssue {
        severity,
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn has_tag(tags: &[String], expected: &str) -> bool {
    tags.iter().any(|tag| tag == expected)
}

fn volume_medium_from_event(event: &WorldEvent) -> VolumeMedium {
    if let Some(kind) = evidence_value(&event.physical_evidence, "gas_kind") {
        return match kind {
            "steam" => VolumeMedium::Steam,
            "toxic" | "toxic_gas" => VolumeMedium::ToxicGas,
            "smoke" => VolumeMedium::Smoke,
            _ => VolumeMedium::Unknown,
        };
    }

    if has_tag(&event.narrative_tags, "steam") {
        VolumeMedium::Steam
    } else if has_tag(&event.narrative_tags, "toxic") {
        VolumeMedium::ToxicGas
    } else if has_tag(&event.narrative_tags, "smoke") {
        VolumeMedium::Smoke
    } else {
        VolumeMedium::Unknown
    }
}

fn default_volume_density(medium: VolumeMedium) -> f32 {
    match medium {
        VolumeMedium::Steam => 0.45,
        VolumeMedium::ToxicGas => 0.9,
        VolumeMedium::Smoke => 0.68,
        VolumeMedium::Unknown => 0.55,
    }
}

fn default_volume_visibility(medium: VolumeMedium) -> f32 {
    match medium {
        VolumeMedium::Steam => 0.45,
        VolumeMedium::ToxicGas => 0.72,
        VolumeMedium::Smoke => 0.64,
        VolumeMedium::Unknown => 0.5,
    }
}

fn default_volume_hazard(medium: VolumeMedium) -> f32 {
    match medium {
        VolumeMedium::Steam => 0.28,
        VolumeMedium::ToxicGas => 0.86,
        VolumeMedium::Smoke => 0.45,
        VolumeMedium::Unknown => 0.35,
    }
}

fn default_volume_radius(medium: VolumeMedium) -> f32 {
    match medium {
        VolumeMedium::Steam => 2.0,
        VolumeMedium::ToxicGas => 2.4,
        VolumeMedium::Smoke => 2.2,
        VolumeMedium::Unknown => 2.0,
    }
}

fn default_volume_height(medium: VolumeMedium) -> f32 {
    match medium {
        VolumeMedium::Steam => 3.0,
        VolumeMedium::ToxicGas => 3.2,
        VolumeMedium::Smoke => 3.4,
        VolumeMedium::Unknown => 3.0,
    }
}

fn volume_quality_hint(
    medium: VolumeMedium,
    hazard_level: f32,
    visibility_blocking: f32,
) -> QualityTier {
    if matches!(medium, VolumeMedium::ToxicGas) || hazard_level >= 0.7 || visibility_blocking >= 0.7
    {
        QualityTier::HeroHighFidelityRuntime
    } else if visibility_blocking >= 0.35 {
        QualityTier::NormalRuntime
    } else {
        QualityTier::BackgroundApproximation
    }
}

fn volume_scattering(
    medium: VolumeMedium,
    density: f32,
    visibility_blocking: f32,
    hazard_level: f32,
) -> f32 {
    let medium_base: f32 = match medium {
        VolumeMedium::Steam => 0.68,
        VolumeMedium::ToxicGas => 0.58,
        VolumeMedium::Smoke => 0.74,
        VolumeMedium::Unknown => 0.62,
    };
    (medium_base + density * 0.18 + visibility_blocking * 0.12 - hazard_level * 0.04)
        .clamp(0.1, 1.0)
}

fn evidence_f32(evidence: &[String], key: &str) -> Option<f32> {
    evidence_value(evidence, key).and_then(|value| value.parse().ok())
}

fn evidence_u64(evidence: &[String], key: &str) -> Option<u64> {
    evidence_value(evidence, key).and_then(|value| value.parse().ok())
}

fn evidence_value<'a>(evidence: &'a [String], key: &str) -> Option<&'a str> {
    evidence.iter().find_map(|entry| {
        entry
            .split_once(':')
            .and_then(|(entry_key, value)| (entry_key == key).then_some(value))
    })
}

fn deterministic_event_id(tick: u64, module: u64, local: AssetId) -> WorldEventId {
    ((tick as u128) << 80) | ((module as u128) << 64) | (local & u64::MAX as u128)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ashfall_core::runtime::FrameContext;
    use ashfall_core::world::{
        CityCellRuntimeRef, CommandSink, ComponentView, EntityTemplate, HumanState, PhysicalBody,
        PopulationSummary, WorldState,
    };

    fn render_world() -> WorldState {
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
                name: "broken glass".to_string(),
                transform: Transform::at(Vec3::new(2.0, 0.0, 0.0)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(2_001),
                    material: 1,
                    visible: true,
                    fracture_replacement_mesh: None,
                }),
                physical_body: Some(PhysicalBody {
                    mass_kg: 120.0,
                    material_id: 1,
                    dynamic: false,
                    fragile: true,
                }),
                material_state: Some(MaterialState {
                    crack_density: 0.85,
                    ..MaterialState::default()
                }),
                human: None,
                agent: None,
                tags: vec!["glass".to_string(), "destructible".to_string()],
            },
            EntityTemplate {
                entity_id: Some(3),
                name: "Mara".to_string(),
                transform: Transform::at(Vec3::new(5.0, 0.0, 0.0)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(3_000),
                    material: 4,
                    visible: true,
                    fracture_replacement_mesh: None,
                }),
                physical_body: Some(PhysicalBody {
                    mass_kg: 68.0,
                    material_id: 4,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: Some(MaterialState {
                    moisture: 0.62,
                    crack_density: 0.22,
                    soot: 0.1,
                    ..MaterialState::default()
                }),
                human: Some(HumanState {
                    human_id: 300,
                    quality_tier: QualityTier::HeroHighFidelityRuntime,
                }),
                agent: None,
                tags: vec!["npc".to_string()],
            },
            EntityTemplate {
                entity_id: Some(4),
                name: "neon".to_string(),
                transform: Transform::at(Vec3::new(1.0, 3.0, 2.0)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(4_000),
                    material: 3,
                    visible: true,
                    fracture_replacement_mesh: None,
                }),
                physical_body: None,
                material_state: Some(MaterialState {
                    electrical_charge: 120.0,
                    ..MaterialState::default()
                }),
                human: None,
                agent: None,
                tags: vec!["light".to_string(), "neon".to_string()],
            },
            EntityTemplate {
                entity_id: Some(6),
                name: "wet asphalt".to_string(),
                transform: Transform::at(Vec3::new(-0.8, 0.0, -0.02)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(6_000),
                    material: 2,
                    visible: true,
                    fracture_replacement_mesh: None,
                }),
                physical_body: Some(PhysicalBody {
                    mass_kg: 10_000.0,
                    material_id: 2,
                    dynamic: false,
                    fragile: false,
                }),
                material_state: Some(MaterialState {
                    moisture: 0.95,
                    ..MaterialState::default()
                }),
                human: None,
                agent: None,
                tags: vec!["asphalt".to_string(), "wettable".to_string()],
            },
            EntityTemplate {
                entity_id: Some(8),
                name: "dented steel panel".to_string(),
                transform: Transform::at(Vec3::new(2.4, 0.4, 0.0)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(8_000),
                    material: 80,
                    visible: true,
                    fracture_replacement_mesh: None,
                }),
                physical_body: Some(PhysicalBody {
                    mass_kg: 40.0,
                    material_id: 80,
                    dynamic: false,
                    fragile: false,
                }),
                material_state: Some(MaterialState {
                    plastic_strain: 0.32,
                    soot: 0.22,
                    corrosion: 0.35,
                    temperature: 455.0,
                    ..MaterialState::default()
                }),
                human: None,
                agent: None,
                tags: vec!["metal".to_string(), "ductile".to_string()],
            },
            EntityTemplate {
                entity_id: Some(10),
                name: "service cable".to_string(),
                transform: Transform::at(Vec3::new(0.5, -0.2, 0.0)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(10_000),
                    material: 10,
                    visible: true,
                    fracture_replacement_mesh: None,
                }),
                physical_body: Some(PhysicalBody {
                    mass_kg: 6.0,
                    material_id: 10,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: Some(MaterialState::default()),
                human: None,
                agent: None,
                tags: vec!["cable".to_string(), "cloth".to_string()],
            },
        ] {
            world.spawn_entity_template(template).expect("spawn");
        }
        world
    }

    fn test_city_cells() -> ComponentView<CityCellRuntimeRef> {
        ComponentView::new(vec![
            (
                65_536_000,
                CityCellRuntimeRef {
                    cell_id: 65_536_000,
                    district_id: 900,
                    population_summary: PopulationSummary {
                        expected_active_npcs: 8,
                        expected_background_crowd: 42,
                        expected_vehicle_or_transit_count: 5,
                        crowd_spawn_rule_count: 3,
                        traffic_rule_count: 2,
                        simulated_crowd_rule_count: 2,
                        summary_only_crowd_rule_count: 1,
                        average_crowd_density: 0.72,
                        average_traffic_density: 0.48,
                        faction_control_count: 2,
                        alertness: 0.64,
                        commerce_activity: 0.36,
                        observation_coverage: 0.7,
                        ..PopulationSummary::default()
                    },
                    quality_tier: QualityTier::HeroHighFidelityRuntime,
                    loaded: true,
                    resident_asset_count: 18,
                    active_story_thread_count: 1,
                },
            ),
            (
                65_667_074,
                CityCellRuntimeRef {
                    cell_id: 65_667_074,
                    district_id: 901,
                    population_summary: PopulationSummary {
                        expected_active_npcs: 2,
                        expected_background_crowd: 18,
                        expected_vehicle_or_transit_count: 2,
                        crowd_spawn_rule_count: 2,
                        traffic_rule_count: 1,
                        simulated_crowd_rule_count: 1,
                        summary_only_crowd_rule_count: 1,
                        average_crowd_density: 0.31,
                        average_traffic_density: 0.22,
                        faction_control_count: 1,
                        alertness: 0.28,
                        commerce_activity: 0.52,
                        observation_coverage: 0.34,
                        ..PopulationSummary::default()
                    },
                    quality_tier: QualityTier::BackgroundApproximation,
                    loaded: true,
                    resident_asset_count: 7,
                    active_story_thread_count: 0,
                },
            ),
        ])
    }

    #[test]
    fn packet_tracks_materials_humans_lights_and_surface_effects() {
        let world = render_world();
        let mut snapshot = world.snapshot(1, SimTime::new(1.0 / 60.0, 1));
        snapshot.city_cells = test_city_cells();
        let packet = build_render_packet(&snapshot);

        assert_eq!(packet.cameras.len(), 1);
        assert_eq!(packet.lights.len(), 1);
        assert_eq!(packet.debug_request, packet.debug_views);
        for expected_view in [
            DebugRenderView::MaterialState,
            DebugRenderView::Lighting,
            DebugRenderView::Overdraw,
            DebugRenderView::Lod,
            DebugRenderView::GpuCost,
            DebugRenderView::Visibility,
            DebugRenderView::HumanRendering,
        ] {
            assert!(
                packet.debug_views.contains(&expected_view),
                "{expected_view:?} debug view should be enabled"
            );
        }
        assert!(packet.visible_entities.contains(&2));
        assert!(packet.cracked_material_entities.contains(&2));
        assert!(packet.wet_material_entities.contains(&6));
        assert!(packet.deformed_material_entities.contains(&8));
        assert!(packet.virtual_shadow_pages.iter().any(|page| {
            page.light_entity == Some(4)
                && page.reason == ShadowPageInvalidationReason::ShadowCastingLight
                && !page.dirty
        }));
        assert!(packet.virtual_shadow_pages.iter().any(|page| {
            page.caster_entity == Some(2)
                && page.reason == ShadowPageInvalidationReason::DestructibleGeometry
                && page.dirty
                && page.priority == QualityTier::HeroHighFidelityRuntime
        }));
        assert!(packet.virtual_shadow_pages.iter().any(|page| {
            page.caster_entity == Some(3)
                && page.reason == ShadowPageInvalidationReason::AnimatedHuman
                && page.dirty
                && page.estimated_page_count >= 4
        }));
        assert!(packet.virtual_shadow_pages.iter().any(|page| {
            page.caster_entity == Some(8)
                && page.reason == ShadowPageInvalidationReason::Deformation
                && page.contact_sharpness > 0.5
        }));
        assert!(
            packet
                .humans
                .iter()
                .any(|human| { human.entity == 3 && human.lod == HumanRenderLod::HeroFace })
        );
        assert!(packet.humans.iter().any(|human| {
            human.entity == 3
                && human.skin_wetness > 0.4
                && human.injury_overlay > 0.1
                && human.hair_wetness > 0.4
                && human.clothing_wetness > 0.4
                && human.subsurface_strength > 0.4
                && human.pore_microdetail_strength > 0.6
                && human.eye_wetness > 0.5
                && human.tearline_strength > 0.5
                && human.hair_lod == HumanHairRenderLod::Strand
                && human.hair_motion_response > 0.5
                && human.clothing_motion_response > 0.4
                && human.cybernetic_surface_ratio > 0.0
                && human.closeup_quality_score > 0.85
        }));
        assert!(
            packet
                .material_records
                .iter()
                .any(|material| { material.entity == 2 && material.transparency > 0.1 })
        );
        assert!(
            packet
                .material_records
                .iter()
                .any(|material| { material.entity == 6 && material.roughness < 0.4 })
        );
        assert!(
            packet
                .material_state_records
                .iter()
                .any(|state| { state.entity == 8 && state.plastic_strain > 0.3 })
        );
        assert!(packet.material_state_records.iter().any(|state| {
            state.entity == 8 && state.soot > 0.2 && state.corrosion > 0.3 && state.heat > 0.12
        }));
        assert!(packet.decals.iter().any(|decal| {
            decal.entity == 2
                && decal.kind == DecalKind::CrackField
                && decal.generated_from_material_state
                && decal.intensity > 0.8
                && decal.state_channels.contains(&MaterialStateChannel::Cracks)
        }));
        assert!(packet.decals.iter().any(|decal| {
            decal.entity == 6
                && decal.kind == DecalKind::WetnessFilm
                && decal.intensity > 0.9
                && decal.quality_hint == QualityTier::HeroHighFidelityRuntime
                && decal
                    .state_channels
                    .contains(&MaterialStateChannel::Wetness)
        }));
        assert!(packet.decals.iter().any(|decal| {
            decal.entity == 8
                && decal.kind == DecalKind::CorrosionBloom
                && decal
                    .state_channels
                    .contains(&MaterialStateChannel::Corrosion)
        }));
        assert!(packet.decals.iter().any(|decal| {
            decal.entity == 8
                && decal.kind == DecalKind::PlasticDeformation
                && decal
                    .state_channels
                    .contains(&MaterialStateChannel::PlasticStrain)
        }));
        assert!(decal_atlas_page_count(&packet) >= 4);
        assert!(packet.weather.surface_wetness > 0.25);
        assert!(packet.weather.neon_haze > 0.1);
        assert_eq!(packet.weather.condition, WeatherCondition::Clear);
        assert_eq!(packet.world_cell_visibility.len(), 2);
        assert!(packet.world_cell_visibility.iter().any(|cell| {
            cell.cell_id == 65_536_000
                && cell.render_state == WorldCellRenderState::Hero
                && cell.streaming_priority == AssetPriority::Hero
                && cell.visible_entity_count > 0
                && cell.visibility_weight > 0.8
        }));
        assert!(packet.world_cell_visibility.iter().any(|cell| {
            cell.cell_id == 65_667_074
                && cell.render_state == WorldCellRenderState::Background
                && cell.population_activity > 0.0
                && cell.atmospheric_density > 0.0
        }));
        assert!(packet.material_programs.iter().any(|program| {
            program.material_id == 1
                && program
                    .required_state_channels
                    .contains(&MaterialStateChannel::Cracks)
                && program.displacement_policy == MaterialDisplacementPolicy::FractureInterior
                && program
                    .cache_textures
                    .iter()
                    .any(|texture| texture.0 == 30_001)
                && program.fallback_available
        }));
        assert!(packet.material_programs.iter().any(|program| {
            program.material_id == 2
                && program
                    .required_state_channels
                    .contains(&MaterialStateChannel::Wetness)
                && program
                    .cache_textures
                    .iter()
                    .any(|texture| texture.0 == 30_002)
                && program.cache_policy.update_frequency
                    == MaterialCacheUpdateFrequency::OnStateChange
        }));
        assert!(packet.material_programs.iter().any(|program| {
            program.material_id == 4
                && program.shader_model == MaterialProgramShaderModel::Subsurface
                && program
                    .cache_textures
                    .iter()
                    .any(|texture| texture.0 == 30_003)
                && program.cache_policy.update_frequency
                    == MaterialCacheUpdateFrequency::PerFrameHero
        }));
        assert!(packet.material_programs.iter().any(|program| {
            program.material_id == 80
                && program
                    .required_state_channels
                    .contains(&MaterialStateChannel::Corrosion)
                && program
                    .required_state_channels
                    .contains(&MaterialStateChannel::Soot)
                && program
                    .required_state_channels
                    .contains(&MaterialStateChannel::Heat)
                && program.cache_textures.contains(&TEXTURE_AGED_SURFACE_CACHE)
                && program.displacement_policy == MaterialDisplacementPolicy::MicroDisplacement
        }));
        assert!(
            packet
                .material_programs
                .iter()
                .map(|program| program.deterministic_key)
                .collect::<BTreeSet<_>>()
                .len()
                == packet.material_programs.len()
        );

        let output = render_output_from_packet(&packet);
        assert_eq!(
            output.draw_plan.visible_instance_count,
            packet.visible_entities.len()
        );
        assert!(output.draw_plan.selected_cluster_count > packet.visible_entities.len() as u32);
        assert_eq!(output.draw_plan.cpu_draw_call_count, 1);
        assert!(output.draw_plan.indirect_draw_count <= packet.visible_entities.len());
        assert!(output.draw_plan.batches.iter().any(|batch| {
            batch.material == 1
                && batch.transparent
                && batch.cluster_count > 0
                && batch.lod == RenderLodTier::Hero
        }));
        assert_eq!(output.picking.len(), output.visibility_records.len());
        assert!(output.picking.iter().any(|pick| {
            pick.entity == 2
                && pick.material == 1
                && pick.editor_selectable
                && pick.screen_position[0] >= 0.0
                && pick.screen_position[0] <= 1.0
        }));
        assert!(output.reference_comparison.is_none());
        assert!(output.capture_frames.iter().any(|capture| {
            capture.kind == RenderCaptureKind::Screenshot
                && capture.frame_id == output.frame_id
                && capture.depth == output.depth_buffer
                && capture.motion_vectors == output.motion_vectors
        }));
        assert!(output.capture_frames.iter().any(|capture| {
            capture.kind == RenderCaptureKind::CinematicFrame
                && capture.story_entity_count >= packet.humans.len()
        }));
        let perception = output
            .optional_perception_buffers
            .as_ref()
            .expect("visible scene should produce perception buffers");
        assert_eq!(
            perception.visible_entity_count,
            packet.visible_entities.len()
        );
        assert!(perception.confidence > 0.5);
    }

    #[test]
    fn debug_data_covers_required_renderer_views() {
        let world = render_world();
        let mut snapshot = world.snapshot(3, SimTime::new(3.0 / 60.0, 3));
        snapshot.city_cells = test_city_cells();
        let packet = build_render_packet(&snapshot);
        let output = render_output_from_packet(&packet);

        for expected_view in [
            DebugRenderView::MaterialState,
            DebugRenderView::Lighting,
            DebugRenderView::Overdraw,
            DebugRenderView::Lod,
            DebugRenderView::GpuCost,
            DebugRenderView::Visibility,
            DebugRenderView::Luminance,
            DebugRenderView::Exposure,
            DebugRenderView::Normals,
            DebugRenderView::Roughness,
            DebugRenderView::MaterialIds,
            DebugRenderView::GlobalIllumination,
            DebugRenderView::Shadows,
            DebugRenderView::Velocity,
            DebugRenderView::Depth,
            DebugRenderView::HumanRendering,
        ] {
            assert!(
                output
                    .debug_data
                    .iter()
                    .any(|entry| entry.view == expected_view),
                "{expected_view:?} debug data should be present"
            );
        }
        let camera = packet
            .cameras
            .first()
            .expect("packet should include a physical camera");
        assert!(camera.focal_length_mm > 0.0);
        assert!(camera.sensor_width_mm > 0.0);
        assert!(camera.sensor_height_mm > 0.0);
        assert!(camera.aperture_f_stop > 0.0);
        assert!(camera.shutter_seconds > 0.0);
        assert!(camera.iso > 0.0);
        assert!(camera.motion_blur_enabled);
        assert!(camera.depth_of_field_enabled);
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Luminance
                && entry.label == "linear_hdr_until_display"
                && entry.value == "true"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Luminance
                && entry.label == "hdr_histogram"
                && entry.value == "camera_exposure_metering"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Exposure
                && entry.label == "focal_length_mm"
                && entry.value.parse::<f32>().is_ok_and(|value| value > 0.0)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Exposure
                && entry.label == "aperture_f_stop"
                && entry.value.parse::<f32>().is_ok_and(|value| value > 0.0)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Exposure
                && entry.label == "iso"
                && entry.value.parse::<f32>().is_ok_and(|value| value >= 100.0)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Velocity
                && entry.label == "debug_view"
                && entry.value == "motion_vectors"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Visibility
                && entry.label == "picking_records"
                && entry.value == output.picking.len().to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Visibility
                && entry.label == "perception_candidate_entities"
                && entry.value == packet.visible_entities.len().to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::GpuCost
                && entry.label == "capture_outputs"
                && entry.value == output.capture_frames.len().to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::GpuCost
                && entry.label == "reference_comparison_images"
                && entry.value == output.reference_comparison.is_some().to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::GpuCost
                && entry.label == "perception_buffers"
                && entry
                    .value
                    .parse::<bool>()
                    .is_ok_and(|value| value == output.optional_perception_buffers.is_some())
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Depth
                && entry.label == "debug_view"
                && entry.value == "depth_prepass"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "dynamic_lights"
                && entry.value == packet.lights.len().to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "shadow_casting_lights"
                && entry.value == "1"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "virtual_shadow_requests"
                && entry.value == packet.virtual_shadow_pages.len().to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "shadow_page_estimate"
                && entry
                    .value
                    .parse::<u32>()
                    .is_ok_and(|pages| pages >= estimated_shadow_page_count(&packet))
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "shadow_invalidation_reasons"
                && entry.value.contains("animated_human")
                && entry.value.contains("destructible_geometry")
                && entry.value.contains("shadow_casting_light")
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "total_light_lumens"
                && entry.value.parse::<f32>().is_ok_and(|lumens| lumens > 0.0)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "weather_condition"
                && entry.value == packet.weather.condition.label()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "weather_surface_wetness"
                && entry
                    .value
                    .parse::<f32>()
                    .is_ok_and(|wetness| wetness > 0.25)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "weather_neon_haze"
                && entry.value.parse::<f32>().is_ok_and(|haze| haze > 0.1)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Visibility
                && entry.label == "world_cell_visibility_records"
                && entry.value == packet.world_cell_visibility.len().to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Visibility
                && entry.label == "world_cell_render_states"
                && entry.value.contains("hero:")
                && entry.value.contains("background:")
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::GpuCost
                && entry.label == "debug_request_views"
                && entry.value == packet.debug_request.len().to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "material_runtime_programs"
                && entry.value == packet.material_programs.len().to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "material_cache_bindings"
                && entry.value == material_cache_binding_count(&packet).to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "material_cache_textures"
                && entry.value.contains("aged_surface")
                && entry.value.contains("crack_detail")
                && entry.value.contains("wet_reflection")
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "aged_surface_materials"
                && entry.value.parse::<usize>().is_ok_and(|count| count >= 1)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "aged_surface_cache_bindings"
                && entry.value == aged_surface_cache_binding_count(&packet).to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "material_state_channels"
                && entry.value.contains("cracks")
                && entry.value.contains("wetness")
                && entry.value.contains("biological")
                && entry.value.contains("corrosion")
                && entry.value.contains("soot")
                && entry.value.contains("heat")
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "material_state_active_counts"
                && entry.value.contains("corrosion:")
                && entry.value.contains("soot:")
                && entry.value.contains("heat:")
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "material_state_maxima"
                && entry.value.contains("corrosion:0.35")
                && entry.value.contains("soot:0.22")
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "fracture_interior_materials"
                && entry.value == "1"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "decal_records"
                && entry.value == packet.decals.len().to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "decal_kinds"
                && entry.value.contains("crack_field")
                && entry.value.contains("wetness_film")
                && entry.value.contains("corrosion_bloom")
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "decal_atlas_pages"
                && entry.value == decal_atlas_page_count(&packet).to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "state_backed_decals"
                && entry
                    .value
                    .parse::<usize>()
                    .is_ok_and(|count| count >= packet.decals.len().saturating_sub(1))
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "max_decal_intensity"
                && entry.value.parse::<f32>().is_ok_and(|value| value > 0.8)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::HumanRendering
                && entry.label == "human_records"
                && entry.value == packet.humans.len().to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::HumanRendering
                && entry.label == "hero_face_humans"
                && entry.value == "1"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::HumanRendering
                && entry.label == "skin_subsurface_avg"
                && entry.value.parse::<f32>().is_ok_and(|value| value > 0.4)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::HumanRendering
                && entry.label == "eye_wetness_avg"
                && entry.value.parse::<f32>().is_ok_and(|value| value > 0.5)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::HumanRendering
                && entry.label == "cybernetic_surfaces"
                && entry.value == "1"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Overdraw
                && entry.label == "transparent_batches"
                && entry.value.parse::<usize>().is_ok()
        }));

        let lod_debug_total = ["lod_hero", "lod_near", "lod_mid", "lod_far", "lod_impostor"]
            .into_iter()
            .map(|label| {
                output
                    .debug_data
                    .iter()
                    .find(|entry| entry.view == DebugRenderView::Lod && entry.label == label)
                    .and_then(|entry| entry.value.parse::<usize>().ok())
                    .expect("LOD debug entry should be numeric")
            })
            .sum::<usize>();
        assert_eq!(lod_debug_total, packet.visible_entities.len());
    }

    #[test]
    fn frame_packet_adds_physics_driven_fluid_and_gas_inputs() {
        let world = render_world();
        let mut snapshot = world.snapshot(7, SimTime::new(7.0 / 60.0, 7));
        snapshot.city_cells = test_city_cells();
        let frame = FrameContext {
            frame_id: 7,
            sim_time: SimTime::new(7.0 / 60.0, 7),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot,
            recent_events: vec![
                WorldEvent {
                    event_id: 100,
                    tick: 7,
                    location_meters: Vec3::ZERO,
                    actors: vec![5, 6],
                    kind: WorldEventKind::StreetFlooded,
                    physical_evidence: vec!["water_flow".to_string()],
                    narrative_tags: vec!["hazard".to_string()],
                },
                WorldEvent {
                    event_id: 101,
                    tick: 7,
                    location_meters: Vec3::new(4.0, 0.0, 0.0),
                    actors: vec![2],
                    kind: WorldEventKind::ToxicGasReleased,
                    physical_evidence: vec![
                        "gas_density_field".to_string(),
                        "gas_kind:steam".to_string(),
                        "gas_density:0.45".to_string(),
                        "visibility_blocking:0.45".to_string(),
                        "hazard_level:0.28".to_string(),
                        "density_cells:343".to_string(),
                        "temperature_cells:125".to_string(),
                        "pressure_cells:125".to_string(),
                        "volume_radius_meters:2.00".to_string(),
                        "volume_height_meters:3.00".to_string(),
                    ],
                    narrative_tags: vec![
                        "hazard".to_string(),
                        "gas".to_string(),
                        "steam".to_string(),
                        "visibility_blocking".to_string(),
                    ],
                },
                WorldEvent {
                    event_id: 102,
                    tick: 7,
                    location_meters: Vec3::new(2.4, 0.4, 0.0),
                    actors: vec![8],
                    kind: WorldEventKind::MetalBent {
                        entity: 8,
                        plastic_strain: 0.32,
                    },
                    physical_evidence: vec!["plastic_strain".to_string()],
                    narrative_tags: vec!["metal".to_string()],
                },
                WorldEvent {
                    event_id: 103,
                    tick: 7,
                    location_meters: Vec3::new(0.5, 0.0, 0.0),
                    actors: vec![10],
                    kind: WorldEventKind::FlexibleConstraintResolved {
                        entity: 10,
                        correction_meters: 0.42,
                    },
                    physical_evidence: vec!["constraint_correction".to_string()],
                    narrative_tags: vec!["constraint".to_string(), "cloth".to_string()],
                },
                WorldEvent {
                    event_id: 104,
                    tick: 7,
                    location_meters: Vec3::new(5.0, 0.0, 0.0),
                    actors: vec![3],
                    kind: WorldEventKind::AgentIntentProposed {
                        agent: 3,
                        action: "Investigate".to_string(),
                        source_event: Some(102),
                        validation_passed: true,
                    },
                    physical_evidence: vec!["ai_intent_validation".to_string()],
                    narrative_tags: vec!["ai".to_string(), "intent".to_string()],
                },
                WorldEvent {
                    event_id: 105,
                    tick: 7,
                    location_meters: Vec3::new(-0.8, 0.0, -0.02),
                    actors: vec![6],
                    kind: WorldEventKind::MaterialStateChanged { entity: 6 },
                    physical_evidence: vec!["wetness_delta".to_string()],
                    narrative_tags: vec!["material".to_string(), "wet".to_string()],
                },
            ],
            forces: Vec::new(),
        };

        let packet = build_render_packet_for_frame(&frame);

        assert_eq!(packet.fluids.len(), 1);
        assert_eq!(packet.fluids[0].wetness_targets, vec![6]);
        assert!(packet.fluids[0].screen_space_reflection);
        assert_eq!(packet.volumes.len(), 1);
        let volume = &packet.volumes[0];
        assert_eq!(volume.source_event, 101);
        assert_eq!(volume.source_entities, vec![2]);
        assert_eq!(volume.medium, VolumeMedium::Steam);
        assert_eq!(volume.material_id, 1);
        assert_eq!(volume.quality_hint, QualityTier::NormalRuntime);
        assert_eq!(volume.density, 0.45);
        assert_eq!(volume.visibility_blocking, 0.45);
        assert_eq!(volume.hazard_level, 0.28);
        assert_eq!(volume.density_cell_count, 343);
        assert_eq!(volume.temperature_cell_count, 125);
        assert_eq!(volume.pressure_cell_count, 125);
        assert!(volume.scattering > 0.7);
        assert_eq!(packet.surface_effects.len(), 2);
        assert!(packet.surface_effects.iter().any(|effect| {
            effect.entity == 8
                && matches!(effect.kind, SurfaceEffectKind::MetalDent { plastic_strain } if plastic_strain > 0.3)
        }));
        assert!(packet.surface_effects.iter().any(|effect| {
            effect.entity == 10
                && matches!(
                    effect.kind,
                    SurfaceEffectKind::ConstraintCorrection { correction_meters } if correction_meters > 0.4
                )
        }));
        assert!(packet.decals.iter().any(|decal| {
            decal.entity == 6
                && decal.source_event == Some(100)
                && decal.kind == DecalKind::WetnessFilm
                && decal.intensity > 0.8
        }));
        assert!(packet.decals.iter().any(|decal| {
            decal.entity == 8
                && decal.source_event == Some(102)
                && decal.kind == DecalKind::PlasticDeformation
                && decal.quality_hint == QualityTier::HeroHighFidelityRuntime
        }));
        assert!(packet.decals.iter().any(|decal| {
            decal.entity == 10
                && decal.source_event == Some(103)
                && decal.kind == DecalKind::ConstraintStress
        }));
        assert!(event_backed_decal_count(&packet) >= 3);
        assert!(packet.constraint_debug_entities.contains(&10));
        assert_eq!(packet.ai_intent_overlays.len(), 1);
        assert_eq!(packet.weather.condition, WeatherCondition::MixedHazard);
        assert!(packet.weather.flood_depth_meters >= 0.08);
        assert!(packet.weather.toxic_gas_density >= 0.6);
        assert!(packet.weather.fog_density > 0.1);
        assert_eq!(packet.weather.source_event_count, 2);
        assert_eq!(packet.world_cell_visibility.len(), 2);
        assert!(
            packet
                .world_cell_visibility
                .iter()
                .any(|cell| cell.render_state == WorldCellRenderState::Hero)
        );

        let overlay = &packet.ai_intent_overlays[0];
        assert_eq!(overlay.event_id, 104);
        assert_eq!(overlay.agent, 3);
        assert_eq!(overlay.action, "Investigate");
        assert_eq!(overlay.source_event, Some(102));
        assert_eq!(overlay.position, Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(overlay.source_position, Some(Vec3::new(2.4, 0.4, 0.0)));
        assert!(overlay.validation_passed);
        assert_eq!(overlay.color_linear, [0.14, 0.72, 1.0, 1.0]);
        assert!(packet.virtual_shadow_pages.iter().any(|page| {
            page.caster_entity == Some(2)
                && page.source_event == Some(101)
                && page.reason == ShadowPageInvalidationReason::VolumeScattering
                && page.dirty
        }));
        assert!(packet.virtual_shadow_pages.iter().any(|page| {
            page.caster_entity == Some(6)
                && page.source_event == Some(105)
                && page.reason == ShadowPageInvalidationReason::MaterialStateChange
                && page.dirty
        }));
        assert!(packet.virtual_shadow_pages.iter().any(|page| {
            page.caster_entity == Some(8)
                && page.source_event == Some(102)
                && page.reason == ShadowPageInvalidationReason::Deformation
                && page.dirty
        }));

        let output = render_output_from_packet(&packet);
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::AiDecisions
                && entry.label == "ai_intent_overlays"
                && entry.value == "1"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::AiDecisions
                && entry.label == "ai_intent_actions"
                && entry.value == "Investigate"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::GpuCost
                && entry.label == "temporal_reconstruction"
                && entry.value == "true"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::GpuCost
                && entry.label == "indirect_draws"
                && entry.value == output.draw_plan.indirect_draw_count.to_string()
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "volume_regions"
                && entry.value == "1"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "volume_media"
                && entry.value == "steam"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "max_volume_visibility"
                && entry.value == "0.45"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "max_volume_hazard"
                && entry.value == "0.28"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "weather_condition"
                && entry.value == "mixed_hazard"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "weather_toxic_gas_density"
                && entry
                    .value
                    .parse::<f32>()
                    .is_ok_and(|density| density >= 0.6)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Visibility
                && entry.label == "world_cell_visibility_records"
                && entry.value == "2"
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "dirty_shadow_pages"
                && entry.value.parse::<usize>().is_ok_and(|dirty| dirty >= 4)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::MaterialState
                && entry.label == "event_backed_decals"
                && entry.value.parse::<usize>().is_ok_and(|count| count >= 3)
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Lighting
                && entry.label == "shadow_invalidation_reasons"
                && entry.value.contains("material_state_change")
                && entry.value.contains("volume_scattering")
                && entry.value.contains("deformation")
        }));
        assert!(output.debug_data.iter().any(|entry| {
            entry.view == DebugRenderView::Overdraw
                && entry.label == "gpu_selected_clusters"
                && entry.value == output.draw_plan.selected_cluster_count.to_string()
        }));

        let mut module = RenderingModule::default();
        let mut sink = CommandSink::default();
        module.tick(&frame, &mut sink);
        let services = ashfall_core::gpu::GpuServices::new_vulkan();
        let mut graph = services.begin_frame(7);
        module.schedule_gpu(&mut graph);
        let gpu_report = services.submit(graph);
        let main_lighting = gpu_report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "main_lighting")
            .expect("main lighting pipeline should be reported");
        for expected_define in [
            "PROCEDURAL_MATERIAL_PROGRAMS",
            "MATERIAL_CACHE_TEXTURES",
            "MATERIAL_STATE_CHANNELS",
            "WETNESS_RESPONSE",
            "DAMAGE_RESPONSE",
            "SUBSURFACE_MATERIALS",
            "FRACTURE_INTERIOR_DISPLACEMENT",
            "MATERIAL_MICRO_DISPLACEMENT",
            "PROCEDURAL_DECAL_PACKET",
            "GENERATED_DECAL_ATLAS",
            "DECAL_CRACK_FIELDS",
            "DECAL_WETNESS_FILMS",
            "DECAL_AGED_SURFACES",
            "WORLD_CELL_VISIBILITY_PACKET",
            "WEATHER_SURFACE_WETNESS",
            "WEATHER_FLOOD_REFLECTIONS",
            "WEATHER_NEON_HAZE",
        ] {
            assert!(
                main_lighting
                    .permutation_defines
                    .iter()
                    .any(|define| define == expected_define),
                "{expected_define} should be enabled for stateful procedural materials"
            );
        }
        assert!(gpu_report.registered_resource_usage.iter().any(|usage| {
            usage.label == "material runtime program table"
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "material_state_resolve")
                && usage.readers.iter().any(|reader| reader == "main_lighting")
                && usage.byte_len >= packet.material_programs.len() as u64 * 256
        }));
        assert!(gpu_report.registered_resource_usage.iter().any(|usage| {
            usage.label == "material cache residency table"
                && usage.readers.iter().any(|reader| reader == "main_lighting")
                && usage.byte_len >= material_cache_binding_count(&packet) as u64 * 64
        }));
        let decal_resolve = gpu_report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "decal_atlas_resolve")
            .expect("decal atlas resolve pipeline should be reported");
        for expected_define in [
            "GENERATED_DECAL_ATLAS",
            "MATERIAL_STATE_DECALS",
            "EVENT_BACKED_DECALS",
            "STATE_BACKED_DECALS",
            "HERO_DECAL_PAGES",
        ] {
            assert!(
                decal_resolve
                    .permutation_defines
                    .iter()
                    .any(|define| define == expected_define),
                "{expected_define} should be enabled for decal atlas resolve"
            );
        }
        assert!(gpu_report.registered_resource_usage.iter().any(|usage| {
            usage.label == "decal packet buffer"
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "decal_atlas_resolve")
                && usage.readers.iter().any(|reader| reader == "main_lighting")
                && usage.byte_len >= packet.decals.len() as u64 * 192
        }));
        assert!(gpu_report.registered_resource_usage.iter().any(|usage| {
            usage.label == "generated decal atlas"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "decal_atlas_resolve")
                && usage.readers.iter().any(|reader| reader == "main_lighting")
                && usage.byte_len >= decal_atlas_page_count(&packet) as u64 * 512 * 1024
        }));
        assert!(gpu_report.registered_resource_usage.iter().any(|usage| {
            usage.label == "weather state buffer"
                && usage.readers.iter().any(|reader| reader == "main_lighting")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "volumetrics_and_transparency")
        }));
        assert!(gpu_report.registered_resource_usage.iter().any(|usage| {
            usage.label == "world cell visibility buffer"
                && usage.readers.iter().any(|reader| reader == "gpu_culling")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "render_streaming_feedback")
                && usage.readers.iter().any(|reader| reader == "main_lighting")
                && usage.byte_len >= packet.world_cell_visibility.len() as u64 * 128
        }));
        let volumetrics = gpu_report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "volumetrics_and_transparency")
            .expect("volumetrics pipeline should be reported");
        assert!(
            volumetrics
                .permutation_defines
                .iter()
                .any(|define| define == "VOLUME_STEAM")
        );
        assert!(
            volumetrics
                .permutation_defines
                .iter()
                .any(|define| define == "VOLUME_SCALE_HALF")
        );
        assert!(
            !volumetrics
                .permutation_defines
                .iter()
                .any(|define| define == "VOLUME_HAZARD_GAMEPLAY")
        );
        for expected_define in [
            "WEATHER_TOXIC_GAS",
            "WEATHER_FOG",
            "WORLD_CELL_VISIBILITY_PACKET",
        ] {
            assert!(
                volumetrics
                    .permutation_defines
                    .iter()
                    .any(|define| define == expected_define),
                "{expected_define} should be enabled for weather-aware volumetrics"
            );
        }
        assert!(gpu_report.registered_resource_usage.iter().any(|usage| {
            usage.label == "virtual shadow page table"
                && usage.writers.iter().any(|writer| writer == "shadow_maps")
                && usage.readers.iter().any(|reader| reader == "main_lighting")
                && usage.byte_len >= estimated_shadow_page_count(&packet) as u64 * 128
        }));
        assert!(gpu_report.registered_resource_usage.iter().any(|usage| {
            usage.label == "dirty shadow page list"
                && usage.readers.iter().any(|reader| reader == "shadow_maps")
                && usage.writers.is_empty()
        }));
        let picking_resolve = gpu_report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "editor_picking_resolve")
            .expect("editor picking pipeline should be reported");
        for expected_define in [
            "EDITOR_PICKING",
            "ENTITY_ID_OUTPUT",
            "MATERIAL_ID_OUTPUT",
            "DEPTH_AWARE_PICKING",
        ] {
            assert!(
                picking_resolve
                    .permutation_defines
                    .iter()
                    .any(|define| define == expected_define),
                "{expected_define} should be enabled for editor picking"
            );
        }
        let perception_resolve = gpu_report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "perception_buffer_resolve")
            .expect("perception buffer pipeline should be reported");
        for expected_define in [
            "AI_PERCEPTION_BUFFERS",
            "ENTITY_ID_OUTPUT",
            "MATERIAL_ID_OUTPUT",
            "HAZARD_VISIBILITY",
            "MOTION_AWARE",
        ] {
            assert!(
                perception_resolve
                    .permutation_defines
                    .iter()
                    .any(|define| define == expected_define),
                "{expected_define} should be enabled for AI/tool perception buffers"
            );
        }
        let frame_capture = gpu_report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "frame_capture_export")
            .expect("frame capture export pipeline should be reported");
        for expected_define in [
            "SCREENSHOT_CAPTURE",
            "CINEMATIC_FRAME_CAPTURE",
            "REPLAYABLE_FRAME_METADATA",
            "TOOLS_FRAME_CAPTURE",
        ] {
            assert!(
                frame_capture
                    .permutation_defines
                    .iter()
                    .any(|define| define == expected_define),
                "{expected_define} should be enabled for frame capture export"
            );
        }
        assert!(gpu_report.registered_resource_usage.iter().any(|usage| {
            usage.label == "editor picking buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "editor_picking_resolve")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "frame_capture_export")
        }));
        assert!(gpu_report.registered_resource_usage.iter().any(|usage| {
            usage.label == "editor picking readback buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "editor_picking_resolve")
        }));
        for label in [
            "perception entity id buffer",
            "perception material id buffer",
            "perception depth pyramid",
            "perception hazard buffer",
        ] {
            assert!(
                gpu_report.registered_resource_usage.iter().any(|usage| {
                    usage.label == label
                        && usage
                            .writers
                            .iter()
                            .any(|writer| writer == "perception_buffer_resolve")
                        && usage
                            .readers
                            .iter()
                            .any(|reader| reader == "frame_capture_export")
                }),
                "{label} should be produced for AI/tool perception and consumed by capture export"
            );
        }
        for label in [
            "frame capture manifest buffer",
            "screenshot capture target",
            "cinematic capture target",
        ] {
            assert!(
                gpu_report.registered_resource_usage.iter().any(|usage| {
                    usage.label == label
                        && usage
                            .writers
                            .iter()
                            .any(|writer| writer == "frame_capture_export")
                }),
                "{label} should be written by frame capture export"
            );
        }
    }

    #[test]
    fn output_degrades_quality_and_reports_budget_pressure() {
        let world = render_world();
        let snapshot = world.snapshot(2, SimTime::new(2.0 / 60.0, 2));
        let mut packet = build_render_packet(&snapshot);
        packet.quality_profile =
            RenderQualityProfile::for_quality(QualityTier::HeroHighFidelityRuntime);
        let tight_budget = RenderBudget {
            quality_tier: QualityTier::HeroHighFidelityRuntime,
            max_gpu_milliseconds: 0.5,
            max_visible_instances: 2,
            max_dynamic_lights: 1,
            max_volume_regions: 0,
            max_fluid_surfaces: 0,
            max_decals: 0,
            max_world_cell_visibility_records: 0,
            max_surface_effects: 0,
            max_memory_bytes: 4 * 1024 * 1024,
            allow_ray_tracing: true,
        };

        let output = render_output_with_budget(&packet, &tight_budget);

        assert_eq!(
            output.quality_profile.quality_tier,
            QualityTier::BackgroundApproximation
        );
        assert!(output.validation_report.passed);
        assert!(output.validation_report.issues.iter().any(|issue| {
            issue.code == "visible_instance_budget_exceeded"
                || issue.code == "render_memory_budget_exceeded"
        }));
    }

    #[test]
    fn module_requests_streaming_and_keeps_last_output() {
        let world = render_world();
        let snapshot = world.snapshot(1, SimTime::new(1.0 / 60.0, 1));
        let frame = FrameContext {
            frame_id: 1,
            sim_time: SimTime::new(1.0 / 60.0, 1),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot,
            recent_events: Vec::new(),
            forces: Vec::new(),
        };
        let mut module = RenderingModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        assert!(sink.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::AssetStreamingRequested {
                    asset_id: 2_001,
                    requester: 10,
                    ..
                }
            )
        }));
        assert!(module.last_packet().is_some());
        assert!(module.last_output().is_some_and(|output| {
            output.visibility.contains(&2)
                && output
                    .debug_data
                    .iter()
                    .any(|entry| entry.label == "estimated_gpu_ms")
                && output.draw_plan.cpu_draw_call_count == 1
                && output.draw_plan.indirect_draw_count > 0
        }));
    }

    #[test]
    fn baseline_vulkan_uses_screen_space_reflection_fallback() {
        let mut module = RenderingModule::default();
        let services = ashfall_core::gpu::GpuServices::new_vulkan();
        let mut graph = services.begin_frame(9);

        module.schedule_gpu(&mut graph);

        assert!(graph.passes().iter().any(|pass| {
            pass.name == "screen_space_reflections"
                && matches!(
                    pass.dispatch,
                    ashfall_core::gpu::GpuDispatchKind::Compute(_)
                )
        }));
        assert!(
            !graph
                .passes()
                .iter()
                .any(|pass| pass.name == "selective_ray_reflections")
        );
        let culling_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "gpu_culling")
            .expect("GPU culling should be scheduled");
        let indirect_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "indirect_draw_compaction")
            .expect("indirect draw compaction should be scheduled");
        let shadow_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "shadow_maps")
            .expect("shadow maps should be scheduled");
        let human_skin_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "human_skin_shading")
            .expect("human skin shading should be scheduled");
        let human_eye_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "human_eye_shading")
            .expect("human eye shading should be scheduled");
        let human_hair_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "human_hair_shading")
            .expect("human hair shading should be scheduled");
        let human_clothing_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "human_clothing_cybernetics")
            .expect("human clothing/cybernetics should be scheduled");
        let human_composite_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "human_closeup_composite")
            .expect("human close-up composite should be scheduled");
        let main_lighting_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "main_lighting")
            .expect("main lighting should be scheduled");
        let exposure_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "camera_exposure_metering")
            .expect("camera exposure metering should be scheduled");
        let lens_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "camera_lens_effects")
            .expect("camera lens effects should be scheduled");
        let post_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "post_process")
            .expect("post process should be scheduled");
        let editor_picking_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "editor_picking_resolve")
            .expect("editor picking resolve should be scheduled");
        let perception_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "perception_buffer_resolve")
            .expect("perception buffer resolve should be scheduled");
        let temporal_masks_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "temporal_stability_masks")
            .expect("temporal stability masks should be scheduled");
        let temporal_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "temporal_reconstruction")
            .expect("temporal reconstruction should be scheduled");
        let frame_capture_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "frame_capture_export")
            .expect("frame capture export should be scheduled");
        let ui_index = graph
            .passes()
            .iter()
            .position(|pass| pass.name == "ui_composition")
            .expect("UI composition should be scheduled");
        assert!(main_lighting_index < exposure_index);
        assert!(exposure_index < lens_index);
        assert!(lens_index < post_index);
        assert!(post_index < editor_picking_index);
        assert!(post_index < perception_index);
        assert!(post_index < temporal_masks_index);
        assert!(temporal_masks_index < temporal_index);
        assert!(temporal_index < frame_capture_index);
        assert!(frame_capture_index < ui_index);
        assert!(shadow_index < human_skin_index);
        assert!(human_skin_index < human_composite_index);
        assert!(human_eye_index < human_composite_index);
        assert!(human_hair_index < human_composite_index);
        assert!(human_clothing_index < human_composite_index);
        assert!(human_composite_index < main_lighting_index);
        assert!(matches!(
            graph.passes()[temporal_index].dispatch,
            ashfall_core::gpu::GpuDispatchKind::Compute(_)
        ));
        assert!(matches!(
            graph.passes()[temporal_masks_index].dispatch,
            ashfall_core::gpu::GpuDispatchKind::Compute(_)
        ));
        assert!(matches!(
            graph.passes()[editor_picking_index].dispatch,
            ashfall_core::gpu::GpuDispatchKind::Compute(_)
        ));
        assert!(matches!(
            graph.passes()[perception_index].dispatch,
            ashfall_core::gpu::GpuDispatchKind::Compute(_)
        ));
        assert!(matches!(
            graph.passes()[frame_capture_index].dispatch,
            ashfall_core::gpu::GpuDispatchKind::Compute(_)
        ));
        assert!(matches!(
            graph.passes()[exposure_index].dispatch,
            ashfall_core::gpu::GpuDispatchKind::Compute(_)
        ));
        assert!(matches!(
            graph.passes()[lens_index].dispatch,
            ashfall_core::gpu::GpuDispatchKind::Graphics(_)
        ));
        assert!(matches!(
            graph.passes()[human_clothing_index].dispatch,
            ashfall_core::gpu::GpuDispatchKind::Compute(_)
        ));
        assert!(matches!(
            graph.passes()[human_composite_index].dispatch,
            ashfall_core::gpu::GpuDispatchKind::Graphics(_)
        ));
        assert!(culling_index < indirect_index);
        assert!(indirect_index < main_lighting_index);
        assert!(matches!(
            graph.passes()[indirect_index].dispatch,
            ashfall_core::gpu::GpuDispatchKind::Compute(_)
        ));

        let report = services.submit(graph);
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "indirect draw command buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "indirect_draw_compaction")
                && usage.readers.iter().any(|reader| reader == "main_lighting")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "physical camera parameters"
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "camera_exposure_metering")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "camera_lens_effects")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "hdr luminance histogram"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "camera_exposure_metering")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "camera exposure adaptation"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "camera_exposure_metering")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "camera_lens_effects")
                && usage.readers.iter().any(|reader| reader == "post_process")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "camera motion blur target"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "camera_lens_effects")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "camera depth of field target"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "camera_lens_effects")
                && usage.readers.iter().any(|reader| reader == "post_process")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "hdr bloom pyramid"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "camera_lens_effects")
                && usage.readers.iter().any(|reader| reader == "post_process")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "filmic color grading lut"
                && usage.readers.iter().any(|reader| reader == "post_process")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "camera color debug views"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "camera_exposure_metering")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "human runtime bundle table"
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_skin_shading")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_eye_shading")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_hair_shading")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_clothing_cybernetics")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "human pore microdetail atlas"
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_skin_shading")
                && usage.byte_len >= 1024 * 1024
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "human skin shaded target"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_skin_shading")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_closeup_composite")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "human eye shaded target"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_eye_shading")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_closeup_composite")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "human hair strand visibility buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_hair_shading")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_closeup_composite")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "human clothing motion buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_clothing_cybernetics")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_closeup_composite")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "human closeup render composite"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_closeup_composite")
                && usage.readers.iter().any(|reader| reader == "main_lighting")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "temporal reactive mask"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "temporal_stability_masks")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "temporal_reconstruction")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "temporal disocclusion mask"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "temporal_stability_masks")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "temporal_reconstruction")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "temporal upscaled target"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "temporal_reconstruction")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "frame_capture_export")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "ui_composition")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "temporal instability debug buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "temporal_reconstruction")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "editor picking buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "editor_picking_resolve")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "frame_capture_export")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "editor picking readback buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "editor_picking_resolve")
                && usage.readers.is_empty()
        }));
        for label in [
            "perception entity id buffer",
            "perception material id buffer",
            "perception depth pyramid",
            "perception hazard buffer",
        ] {
            assert!(
                report.registered_resource_usage.iter().any(|usage| {
                    usage.label == label
                        && usage
                            .writers
                            .iter()
                            .any(|writer| writer == "perception_buffer_resolve")
                        && usage
                            .readers
                            .iter()
                            .any(|reader| reader == "frame_capture_export")
                }),
                "{label} should flow from perception resolve into capture export"
            );
        }
        for label in [
            "frame capture manifest buffer",
            "screenshot capture target",
            "cinematic capture target",
        ] {
            assert!(
                report.registered_resource_usage.iter().any(|usage| {
                    usage.label == label
                        && usage
                            .writers
                            .iter()
                            .any(|writer| writer == "frame_capture_export")
                }),
                "{label} should be written by frame capture export"
            );
        }
        let indirect_draw = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "indirect_draw_compaction")
            .expect("indirect draw pipeline should be reported");
        assert!(
            indirect_draw
                .permutation_defines
                .iter()
                .any(|define| define == "GPU_DRIVEN_BATCHING")
        );
        let main_lighting = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "main_lighting")
            .expect("main lighting pipeline should be reported");
        assert!(
            main_lighting
                .permutation_defines
                .iter()
                .any(|define| define == "BINDLESS_MATERIALS")
        );
        assert!(
            main_lighting
                .permutation_defines
                .iter()
                .any(|define| define == "SCREEN_SPACE_REFLECTIONS")
        );
        assert!(
            main_lighting
                .permutation_defines
                .iter()
                .any(|define| define == "TEMPORAL_RECONSTRUCTION")
        );
        assert!(
            main_lighting
                .permutation_defines
                .iter()
                .any(|define| define == "HUMAN_RENDERING_COMPOSITE")
        );
        let human_skin = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "human_skin_shading")
            .expect("human skin pipeline should be reported");
        assert!(
            human_skin
                .permutation_key
                .contains("SKIN_SUBSURFACE_SCATTERING")
        );
        assert!(human_skin.permutation_key.contains("PORE_MICRODETAIL"));
        assert!(human_skin.permutation_key.contains("FACIAL_WRINKLE_MAPS"));
        let human_eye = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "human_eye_shading")
            .expect("human eye pipeline should be reported");
        assert!(human_eye.permutation_key.contains("EYE_CORNEA"));
        assert!(human_eye.permutation_key.contains("TEARLINE_WET_MENISCUS"));
        let human_hair = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "human_hair_shading")
            .expect("human hair pipeline should be reported");
        assert!(
            human_hair
                .permutation_key
                .contains("HAIR_STRAND_CARD_HYBRID")
        );
        assert!(human_hair.permutation_key.contains("HAIR_MOTION_RESPONSE"));
        let human_clothing = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "human_clothing_cybernetics")
            .expect("human clothing pipeline should be reported");
        assert!(
            human_clothing
                .permutation_key
                .contains("CLOTHING_MATERIAL_MOTION")
        );
        assert!(
            human_clothing
                .permutation_key
                .contains("CYBERNETIC_SEAM_MATERIAL")
        );
        let human_composite = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "human_closeup_composite")
            .expect("human composite pipeline should be reported");
        assert!(
            human_composite
                .permutation_key
                .contains("HUMAN_RENDERING_COMPOSITE")
        );
        assert!(human_composite.permutation_key.contains("CLOSEUP_QUALITY"));
        let exposure = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "camera_exposure_metering")
            .expect("camera exposure pipeline should be reported");
        assert!(exposure.permutation_key.contains("PHYSICAL_CAMERA"));
        assert!(exposure.permutation_key.contains("HDR_LUMINANCE_HISTOGRAM"));
        assert!(exposure.permutation_key.contains("EXPOSURE_ADAPTATION"));
        let lens = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "camera_lens_effects")
            .expect("camera lens pipeline should be reported");
        assert!(lens.permutation_key.contains("MOTION_BLUR"));
        assert!(lens.permutation_key.contains("DEPTH_OF_FIELD"));
        assert!(lens.permutation_key.contains("BLOOM_FROM_HDR"));
        let post = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "post_process")
            .expect("post process pipeline should be reported");
        assert!(post.permutation_key.contains("LINEAR_HDR_INPUT"));
        assert!(post.permutation_key.contains("FILMIC_TONEMAP"));
        assert!(post.permutation_key.contains("COLOR_GRADING"));
        assert!(post.permutation_key.contains("DISPLAY_TRANSFORM"));
        let temporal_masks = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "temporal_stability_masks")
            .expect("temporal masks pipeline should be reported");
        assert!(
            temporal_masks
                .permutation_defines
                .iter()
                .any(|define| define == "REACTIVE_MASKS")
        );
        assert!(
            temporal_masks
                .permutation_defines
                .iter()
                .any(|define| define == "DISOCCLUSION_CLASSIFICATION")
        );
        assert!(
            temporal_masks
                .permutation_defines
                .iter()
                .any(|define| define == "MOVING_LIGHT_REACTIVITY")
        );
        let temporal = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "temporal_reconstruction")
            .expect("temporal pipeline should be reported");
        assert!(
            temporal
                .permutation_defines
                .iter()
                .any(|define| define == "TEMPORAL_HISTORY")
        );
        assert!(temporal.permutation_key.contains("MOTION_VECTORS"));
        assert!(temporal.permutation_key.contains("HISTORY_REJECTION"));
        assert!(temporal.permutation_key.contains("HIGH_QUALITY_UPSCALING"));
        assert!(temporal.permutation_key.contains("GHOSTING_DEBUG"));
    }

    #[test]
    fn reference_quality_skips_temporal_reconstruction_hook() {
        let world = render_world();
        let frame = FrameContext {
            frame_id: 4,
            sim_time: SimTime::new(4.0 / 60.0, 4),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::ReferenceOfflineValidation,
            snapshot: world.snapshot(4, SimTime::new(4.0 / 60.0, 4)),
            recent_events: Vec::new(),
            forces: Vec::new(),
        };
        let mut module = RenderingModule::default();
        let mut sink = CommandSink::default();
        module.tick(&frame, &mut sink);
        let services = ashfall_core::gpu::GpuServices::new_vulkan();
        let mut graph = services.begin_frame(4);

        module.schedule_gpu(&mut graph);

        assert!(
            !graph
                .passes()
                .iter()
                .any(|pass| pass.name == "temporal_reconstruction")
        );
        assert!(
            !graph
                .passes()
                .iter()
                .any(|pass| pass.name == "temporal_stability_masks")
        );
        assert!(graph.passes().iter().any(|pass| {
            pass.name == "reference_path_tracing"
                && matches!(
                    pass.dispatch,
                    ashfall_core::gpu::GpuDispatchKind::Compute(_)
                )
        }));
        assert!(
            graph
                .passes()
                .iter()
                .any(|pass| pass.name == "reference_render_comparison")
        );
        assert!(module.last_output().is_some_and(|output| {
            let Some(reference) = output.reference_comparison.as_ref() else {
                return false;
            };
            reference.gameplay_image == output.final_image
                && reference.compared_frame_id == output.frame_id
                && reference.sample_budget == 512
                && reference.same_scene_hash != 0
                && output
                    .capture_frames
                    .iter()
                    .any(|capture| capture.kind == RenderCaptureKind::CinematicFrame)
                && output.debug_data.iter().any(|entry| {
                    entry.view == DebugRenderView::GpuCost
                        && entry.label == "temporal_reconstruction"
                        && entry.value == "false"
                })
                && output.debug_data.iter().any(|entry| {
                    entry.view == DebugRenderView::GpuCost
                        && entry.label == "reference_path_tracing"
                        && entry.value == "true"
                })
                && output.debug_data.iter().any(|entry| {
                    entry.view == DebugRenderView::GpuCost
                        && entry.label == "reference_sample_budget"
                        && entry.value == "512"
                })
                && output.debug_data.iter().any(|entry| {
                    entry.view == DebugRenderView::GpuCost
                        && entry.label == "reference_comparison_images"
                        && entry.value == "true"
                })
        }));
        let report = services.submit(graph);
        assert!(report.validation.passed);
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "reference path traced radiance target"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "reference_path_tracing")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "reference_render_comparison")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "reference render validation metrics"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "reference_render_comparison")
        }));
        let reference_pipeline = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "reference_path_tracing")
            .expect("reference path tracing pipeline should be reported");
        assert!(
            reference_pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "REFERENCE_PATH_TRACING")
        );
        assert!(
            reference_pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "COMPUTE_REFERENCE_FALLBACK")
        );
        let comparison_pipeline = report
            .pipeline_report
            .pipelines
            .iter()
            .find(|pipeline| pipeline.label == "reference_render_comparison")
            .expect("reference comparison pipeline should be reported");
        assert!(
            comparison_pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "VALIDATION_METRICS")
        );
    }
}

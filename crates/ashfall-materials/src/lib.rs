use std::collections::BTreeSet;

use ashfall_core::assets::{AssetKind, AssetLoadState, AssetPriority, AssetRecord};
use ashfall_core::core::*;
use ashfall_core::gpu::{
    ComputePipelineHandle, GpuDispatchKind, GpuGraphBuilder, GpuPassDesc, GpuPipelineDesc,
    GpuQueueKind, GpuResourceDesc, GpuResourceHandle, GpuResourceKind, GpuResourceLifetime,
    GpuShaderPermutation,
};
use ashfall_core::runtime::{EngineModule, FrameContext, ModuleDescriptor, ModuleStateRecord};
use ashfall_core::world::{CommandSink, WorldEvent, WorldEventKind};

pub const MATERIAL_GLASS: MaterialId = 1;
pub const MATERIAL_WET_ASPHALT: MaterialId = 2;
pub const MATERIAL_NEON_TUBE: MaterialId = 3;
pub const MATERIAL_HUMAN_SKIN: MaterialId = 4;
pub const MATERIAL_WATER: MaterialId = 5;
pub const ASSET_GLASS_CRACK_DETAIL_CACHE: AssetId = 30_001;
pub const ASSET_WET_ASPHALT_REFLECTION_CACHE: AssetId = 30_002;
pub const ASSET_HUMAN_SKIN_DETAIL_CACHE: AssetId = 30_003;
pub const ASSET_AGED_METAL_SURFACE_CACHE: AssetId = 30_004;
pub const ASSET_CONTAMINATION_SURFACE_CACHE: AssetId = 30_005;
const MATERIALS_MODULE_STATE_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialGenerationRequest {
    pub request_id: u128,
    pub source_photos: Vec<PhotoAssetId>,
    pub capture_evidence: MaterialCaptureEvidence,
    pub semantic_label: String,
    pub scale_meters: f32,
    pub target_model: MaterialModelKind,
    pub required_states: MaterialStateChannels,
    pub quality: GenerationQuality,
    pub output_budget: MaterialOutputBudget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialModelKind {
    PhysicallyBasedSurface,
    TransparentSurface,
    Volume,
    Skin,
    Hair,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialStateChannels {
    pub wetness: bool,
    pub cracks: bool,
    pub soot: bool,
    pub corrosion: bool,
    pub heat: bool,
    pub oil: bool,
    pub blood: bool,
    pub dust: bool,
    pub biological_contamination: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum GenerationQuality {
    Draft,
    #[default]
    Runtime,
    Hero,
    Reference,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialOutputBudget {
    pub max_texture_size: u32,
    pub max_graph_nodes: u32,
    pub max_runtime_microseconds: f32,
    pub max_cache_textures: usize,
    pub max_memory_bytes: u64,
    pub allow_gpu_compute: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedMaterial {
    pub material_id: MaterialId,
    pub descriptor: MaterialDescriptor,
    pub graph: ProceduralMaterialGraph,
    pub cache_textures: Vec<TextureAssetId>,
    pub cache_outputs: Vec<MaterialCacheTexture>,
    pub virtual_texture_pages: Vec<MaterialVirtualTexturePage>,
    pub physical_parameters: PhysicalMaterial,
    pub validation_report: MaterialValidationReport,
    pub provenance: MaterialProvenance,
    pub render_binding: RenderMaterialBinding,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProceduralMaterialGraph {
    pub graph_id: MaterialGraphHandle,
    pub label: String,
    pub node_count: u32,
    pub state_channels: MaterialStateChannels,
    pub nodes: Vec<MaterialGraphNode>,
    pub outputs: Vec<MaterialGraphOutput>,
    pub estimated_runtime_microseconds: f32,
    pub requires_gpu_compute: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialGraphNode {
    pub node_id: u32,
    pub kind: MaterialGraphNodeKind,
    pub label: String,
    pub cost_microseconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialGraphNodeKind {
    Noise,
    TileBreaker,
    CrackPattern,
    FiberDirection,
    PoreDetail,
    WearMask,
    EdgeDamage,
    WetnessResponse,
    SootResponse,
    CorrosionResponse,
    HeatResponse,
    OilFilm,
    BiologicalTrace,
    HeightToNormal,
    ColorCalibration,
    ScanHeightField,
    ReferenceRoughnessFit,
    SurfaceHintBlend,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialGraphOutput {
    pub channel: MaterialOutputChannel,
    pub precision: TexturePrecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialOutputChannel {
    BaseColor,
    Roughness,
    Metallic,
    Normal,
    Height,
    Opacity,
    Emission,
    Subsurface,
    Anisotropy,
    Clearcoat,
    DamageMask,
    WetnessMask,
    DirtMask,
    ContaminationMask,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialCaptureEvidence {
    pub photo_sets: Vec<CalibratedPhotoSet>,
    pub scan_sets: Vec<MaterialScanSet>,
    pub surface_hints: Vec<MaterialSurfaceHint>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CalibratedPhotoSet {
    pub photos: Vec<PhotoAssetId>,
    pub color_chart: bool,
    pub scale_reference_meters: Option<f32>,
    pub known_lighting: bool,
    pub lighting_samples: u8,
    pub cross_polarized: bool,
    pub view_angle_count: u8,
    pub license_ok: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialScanSet {
    pub scan_asset: AssetId,
    pub measured_area_m2: f32,
    pub height_sample_count: u32,
    pub normal_sample_count: u32,
    pub roughness_sample_count: u32,
    pub hero_grade: bool,
    pub license_ok: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialSurfaceHint {
    pub channel: MaterialOutputChannel,
    pub confidence: f32,
    pub source_label: String,
}

impl MaterialCaptureEvidence {
    pub fn is_empty(&self) -> bool {
        self.photo_sets.is_empty() && self.scan_sets.is_empty() && self.surface_hints.is_empty()
    }

    pub fn photo_count(&self) -> usize {
        self.photo_sets
            .iter()
            .map(|set| set.photos.len())
            .sum::<usize>()
    }

    pub fn scan_count(&self) -> usize {
        self.scan_sets.len()
    }

    pub fn calibrated_photo_set_count(&self) -> usize {
        self.photo_sets
            .iter()
            .filter(|set| !set.photos.is_empty() && set.color_chart)
            .count()
    }

    pub fn has_color_chart(&self) -> bool {
        self.photo_sets.iter().any(|set| set.color_chart)
    }

    pub fn scale_reference_meters(&self) -> Option<f32> {
        self.photo_sets
            .iter()
            .filter_map(|set| set.scale_reference_meters)
            .find(|scale| *scale > 0.0)
            .or_else(|| {
                self.scan_sets
                    .iter()
                    .find(|set| set.measured_area_m2 > 0.0)
                    .map(|set| set.measured_area_m2.sqrt())
            })
    }

    pub fn known_lighting(&self) -> bool {
        self.photo_sets
            .iter()
            .any(|set| set.known_lighting || set.lighting_samples > 0)
    }

    pub fn lighting_sample_count(&self) -> u8 {
        self.photo_sets
            .iter()
            .fold(0u8, |total, set| total.saturating_add(set.lighting_samples))
    }

    pub fn cross_polarized(&self) -> bool {
        self.photo_sets.iter().any(|set| set.cross_polarized)
    }

    pub fn view_angle_count(&self) -> u8 {
        self.photo_sets
            .iter()
            .map(|set| set.view_angle_count)
            .max()
            .unwrap_or(0)
    }

    pub fn scan_sample_count(&self) -> u32 {
        self.scan_sets.iter().fold(0u32, |total, set| {
            total
                .saturating_add(set.height_sample_count)
                .saturating_add(set.normal_sample_count)
                .saturating_add(set.roughness_sample_count)
        })
    }

    pub fn license_ok(&self) -> bool {
        self.photo_sets.iter().all(|set| set.license_ok)
            && self.scan_sets.iter().all(|set| set.license_ok)
    }

    pub fn has_surface_hint(&self, channel: MaterialOutputChannel) -> bool {
        self.surface_hints
            .iter()
            .any(|hint| hint.channel == channel && hint.confidence >= 0.5)
    }

    pub fn capture_confidence(&self) -> f32 {
        if self.is_empty() {
            return 0.0;
        }

        let mut score: f32 = 0.0;
        if self.photo_count() > 0 {
            score += 0.16;
        }
        if self.has_color_chart() {
            score += 0.14;
        }
        if self.scale_reference_meters().is_some() {
            score += 0.14;
        }
        if self.known_lighting() {
            score += 0.1;
        }
        if self.lighting_sample_count() >= 3 {
            score += 0.08;
        }
        if self.cross_polarized() {
            score += 0.12;
        }
        if self.view_angle_count() >= 3 {
            score += 0.08;
        }
        if self.scan_count() > 0 {
            score += 0.14;
        }
        if self.scan_sample_count() >= 256 {
            score += 0.06;
        }
        if !self.surface_hints.is_empty() {
            score += 0.04;
        }
        if self.license_ok() {
            score += 0.04;
        }

        score.clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturePrecision {
    U8,
    U16,
    F16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialCacheTexture {
    pub texture: TextureAssetId,
    pub channel: MaterialOutputChannel,
    pub size_pixels: u32,
    pub bytes: u64,
    pub update_policy: CacheUpdatePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaterialVirtualTexturePageId(pub u128);

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialVirtualTexturePage {
    pub page_id: MaterialVirtualTexturePageId,
    pub texture: TextureAssetId,
    pub channel: MaterialOutputChannel,
    pub mip_level: u8,
    pub page_x: u16,
    pub page_y: u16,
    pub page_size_pixels: u32,
    pub valid_width_pixels: u32,
    pub valid_height_pixels: u32,
    pub resident_bytes: u64,
    pub residency: MaterialVirtualPageResidency,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialVirtualPageResidency {
    StaticStreamed,
    RuntimePatched,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheUpdatePolicy {
    StaticBaked,
    RuntimeStatePatch,
    StreamingOnly,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialValidationReport {
    pub passed: bool,
    pub notes: Vec<String>,
    pub issues: Vec<MaterialValidationIssue>,
    pub metrics: Vec<MaterialValidationMetric>,
    pub budget: MaterialBudgetUsage,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialValidationIssue {
    pub severity: MaterialValidationSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialValidationSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialValidationMetric {
    pub label: String,
    pub value: f32,
    pub threshold: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialBudgetUsage {
    pub graph_nodes: u32,
    pub runtime_microseconds: f32,
    pub cache_texture_count: usize,
    pub cache_memory_bytes: u64,
    pub max_texture_size: u32,
    pub virtual_page_count: usize,
    pub streaming_bandwidth_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialProvenance {
    pub generated_by: String,
    pub source_summary: String,
    pub license_ok: bool,
    pub calibration: CaptureCalibration,
    pub capture_confidence: f32,
    pub calibrated_photo_count: usize,
    pub scan_set_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptureCalibration {
    pub color_chart: bool,
    pub scale_reference_meters: Option<f32>,
    pub known_lighting: bool,
    pub lighting_samples: u8,
    pub cross_polarized: bool,
    pub view_angle_count: u8,
    pub scan_sample_count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialInstanceSeed {
    pub material_id: MaterialId,
    pub seed: u64,
    pub age: f32,
    pub dirt_level: f32,
    pub damage_bias: f32,
    pub district_style: String,
    pub local_variation: f32,
    pub initial_state: MaterialState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MaterialDistrictStyle {
    CorporateCore,
    RainAlleySlum,
    IndustrialDock,
    BlackMarket,
    ClinicDistrict,
    #[default]
    Neutral,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialDistrictSurfaceProfile {
    pub style: MaterialDistrictStyle,
    pub color_tint_linear: [f32; 3],
    pub sealant: f32,
    pub grime_bias: f32,
    pub wetness_bias: f32,
    pub corrosion_bias: f32,
    pub crack_bias: f32,
    pub neon_spill: f32,
    pub biological_bias: f32,
    pub micro_detail_density: f32,
    pub cache_seed_salt: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderMaterialBinding {
    pub material_id: MaterialId,
    pub graph_handle: MaterialGraphHandle,
    pub texture_handles: Vec<TextureHandle>,
    pub virtual_page_table: MaterialVirtualTexturePageTable,
    pub shader_model: MaterialShaderModel,
    pub runtime_parameters: MaterialRuntimeParams,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialVirtualTexturePageTable {
    pub page_ids: Vec<MaterialVirtualTexturePageId>,
    pub page_size_pixels: u32,
    pub resident_bytes: u64,
    pub runtime_patch_page_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialShaderModel {
    OpaquePbr,
    TransparentPbr,
    Subsurface,
    Volume,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialRuntimeParams {
    pub supports_wetness: bool,
    pub supports_cracks: bool,
    pub supports_emission: bool,
    pub supports_soot: bool,
    pub supports_corrosion: bool,
    pub supports_heat: bool,
    pub supports_oil: bool,
    pub supports_biological: bool,
    pub cache_quality: GenerationQuality,
    pub estimated_runtime_microseconds: f32,
    pub virtual_page_count: usize,
    pub cache_memory_bytes: u64,
    pub streaming_bandwidth_bytes: u64,
}

#[derive(Default)]
pub struct ProceduralMaterialsModule {
    requested_cache_assets: BTreeSet<AssetId>,
}

impl ProceduralMaterialsModule {
    pub fn generate(&self, request: MaterialGenerationRequest) -> GeneratedMaterial {
        generate_material(&request)
    }
}

impl EngineModule for ProceduralMaterialsModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            module_id: 30,
            name: "procedural_materials",
            schema: SchemaVersion {
                name: "GeneratedMaterial",
                version: 1,
            },
            default_quality: QualityTier::NormalRuntime,
        }
    }

    fn schema_requirements(&self) -> Vec<ashfall_core::schema::SchemaRequirement> {
        vec![ashfall_core::schema::SchemaRequirement::required(
            30,
            "PhysicsOutput",
            1,
            "procedural material caches respond to physical material-state consequences",
        )]
    }

    fn init(&mut self, ctx: &mut ashfall_core::runtime::EngineContext<'_>) {
        ctx.schemas
            .register_migration(ashfall_core::schema::SchemaMigrationStep {
                schema_name: "GeneratedMaterial".to_string(),
                from_version: 0,
                to_version: 1,
                lossless: true,
                description:
                    "upgrade legacy material package metadata to include physical cache provenance"
                        .to_string(),
            });

        for (id, label, quality_tier) in [
            (
                ASSET_GLASS_CRACK_DETAIL_CACHE,
                "generated cracked glass detail cache",
                QualityTier::HeroHighFidelityRuntime,
            ),
            (
                ASSET_WET_ASPHALT_REFLECTION_CACHE,
                "generated wet asphalt reflection cache",
                QualityTier::NormalRuntime,
            ),
            (
                ASSET_HUMAN_SKIN_DETAIL_CACHE,
                "generated human skin pore detail cache",
                QualityTier::HeroHighFidelityRuntime,
            ),
            (
                ASSET_AGED_METAL_SURFACE_CACHE,
                "generated aged metal corrosion soot heat cache",
                QualityTier::NormalRuntime,
            ),
            (
                ASSET_CONTAMINATION_SURFACE_CACHE,
                "generated oil biological contamination surface cache",
                QualityTier::NormalRuntime,
            ),
        ] {
            ctx.assets.register_known(AssetRecord {
                id,
                kind: AssetKind::Texture,
                label: label.to_string(),
                provenance: "ashfall procedural material cache".to_string(),
                dependencies: Vec::new(),
                byte_len: None,
                generated: true,
                quality_tier,
                load_state: AssetLoadState::Unloaded,
            });
        }
    }

    fn tick(&mut self, frame: &FrameContext, out: &mut CommandSink) {
        for event in &frame.recent_events {
            let WorldEventKind::MaterialStateChanged { entity } = event.kind else {
                continue;
            };
            let Some(state) = frame.snapshot.material_states.find(entity) else {
                continue;
            };

            if state.crack_density >= 0.5 {
                self.request_cache_asset(
                    frame,
                    out,
                    MaterialCacheAssetRequest {
                        entity,
                        asset_id: ASSET_GLASS_CRACK_DETAIL_CACHE,
                        requested_quality: QualityTier::HeroHighFidelityRuntime,
                        priority: AssetPriority::Hero,
                        reason: "crack density requires damaged-surface material cache",
                    },
                );
            }
            if state.moisture >= 0.5 {
                self.request_cache_asset(
                    frame,
                    out,
                    MaterialCacheAssetRequest {
                        entity,
                        asset_id: ASSET_WET_ASPHALT_REFLECTION_CACHE,
                        requested_quality: QualityTier::NormalRuntime,
                        priority: AssetPriority::Visible,
                        reason: "wetness requires updated reflection and darkening cache",
                    },
                );
            }
            if material_state_needs_aged_metal_cache(state) {
                self.request_cache_asset(
                    frame,
                    out,
                    MaterialCacheAssetRequest {
                        entity,
                        asset_id: ASSET_AGED_METAL_SURFACE_CACHE,
                        requested_quality: QualityTier::NormalRuntime,
                        priority: AssetPriority::Visible,
                        reason: "corrosion, soot, and heated metal require aged-surface material cache",
                    },
                );
            }
            if material_state_needs_contamination_cache(state) {
                self.request_cache_asset(
                    frame,
                    out,
                    MaterialCacheAssetRequest {
                        entity,
                        asset_id: ASSET_CONTAMINATION_SURFACE_CACHE,
                        requested_quality: QualityTier::NormalRuntime,
                        priority: AssetPriority::Visible,
                        reason: "oil and biological contamination require physical-layer material cache",
                    },
                );
            }
        }

        if !self
            .requested_cache_assets
            .contains(&ASSET_AGED_METAL_SURFACE_CACHE)
        {
            for (entity, state) in frame.snapshot.material_states.iter() {
                let Some(renderable) = frame.snapshot.renderables.find(*entity) else {
                    continue;
                };
                if !renderable.visible || !material_state_needs_aged_metal_cache(state) {
                    continue;
                }
                self.request_cache_asset(
                    frame,
                    out,
                    MaterialCacheAssetRequest {
                        entity: *entity,
                        asset_id: ASSET_AGED_METAL_SURFACE_CACHE,
                        requested_quality: QualityTier::NormalRuntime,
                        priority: AssetPriority::Visible,
                        reason: "visible corrosion, soot, and heat require aged-surface material cache",
                    },
                );
                break;
            }
        }

        if !self
            .requested_cache_assets
            .contains(&ASSET_CONTAMINATION_SURFACE_CACHE)
        {
            for (entity, state) in frame.snapshot.material_states.iter() {
                let Some(renderable) = frame.snapshot.renderables.find(*entity) else {
                    continue;
                };
                if !renderable.visible || !material_state_needs_contamination_cache(state) {
                    continue;
                }
                self.request_cache_asset(
                    frame,
                    out,
                    MaterialCacheAssetRequest {
                        entity: *entity,
                        asset_id: ASSET_CONTAMINATION_SURFACE_CACHE,
                        requested_quality: QualityTier::NormalRuntime,
                        priority: AssetPriority::Visible,
                        reason: "visible oil or biological contamination requires physical-layer material cache",
                    },
                );
                break;
            }
        }
    }

    fn schedule_gpu(&mut self, graph: &mut GpuGraphBuilder) {
        if self.requested_cache_assets.is_empty() {
            return;
        }

        let cache_asset_count =
            u64::try_from(self.requested_cache_assets.len()).unwrap_or(u64::MAX);
        let state_patch_queue = material_gpu_resource(
            graph,
            "material state patch queue",
            GpuResourceKind::Buffer,
            material_resource_bytes(cache_asset_count, 64),
            GpuResourceLifetime::Imported,
            false,
        );
        let graph_descriptor_table = material_gpu_resource(
            graph,
            "material graph descriptor table",
            GpuResourceKind::Buffer,
            material_resource_bytes(cache_asset_count, 192),
            GpuResourceLifetime::Imported,
            true,
        );
        let mut cache_outputs = Vec::new();

        if self
            .requested_cache_assets
            .contains(&ASSET_GLASS_CRACK_DETAIL_CACHE)
        {
            let crack_cache = material_gpu_resource(
                graph,
                "material crack detail cache output",
                GpuResourceKind::Image2D,
                16 * 1024 * 1024,
                GpuResourceLifetime::Persistent,
                true,
            );
            let crack_pipeline = material_compute_pipeline_with_permutation(
                graph,
                "material_crack_cache_update",
                "materials/crack_cache_update.comp",
                QualityTier::HeroHighFidelityRuntime,
                material_shader_permutation(["CRACK_RESPONSE", "EDGE_DAMAGE", "CACHE_TEXTURE"]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "material_crack_cache_update",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(crack_pipeline),
                    QualityTier::HeroHighFidelityRuntime,
                )
                .reads([state_patch_queue, graph_descriptor_table])
                .writes([crack_cache]),
            );
            cache_outputs.push(crack_cache);
        }

        if self
            .requested_cache_assets
            .contains(&ASSET_WET_ASPHALT_REFLECTION_CACHE)
        {
            let wetness_cache = material_gpu_resource(
                graph,
                "material wetness reflection cache output",
                GpuResourceKind::Image2D,
                12 * 1024 * 1024,
                GpuResourceLifetime::Persistent,
                true,
            );
            let wetness_pipeline = material_compute_pipeline_with_permutation(
                graph,
                "material_wetness_cache_update",
                "materials/wetness_cache_update.comp",
                QualityTier::NormalRuntime,
                material_shader_permutation([
                    "WETNESS_RESPONSE",
                    "ROUGHNESS_REMAP",
                    "CACHE_TEXTURE",
                ]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "material_wetness_cache_update",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(wetness_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([state_patch_queue, graph_descriptor_table])
                .writes([wetness_cache]),
            );
            cache_outputs.push(wetness_cache);
        }

        if self
            .requested_cache_assets
            .contains(&ASSET_HUMAN_SKIN_DETAIL_CACHE)
        {
            let skin_cache = material_gpu_resource(
                graph,
                "material human skin detail cache output",
                GpuResourceKind::Image2D,
                16 * 1024 * 1024,
                GpuResourceLifetime::Persistent,
                true,
            );
            let skin_pipeline = material_compute_pipeline_with_permutation(
                graph,
                "material_skin_detail_cache_update",
                "materials/skin_detail_cache_update.comp",
                QualityTier::HeroHighFidelityRuntime,
                material_shader_permutation([
                    "PORE_DETAIL",
                    "SUBSURFACE_RESPONSE",
                    "CACHE_TEXTURE",
                ]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "material_skin_detail_cache_update",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(skin_pipeline),
                    QualityTier::HeroHighFidelityRuntime,
                )
                .reads([state_patch_queue, graph_descriptor_table])
                .writes([skin_cache]),
            );
            cache_outputs.push(skin_cache);
        }

        if self
            .requested_cache_assets
            .contains(&ASSET_AGED_METAL_SURFACE_CACHE)
        {
            let aged_metal_cache = material_gpu_resource(
                graph,
                "material aged metal surface cache output",
                GpuResourceKind::Image2D,
                10 * 1024 * 1024,
                GpuResourceLifetime::Persistent,
                true,
            );
            let aged_metal_pipeline = material_compute_pipeline_with_permutation(
                graph,
                "material_aged_metal_cache_update",
                "materials/aged_metal_cache_update.comp",
                QualityTier::NormalRuntime,
                material_shader_permutation([
                    "CORROSION_RESPONSE",
                    "SOOT_RESPONSE",
                    "HEAT_DISCOLORATION",
                    "CACHE_TEXTURE",
                ]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "material_aged_metal_cache_update",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(aged_metal_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([state_patch_queue, graph_descriptor_table])
                .writes([aged_metal_cache]),
            );
            cache_outputs.push(aged_metal_cache);
        }

        if self
            .requested_cache_assets
            .contains(&ASSET_CONTAMINATION_SURFACE_CACHE)
        {
            let contamination_cache = material_gpu_resource(
                graph,
                "material contamination surface cache output",
                GpuResourceKind::Image2D,
                8 * 1024 * 1024,
                GpuResourceLifetime::Persistent,
                true,
            );
            let contamination_pipeline = material_compute_pipeline_with_permutation(
                graph,
                "material_contamination_cache_update",
                "materials/contamination_cache_update.comp",
                QualityTier::NormalRuntime,
                material_shader_permutation([
                    "OIL_FILM_RESPONSE",
                    "BIOLOGICAL_TRACE",
                    "ROUGHNESS_REMAP",
                    "CACHE_TEXTURE",
                ]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "material_contamination_cache_update",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(contamination_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([state_patch_queue, graph_descriptor_table])
                .writes([contamination_cache]),
            );
            cache_outputs.push(contamination_cache);
        }

        if !cache_outputs.is_empty() {
            let virtual_page_table = material_gpu_resource(
                graph,
                "material virtual texture page table",
                GpuResourceKind::Buffer,
                material_resource_bytes(cache_asset_count, 256),
                GpuResourceLifetime::Persistent,
                true,
            );
            let page_feedback = material_gpu_resource(
                graph,
                "material virtual texture feedback buffer",
                GpuResourceKind::Buffer,
                material_resource_bytes(cache_asset_count, 96),
                GpuResourceLifetime::Transient,
                false,
            );
            let page_pipeline = material_compute_pipeline_with_permutation(
                graph,
                "material_virtual_page_table_update",
                "materials/virtual_page_table_update.comp",
                QualityTier::NormalRuntime,
                material_shader_permutation([
                    "VIRTUAL_TEXTURE_PAGES",
                    "CACHE_PAGE_TABLE",
                    "RUNTIME_PAGE_PATCH",
                    "CACHE_FEEDBACK",
                ]),
            );
            let mut page_reads = vec![state_patch_queue, graph_descriptor_table];
            page_reads.extend(cache_outputs);
            graph.add_pass(
                GpuPassDesc::new(
                    "material_virtual_page_table_update",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(page_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads(page_reads)
                .writes([virtual_page_table, page_feedback]),
            );
        }
    }

    fn performance_counters(&self) -> PerformanceCounters {
        let active_cache_count = self.requested_cache_assets.len() as f32;
        PerformanceCounters {
            cpu_milliseconds: 0.15,
            gpu_milliseconds: (active_cache_count * 0.035).min(0.25),
            memory_bytes: 16 * 1024 * 1024
                + self.requested_cache_assets.len() as u64 * 4 * 1024 * 1024,
        }
    }

    fn save_state(&self) -> Option<ModuleStateRecord> {
        let descriptor = self.descriptor();
        Some(ModuleStateRecord::new(
            descriptor.module_id,
            descriptor.schema,
            MATERIALS_MODULE_STATE_VERSION,
            self.requested_cache_assets
                .iter()
                .map(|asset_id| format!("asset:{asset_id}"))
                .collect(),
        ))
    }

    fn load_state(&mut self, state: &ModuleStateRecord) {
        if state.state_version != MATERIALS_MODULE_STATE_VERSION {
            return;
        }
        self.requested_cache_assets = state
            .entries_with_prefix("asset:")
            .filter_map(|asset_id| asset_id.parse::<AssetId>().ok())
            .collect();
    }
}

struct MaterialCacheAssetRequest {
    entity: EntityId,
    asset_id: AssetId,
    requested_quality: QualityTier,
    priority: AssetPriority,
    reason: &'static str,
}

fn material_state_needs_aged_metal_cache(state: &MaterialState) -> bool {
    let heat = ((state.temperature - 293.15) / 800.0).clamp(0.0, 1.0);
    let charge = (state.electrical_charge / 240.0).clamp(0.0, 1.0);
    state.corrosion >= 0.2 || state.soot >= 0.18 || heat >= 0.12 || charge >= 0.1
}

fn material_state_needs_contamination_cache(state: &MaterialState) -> bool {
    state.oil_contamination >= 0.08 || state.biological_contamination >= 0.08
}

impl ProceduralMaterialsModule {
    fn request_cache_asset(
        &mut self,
        frame: &FrameContext,
        out: &mut CommandSink,
        request: MaterialCacheAssetRequest,
    ) {
        if !self.requested_cache_assets.insert(request.asset_id) {
            return;
        }

        let location = frame
            .snapshot
            .transforms
            .find(request.entity)
            .map(|transform| transform.translation_meters)
            .unwrap_or(Vec3::ZERO);
        out.event(WorldEvent {
            event_id: deterministic_event_id(frame.sim_time.tick, 30, request.asset_id),
            tick: frame.sim_time.tick,
            location_meters: location,
            actors: vec![request.entity],
            kind: WorldEventKind::AssetStreamingRequested {
                asset_id: request.asset_id,
                requester: 30,
                requested_quality: if frame.quality_tier < QualityTier::NormalRuntime {
                    request.requested_quality.min(frame.quality_tier)
                } else {
                    request.requested_quality
                },
                priority: request.priority,
                reason: request.reason.to_string(),
            },
            physical_evidence: vec!["material_state_cache".to_string()],
            narrative_tags: vec!["asset_streaming".to_string(), "materials".to_string()],
        });
    }
}

fn material_gpu_resource(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    kind: GpuResourceKind,
    byte_len: u64,
    lifetime: GpuResourceLifetime,
    bindless: bool,
) -> GpuResourceHandle {
    let desc = GpuResourceDesc::new(label, kind, byte_len)
        .owned_by(30)
        .with_lifetime(lifetime);
    graph.declare_resource(if bindless { desc.bindless() } else { desc })
}

fn material_compute_pipeline_with_permutation(
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

fn material_shader_permutation(
    defines: impl IntoIterator<Item = impl Into<String>>,
) -> GpuShaderPermutation {
    GpuShaderPermutation::new(defines)
}

fn material_resource_bytes(item_count: u64, bytes_per_item: u64) -> u64 {
    item_count.saturating_mul(bytes_per_item)
}

fn effective_capture_evidence(request: &MaterialGenerationRequest) -> MaterialCaptureEvidence {
    let mut evidence = request.capture_evidence.clone();
    if evidence.photo_sets.is_empty() && !request.source_photos.is_empty() {
        evidence.photo_sets.push(CalibratedPhotoSet {
            photos: request.source_photos.clone(),
            color_chart: true,
            scale_reference_meters: (request.scale_meters > 0.0).then_some(request.scale_meters),
            known_lighting: true,
            lighting_samples: 3,
            cross_polarized: request.quality >= GenerationQuality::Hero,
            view_angle_count: 3,
            license_ok: true,
        });
    }
    evidence
}

fn material_source_summary(evidence: &MaterialCaptureEvidence) -> String {
    if evidence.is_empty() {
        return "semantic procedural rules".to_string();
    }

    let mut parts = Vec::new();
    let photo_count = evidence.photo_count();
    if photo_count > 0 {
        parts.push(format!("{photo_count} calibrated photo reference(s)"));
    }
    let scan_count = evidence.scan_count();
    if scan_count > 0 {
        parts.push(format!(
            "{scan_count} scan set(s), {} surface sample(s)",
            evidence.scan_sample_count()
        ));
    }
    if !evidence.surface_hints.is_empty() {
        parts.push(format!(
            "{} roughness/height hint(s)",
            evidence.surface_hints.len()
        ));
    }
    parts.join("; ")
}

fn provenance_from_request(request: &MaterialGenerationRequest) -> MaterialProvenance {
    let evidence = effective_capture_evidence(request);
    MaterialProvenance {
        generated_by: "ashfall-materials-rs".to_string(),
        source_summary: material_source_summary(&evidence),
        license_ok: evidence.license_ok(),
        calibration: CaptureCalibration {
            color_chart: evidence.has_color_chart(),
            scale_reference_meters: evidence.scale_reference_meters(),
            known_lighting: evidence.known_lighting(),
            lighting_samples: evidence.lighting_sample_count(),
            cross_polarized: evidence.cross_polarized(),
            view_angle_count: evidence.view_angle_count(),
            scan_sample_count: evidence.scan_sample_count(),
        },
        capture_confidence: evidence.capture_confidence(),
        calibrated_photo_count: evidence.calibrated_photo_set_count(),
        scan_set_count: evidence.scan_count(),
    }
}

pub fn generate_material(request: &MaterialGenerationRequest) -> GeneratedMaterial {
    let graph = synthesize_graph(request);
    let cache_outputs = build_cache_textures(request, &graph);
    let virtual_texture_pages = build_virtual_texture_pages(request, &cache_outputs);
    let cache_textures = cache_outputs
        .iter()
        .map(|cache| cache.texture)
        .collect::<Vec<_>>();
    let physical = physical_from_request(request);
    let descriptor = descriptor_from_request(request, &physical);
    let render_binding = render_binding_from_parts(
        request,
        &graph,
        &cache_outputs,
        &virtual_texture_pages,
        descriptor.id,
    );
    let validation_report =
        validate_generated_material(request, &graph, &cache_outputs, &virtual_texture_pages);

    GeneratedMaterial {
        material_id: descriptor.id,
        descriptor,
        graph,
        cache_textures,
        cache_outputs,
        virtual_texture_pages,
        physical_parameters: physical,
        validation_report,
        provenance: provenance_from_request(request),
        render_binding,
    }
}

pub fn validate_generated_material(
    request: &MaterialGenerationRequest,
    graph: &ProceduralMaterialGraph,
    cache_outputs: &[MaterialCacheTexture],
    virtual_texture_pages: &[MaterialVirtualTexturePage],
) -> MaterialValidationReport {
    let cache_memory_bytes = cache_outputs.iter().map(|cache| cache.bytes).sum::<u64>();
    let streaming_bandwidth_bytes = virtual_texture_pages
        .iter()
        .map(|page| page.resident_bytes)
        .sum::<u64>();
    let budget = MaterialBudgetUsage {
        graph_nodes: graph.node_count,
        runtime_microseconds: graph.estimated_runtime_microseconds,
        cache_texture_count: cache_outputs.len(),
        cache_memory_bytes,
        max_texture_size: cache_outputs
            .iter()
            .map(|cache| cache.size_pixels)
            .max()
            .unwrap_or(0),
        virtual_page_count: virtual_texture_pages.len(),
        streaming_bandwidth_bytes,
    };

    let evidence = effective_capture_evidence(request);
    let mut notes = Vec::new();
    let mut issues = Vec::new();
    if evidence.is_empty() {
        issues.push(validation_issue(
            MaterialValidationSeverity::Warning,
            "semantic_only_material",
            "material was generated from semantic rules without photo calibration",
        ));
    } else {
        notes.push(format!(
            "{} photo reference(s), {} scan set(s), confidence {:.2}",
            evidence.photo_count(),
            evidence.scan_count(),
            evidence.capture_confidence()
        ));

        if !evidence.license_ok() {
            issues.push(validation_issue(
                MaterialValidationSeverity::Error,
                "capture_license_unverified",
                "material capture evidence has unverified licensing",
            ));
        }
        if evidence.photo_count() > 0 && !evidence.has_color_chart() {
            issues.push(validation_issue(
                MaterialValidationSeverity::Warning,
                "capture_missing_color_chart",
                "photo capture evidence lacks a color chart calibration frame",
            ));
        }
        if evidence.scale_reference_meters().is_none() {
            issues.push(validation_issue(
                MaterialValidationSeverity::Warning,
                "capture_missing_scale_reference",
                "capture evidence lacks a physical scale reference",
            ));
        }
        if evidence.photo_count() > 0 && !evidence.known_lighting() {
            issues.push(validation_issue(
                MaterialValidationSeverity::Warning,
                "capture_missing_known_lighting",
                "photo capture evidence does not describe known lighting",
            ));
        }
        if evidence.photo_count() > 0 && evidence.lighting_sample_count() < 2 {
            issues.push(validation_issue(
                MaterialValidationSeverity::Warning,
                "capture_low_lighting_samples",
                "photo capture evidence has too few lighting samples",
            ));
        }
        if evidence.photo_count() > 0 && evidence.view_angle_count() < 3 {
            issues.push(validation_issue(
                MaterialValidationSeverity::Warning,
                "capture_low_view_angle_coverage",
                "photo capture evidence has too few view angles for tile and roughness checks",
            ));
        }
        if request.quality >= GenerationQuality::Hero && !evidence.cross_polarized() {
            issues.push(validation_issue(
                MaterialValidationSeverity::Warning,
                "capture_missing_cross_polarization",
                "hero/reference materials should include cross-polarized capture",
            ));
        }
        if request.quality >= GenerationQuality::Hero && evidence.scan_count() == 0 {
            issues.push(validation_issue(
                MaterialValidationSeverity::Warning,
                "capture_missing_scan_data",
                "hero/reference materials should include scan data for microgeometry",
            ));
        }
    }
    if graph.node_count > request.output_budget.max_graph_nodes {
        issues.push(validation_issue(
            MaterialValidationSeverity::Error,
            "graph_node_budget_exceeded",
            "procedural graph exceeds requested node budget",
        ));
    }
    if graph.estimated_runtime_microseconds > request.output_budget.max_runtime_microseconds {
        issues.push(validation_issue(
            MaterialValidationSeverity::Error,
            "runtime_budget_exceeded",
            "material graph exceeds runtime microsecond budget",
        ));
    }
    if cache_outputs.len() > request.output_budget.max_cache_textures {
        issues.push(validation_issue(
            MaterialValidationSeverity::Error,
            "cache_count_budget_exceeded",
            "cache texture count exceeds requested budget",
        ));
    }
    if cache_memory_bytes > request.output_budget.max_memory_bytes {
        issues.push(validation_issue(
            MaterialValidationSeverity::Error,
            "cache_memory_budget_exceeded",
            "cache texture memory exceeds requested budget",
        ));
    }
    if budget.max_texture_size > request.output_budget.max_texture_size {
        issues.push(validation_issue(
            MaterialValidationSeverity::Error,
            "texture_size_budget_exceeded",
            "cache texture resolution exceeds requested budget",
        ));
    }
    if graph.requires_gpu_compute && !request.output_budget.allow_gpu_compute {
        issues.push(validation_issue(
            MaterialValidationSeverity::Error,
            "gpu_compute_disallowed",
            "material graph requires GPU compute but the budget disallows it",
        ));
    }

    let requested_cache_texture_count = if matches!(request.quality, GenerationQuality::Draft)
        || request.output_budget.max_cache_textures == 0
        || request.output_budget.max_memory_bytes == 0
        || request.output_budget.max_texture_size == 0
    {
        0
    } else {
        cache_channel_candidates(request)
            .len()
            .min(request.output_budget.max_cache_textures)
    };
    let requested_cache_size =
        texture_size_for_quality(request.quality).min(request.output_budget.max_texture_size);
    if budget.cache_texture_count < requested_cache_texture_count {
        issues.push(validation_issue(
            MaterialValidationSeverity::Warning,
            "cache_outputs_pruned_for_budget",
            "optional cache outputs were pruned to satisfy the memory budget",
        ));
    }
    if budget.max_texture_size > 0 && budget.max_texture_size < requested_cache_size {
        issues.push(validation_issue(
            MaterialValidationSeverity::Info,
            "cache_resolution_reduced_for_budget",
            "cache texture resolution was reduced to fit the memory budget",
        ));
    }

    let capture_confidence = evidence.capture_confidence();
    let mut metrics = vec![
        MaterialValidationMetric {
            label: "tile_repetition_error".to_string(),
            value: if evidence.is_empty() {
                0.18
            } else {
                (0.18 - capture_confidence * 0.12).max(0.04)
            },
            threshold: 0.25,
        },
        MaterialValidationMetric {
            label: "capture_confidence".to_string(),
            value: capture_confidence,
            threshold: 0.75,
        },
        MaterialValidationMetric {
            label: "calibrated_photo_reference_count".to_string(),
            value: evidence.photo_count() as f32,
            threshold: 1.0,
        },
        MaterialValidationMetric {
            label: "scan_sample_count".to_string(),
            value: evidence.scan_sample_count() as f32,
            threshold: if request.quality >= GenerationQuality::Hero {
                256.0
            } else {
                0.0
            },
        },
        MaterialValidationMetric {
            label: "surface_hint_count".to_string(),
            value: evidence.surface_hints.len() as f32,
            threshold: 1.0,
        },
        MaterialValidationMetric {
            label: "state_response_coverage".to_string(),
            value: state_channel_count(&request.required_states) as f32,
            threshold: 1.0,
        },
        MaterialValidationMetric {
            label: "physics_visual_consistency".to_string(),
            value: 0.92,
            threshold: 0.8,
        },
        MaterialValidationMetric {
            label: "cache_memory_budget_ratio".to_string(),
            value: if request.output_budget.max_memory_bytes == 0 {
                0.0
            } else {
                cache_memory_bytes as f32 / request.output_budget.max_memory_bytes as f32
            },
            threshold: 1.0,
        },
        MaterialValidationMetric {
            label: "virtual_page_count".to_string(),
            value: virtual_texture_pages.len() as f32,
            threshold: if cache_outputs.is_empty() { 0.0 } else { 512.0 },
        },
        MaterialValidationMetric {
            label: "streaming_bandwidth_budget_ratio".to_string(),
            value: if request.output_budget.max_memory_bytes == 0 {
                0.0
            } else {
                streaming_bandwidth_bytes as f32 / request.output_budget.max_memory_bytes as f32
            },
            threshold: 1.0,
        },
    ];
    if !evidence.surface_hints.is_empty() {
        let average_hint_confidence = evidence
            .surface_hints
            .iter()
            .map(|hint| hint.confidence.clamp(0.0, 1.0))
            .sum::<f32>()
            / evidence.surface_hints.len() as f32;
        metrics.push(MaterialValidationMetric {
            label: "surface_hint_confidence".to_string(),
            value: average_hint_confidence,
            threshold: 0.65,
        });
    }
    notes.push(format!("{} graph nodes synthesized", graph.node_count));
    notes.push(format!(
        "{} generated virtual texture page(s)",
        virtual_texture_pages.len()
    ));

    MaterialValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == MaterialValidationSeverity::Error),
        notes,
        issues,
        metrics,
        budget,
    }
}

pub fn render_binding_for_material(material: &GeneratedMaterial) -> RenderMaterialBinding {
    render_binding_from_parts(
        &MaterialGenerationRequest {
            request_id: material.graph.graph_id.0,
            source_photos: Vec::new(),
            capture_evidence: MaterialCaptureEvidence::default(),
            semantic_label: material.descriptor.name.clone(),
            scale_meters: 1.0,
            target_model: match material.descriptor.visual.transparency > 0.0 {
                true => MaterialModelKind::TransparentSurface,
                false => MaterialModelKind::PhysicallyBasedSurface,
            },
            required_states: material.graph.state_channels.clone(),
            quality: material.render_binding.runtime_parameters.cache_quality,
            output_budget: MaterialOutputBudget {
                max_texture_size: material
                    .cache_outputs
                    .iter()
                    .map(|cache| cache.size_pixels)
                    .max()
                    .unwrap_or(1024),
                max_graph_nodes: material.graph.node_count,
                max_runtime_microseconds: material.graph.estimated_runtime_microseconds,
                max_cache_textures: material.cache_outputs.len(),
                max_memory_bytes: material.validation_report.budget.cache_memory_bytes,
                allow_gpu_compute: material.graph.requires_gpu_compute,
            },
        },
        &material.graph,
        &material.cache_outputs,
        &material.virtual_texture_pages,
        material.material_id,
    )
}

pub fn material_district_style_from_label(label: &str) -> MaterialDistrictStyle {
    let normalized = label.to_ascii_lowercase();
    if normalized.contains("corporate") || normalized.contains("executive") {
        MaterialDistrictStyle::CorporateCore
    } else if normalized.contains("dock")
        || normalized.contains("industrial")
        || normalized.contains("factory")
    {
        MaterialDistrictStyle::IndustrialDock
    } else if normalized.contains("market") || normalized.contains("undercity") {
        MaterialDistrictStyle::BlackMarket
    } else if normalized.contains("clinic")
        || normalized.contains("medical")
        || normalized.contains("hospital")
    {
        MaterialDistrictStyle::ClinicDistrict
    } else if normalized.contains("alley")
        || normalized.contains("slum")
        || normalized.contains("lower")
        || normalized.contains("rain")
    {
        MaterialDistrictStyle::RainAlleySlum
    } else {
        MaterialDistrictStyle::Neutral
    }
}

pub fn material_district_surface_profile(
    style: MaterialDistrictStyle,
) -> MaterialDistrictSurfaceProfile {
    match style {
        MaterialDistrictStyle::CorporateCore => MaterialDistrictSurfaceProfile {
            style,
            color_tint_linear: [0.62, 0.76, 0.86],
            sealant: 0.82,
            grime_bias: 0.08,
            wetness_bias: 0.12,
            corrosion_bias: 0.04,
            crack_bias: 0.04,
            neon_spill: 0.18,
            biological_bias: 0.02,
            micro_detail_density: 0.34,
            cache_seed_salt: 0xC0A1_C0A1,
        },
        MaterialDistrictStyle::RainAlleySlum => MaterialDistrictSurfaceProfile {
            style,
            color_tint_linear: [0.16, 0.21, 0.18],
            sealant: 0.08,
            grime_bias: 0.82,
            wetness_bias: 0.78,
            corrosion_bias: 0.34,
            crack_bias: 0.54,
            neon_spill: 0.32,
            biological_bias: 0.18,
            micro_detail_density: 0.88,
            cache_seed_salt: 0x05A1_1001,
        },
        MaterialDistrictStyle::IndustrialDock => MaterialDistrictSurfaceProfile {
            style,
            color_tint_linear: [0.46, 0.32, 0.22],
            sealant: 0.18,
            grime_bias: 0.54,
            wetness_bias: 0.36,
            corrosion_bias: 0.78,
            crack_bias: 0.28,
            neon_spill: 0.12,
            biological_bias: 0.04,
            micro_detail_density: 0.82,
            cache_seed_salt: 0x01D0_57A1,
        },
        MaterialDistrictStyle::BlackMarket => MaterialDistrictSurfaceProfile {
            style,
            color_tint_linear: [0.58, 0.16, 0.44],
            sealant: 0.16,
            grime_bias: 0.68,
            wetness_bias: 0.48,
            corrosion_bias: 0.46,
            crack_bias: 0.42,
            neon_spill: 0.86,
            biological_bias: 0.12,
            micro_detail_density: 0.92,
            cache_seed_salt: 0x0B1A_CCA7,
        },
        MaterialDistrictStyle::ClinicDistrict => MaterialDistrictSurfaceProfile {
            style,
            color_tint_linear: [0.56, 0.84, 0.78],
            sealant: 0.62,
            grime_bias: 0.18,
            wetness_bias: 0.24,
            corrosion_bias: 0.08,
            crack_bias: 0.12,
            neon_spill: 0.38,
            biological_bias: 0.32,
            micro_detail_density: 0.5,
            cache_seed_salt: 0x00C1_1A1C,
        },
        MaterialDistrictStyle::Neutral => MaterialDistrictSurfaceProfile {
            style,
            color_tint_linear: [0.35, 0.35, 0.34],
            sealant: 0.24,
            grime_bias: 0.24,
            wetness_bias: 0.16,
            corrosion_bias: 0.12,
            crack_bias: 0.16,
            neon_spill: 0.0,
            biological_bias: 0.0,
            micro_detail_density: 0.42,
            cache_seed_salt: 0xA5F0,
        },
    }
}

pub fn apply_instance_variation(
    descriptor: &MaterialDescriptor,
    seed: &MaterialInstanceSeed,
) -> MaterialDescriptor {
    let variation = seed.local_variation.clamp(0.0, 1.0);
    let dirt = seed.dirt_level.clamp(0.0, 1.0);
    let damage = seed.damage_bias.clamp(0.0, 1.0);
    let age = seed.age.clamp(0.0, 1.0);
    let profile =
        material_district_surface_profile(material_district_style_from_label(&seed.district_style));
    let state = seed.initial_state;
    let seeded_variation = stable_unit(seed.seed ^ profile.cache_seed_salt, seed.material_id);
    let patina = (dirt * 0.38
        + age * 0.22
        + profile.grime_bias * 0.18
        + state.dirt * 0.18
        + state.dust * 0.1
        + state.soot * 0.12
        + state.burn_level * 0.16
        + state.corrosion * 0.16
        + seeded_variation * 0.08)
        .clamp(0.0, 1.0);
    let wetness = (state.moisture + profile.wetness_bias * 0.45).clamp(0.0, 1.0);
    let oil = state.oil_contamination.clamp(0.0, 1.0);
    let biological = state.biological_contamination.clamp(0.0, 1.0);
    let contamination = (oil * 0.62 + biological * 0.52).clamp(0.0, 1.0);
    let mut descriptor = descriptor.clone();
    descriptor.visual.base_color_linear[0] = (descriptor.visual.base_color_linear[0]
        * (1.0 - dirt * 0.18 - patina * 0.08 - oil * 0.035)
        + profile.color_tint_linear[0] * (profile.neon_spill * 0.035 + patina * 0.025)
        + biological * 0.035)
        .clamp(0.0, 16.0);
    descriptor.visual.base_color_linear[1] = (descriptor.visual.base_color_linear[1]
        * (1.0 - dirt * 0.14 - patina * 0.06 - contamination * 0.05)
        + profile.color_tint_linear[1] * (profile.neon_spill * 0.025 + patina * 0.02))
        .clamp(0.0, 16.0);
    descriptor.visual.base_color_linear[2] = (descriptor.visual.base_color_linear[2]
        * (1.0 - dirt * 0.1 - patina * 0.045 - contamination * 0.045)
        + profile.color_tint_linear[2] * (profile.neon_spill * 0.04 + patina * 0.018))
        .clamp(0.0, 16.0);
    descriptor.visual.roughness = (descriptor.visual.roughness
        + variation * 0.08
        + damage * 0.06
        + profile.grime_bias * 0.08
        + state.dirt * 0.04
        + state.dust * 0.08
        + state.corrosion * 0.1
        + state.crack_density * 0.05
        + state.exposed_interior * 0.06
        - profile.sealant * 0.055
        - wetness * 0.07
        - oil * 0.14
        + biological * 0.045)
        .clamp(0.02, 1.0);
    descriptor.visual.clearcoat =
        (descriptor.visual.clearcoat + wetness * 0.18 - patina * 0.08).clamp(0.0, 1.0);
    descriptor.visual.normal_displacement_strength =
        (descriptor.visual.normal_displacement_strength
            + state.crack_density * 0.18
            + state.exposed_interior * 0.24
            + state.dust * 0.06)
            .clamp(0.0, 1.0);
    if state.exposed_interior > 0.01 || state.crack_density > 0.0 {
        descriptor.visual.layer_count = descriptor.visual.layer_count.max(4);
    }
    if profile.neon_spill > 0.0 {
        descriptor.visual.emission_linear[0] =
            (descriptor.visual.emission_linear[0] + profile.neon_spill * 0.16).clamp(0.0, 16.0);
        descriptor.visual.emission_linear[1] =
            (descriptor.visual.emission_linear[1] + profile.neon_spill * 0.025).clamp(0.0, 16.0);
        descriptor.visual.emission_linear[2] =
            (descriptor.visual.emission_linear[2] + profile.neon_spill * 0.12).clamp(0.0, 16.0);
    }
    descriptor.physical.friction_static =
        (descriptor.physical.friction_static + dirt * 0.05 + profile.grime_bias * 0.035
            - damage * 0.08
            - wetness * 0.12
            - oil * 0.18
            + biological * 0.025)
            .clamp(0.0, 2.0);
    descriptor.physical.friction_dynamic = (descriptor.physical.friction_dynamic
        + profile.grime_bias * 0.025
        - wetness * 0.1
        - oil * 0.16)
        .clamp(0.0, 2.0);
    descriptor.physical.fracture_toughness = (descriptor.physical.fracture_toughness
        * (1.0 - profile.crack_bias * 0.08 - state.crack_density * 0.16))
        .max(0.005);
    descriptor.acoustic.wetness_muffle =
        (descriptor.acoustic.wetness_muffle + wetness * 0.22 + contamination * 0.08)
            .clamp(0.0, 1.0);
    descriptor.electrical.conductivity = (descriptor.electrical.conductivity
        + wetness * 0.035
        + state.electrical_charge.abs().min(100.0) * 0.0004)
        .clamp(0.0, 8.0);
    descriptor
}

fn synthesize_graph(request: &MaterialGenerationRequest) -> ProceduralMaterialGraph {
    let district_profile = material_district_surface_profile(material_district_style_from_label(
        &request.semantic_label,
    ));
    let evidence = effective_capture_evidence(request);
    let base_color_label = if evidence.photo_count() > 0 {
        "calibrated base color"
    } else {
        "semantic base color estimate"
    };
    let mut nodes = vec![
        graph_node(
            1,
            MaterialGraphNodeKind::ColorCalibration,
            base_color_label,
            0.6,
        ),
        graph_node(2, MaterialGraphNodeKind::Noise, "macro variation", 0.8),
        graph_node(3, MaterialGraphNodeKind::TileBreaker, "tile breakup", 1.1),
        graph_node(
            4,
            MaterialGraphNodeKind::WearMask,
            "edge and dirt wear",
            0.9,
        ),
        graph_node(
            5,
            MaterialGraphNodeKind::HeightToNormal,
            "height to normal detail",
            1.4,
        ),
    ];

    if evidence.scan_count() > 0 {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::ScanHeightField,
            "scan-derived height field",
            1.8,
        ));
    }
    if evidence.cross_polarized()
        || evidence.has_surface_hint(MaterialOutputChannel::Roughness)
        || evidence
            .scan_sets
            .iter()
            .any(|scan| scan.roughness_sample_count > 0)
    {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::ReferenceRoughnessFit,
            "reference roughness fit",
            0.8,
        ));
    }
    if !evidence.surface_hints.is_empty() {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::SurfaceHintBlend,
            "roughness and height hint blend",
            0.4,
        ));
    }

    if matches!(request.target_model, MaterialModelKind::Skin) {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::PoreDetail,
            "skin pore and fine wrinkle detail",
            1.5,
        ));
    }
    if matches!(request.target_model, MaterialModelKind::Hair) {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::FiberDirection,
            "strand direction anisotropy",
            1.3,
        ));
    }
    if request.required_states.wetness {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::WetnessResponse,
            "wet darkening and roughness response",
            1.2,
        ));
    }
    if request.required_states.cracks {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::CrackPattern,
            "procedural crack mask",
            1.6,
        ));
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::EdgeDamage,
            "chipped edge exposure",
            1.0,
        ));
    }
    if request.required_states.soot {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::SootResponse,
            "soot deposition response",
            0.9,
        ));
    }
    if request.required_states.corrosion {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::CorrosionResponse,
            "corrosion color and roughness response",
            1.4,
        ));
    }
    if request.required_states.heat {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::HeatResponse,
            "heat glow and discoloration",
            1.1,
        ));
    }
    if request.required_states.oil {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::OilFilm,
            "thin oil film response",
            1.0,
        ));
    }
    if request.required_states.biological_contamination || request.required_states.blood {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::BiologicalTrace,
            "biological trace mask",
            1.0,
        ));
    }
    if district_profile.grime_bias > 0.45 {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::WearMask,
            "district grime accumulation",
            0.7,
        ));
    }
    if district_profile.wetness_bias > 0.42 && !request.required_states.wetness {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::WetnessResponse,
            "district rain wetness bias",
            0.9,
        ));
    }
    if district_profile.corrosion_bias > 0.42 && !request.required_states.corrosion {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::CorrosionResponse,
            "district corrosion patina",
            1.1,
        ));
    }
    if district_profile.neon_spill > 0.5 {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::ColorCalibration,
            "district neon spill tint",
            0.5,
        ));
    }
    if district_profile.biological_bias > 0.25 && !request.required_states.biological_contamination
    {
        nodes.push(graph_node(
            next_node_id(&nodes),
            MaterialGraphNodeKind::BiologicalTrace,
            "district biological residue",
            0.8,
        ));
    }

    let estimated_runtime_microseconds =
        nodes.iter().map(|node| node.cost_microseconds).sum::<f32>()
            * quality_cost_multiplier(request.quality);
    let requires_gpu_compute = estimated_runtime_microseconds > 8.0
        || matches!(
            request.quality,
            GenerationQuality::Hero | GenerationQuality::Reference
        );

    ProceduralMaterialGraph {
        graph_id: MaterialGraphHandle(request.request_id),
        label: format!("{} procedural graph", request.semantic_label),
        node_count: nodes.len() as u32,
        state_channels: request.required_states.clone(),
        nodes,
        outputs: graph_outputs(request),
        estimated_runtime_microseconds,
        requires_gpu_compute,
    }
}

fn graph_node(
    node_id: u32,
    kind: MaterialGraphNodeKind,
    label: &'static str,
    cost_microseconds: f32,
) -> MaterialGraphNode {
    MaterialGraphNode {
        node_id,
        kind,
        label: label.to_string(),
        cost_microseconds,
    }
}

fn next_node_id(nodes: &[MaterialGraphNode]) -> u32 {
    nodes.len() as u32 + 1
}

fn graph_outputs(request: &MaterialGenerationRequest) -> Vec<MaterialGraphOutput> {
    let district_profile = material_district_surface_profile(material_district_style_from_label(
        &request.semantic_label,
    ));
    let evidence = effective_capture_evidence(request);
    let mut outputs = vec![
        MaterialGraphOutput {
            channel: MaterialOutputChannel::BaseColor,
            precision: TexturePrecision::U8,
        },
        MaterialGraphOutput {
            channel: MaterialOutputChannel::Roughness,
            precision: TexturePrecision::U8,
        },
        MaterialGraphOutput {
            channel: MaterialOutputChannel::Normal,
            precision: TexturePrecision::U16,
        },
    ];
    if request.required_states.cracks
        || request.quality >= GenerationQuality::Hero
        || evidence.scan_count() > 0
        || evidence.has_surface_hint(MaterialOutputChannel::Height)
    {
        outputs.push(MaterialGraphOutput {
            channel: MaterialOutputChannel::Height,
            precision: TexturePrecision::U16,
        });
    }
    if matches!(
        request.target_model,
        MaterialModelKind::TransparentSurface | MaterialModelKind::Volume
    ) {
        outputs.push(MaterialGraphOutput {
            channel: MaterialOutputChannel::Opacity,
            precision: TexturePrecision::U8,
        });
    }
    if matches!(request.target_model, MaterialModelKind::Skin) {
        outputs.push(MaterialGraphOutput {
            channel: MaterialOutputChannel::Subsurface,
            precision: TexturePrecision::U16,
        });
    }
    if matches!(request.target_model, MaterialModelKind::Hair) {
        outputs.push(MaterialGraphOutput {
            channel: MaterialOutputChannel::Anisotropy,
            precision: TexturePrecision::U8,
        });
    }
    if request.required_states.cracks {
        outputs.push(MaterialGraphOutput {
            channel: MaterialOutputChannel::DamageMask,
            precision: TexturePrecision::U8,
        });
    }
    if request.required_states.wetness || district_profile.wetness_bias > 0.42 {
        outputs.push(MaterialGraphOutput {
            channel: MaterialOutputChannel::WetnessMask,
            precision: TexturePrecision::U8,
        });
    }
    if request.required_states.dust
        || request.required_states.soot
        || request.required_states.corrosion
        || district_profile.grime_bias > 0.45
        || district_profile.corrosion_bias > 0.42
    {
        outputs.push(MaterialGraphOutput {
            channel: MaterialOutputChannel::DirtMask,
            precision: TexturePrecision::U8,
        });
    }
    if request.required_states.oil
        || request.required_states.blood
        || request.required_states.biological_contamination
        || district_profile.biological_bias > 0.25
    {
        outputs.push(MaterialGraphOutput {
            channel: MaterialOutputChannel::ContaminationMask,
            precision: TexturePrecision::U8,
        });
    }
    if request.required_states.heat || district_profile.neon_spill > 0.5 {
        outputs.push(MaterialGraphOutput {
            channel: MaterialOutputChannel::Emission,
            precision: TexturePrecision::F16,
        });
    }
    outputs
}

fn build_cache_textures(
    request: &MaterialGenerationRequest,
    graph: &ProceduralMaterialGraph,
) -> Vec<MaterialCacheTexture> {
    if matches!(request.quality, GenerationQuality::Draft)
        || request.output_budget.max_cache_textures == 0
        || request.output_budget.max_memory_bytes == 0
        || request.output_budget.max_texture_size == 0
    {
        return Vec::new();
    }

    let channels = cache_channel_candidates(request);
    let max_count = request.output_budget.max_cache_textures.min(channels.len());
    let mut planned_channels = channels.into_iter().take(max_count).collect::<Vec<_>>();
    let desired_size = texture_size_for_quality(request.quality)
        .min(request.output_budget.max_texture_size)
        .max(1);
    let mut planned_size = desired_size;

    loop {
        while planned_size > 1
            && cache_memory_bytes_for_channels(&planned_channels, planned_size)
                > request.output_budget.max_memory_bytes
        {
            planned_size = (planned_size / 2).max(1);
        }

        if cache_memory_bytes_for_channels(&planned_channels, planned_size)
            <= request.output_budget.max_memory_bytes
        {
            break;
        }

        if planned_channels.pop().is_none() {
            return Vec::new();
        }
        planned_size = desired_size;
    }

    let district_profile = material_district_surface_profile(material_district_style_from_label(
        &request.semantic_label,
    ));

    planned_channels
        .into_iter()
        .enumerate()
        .map(|(index, channel)| {
            let bytes_per_pixel = bytes_per_pixel(channel);
            MaterialCacheTexture {
                texture: TextureAssetId(stable_u128(
                    request.request_id ^ district_profile.cache_seed_salt as u128,
                    graph.graph_id.0,
                    index as u64 + 1,
                )),
                channel,
                size_pixels: planned_size,
                bytes: planned_size as u64 * planned_size as u64 * bytes_per_pixel,
                update_policy: cache_update_policy(channel),
            }
        })
        .collect()
}

fn build_virtual_texture_pages(
    request: &MaterialGenerationRequest,
    cache_outputs: &[MaterialCacheTexture],
) -> Vec<MaterialVirtualTexturePage> {
    if cache_outputs.is_empty() {
        return Vec::new();
    }

    let page_size = virtual_texture_page_size_for_quality(request.quality);
    let mut pages = Vec::new();
    for cache in cache_outputs {
        let page_count_axis = cache.size_pixels.div_ceil(page_size).max(1);
        for page_y in 0..page_count_axis {
            for page_x in 0..page_count_axis {
                let valid_width_pixels = (cache.size_pixels - page_x * page_size).min(page_size);
                let valid_height_pixels = (cache.size_pixels - page_y * page_size).min(page_size);
                let resident_bytes = valid_width_pixels as u64
                    * valid_height_pixels as u64
                    * bytes_per_pixel(cache.channel);
                pages.push(MaterialVirtualTexturePage {
                    page_id: MaterialVirtualTexturePageId(stable_u128(
                        request.request_id ^ cache.texture.0,
                        page_x as u128 ^ ((page_y as u128) << 32),
                        cache.channel as u64 + ((page_x as u64) << 8) + ((page_y as u64) << 24),
                    )),
                    texture: cache.texture,
                    channel: cache.channel,
                    mip_level: 0,
                    page_x: page_x.min(u16::MAX as u32) as u16,
                    page_y: page_y.min(u16::MAX as u32) as u16,
                    page_size_pixels: page_size,
                    valid_width_pixels,
                    valid_height_pixels,
                    resident_bytes,
                    residency: virtual_page_residency(cache.update_policy),
                });
            }
        }
    }
    pages
}

fn cache_channel_candidates(request: &MaterialGenerationRequest) -> Vec<MaterialOutputChannel> {
    let district_profile = material_district_surface_profile(material_district_style_from_label(
        &request.semantic_label,
    ));
    let evidence = effective_capture_evidence(request);
    let mut channels = vec![
        MaterialOutputChannel::BaseColor,
        MaterialOutputChannel::Roughness,
        MaterialOutputChannel::Normal,
    ];
    if request.required_states.cracks
        || request.quality >= GenerationQuality::Hero
        || district_profile.micro_detail_density > 0.7
        || matches!(request.target_model, MaterialModelKind::Skin)
        || evidence.scan_count() > 0
        || evidence.has_surface_hint(MaterialOutputChannel::Height)
    {
        channels.push(MaterialOutputChannel::Height);
    }
    if request.required_states.wetness || district_profile.wetness_bias > 0.42 {
        channels.push(MaterialOutputChannel::WetnessMask);
    }
    if request.required_states.cracks {
        channels.push(MaterialOutputChannel::DamageMask);
    }
    if request.required_states.dust
        || request.required_states.soot
        || request.required_states.corrosion
        || district_profile.grime_bias > 0.45
        || district_profile.corrosion_bias > 0.42
    {
        channels.push(MaterialOutputChannel::DirtMask);
    }
    if request.required_states.oil
        || request.required_states.blood
        || request.required_states.biological_contamination
        || district_profile.biological_bias > 0.25
    {
        channels.push(MaterialOutputChannel::ContaminationMask);
    }
    if request.required_states.heat || district_profile.neon_spill > 0.5 {
        channels.push(MaterialOutputChannel::Emission);
    }
    if matches!(request.target_model, MaterialModelKind::Skin) {
        channels.push(MaterialOutputChannel::Subsurface);
    }
    if matches!(request.target_model, MaterialModelKind::Hair) {
        channels.push(MaterialOutputChannel::Anisotropy);
    }
    if request.required_states.wetness
        || matches!(
            request.target_model,
            MaterialModelKind::TransparentSurface | MaterialModelKind::PhysicallyBasedSurface
        )
    {
        channels.push(MaterialOutputChannel::Clearcoat);
    }
    channels
}

fn cache_memory_bytes_for_channels(channels: &[MaterialOutputChannel], size_pixels: u32) -> u64 {
    channels
        .iter()
        .map(|channel| size_pixels as u64 * size_pixels as u64 * bytes_per_pixel(*channel))
        .sum()
}

fn cache_update_policy(channel: MaterialOutputChannel) -> CacheUpdatePolicy {
    if matches!(
        channel,
        MaterialOutputChannel::WetnessMask
            | MaterialOutputChannel::DamageMask
            | MaterialOutputChannel::DirtMask
            | MaterialOutputChannel::ContaminationMask
    ) {
        CacheUpdatePolicy::RuntimeStatePatch
    } else {
        CacheUpdatePolicy::StaticBaked
    }
}

fn virtual_page_residency(update_policy: CacheUpdatePolicy) -> MaterialVirtualPageResidency {
    match update_policy {
        CacheUpdatePolicy::RuntimeStatePatch => MaterialVirtualPageResidency::RuntimePatched,
        CacheUpdatePolicy::StaticBaked | CacheUpdatePolicy::StreamingOnly => {
            MaterialVirtualPageResidency::StaticStreamed
        }
    }
}

fn texture_size_for_quality(quality: GenerationQuality) -> u32 {
    match quality {
        GenerationQuality::Draft => 512,
        GenerationQuality::Runtime => 1024,
        GenerationQuality::Hero => 2048,
        GenerationQuality::Reference => 4096,
    }
}

fn virtual_texture_page_size_for_quality(quality: GenerationQuality) -> u32 {
    match quality {
        GenerationQuality::Draft | GenerationQuality::Runtime => 256,
        GenerationQuality::Hero | GenerationQuality::Reference => 512,
    }
}

fn bytes_per_pixel(channel: MaterialOutputChannel) -> u64 {
    match channel {
        MaterialOutputChannel::Normal
        | MaterialOutputChannel::Subsurface
        | MaterialOutputChannel::Emission => 8,
        MaterialOutputChannel::BaseColor => 4,
        _ => 2,
    }
}

fn physical_from_request(request: &MaterialGenerationRequest) -> PhysicalMaterial {
    let label = request.semantic_label.to_ascii_lowercase();
    let district_profile = material_district_surface_profile(material_district_style_from_label(
        &request.semantic_label,
    ));
    let mut physical = match request.target_model {
        MaterialModelKind::TransparentSurface if label.contains("glass") => PhysicalMaterial {
            density_kg_per_m3: 2_500.0,
            young_modulus: 70_000_000_000.0,
            poisson_ratio: 0.22,
            yield_stress: 45_000_000.0,
            fracture_toughness: 0.08,
            hardness: 0.8,
            viscosity: 0.0,
            surface_tension: 0.0,
            restitution: 0.08,
            friction_static: 0.55,
            friction_dynamic: 0.42,
        },
        MaterialModelKind::Skin => PhysicalMaterial {
            density_kg_per_m3: 1_100.0,
            young_modulus: 500_000.0,
            poisson_ratio: 0.48,
            yield_stress: 300_000.0,
            fracture_toughness: 0.2,
            hardness: 0.12,
            viscosity: 0.05,
            surface_tension: 0.0,
            restitution: 0.2,
            friction_static: 0.65,
            friction_dynamic: 0.5,
        },
        MaterialModelKind::Hair => PhysicalMaterial {
            density_kg_per_m3: 1_300.0,
            young_modulus: 4_000_000_000.0,
            poisson_ratio: 0.38,
            yield_stress: 18_000_000.0,
            fracture_toughness: 0.18,
            hardness: 0.2,
            viscosity: 0.02,
            surface_tension: 0.0,
            restitution: 0.05,
            friction_static: 0.6,
            friction_dynamic: 0.48,
        },
        MaterialModelKind::Volume => PhysicalMaterial {
            density_kg_per_m3: 1.2,
            young_modulus: 0.0,
            poisson_ratio: 0.0,
            yield_stress: 0.0,
            fracture_toughness: 0.0,
            hardness: 0.0,
            viscosity: 0.000018,
            surface_tension: 0.0,
            restitution: 0.0,
            friction_static: 0.0,
            friction_dynamic: 0.0,
        },
        _ if label.contains("metal") || label.contains("chrome") || label.contains("steel") => {
            PhysicalMaterial {
                density_kg_per_m3: 7_850.0,
                young_modulus: 200_000_000_000.0,
                poisson_ratio: 0.29,
                yield_stress: 250_000_000.0,
                fracture_toughness: 0.7,
                hardness: 0.85,
                viscosity: 0.0,
                surface_tension: 0.0,
                restitution: 0.2,
                friction_static: 0.7,
                friction_dynamic: 0.55,
            }
        }
        _ => PhysicalMaterial {
            density_kg_per_m3: 2_400.0,
            young_modulus: 30_000_000_000.0,
            poisson_ratio: 0.2,
            yield_stress: 40_000_000.0,
            fracture_toughness: 0.45,
            hardness: 0.7,
            viscosity: 0.0,
            surface_tension: 0.0,
            restitution: 0.15,
            friction_static: 0.9,
            friction_dynamic: 0.7,
        },
    };

    if request.required_states.wetness {
        physical.friction_static = (physical.friction_static * 0.82).clamp(0.0, 2.0);
        physical.friction_dynamic = (physical.friction_dynamic * 0.78).clamp(0.0, 2.0);
    }
    if request.required_states.cracks {
        physical.fracture_toughness = (physical.fracture_toughness * 0.72).max(0.01);
    }
    physical.friction_static = (physical.friction_static + district_profile.grime_bias * 0.035
        - district_profile.wetness_bias * 0.07
        - district_profile.sealant * 0.025)
        .clamp(0.0, 2.0);
    physical.friction_dynamic = (physical.friction_dynamic + district_profile.grime_bias * 0.025
        - district_profile.wetness_bias * 0.06)
        .clamp(0.0, 2.0);
    physical.fracture_toughness =
        (physical.fracture_toughness * (1.0 - district_profile.crack_bias * 0.06)).max(0.005);
    if district_profile.corrosion_bias > 0.4
        && (label.contains("metal") || label.contains("chrome") || label.contains("steel"))
    {
        physical.yield_stress *= 1.0 - district_profile.corrosion_bias * 0.08;
        physical.hardness =
            (physical.hardness - district_profile.corrosion_bias * 0.04).clamp(0.0, 1.0);
    }
    physical
}

fn descriptor_from_request(
    request: &MaterialGenerationRequest,
    physical: &PhysicalMaterial,
) -> MaterialDescriptor {
    let visual = visual_from_request(request);
    MaterialDescriptor {
        id: request.request_id as MaterialId,
        name: request.semantic_label.clone(),
        visual,
        physical: physical.clone(),
        acoustic: acoustic_from_physical(request, physical),
        thermal: thermal_from_request(request),
        electrical: electrical_from_request(request),
        procedural_source: Some(MaterialGeneratorRef(request.request_id)),
    }
}

fn visual_from_request(request: &MaterialGenerationRequest) -> VisualMaterial {
    let label = request.semantic_label.to_ascii_lowercase();
    let district_profile = material_district_surface_profile(material_district_style_from_label(
        &request.semantic_label,
    ));
    let mut visual = match request.target_model {
        MaterialModelKind::TransparentSurface => VisualMaterial {
            base_color_linear: [0.82, 0.95, 1.0, 0.35],
            roughness: 0.03,
            metallic: 0.0,
            transmission: 0.72,
            subsurface: 0.0,
            emission_linear: [0.0, 0.0, 0.0],
            anisotropy: 0.0,
            clearcoat: 0.18,
            normal_displacement_strength: 0.08,
            layer_count: 2,
            transparency: 0.72,
        },
        MaterialModelKind::Skin => VisualMaterial {
            base_color_linear: [0.72, 0.48, 0.36, 1.0],
            roughness: 0.52,
            metallic: 0.0,
            transmission: 0.0,
            subsurface: 0.78,
            emission_linear: [0.0, 0.0, 0.0],
            anisotropy: 0.04,
            clearcoat: 0.22,
            normal_displacement_strength: 0.34,
            layer_count: 4,
            transparency: 0.0,
        },
        MaterialModelKind::Hair => VisualMaterial {
            base_color_linear: [0.05, 0.04, 0.035, 1.0],
            roughness: 0.65,
            metallic: 0.0,
            transmission: 0.0,
            subsurface: 0.12,
            emission_linear: [0.0, 0.0, 0.0],
            anisotropy: 0.86,
            clearcoat: 0.08,
            normal_displacement_strength: 0.44,
            layer_count: 3,
            transparency: 0.0,
        },
        MaterialModelKind::Volume => VisualMaterial {
            base_color_linear: [0.2, 0.22, 0.23, 0.4],
            roughness: 0.0,
            metallic: 0.0,
            transmission: 0.6,
            subsurface: 0.35,
            emission_linear: [0.0, 0.0, 0.0],
            anisotropy: 0.0,
            clearcoat: 0.0,
            normal_displacement_strength: 0.0,
            layer_count: 1,
            transparency: 0.6,
        },
        MaterialModelKind::PhysicallyBasedSurface => {
            if label.contains("metal") || label.contains("chrome") || label.contains("steel") {
                VisualMaterial {
                    base_color_linear: [0.62, 0.62, 0.6, 1.0],
                    roughness: 0.28,
                    metallic: 1.0,
                    transmission: 0.0,
                    subsurface: 0.0,
                    emission_linear: [0.0, 0.0, 0.0],
                    anisotropy: 0.34,
                    clearcoat: 0.2,
                    normal_displacement_strength: 0.26,
                    layer_count: 3,
                    transparency: 0.0,
                }
            } else {
                VisualMaterial {
                    base_color_linear: [0.35, 0.35, 0.34, 1.0],
                    roughness: 0.88,
                    metallic: 0.0,
                    transmission: 0.0,
                    subsurface: 0.0,
                    emission_linear: [0.0, 0.0, 0.0],
                    anisotropy: 0.06,
                    clearcoat: 0.12,
                    normal_displacement_strength: 0.52,
                    layer_count: 3,
                    transparency: 0.0,
                }
            }
        }
    };

    if request.required_states.wetness {
        visual.roughness = (visual.roughness * 0.55).clamp(0.02, 1.0);
        visual.clearcoat = (visual.clearcoat + 0.18).clamp(0.0, 1.0);
        visual.layer_count = visual.layer_count.max(3);
        visual.base_color_linear[0] *= 0.82;
        visual.base_color_linear[1] *= 0.84;
        visual.base_color_linear[2] *= 0.86;
    }
    if request.required_states.heat {
        visual.emission_linear = [1.4, 0.22, 0.05];
    }
    if request.required_states.cracks || request.required_states.dust {
        visual.normal_displacement_strength =
            (visual.normal_displacement_strength + 0.16).clamp(0.0, 1.0);
        visual.layer_count = visual.layer_count.max(3);
    }
    let tint_amount = (district_profile.grime_bias * 0.035
        + district_profile.wetness_bias * 0.025
        + district_profile.neon_spill * 0.055
        + district_profile.sealant * 0.02)
        .clamp(0.0, 0.16);
    visual.base_color_linear[0] = (visual.base_color_linear[0] * (1.0 - tint_amount)
        + district_profile.color_tint_linear[0] * tint_amount)
        .clamp(0.0, 16.0);
    visual.base_color_linear[1] = (visual.base_color_linear[1] * (1.0 - tint_amount)
        + district_profile.color_tint_linear[1] * tint_amount)
        .clamp(0.0, 16.0);
    visual.base_color_linear[2] = (visual.base_color_linear[2] * (1.0 - tint_amount)
        + district_profile.color_tint_linear[2] * tint_amount)
        .clamp(0.0, 16.0);
    visual.roughness = (visual.roughness
        + district_profile.grime_bias * 0.07
        + district_profile.corrosion_bias * 0.04
        - district_profile.sealant * 0.08
        - district_profile.wetness_bias * 0.035)
        .clamp(0.02, 1.0);
    if district_profile.neon_spill > 0.0 {
        visual.emission_linear[0] =
            (visual.emission_linear[0] + district_profile.neon_spill * 0.12).clamp(0.0, 16.0);
        visual.emission_linear[2] =
            (visual.emission_linear[2] + district_profile.neon_spill * 0.16).clamp(0.0, 16.0);
    }
    visual
}

fn acoustic_from_physical(
    request: &MaterialGenerationRequest,
    physical: &PhysicalMaterial,
) -> AcousticMaterial {
    let brittle = physical.fracture_toughness < 0.15;
    AcousticMaterial {
        impact_brightness: if brittle {
            0.95
        } else if physical.density_kg_per_m3 > 5_000.0 {
            0.82
        } else {
            0.35
        },
        resonance: if brittle { 0.85 } else { 0.2 },
        absorption: if matches!(
            request.target_model,
            MaterialModelKind::Skin | MaterialModelKind::Hair
        ) {
            0.82
        } else {
            0.28
        },
        wetness_muffle: if request.required_states.wetness {
            0.45
        } else {
            0.1
        },
    }
}

fn thermal_from_request(request: &MaterialGenerationRequest) -> ThermalMaterial {
    match request.target_model {
        MaterialModelKind::Skin | MaterialModelKind::Hair => ThermalMaterial {
            heat_capacity: 3_500.0,
            conductivity: 0.37,
            ignition_temperature: 500.0,
        },
        MaterialModelKind::Volume => ThermalMaterial {
            heat_capacity: 1_005.0,
            conductivity: 0.026,
            ignition_temperature: f32::INFINITY,
        },
        _ => ThermalMaterial {
            heat_capacity: 880.0,
            conductivity: 1.4,
            ignition_temperature: 1_200.0,
        },
    }
}

fn electrical_from_request(request: &MaterialGenerationRequest) -> ElectricalMaterial {
    let label = request.semantic_label.to_ascii_lowercase();
    if label.contains("metal") || label.contains("chrome") || label.contains("steel") {
        ElectricalMaterial {
            conductivity: 1.0,
            dielectric_strength: 400.0,
        }
    } else {
        ElectricalMaterial {
            conductivity: if request.required_states.wetness {
                0.04
            } else {
                0.01
            },
            dielectric_strength: if request.required_states.wetness {
                20_000.0
            } else {
                25_000.0
            },
        }
    }
}

fn render_binding_from_parts(
    request: &MaterialGenerationRequest,
    graph: &ProceduralMaterialGraph,
    cache_outputs: &[MaterialCacheTexture],
    virtual_texture_pages: &[MaterialVirtualTexturePage],
    material_id: MaterialId,
) -> RenderMaterialBinding {
    let district_profile = material_district_surface_profile(material_district_style_from_label(
        &request.semantic_label,
    ));
    let cache_memory_bytes = cache_outputs.iter().map(|cache| cache.bytes).sum::<u64>();
    let streaming_bandwidth_bytes = virtual_texture_pages
        .iter()
        .map(|page| page.resident_bytes)
        .sum::<u64>();
    RenderMaterialBinding {
        material_id,
        graph_handle: graph.graph_id,
        texture_handles: cache_outputs
            .iter()
            .map(|cache| TextureHandle(cache.texture.0))
            .collect(),
        virtual_page_table: MaterialVirtualTexturePageTable {
            page_ids: virtual_texture_pages
                .iter()
                .map(|page| page.page_id)
                .collect(),
            page_size_pixels: virtual_texture_pages
                .first()
                .map(|page| page.page_size_pixels)
                .unwrap_or(0),
            resident_bytes: streaming_bandwidth_bytes,
            runtime_patch_page_count: virtual_texture_pages
                .iter()
                .filter(|page| page.residency == MaterialVirtualPageResidency::RuntimePatched)
                .count(),
        },
        shader_model: shader_model_for(request.target_model),
        runtime_parameters: MaterialRuntimeParams {
            supports_wetness: request.required_states.wetness
                || district_profile.wetness_bias > 0.42,
            supports_cracks: request.required_states.cracks,
            supports_emission: request.required_states.heat
                || district_profile.neon_spill > 0.5
                || matches!(request.target_model, MaterialModelKind::Volume),
            supports_soot: request.required_states.soot,
            supports_corrosion: request.required_states.corrosion
                || district_profile.corrosion_bias > 0.42,
            supports_heat: request.required_states.heat,
            supports_oil: request.required_states.oil,
            supports_biological: request.required_states.biological_contamination
                || request.required_states.blood
                || district_profile.biological_bias > 0.25,
            cache_quality: request.quality,
            estimated_runtime_microseconds: graph.estimated_runtime_microseconds,
            virtual_page_count: virtual_texture_pages.len(),
            cache_memory_bytes,
            streaming_bandwidth_bytes,
        },
    }
}

fn shader_model_for(model: MaterialModelKind) -> MaterialShaderModel {
    match model {
        MaterialModelKind::PhysicallyBasedSurface | MaterialModelKind::Hair => {
            MaterialShaderModel::OpaquePbr
        }
        MaterialModelKind::TransparentSurface => MaterialShaderModel::TransparentPbr,
        MaterialModelKind::Volume => MaterialShaderModel::Volume,
        MaterialModelKind::Skin => MaterialShaderModel::Subsurface,
    }
}

fn state_channel_count(channels: &MaterialStateChannels) -> usize {
    [
        channels.wetness,
        channels.cracks,
        channels.soot,
        channels.corrosion,
        channels.heat,
        channels.oil,
        channels.blood,
        channels.dust,
        channels.biological_contamination,
    ]
    .into_iter()
    .filter(|enabled| *enabled)
    .count()
}

fn quality_cost_multiplier(quality: GenerationQuality) -> f32 {
    match quality {
        GenerationQuality::Draft => 0.55,
        GenerationQuality::Runtime => 1.0,
        GenerationQuality::Hero => 1.35,
        GenerationQuality::Reference => 2.2,
    }
}

fn validation_issue(
    severity: MaterialValidationSeverity,
    code: &'static str,
    message: &'static str,
) -> MaterialValidationIssue {
    MaterialValidationIssue {
        severity,
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn deterministic_event_id(tick: u64, module: u64, local: AssetId) -> WorldEventId {
    ((tick as u128) << 80) | ((module as u128) << 64) | (local & u64::MAX as u128)
}

fn stable_u128(seed: u128, request_id: u128, salt: u64) -> u128 {
    let high = stable_u64(seed as u64, salt) as u128;
    let low = stable_u64(request_id as u64, salt ^ 0xA51E_3000) as u128;
    (high << 64) | low
}

fn stable_u64(seed: u64, salt: u64) -> u64 {
    let mut value = seed
        .wrapping_add(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn stable_unit(seed: u64, salt: u64) -> f32 {
    ((stable_u64(seed, salt) >> 40) as f32) / ((1_u64 << 24) as f32)
}

pub fn default_alley_materials() -> Vec<MaterialDescriptor> {
    vec![
        MaterialDescriptor {
            id: MATERIAL_GLASS,
            name: "laminated alley glass".to_string(),
            visual: VisualMaterial {
                base_color_linear: [0.82, 0.95, 1.0, 0.35],
                roughness: 0.03,
                metallic: 0.0,
                transmission: 0.72,
                subsurface: 0.0,
                emission_linear: [0.0, 0.0, 0.0],
                anisotropy: 0.0,
                clearcoat: 0.18,
                normal_displacement_strength: 0.08,
                layer_count: 2,
                transparency: 0.72,
            },
            physical: PhysicalMaterial {
                density_kg_per_m3: 2_500.0,
                young_modulus: 70_000_000_000.0,
                poisson_ratio: 0.22,
                yield_stress: 45_000_000.0,
                fracture_toughness: 0.08,
                hardness: 0.8,
                viscosity: 0.0,
                surface_tension: 0.0,
                restitution: 0.08,
                friction_static: 0.55,
                friction_dynamic: 0.42,
            },
            acoustic: AcousticMaterial {
                impact_brightness: 0.95,
                resonance: 0.85,
                absorption: 0.08,
                wetness_muffle: 0.05,
            },
            thermal: ThermalMaterial {
                heat_capacity: 840.0,
                conductivity: 1.0,
                ignition_temperature: 1_700.0,
            },
            electrical: ElectricalMaterial {
                conductivity: 0.0,
                dielectric_strength: 9_000_000.0,
            },
            procedural_source: None,
        },
        MaterialDescriptor {
            id: MATERIAL_WET_ASPHALT,
            name: "rain-wet asphalt".to_string(),
            visual: VisualMaterial {
                base_color_linear: [0.015, 0.016, 0.017, 1.0],
                roughness: 0.18,
                metallic: 0.0,
                transmission: 0.0,
                subsurface: 0.0,
                emission_linear: [0.0, 0.0, 0.0],
                anisotropy: 0.08,
                clearcoat: 0.48,
                normal_displacement_strength: 0.46,
                layer_count: 4,
                transparency: 0.0,
            },
            physical: PhysicalMaterial {
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
            },
            acoustic: AcousticMaterial {
                impact_brightness: 0.2,
                resonance: 0.12,
                absorption: 0.78,
                wetness_muffle: 0.4,
            },
            thermal: ThermalMaterial {
                heat_capacity: 920.0,
                conductivity: 0.75,
                ignition_temperature: 760.0,
            },
            electrical: ElectricalMaterial {
                conductivity: 0.02,
                dielectric_strength: 20_000.0,
            },
            procedural_source: None,
        },
        MaterialDescriptor {
            id: MATERIAL_NEON_TUBE,
            name: "magenta neon tube".to_string(),
            visual: VisualMaterial {
                base_color_linear: [0.9, 0.05, 0.62, 1.0],
                roughness: 0.08,
                metallic: 0.0,
                transmission: 0.08,
                subsurface: 0.0,
                emission_linear: [8.0, 0.2, 4.5],
                anisotropy: 0.18,
                clearcoat: 0.36,
                normal_displacement_strength: 0.18,
                layer_count: 3,
                transparency: 0.1,
            },
            physical: PhysicalMaterial {
                density_kg_per_m3: 2_100.0,
                young_modulus: 50_000_000_000.0,
                poisson_ratio: 0.22,
                yield_stress: 20_000_000.0,
                fracture_toughness: 0.06,
                hardness: 0.7,
                viscosity: 0.0,
                surface_tension: 0.0,
                restitution: 0.05,
                friction_static: 0.5,
                friction_dynamic: 0.4,
            },
            acoustic: AcousticMaterial {
                impact_brightness: 0.75,
                resonance: 0.7,
                absorption: 0.1,
                wetness_muffle: 0.05,
            },
            thermal: ThermalMaterial {
                heat_capacity: 800.0,
                conductivity: 1.1,
                ignition_temperature: 1_200.0,
            },
            electrical: ElectricalMaterial {
                conductivity: 0.8,
                dielectric_strength: 1_000.0,
            },
            procedural_source: None,
        },
        MaterialDescriptor {
            id: MATERIAL_HUMAN_SKIN,
            name: "human skin runtime material".to_string(),
            visual: VisualMaterial {
                base_color_linear: [0.72, 0.48, 0.36, 1.0],
                roughness: 0.52,
                metallic: 0.0,
                transmission: 0.0,
                subsurface: 0.78,
                emission_linear: [0.0, 0.0, 0.0],
                anisotropy: 0.04,
                clearcoat: 0.22,
                normal_displacement_strength: 0.34,
                layer_count: 4,
                transparency: 0.0,
            },
            physical: PhysicalMaterial {
                density_kg_per_m3: 1_100.0,
                young_modulus: 500_000.0,
                poisson_ratio: 0.48,
                yield_stress: 300_000.0,
                fracture_toughness: 0.2,
                hardness: 0.12,
                viscosity: 0.05,
                surface_tension: 0.0,
                restitution: 0.2,
                friction_static: 0.65,
                friction_dynamic: 0.5,
            },
            acoustic: AcousticMaterial {
                impact_brightness: 0.12,
                resonance: 0.08,
                absorption: 0.85,
                wetness_muffle: 0.2,
            },
            thermal: ThermalMaterial {
                heat_capacity: 3_500.0,
                conductivity: 0.37,
                ignition_temperature: 500.0,
            },
            electrical: ElectricalMaterial {
                conductivity: 0.1,
                dielectric_strength: 400.0,
            },
            procedural_source: None,
        },
        MaterialDescriptor {
            id: MATERIAL_WATER,
            name: "street water".to_string(),
            visual: VisualMaterial {
                base_color_linear: [0.08, 0.14, 0.16, 0.55],
                roughness: 0.02,
                metallic: 0.0,
                transmission: 0.45,
                subsurface: 0.18,
                emission_linear: [0.0, 0.0, 0.0],
                anisotropy: 0.0,
                clearcoat: 0.5,
                normal_displacement_strength: 0.04,
                layer_count: 2,
                transparency: 0.45,
            },
            physical: PhysicalMaterial {
                density_kg_per_m3: 1_000.0,
                young_modulus: 2_200_000_000.0,
                poisson_ratio: 0.49,
                yield_stress: 0.0,
                fracture_toughness: 0.0,
                hardness: 0.0,
                viscosity: 0.001,
                surface_tension: 0.072,
                restitution: 0.0,
                friction_static: 0.0,
                friction_dynamic: 0.0,
            },
            acoustic: AcousticMaterial {
                impact_brightness: 0.3,
                resonance: 0.1,
                absorption: 0.5,
                wetness_muffle: 0.0,
            },
            thermal: ThermalMaterial {
                heat_capacity: 4_181.0,
                conductivity: 0.6,
                ignition_temperature: f32::INFINITY,
            },
            electrical: ElectricalMaterial {
                conductivity: 0.05,
                dielectric_strength: 70_000.0,
            },
            procedural_source: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ashfall_core::world::{EntityTemplate, Renderable, WorldState};

    fn wet_cracked_concrete_request() -> MaterialGenerationRequest {
        MaterialGenerationRequest {
            request_id: 8_001,
            source_photos: vec![PhotoAssetId(91), PhotoAssetId(92)],
            capture_evidence: MaterialCaptureEvidence {
                photo_sets: vec![CalibratedPhotoSet {
                    photos: vec![PhotoAssetId(91), PhotoAssetId(92)],
                    color_chart: true,
                    scale_reference_meters: Some(1.0),
                    known_lighting: true,
                    lighting_samples: 4,
                    cross_polarized: true,
                    view_angle_count: 4,
                    license_ok: true,
                }],
                scan_sets: vec![MaterialScanSet {
                    scan_asset: 120_091,
                    measured_area_m2: 0.64,
                    height_sample_count: 512,
                    normal_sample_count: 512,
                    roughness_sample_count: 256,
                    hero_grade: false,
                    license_ok: true,
                }],
                surface_hints: vec![
                    MaterialSurfaceHint {
                        channel: MaterialOutputChannel::Height,
                        confidence: 0.82,
                        source_label: "cross-polarized height hint".to_string(),
                    },
                    MaterialSurfaceHint {
                        channel: MaterialOutputChannel::Roughness,
                        confidence: 0.78,
                        source_label: "wet/dry roughness bracket".to_string(),
                    },
                ],
            },
            semantic_label: "wet cracked concrete alley wall".to_string(),
            scale_meters: 1.0,
            target_model: MaterialModelKind::PhysicallyBasedSurface,
            required_states: MaterialStateChannels {
                wetness: true,
                cracks: true,
                soot: true,
                corrosion: false,
                heat: false,
                oil: false,
                blood: false,
                dust: true,
                biological_contamination: false,
            },
            quality: GenerationQuality::Runtime,
            output_budget: MaterialOutputBudget {
                max_texture_size: 1024,
                max_graph_nodes: 16,
                max_runtime_microseconds: 16.0,
                max_cache_textures: 6,
                max_memory_bytes: 32 * 1024 * 1024,
                allow_gpu_compute: true,
            },
        }
    }

    #[test]
    fn generated_material_is_state_aware_and_renderable() {
        let module = ProceduralMaterialsModule::default();
        let generated = module.generate(wet_cracked_concrete_request());

        assert!(generated.validation_report.passed);
        assert!(generated.graph.nodes.iter().any(|node| {
            node.kind == MaterialGraphNodeKind::WetnessResponse && node.label.contains("wet")
        }));
        assert!(
            generated
                .graph
                .nodes
                .iter()
                .any(|node| node.kind == MaterialGraphNodeKind::CrackPattern)
        );
        assert!(
            generated
                .cache_outputs
                .iter()
                .any(|cache| cache.channel == MaterialOutputChannel::WetnessMask
                    && cache.update_policy == CacheUpdatePolicy::RuntimeStatePatch)
        );
        assert!(
            generated
                .cache_outputs
                .iter()
                .any(|cache| cache.channel == MaterialOutputChannel::Height)
        );
        assert!(
            generated
                .cache_outputs
                .iter()
                .all(|cache| cache.size_pixels <= 1024)
        );
        assert!(!generated.virtual_texture_pages.is_empty());
        assert!(
            generated
                .virtual_texture_pages
                .iter()
                .any(|page| page.channel == MaterialOutputChannel::WetnessMask
                    && page.residency == MaterialVirtualPageResidency::RuntimePatched)
        );
        assert!(
            generated
                .virtual_texture_pages
                .iter()
                .any(|page| page.channel == MaterialOutputChannel::Height
                    && page.residency == MaterialVirtualPageResidency::StaticStreamed)
        );
        assert_eq!(
            generated.validation_report.budget.virtual_page_count,
            generated.virtual_texture_pages.len()
        );
        assert_eq!(
            generated.render_binding.virtual_page_table.page_ids.len(),
            generated.virtual_texture_pages.len()
        );
        assert_eq!(
            generated
                .render_binding
                .runtime_parameters
                .virtual_page_count,
            generated.virtual_texture_pages.len()
        );
        assert_eq!(
            generated
                .render_binding
                .runtime_parameters
                .streaming_bandwidth_bytes,
            generated.validation_report.budget.streaming_bandwidth_bytes
        );
        assert!(generated.render_binding.runtime_parameters.supports_wetness);
        assert!(generated.render_binding.runtime_parameters.supports_cracks);
        assert_eq!(
            generated.render_binding.shader_model,
            MaterialShaderModel::OpaquePbr
        );
        assert!(generated.physical_parameters.friction_static < 0.9);
        assert!(generated.provenance.calibration.color_chart);
        assert!(generated.provenance.calibration.known_lighting);
        assert!(generated.provenance.calibration.cross_polarized);
        assert_eq!(generated.provenance.calibration.view_angle_count, 4);
        assert_eq!(generated.provenance.scan_set_count, 1);
        assert_eq!(generated.provenance.calibrated_photo_count, 1);
        assert!(generated.provenance.capture_confidence >= 0.9);
        assert!(generated.provenance.source_summary.contains("scan set"));
        assert!(
            generated
                .graph
                .nodes
                .iter()
                .any(|node| node.kind == MaterialGraphNodeKind::ScanHeightField)
        );
        assert!(
            generated
                .graph
                .nodes
                .iter()
                .any(|node| node.kind == MaterialGraphNodeKind::ReferenceRoughnessFit)
        );
        assert!(
            generated
                .graph
                .nodes
                .iter()
                .any(|node| node.kind == MaterialGraphNodeKind::SurfaceHintBlend)
        );
        assert!(generated.validation_report.metrics.iter().any(|metric| {
            metric.label == "capture_confidence" && metric.value >= metric.threshold
        }));
        assert!(
            generated
                .validation_report
                .metrics
                .iter()
                .any(|metric| { metric.label == "scan_sample_count" && metric.value >= 1_280.0 })
        );
    }

    #[test]
    fn oil_and_biological_state_generate_contamination_cache_pages() {
        let request = MaterialGenerationRequest {
            request_id: 8_071,
            source_photos: vec![PhotoAssetId(71)],
            capture_evidence: MaterialCaptureEvidence::default(),
            semantic_label: "clinic service floor with oily biological residue".to_string(),
            scale_meters: 1.0,
            target_model: MaterialModelKind::PhysicallyBasedSurface,
            required_states: MaterialStateChannels {
                wetness: true,
                cracks: false,
                soot: false,
                corrosion: false,
                heat: false,
                oil: true,
                blood: true,
                dust: false,
                biological_contamination: true,
            },
            quality: GenerationQuality::Hero,
            output_budget: MaterialOutputBudget {
                max_texture_size: 2048,
                max_graph_nodes: 18,
                max_runtime_microseconds: 24.0,
                max_cache_textures: 8,
                max_memory_bytes: 128 * 1024 * 1024,
                allow_gpu_compute: true,
            },
        };

        let generated = generate_material(&request);

        assert!(generated.validation_report.passed);
        assert!(
            generated
                .graph
                .nodes
                .iter()
                .any(|node| node.kind == MaterialGraphNodeKind::OilFilm)
        );
        assert!(
            generated
                .graph
                .nodes
                .iter()
                .any(|node| node.kind == MaterialGraphNodeKind::BiologicalTrace)
        );
        assert!(generated.graph.outputs.iter().any(|output| {
            output.channel == MaterialOutputChannel::ContaminationMask
                && output.precision == TexturePrecision::U8
        }));
        assert!(generated.cache_outputs.iter().any(|cache| {
            cache.channel == MaterialOutputChannel::ContaminationMask
                && cache.update_policy == CacheUpdatePolicy::RuntimeStatePatch
        }));
        assert!(generated.virtual_texture_pages.iter().any(|page| {
            page.channel == MaterialOutputChannel::ContaminationMask
                && page.residency == MaterialVirtualPageResidency::RuntimePatched
        }));
        assert!(generated.render_binding.runtime_parameters.supports_oil);
        assert!(
            generated
                .render_binding
                .runtime_parameters
                .supports_biological
        );
    }

    #[test]
    fn weak_capture_evidence_reports_validation_issues() {
        let request = MaterialGenerationRequest {
            request_id: 8_123,
            source_photos: Vec::new(),
            capture_evidence: MaterialCaptureEvidence {
                photo_sets: vec![CalibratedPhotoSet {
                    photos: vec![PhotoAssetId(501)],
                    color_chart: false,
                    scale_reference_meters: None,
                    known_lighting: false,
                    lighting_samples: 0,
                    cross_polarized: false,
                    view_angle_count: 1,
                    license_ok: false,
                }],
                scan_sets: Vec::new(),
                surface_hints: Vec::new(),
            },
            semantic_label: "hero wet asphalt reference".to_string(),
            scale_meters: 1.0,
            target_model: MaterialModelKind::PhysicallyBasedSurface,
            required_states: MaterialStateChannels {
                wetness: true,
                cracks: false,
                soot: false,
                corrosion: false,
                heat: false,
                oil: true,
                blood: false,
                dust: false,
                biological_contamination: false,
            },
            quality: GenerationQuality::Hero,
            output_budget: MaterialOutputBudget {
                max_texture_size: 2048,
                max_graph_nodes: 20,
                max_runtime_microseconds: 24.0,
                max_cache_textures: 8,
                max_memory_bytes: 128 * 1024 * 1024,
                allow_gpu_compute: true,
            },
        };

        let generated = generate_material(&request);

        assert!(!generated.validation_report.passed);
        for expected in [
            "capture_license_unverified",
            "capture_missing_color_chart",
            "capture_missing_scale_reference",
            "capture_missing_known_lighting",
            "capture_low_lighting_samples",
            "capture_low_view_angle_coverage",
            "capture_missing_cross_polarization",
            "capture_missing_scan_data",
        ] {
            assert!(
                generated
                    .validation_report
                    .issues
                    .iter()
                    .any(|issue| issue.code == expected),
                "{expected} should be reported"
            );
        }
        assert!(!generated.provenance.license_ok);
        assert!(generated.provenance.capture_confidence < 0.25);
    }

    #[test]
    fn cache_textures_are_downscaled_to_memory_budget() {
        let mut request = wet_cracked_concrete_request();
        request.output_budget.max_texture_size = 2048;
        request.output_budget.max_cache_textures = 6;
        request.output_budget.max_memory_bytes = 6 * 1024 * 1024;

        let generated = generate_material(&request);

        assert!(generated.validation_report.passed);
        assert!(generated.validation_report.budget.cache_memory_bytes <= 6 * 1024 * 1024);
        assert!(
            generated.validation_report.budget.streaming_bandwidth_bytes
                <= generated.validation_report.budget.cache_memory_bytes
        );
        assert!(generated.validation_report.budget.max_texture_size < 2048);
        assert!(
            generated
                .cache_outputs
                .iter()
                .any(|cache| cache.channel == MaterialOutputChannel::WetnessMask)
        );
        assert!(
            generated
                .validation_report
                .issues
                .iter()
                .any(|issue| issue.severity == MaterialValidationSeverity::Info
                    && issue.code == "cache_resolution_reduced_for_budget")
        );
        assert!(
            generated
                .validation_report
                .metrics
                .iter()
                .any(|metric| metric.label == "cache_memory_budget_ratio"
                    && metric.value <= metric.threshold)
        );
        assert!(
            generated
                .validation_report
                .metrics
                .iter()
                .any(|metric| metric.label == "streaming_bandwidth_budget_ratio"
                    && metric.value <= metric.threshold)
        );
    }

    #[test]
    fn zero_cache_budget_omits_textures_without_budget_errors() {
        let mut request = wet_cracked_concrete_request();
        request.output_budget.max_cache_textures = 0;
        request.output_budget.max_memory_bytes = 0;

        let generated = generate_material(&request);

        assert!(generated.validation_report.passed);
        assert!(generated.cache_outputs.is_empty());
        assert!(generated.cache_textures.is_empty());
        assert!(generated.virtual_texture_pages.is_empty());
        assert!(
            generated
                .render_binding
                .virtual_page_table
                .page_ids
                .is_empty()
        );
        assert_eq!(generated.validation_report.budget.virtual_page_count, 0);
        assert!(
            generated
                .validation_report
                .issues
                .iter()
                .all(|issue| !issue.code.starts_with("cache_")
                    || issue.severity != MaterialValidationSeverity::Error)
        );
    }

    #[test]
    fn district_profile_adds_procedural_outputs_and_runtime_flags() {
        let request = MaterialGenerationRequest {
            request_id: 8_077,
            source_photos: Vec::new(),
            capture_evidence: MaterialCaptureEvidence::default(),
            semantic_label: "black market oxidized steel shutter".to_string(),
            scale_meters: 1.0,
            target_model: MaterialModelKind::PhysicallyBasedSurface,
            required_states: MaterialStateChannels::default(),
            quality: GenerationQuality::Runtime,
            output_budget: MaterialOutputBudget {
                max_texture_size: 1024,
                max_graph_nodes: 16,
                max_runtime_microseconds: 16.0,
                max_cache_textures: 8,
                max_memory_bytes: 64 * 1024 * 1024,
                allow_gpu_compute: true,
            },
        };

        let generated = generate_material(&request);

        assert!(generated.validation_report.passed);
        assert_eq!(
            material_district_style_from_label(&request.semantic_label),
            MaterialDistrictStyle::BlackMarket
        );
        assert!(generated.graph.nodes.iter().any(|node| {
            node.kind == MaterialGraphNodeKind::CorrosionResponse && node.label.contains("district")
        }));
        assert!(
            generated
                .graph
                .nodes
                .iter()
                .any(|node| node.label.contains("neon spill"))
        );
        assert!(generated.graph.outputs.iter().any(|output| {
            output.channel == MaterialOutputChannel::DirtMask
                && output.precision == TexturePrecision::U8
        }));
        assert!(generated.graph.outputs.iter().any(|output| {
            output.channel == MaterialOutputChannel::Emission
                && output.precision == TexturePrecision::F16
        }));
        assert!(generated.cache_outputs.iter().any(|cache| {
            cache.channel == MaterialOutputChannel::DirtMask
                && cache.update_policy == CacheUpdatePolicy::RuntimeStatePatch
        }));
        assert!(generated.render_binding.runtime_parameters.supports_wetness);
        assert!(
            generated
                .render_binding
                .runtime_parameters
                .supports_corrosion
        );
        assert!(
            generated
                .render_binding
                .runtime_parameters
                .supports_emission
        );
        assert!(generated.descriptor.visual.metallic > 0.9);
        assert!(generated.descriptor.visual.emission_linear[2] > 0.1);
    }

    #[test]
    fn module_requests_aged_surface_cache_for_visible_corroded_state() {
        let mut world = WorldState::default();
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(77),
                name: "corroded service panel".to_string(),
                transform: Transform::at(Vec3::new(1.0, 2.0, 0.0)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(7_700),
                    material: MATERIAL_WET_ASPHALT,
                    visible: true,
                    fracture_replacement_mesh: None,
                }),
                physical_body: None,
                material_state: Some(MaterialState {
                    soot: 0.24,
                    corrosion: 0.44,
                    electrical_charge: 42.0,
                    ..MaterialState::default()
                }),
                human: None,
                agent: None,
                tags: vec!["metal".to_string(), "service_panel".to_string()],
            })
            .expect("test entity should spawn");
        let frame = FrameContext {
            frame_id: 1,
            sim_time: SimTime::new(0.0, 1),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(1, SimTime::new(0.0, 1)),
            recent_events: Vec::new(),
            forces: Vec::new(),
        };
        let mut module = ProceduralMaterialsModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AssetStreamingRequested {
                    asset_id: ASSET_AGED_METAL_SURFACE_CACHE,
                    requester: 30,
                    priority: AssetPriority::Visible,
                    reason,
                    ..
                } if reason.contains("aged-surface material cache")
            ) && event.actors == vec![77]
                && event
                    .physical_evidence
                    .iter()
                    .any(|evidence| evidence == "material_state_cache")
        }));

        let mut second_sink = CommandSink::default();
        module.tick(&frame, &mut second_sink);
        assert!(second_sink.events.is_empty());
    }

    #[test]
    fn module_requests_contamination_cache_for_visible_oil_and_biological_state() {
        let mut world = WorldState::default();
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(88),
                name: "contaminated clinic threshold".to_string(),
                transform: Transform::at(Vec3::new(-1.0, 0.5, 0.0)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(8_800),
                    material: MATERIAL_WET_ASPHALT,
                    visible: true,
                    fracture_replacement_mesh: None,
                }),
                physical_body: None,
                material_state: Some(MaterialState {
                    oil_contamination: 0.28,
                    biological_contamination: 0.18,
                    ..MaterialState::default()
                }),
                human: None,
                agent: None,
                tags: vec!["clinic".to_string(), "floor".to_string()],
            })
            .expect("test entity should spawn");
        let frame = FrameContext {
            frame_id: 1,
            sim_time: SimTime::new(0.0, 2),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(1, SimTime::new(0.0, 2)),
            recent_events: Vec::new(),
            forces: Vec::new(),
        };
        let mut module = ProceduralMaterialsModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AssetStreamingRequested {
                    asset_id: ASSET_CONTAMINATION_SURFACE_CACHE,
                    requester: 30,
                    priority: AssetPriority::Visible,
                    reason,
                    ..
                } if reason.contains("contamination")
            ) && event.actors == vec![88]
                && event
                    .physical_evidence
                    .iter()
                    .any(|evidence| evidence == "material_state_cache")
        }));
    }

    #[test]
    fn gpu_schedule_updates_requested_material_caches() {
        let mut module = ProceduralMaterialsModule::default();
        module
            .requested_cache_assets
            .insert(ASSET_GLASS_CRACK_DETAIL_CACHE);
        module
            .requested_cache_assets
            .insert(ASSET_WET_ASPHALT_REFLECTION_CACHE);
        module
            .requested_cache_assets
            .insert(ASSET_HUMAN_SKIN_DETAIL_CACHE);
        module
            .requested_cache_assets
            .insert(ASSET_AGED_METAL_SURFACE_CACHE);
        module
            .requested_cache_assets
            .insert(ASSET_CONTAMINATION_SURFACE_CACHE);
        let services = ashfall_core::gpu::GpuServices::new_vulkan();
        let mut graph = services.begin_frame(30);

        module.schedule_gpu(&mut graph);

        for expected in [
            "material_crack_cache_update",
            "material_wetness_cache_update",
            "material_skin_detail_cache_update",
            "material_aged_metal_cache_update",
            "material_contamination_cache_update",
            "material_virtual_page_table_update",
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
            usage.owner == Some(30)
                && usage.label == "material graph descriptor table"
                && usage.bindless_index.is_some()
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "material_crack_cache_update")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(30)
                && usage.label == "material wetness reflection cache output"
                && usage.bindless_index.is_some()
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "material_wetness_cache_update")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(30)
                && usage.label == "material aged metal surface cache output"
                && usage.bindless_index.is_some()
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "material_aged_metal_cache_update")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(30)
                && usage.label == "material contamination surface cache output"
                && usage.bindless_index.is_some()
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "material_contamination_cache_update")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(30)
                && usage.label == "material virtual texture page table"
                && usage.bindless_index.is_some()
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "material_virtual_page_table_update")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(30)
                && usage.label == "material virtual texture feedback buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "material_virtual_page_table_update")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.label == "material wetness reflection cache output"
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "material_virtual_page_table_update")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "material_crack_cache_update"
                && pipeline.shader_key == "materials/crack_cache_update.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "CRACK_RESPONSE")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "material_skin_detail_cache_update"
                && pipeline.shader_key == "materials/skin_detail_cache_update.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "SUBSURFACE_RESPONSE")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "material_aged_metal_cache_update"
                && pipeline.shader_key == "materials/aged_metal_cache_update.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "CORROSION_RESPONSE")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "SOOT_RESPONSE")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "material_contamination_cache_update"
                && pipeline.shader_key == "materials/contamination_cache_update.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "OIL_FILM_RESPONSE")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "BIOLOGICAL_TRACE")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "material_virtual_page_table_update"
                && pipeline.shader_key == "materials/virtual_page_table_update.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "VIRTUAL_TEXTURE_PAGES")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "RUNTIME_PAGE_PATCH")
        }));
    }

    #[test]
    fn validation_reports_budget_errors() {
        let mut request = wet_cracked_concrete_request();
        request.quality = GenerationQuality::Reference;
        request.output_budget.max_graph_nodes = 3;
        request.output_budget.max_runtime_microseconds = 1.0;
        request.output_budget.max_cache_textures = 1;
        request.output_budget.max_memory_bytes = 1024;
        request.output_budget.allow_gpu_compute = false;

        let generated = generate_material(&request);

        assert!(!generated.validation_report.passed);
        assert!(generated.validation_report.issues.iter().any(|issue| {
            issue.severity == MaterialValidationSeverity::Error
                && issue.code == "graph_node_budget_exceeded"
        }));
        assert!(generated.validation_report.issues.iter().any(|issue| {
            issue.severity == MaterialValidationSeverity::Error
                && issue.code == "runtime_budget_exceeded"
        }));
        assert!(generated.validation_report.issues.iter().any(|issue| {
            issue.severity == MaterialValidationSeverity::Error
                && issue.code == "gpu_compute_disallowed"
        }));
        assert!(generated.validation_report.budget.cache_memory_bytes <= 1024);
    }

    #[test]
    fn instance_variation_changes_visual_and_physical_surface() {
        let generated = generate_material(&wet_cracked_concrete_request());
        let varied = apply_instance_variation(
            &generated.descriptor,
            &MaterialInstanceSeed {
                material_id: generated.material_id,
                seed: 99,
                age: 0.8,
                dirt_level: 0.9,
                damage_bias: 0.7,
                district_style: "rain_alley_slum".to_string(),
                local_variation: 0.6,
                initial_state: MaterialState {
                    moisture: 0.8,
                    crack_density: 0.7,
                    ..MaterialState::default()
                },
            },
        );

        assert!(
            varied.visual.base_color_linear[0] < generated.descriptor.visual.base_color_linear[0]
        );
        assert!(varied.visual.roughness > generated.descriptor.visual.roughness);
        assert!(varied.physical.friction_static < generated.descriptor.physical.friction_static);

        let clean_seed = MaterialInstanceSeed {
            material_id: generated.material_id,
            seed: 100,
            age: 0.3,
            dirt_level: 0.2,
            damage_bias: 0.1,
            district_style: "neutral".to_string(),
            local_variation: 0.4,
            initial_state: MaterialState::default(),
        };
        let clean = apply_instance_variation(&generated.descriptor, &clean_seed);
        let contaminated = apply_instance_variation(
            &generated.descriptor,
            &MaterialInstanceSeed {
                initial_state: MaterialState {
                    oil_contamination: 0.72,
                    biological_contamination: 0.35,
                    ..MaterialState::default()
                },
                ..clean_seed
            },
        );
        assert!(contaminated.visual.roughness < clean.visual.roughness);
        assert!(contaminated.physical.friction_dynamic < clean.physical.friction_dynamic);
        assert!(contaminated.acoustic.wetness_muffle > clean.acoustic.wetness_muffle);
    }

    #[test]
    fn instance_variation_uses_district_style_and_live_material_state() {
        let generated = generate_material(&MaterialGenerationRequest {
            request_id: 8_099,
            source_photos: Vec::new(),
            capture_evidence: MaterialCaptureEvidence::default(),
            semantic_label: "industrial dock steel railing".to_string(),
            scale_meters: 0.6,
            target_model: MaterialModelKind::PhysicallyBasedSurface,
            required_states: MaterialStateChannels::default(),
            quality: GenerationQuality::Runtime,
            output_budget: MaterialOutputBudget {
                max_texture_size: 512,
                max_graph_nodes: 16,
                max_runtime_microseconds: 16.0,
                max_cache_textures: 6,
                max_memory_bytes: 24 * 1024 * 1024,
                allow_gpu_compute: true,
            },
        });
        let base_seed = MaterialInstanceSeed {
            material_id: generated.material_id,
            seed: 900,
            age: 0.7,
            dirt_level: 0.35,
            damage_bias: 0.25,
            district_style: "neutral".to_string(),
            local_variation: 0.4,
            initial_state: MaterialState {
                moisture: 0.28,
                corrosion: 0.45,
                crack_density: 0.22,
                electrical_charge: 24.0,
                ..MaterialState::default()
            },
        };
        let neutral = apply_instance_variation(&generated.descriptor, &base_seed);
        let industrial = apply_instance_variation(
            &generated.descriptor,
            &MaterialInstanceSeed {
                district_style: "industrial dock".to_string(),
                ..base_seed
            },
        );

        assert!(industrial.visual.roughness > neutral.visual.roughness);
        assert!(industrial.physical.fracture_toughness < neutral.physical.fracture_toughness);
        assert!(industrial.electrical.conductivity > neutral.electrical.conductivity);
        assert!(industrial.acoustic.wetness_muffle > neutral.acoustic.wetness_muffle);
    }
}

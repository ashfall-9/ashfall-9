use std::collections::BTreeMap;

use crate::core::{FrameId, ModuleId, QualityTier};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuBackend {
    Vulkan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GpuResourceHandle(pub u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderPipelineHandle(pub u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComputePipelineHandle(pub u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RayPipelineHandle(pub u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GpuResourceKind {
    Buffer,
    Image2D,
    Image3D,
    AccelerationStructure,
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GpuResourceLifetime {
    Imported,
    Persistent,
    Transient,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuResourceDesc {
    pub label: String,
    pub kind: GpuResourceKind,
    pub byte_len: u64,
    pub owner: Option<ModuleId>,
    pub lifetime: GpuResourceLifetime,
    pub residency_policy: GpuResourceResidencyPolicy,
    pub bindless: bool,
}

impl GpuResourceDesc {
    pub fn new(label: impl Into<String>, kind: GpuResourceKind, byte_len: u64) -> Self {
        Self {
            label: label.into(),
            kind,
            byte_len,
            owner: None,
            lifetime: GpuResourceLifetime::Transient,
            residency_policy: GpuResourceResidencyPolicy::Transient,
            bindless: false,
        }
    }

    pub fn owned_by(mut self, owner: ModuleId) -> Self {
        self.owner = Some(owner);
        self
    }

    pub fn with_lifetime(mut self, lifetime: GpuResourceLifetime) -> Self {
        self.lifetime = lifetime;
        self.residency_policy = match lifetime {
            GpuResourceLifetime::Imported | GpuResourceLifetime::Persistent => {
                GpuResourceResidencyPolicy::StaticResident
            }
            GpuResourceLifetime::Transient => GpuResourceResidencyPolicy::Transient,
        };
        self
    }

    pub fn with_residency_policy(mut self, policy: GpuResourceResidencyPolicy) -> Self {
        self.residency_policy = policy;
        self
    }

    pub fn bindless(mut self) -> Self {
        self.bindless = true;
        if matches!(
            self.residency_policy,
            GpuResourceResidencyPolicy::StaticResident
        ) && self.byte_len >= 1024 * 1024
        {
            self.residency_policy = GpuResourceResidencyPolicy::StreamedResident;
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GpuResourceResidencyPolicy {
    StaticResident,
    StreamedResident,
    Transient,
    Readback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuResourceRecord {
    pub handle: GpuResourceHandle,
    pub desc: GpuResourceDesc,
    pub bindless_index: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuShaderPermutation {
    pub defines: Vec<String>,
}

impl GpuShaderPermutation {
    pub fn new(defines: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut defines = defines
            .into_iter()
            .map(Into::into)
            .map(|define: String| define.trim().to_string())
            .filter(|define| !define.is_empty())
            .collect::<Vec<_>>();
        defines.sort();
        defines.dedup();
        Self { defines }
    }

    pub fn is_empty(&self) -> bool {
        self.defines.is_empty()
    }

    pub fn key(&self) -> String {
        if self.defines.is_empty() {
            "base".to_string()
        } else {
            self.defines.join("+")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuPipelineDesc {
    pub label: String,
    pub shader_key: String,
    pub quality_tier: QualityTier,
    pub permutation: GpuShaderPermutation,
    pub shader_contract: GpuShaderContract,
}

impl GpuPipelineDesc {
    pub fn new(
        label: impl Into<String>,
        shader_key: impl Into<String>,
        quality_tier: QualityTier,
    ) -> Self {
        let label = label.into();
        let shader_key = shader_key.into();
        Self {
            shader_contract: GpuShaderContract::for_pipeline(&label, &shader_key, quality_tier),
            label,
            shader_key,
            quality_tier,
            permutation: GpuShaderPermutation::default(),
        }
    }

    pub fn with_permutation(mut self, permutation: GpuShaderPermutation) -> Self {
        self.permutation = permutation;
        self.shader_contract
            .sync_specialization_constants(&self.permutation, self.quality_tier);
        self
    }

    pub fn with_shader_contract(mut self, contract: GpuShaderContract) -> Self {
        self.shader_contract = contract;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuShaderContract {
    pub artifact: GpuShaderArtifactKind,
    pub resource_layout: Vec<GpuShaderResourceBinding>,
    pub specialization_constants: Vec<GpuShaderSpecializationConstant>,
    pub supported_quality_tiers: Vec<QualityTier>,
    pub expected_subgroup_size: Option<u32>,
    pub precision: GpuShaderPrecision,
    pub debug_name: String,
    pub validation_tests: Vec<String>,
    pub hot_reload_allowed: bool,
}

impl GpuShaderContract {
    pub fn for_pipeline(label: &str, shader_key: &str, quality_tier: QualityTier) -> Self {
        let mut contract = Self {
            artifact: GpuShaderArtifactKind::from_shader_key(shader_key),
            resource_layout: vec![GpuShaderResourceBinding {
                set: 0,
                binding: 0,
                name: "engine_frame_resources".to_string(),
                kind: GpuShaderResourceBindingKind::BindlessTable,
            }],
            specialization_constants: Vec::new(),
            supported_quality_tiers: supported_quality_tiers_for(quality_tier),
            expected_subgroup_size: Some(32),
            precision: GpuShaderPrecision::Mixed,
            debug_name: label.to_string(),
            validation_tests: vec![format!("{label}_shader_contract_smoke")],
            hot_reload_allowed: true,
        };
        contract.sync_specialization_constants(&GpuShaderPermutation::default(), quality_tier);
        contract
    }

    pub fn sync_specialization_constants(
        &mut self,
        permutation: &GpuShaderPermutation,
        quality_tier: QualityTier,
    ) {
        let mut constants = vec![GpuShaderSpecializationConstant {
            name: "QUALITY_TIER".to_string(),
            value: quality_tier_name(quality_tier).to_string(),
        }];
        constants.extend(permutation.defines.iter().map(|define| {
            GpuShaderSpecializationConstant {
                name: define.clone(),
                value: "enabled".to_string(),
            }
        }));
        constants.sort_by(|left, right| left.name.cmp(&right.name));
        constants.dedup_by(|left, right| left.name == right.name);
        self.specialization_constants = constants;
        if !self.supported_quality_tiers.contains(&quality_tier) {
            self.supported_quality_tiers.push(quality_tier);
            self.supported_quality_tiers.sort();
            self.supported_quality_tiers.dedup();
        }
    }
}

fn supported_quality_tiers_for(quality_tier: QualityTier) -> Vec<QualityTier> {
    match quality_tier {
        QualityTier::Disabled => vec![QualityTier::Disabled],
        QualityTier::BackgroundApproximation => {
            vec![
                QualityTier::BackgroundApproximation,
                QualityTier::NormalRuntime,
            ]
        }
        QualityTier::NormalRuntime => vec![
            QualityTier::BackgroundApproximation,
            QualityTier::NormalRuntime,
            QualityTier::HeroHighFidelityRuntime,
        ],
        QualityTier::HeroHighFidelityRuntime => vec![
            QualityTier::NormalRuntime,
            QualityTier::HeroHighFidelityRuntime,
        ],
        QualityTier::ReferenceOfflineValidation => vec![
            QualityTier::HeroHighFidelityRuntime,
            QualityTier::ReferenceOfflineValidation,
        ],
    }
}

fn quality_tier_name(quality_tier: QualityTier) -> &'static str {
    match quality_tier {
        QualityTier::Disabled => "disabled",
        QualityTier::BackgroundApproximation => "background",
        QualityTier::NormalRuntime => "normal",
        QualityTier::HeroHighFidelityRuntime => "hero",
        QualityTier::ReferenceOfflineValidation => "reference",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuShaderArtifactKind {
    SpirV,
    RustGpuSource,
    EngineShaderSource,
    Unknown,
}

impl GpuShaderArtifactKind {
    fn from_shader_key(shader_key: &str) -> Self {
        if shader_key.ends_with(".spv") {
            Self::SpirV
        } else if shader_key.ends_with(".rs") {
            Self::RustGpuSource
        } else if shader_key.contains(".vert")
            || shader_key.contains(".frag")
            || shader_key.contains(".comp")
            || shader_key.contains(".rgen")
            || shader_key.contains(".rmiss")
            || shader_key.contains(".rhit")
        {
            Self::EngineShaderSource
        } else {
            Self::Unknown
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuShaderResourceBinding {
    pub set: u32,
    pub binding: u32,
    pub name: String,
    pub kind: GpuShaderResourceBindingKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuShaderResourceBindingKind {
    UniformBuffer,
    StorageBuffer,
    SampledImage,
    StorageImage,
    AccelerationStructure,
    BindlessTable,
    PushConstants,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuShaderSpecializationConstant {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuShaderPrecision {
    Relaxed,
    Mixed,
    FullFloat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuPipelineCacheEntry<H> {
    pub handle: H,
    pub desc: GpuPipelineDesc,
    pub request_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuPipelineCacheResult<H> {
    pub handle: H,
    pub cache_hit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuQueueKind {
    Graphics,
    Compute,
    Transfer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuDispatchKind {
    Graphics(RenderPipelineHandle),
    Compute(ComputePipelineHandle),
    RayTracing(RayPipelineHandle),
    Copy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuCapabilities {
    pub backend: GpuBackend,
    pub vulkan_version: VulkanVersion,
    pub device_name: String,
    pub vendor_name: String,
    pub device_type: String,
    pub ray_tracing: bool,
    pub ray_query: bool,
    pub descriptor_indexing: bool,
    pub buffer_device_address: bool,
    pub mesh_shading: bool,
    pub subgroup_operations: bool,
    pub sparse_resources: bool,
    pub timeline_semaphore: bool,
    pub async_compute: bool,
    pub async_compute_queue_count: u32,
    pub memory_heaps: Vec<GpuMemoryHeapReport>,
    pub limits: GpuLimits,
    pub supported_formats: Vec<GpuFormatCapability>,
    pub supported_compression_formats: Vec<&'static str>,
}

impl GpuCapabilities {
    pub fn vulkan_baseline() -> Self {
        Self {
            backend: GpuBackend::Vulkan,
            vulkan_version: VulkanVersion {
                major: 1,
                minor: 3,
                patch: 0,
            },
            device_name: "Vulkan baseline adapter".to_string(),
            vendor_name: "runtime-selected".to_string(),
            device_type: "unknown".to_string(),
            ray_tracing: false,
            ray_query: false,
            descriptor_indexing: true,
            buffer_device_address: true,
            mesh_shading: false,
            subgroup_operations: true,
            sparse_resources: false,
            timeline_semaphore: true,
            async_compute: true,
            async_compute_queue_count: 1,
            memory_heaps: vec![
                GpuMemoryHeapReport {
                    label: "device_local",
                    size_bytes: 6 * 1024 * 1024 * 1024,
                    device_local: true,
                    host_visible: false,
                },
                GpuMemoryHeapReport {
                    label: "upload_readback",
                    size_bytes: 512 * 1024 * 1024,
                    device_local: false,
                    host_visible: true,
                },
            ],
            limits: GpuLimits {
                max_texture_dimension_2d: 8192,
                max_texture_dimension_3d: 2048,
                max_storage_buffer_range_bytes: 256 * 1024 * 1024,
                max_descriptor_count: 131_072,
                max_bindless_resources: 65_536,
                max_acceleration_structure_instances: 0,
                max_geometry_pages: 4096,
                max_material_cache_pages: 4096,
            },
            supported_formats: vec![
                GpuFormatCapability {
                    format: "Rgba16Float",
                    sampled: true,
                    storage: true,
                    color_attachment: true,
                    depth_attachment: false,
                    compression: None,
                },
                GpuFormatCapability {
                    format: "Rgba8Srgb",
                    sampled: true,
                    storage: false,
                    color_attachment: true,
                    depth_attachment: false,
                    compression: Some("BC7"),
                },
                GpuFormatCapability {
                    format: "Depth32Float",
                    sampled: true,
                    storage: false,
                    color_attachment: false,
                    depth_attachment: true,
                    compression: None,
                },
            ],
            supported_compression_formats: vec!["BC7", "BC5"],
        }
    }

    pub fn capability_report(&self) -> GpuCapabilityReport {
        GpuCapabilityReport::from_capabilities(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VulkanVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl VulkanVersion {
    pub fn label(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuMemoryHeapReport {
    pub label: &'static str,
    pub size_bytes: u64,
    pub device_local: bool,
    pub host_visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuLimits {
    pub max_texture_dimension_2d: u32,
    pub max_texture_dimension_3d: u32,
    pub max_storage_buffer_range_bytes: u64,
    pub max_descriptor_count: u32,
    pub max_bindless_resources: u32,
    pub max_acceleration_structure_instances: u32,
    pub max_geometry_pages: u32,
    pub max_material_cache_pages: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuFormatCapability {
    pub format: &'static str,
    pub sampled: bool,
    pub storage: bool,
    pub color_attachment: bool,
    pub depth_attachment: bool,
    pub compression: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuCapabilityReport {
    pub backend: GpuBackend,
    pub vulkan_version: VulkanVersion,
    pub device_name: String,
    pub vendor_name: String,
    pub device_type: String,
    pub selected_tier: GpuCapabilityTier,
    pub async_compute_queue_count: u32,
    pub features: Vec<GpuFeatureCapability>,
    pub memory_heaps: Vec<GpuMemoryHeapReport>,
    pub limits: GpuLimits,
    pub supported_formats: Vec<GpuFormatCapability>,
    pub supported_compression_formats: Vec<&'static str>,
    pub fallback_count: usize,
    pub missing_required_feature_count: usize,
}

impl GpuCapabilityReport {
    pub fn from_capabilities(capabilities: &GpuCapabilities) -> Self {
        let features = gpu_feature_capabilities(capabilities);
        let fallback_count = features
            .iter()
            .filter(|feature| !feature.supported && feature.fallback.is_some())
            .count();
        let missing_required_feature_count = features
            .iter()
            .filter(|feature| {
                !feature.supported
                    && feature.fallback.is_none()
                    && !feature.required_for_tiers.is_empty()
            })
            .count();

        Self {
            backend: capabilities.backend,
            vulkan_version: capabilities.vulkan_version,
            device_name: capabilities.device_name.clone(),
            vendor_name: capabilities.vendor_name.clone(),
            device_type: capabilities.device_type.clone(),
            selected_tier: selected_capability_tier(capabilities),
            async_compute_queue_count: capabilities.async_compute_queue_count,
            features,
            memory_heaps: capabilities.memory_heaps.clone(),
            limits: capabilities.limits.clone(),
            supported_formats: capabilities.supported_formats.clone(),
            supported_compression_formats: capabilities.supported_compression_formats.clone(),
            fallback_count,
            missing_required_feature_count,
        }
    }

    pub fn passed(&self) -> bool {
        self.missing_required_feature_count == 0
    }

    pub fn total_device_memory_bytes(&self) -> u64 {
        self.memory_heaps
            .iter()
            .filter(|heap| heap.device_local)
            .map(|heap| heap.size_bytes)
            .sum()
    }

    pub fn feature(&self, name: &str) -> Option<&GpuFeatureCapability> {
        self.features.iter().find(|feature| feature.name == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GpuCapabilityTier {
    BaselineVulkan,
    DescriptorIndexedCompute,
    SelectiveRayTracing,
    ReferencePathTracing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuFeatureCapability {
    pub name: &'static str,
    pub supported: bool,
    pub fallback: Option<GpuFeatureFallback>,
    pub required_for_tiers: Vec<QualityTier>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuFeatureFallback {
    ComputeFallback,
    RasterFallback,
    DescriptorSetFallback,
    SerializedGraphicsQueue,
    SparseResourceEmulation,
}

fn selected_capability_tier(capabilities: &GpuCapabilities) -> GpuCapabilityTier {
    if capabilities.ray_tracing
        && capabilities.ray_query
        && capabilities.buffer_device_address
        && capabilities.descriptor_indexing
        && capabilities.mesh_shading
    {
        GpuCapabilityTier::ReferencePathTracing
    } else if capabilities.ray_tracing
        && capabilities.buffer_device_address
        && capabilities.descriptor_indexing
    {
        GpuCapabilityTier::SelectiveRayTracing
    } else if capabilities.descriptor_indexing && capabilities.async_compute {
        GpuCapabilityTier::DescriptorIndexedCompute
    } else {
        GpuCapabilityTier::BaselineVulkan
    }
}

fn gpu_feature_capabilities(capabilities: &GpuCapabilities) -> Vec<GpuFeatureCapability> {
    vec![
        GpuFeatureCapability {
            name: "ray_tracing",
            supported: capabilities.ray_tracing,
            fallback: (!capabilities.ray_tracing).then_some(GpuFeatureFallback::ComputeFallback),
            required_for_tiers: vec![
                QualityTier::HeroHighFidelityRuntime,
                QualityTier::ReferenceOfflineValidation,
            ],
        },
        GpuFeatureCapability {
            name: "ray_query",
            supported: capabilities.ray_query,
            fallback: (!capabilities.ray_query).then_some(GpuFeatureFallback::ComputeFallback),
            required_for_tiers: vec![QualityTier::HeroHighFidelityRuntime],
        },
        GpuFeatureCapability {
            name: "descriptor_indexing",
            supported: capabilities.descriptor_indexing,
            fallback: (!capabilities.descriptor_indexing)
                .then_some(GpuFeatureFallback::DescriptorSetFallback),
            required_for_tiers: vec![QualityTier::NormalRuntime],
        },
        GpuFeatureCapability {
            name: "buffer_device_address",
            supported: capabilities.buffer_device_address,
            fallback: (!capabilities.buffer_device_address)
                .then_some(GpuFeatureFallback::DescriptorSetFallback),
            required_for_tiers: vec![QualityTier::NormalRuntime],
        },
        GpuFeatureCapability {
            name: "mesh_shading",
            supported: capabilities.mesh_shading,
            fallback: (!capabilities.mesh_shading).then_some(GpuFeatureFallback::RasterFallback),
            required_for_tiers: vec![QualityTier::HeroHighFidelityRuntime],
        },
        GpuFeatureCapability {
            name: "subgroup_operations",
            supported: capabilities.subgroup_operations,
            fallback: (!capabilities.subgroup_operations)
                .then_some(GpuFeatureFallback::ComputeFallback),
            required_for_tiers: vec![QualityTier::NormalRuntime],
        },
        GpuFeatureCapability {
            name: "sparse_resources",
            supported: capabilities.sparse_resources,
            fallback: (!capabilities.sparse_resources)
                .then_some(GpuFeatureFallback::SparseResourceEmulation),
            required_for_tiers: vec![QualityTier::HeroHighFidelityRuntime],
        },
        GpuFeatureCapability {
            name: "timeline_semaphore",
            supported: capabilities.timeline_semaphore,
            fallback: (!capabilities.timeline_semaphore)
                .then_some(GpuFeatureFallback::SerializedGraphicsQueue),
            required_for_tiers: vec![QualityTier::NormalRuntime],
        },
        GpuFeatureCapability {
            name: "async_compute",
            supported: capabilities.async_compute,
            fallback: (!capabilities.async_compute)
                .then_some(GpuFeatureFallback::SerializedGraphicsQueue),
            required_for_tiers: vec![QualityTier::NormalRuntime],
        },
    ]
}

#[derive(Clone, Debug, PartialEq)]
pub struct GpuPassDesc {
    pub name: String,
    pub owner: Option<ModuleId>,
    pub debug_label: String,
    pub queue: GpuQueueKind,
    pub reads: Vec<GpuResourceHandle>,
    pub writes: Vec<GpuResourceHandle>,
    pub resource_accesses: Vec<GpuPassResourceAccess>,
    pub dispatch: GpuDispatchKind,
    pub budget_hint: QualityTier,
    pub expected_cost: GpuPassExpectedCost,
    pub dependency_hints: Vec<GpuPassDependencyHint>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuPassExpectedCost {
    pub cpu_milliseconds: Option<f32>,
    pub gpu_milliseconds: Option<f32>,
    pub transient_memory_bytes: Option<u64>,
}

impl GpuPassExpectedCost {
    pub fn is_declared(self) -> bool {
        self.cpu_milliseconds.is_some()
            && self.gpu_milliseconds.is_some()
            && self.transient_memory_bytes.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuPassResourceAccess {
    pub resource: GpuResourceHandle,
    pub access: GpuResourceAccessType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GpuResourceAccessType {
    StorageRead,
    StorageWrite,
    SampledRead,
    CopyRead,
    CopyWrite,
    AccelerationStructureRead,
    AccelerationStructureWrite,
    ExternalRead,
    ExternalWrite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuPassDependencyHint {
    pub kind: GpuPassDependencyHintKind,
    pub pass_name: String,
    pub reason: String,
}

impl GpuPassDependencyHint {
    pub fn after(pass_name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            kind: GpuPassDependencyHintKind::After,
            pass_name: pass_name.into(),
            reason: reason.into(),
        }
    }

    pub fn before(pass_name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            kind: GpuPassDependencyHintKind::Before,
            pass_name: pass_name.into(),
            reason: reason.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuPassDependencyHintKind {
    After,
    Before,
}

impl GpuPassDesc {
    pub fn new(
        name: impl Into<String>,
        queue: GpuQueueKind,
        dispatch: GpuDispatchKind,
        budget_hint: QualityTier,
    ) -> Self {
        let name = name.into();
        Self {
            debug_label: name.clone(),
            name,
            owner: None,
            queue,
            reads: Vec::new(),
            writes: Vec::new(),
            resource_accesses: Vec::new(),
            dispatch,
            budget_hint,
            expected_cost: GpuPassExpectedCost::default(),
            dependency_hints: Vec::new(),
        }
    }

    pub fn owned_by(mut self, owner: ModuleId) -> Self {
        self.owner = Some(owner);
        self
    }

    pub fn debug_label(mut self, label: impl Into<String>) -> Self {
        self.debug_label = label.into();
        self
    }

    pub fn expected_cost(mut self, cost: GpuPassExpectedCost) -> Self {
        self.expected_cost = cost;
        self
    }

    pub fn expected_gpu_milliseconds(mut self, milliseconds: f32) -> Self {
        self.expected_cost.gpu_milliseconds = Some(round_milliseconds(milliseconds.max(0.0)));
        self
    }

    pub fn expected_cpu_milliseconds(mut self, milliseconds: f32) -> Self {
        self.expected_cost.cpu_milliseconds = Some(round_milliseconds(milliseconds.max(0.0)));
        self
    }

    pub fn expected_transient_memory_bytes(mut self, bytes: u64) -> Self {
        self.expected_cost.transient_memory_bytes = Some(bytes);
        self
    }

    pub fn access(mut self, resource: GpuResourceHandle, access: GpuResourceAccessType) -> Self {
        self.resource_accesses
            .push(GpuPassResourceAccess { resource, access });
        self
    }

    pub fn with_estimated_cost(mut self) -> Self {
        fill_missing_expected_cost(&mut self);
        self
    }

    pub fn after_pass(mut self, pass_name: impl Into<String>, reason: impl Into<String>) -> Self {
        self.dependency_hints
            .push(GpuPassDependencyHint::after(pass_name, reason));
        self
    }

    pub fn before_pass(mut self, pass_name: impl Into<String>, reason: impl Into<String>) -> Self {
        self.dependency_hints
            .push(GpuPassDependencyHint::before(pass_name, reason));
        self
    }

    pub fn reads(mut self, resources: impl IntoIterator<Item = GpuResourceHandle>) -> Self {
        self.reads.extend(resources);
        self
    }

    pub fn writes(mut self, resources: impl IntoIterator<Item = GpuResourceHandle>) -> Self {
        self.writes.extend(resources);
        self
    }
}

#[derive(Clone, Debug)]
pub struct GpuGraphBuilder {
    pub frame_id: FrameId,
    pub capabilities: GpuCapabilities,
    active_owner: Option<ModuleId>,
    passes: Vec<GpuPassDesc>,
    resources: BTreeMap<GpuResourceHandle, GpuResourceRecord>,
    next_bindless_index: u32,
    render_pipelines: BTreeMap<RenderPipelineHandle, GpuPipelineCacheEntry<RenderPipelineHandle>>,
    compute_pipelines:
        BTreeMap<ComputePipelineHandle, GpuPipelineCacheEntry<ComputePipelineHandle>>,
    ray_pipelines: BTreeMap<RayPipelineHandle, GpuPipelineCacheEntry<RayPipelineHandle>>,
}

impl GpuGraphBuilder {
    pub fn push_component_owner(&mut self, owner: ModuleId) -> Option<ModuleId> {
        let previous = self.active_owner;
        self.active_owner = Some(owner);
        previous
    }

    pub fn pop_component_owner(&mut self, previous: Option<ModuleId>) {
        self.active_owner = previous;
    }

    pub fn active_component_owner(&self) -> Option<ModuleId> {
        self.active_owner
    }

    pub fn declare_resource(&mut self, desc: GpuResourceDesc) -> GpuResourceHandle {
        let handle = GpuResourceHandle(gpu_resource_id(&desc));
        if let Some(existing) = self.resources.get_mut(&handle) {
            if desc.bindless && existing.bindless_index.is_none() {
                existing.bindless_index = Some(self.next_bindless_index);
                self.next_bindless_index = self.next_bindless_index.saturating_add(1);
            }
            existing.desc = desc;
            return handle;
        }

        let bindless_index = desc.bindless.then(|| {
            let index = self.next_bindless_index;
            self.next_bindless_index = self.next_bindless_index.saturating_add(1);
            index
        });
        self.resources.insert(
            handle,
            GpuResourceRecord {
                handle,
                desc,
                bindless_index,
            },
        );
        handle
    }

    pub fn add_pass(&mut self, mut pass: GpuPassDesc) {
        if pass.owner.is_none() {
            pass.owner = self.active_owner;
        }
        if pass.debug_label.is_empty() {
            pass.debug_label = pass.name.clone();
        }
        fill_missing_expected_cost(&mut pass);
        self.passes.push(pass);
    }

    pub fn create_render_pipeline(
        &mut self,
        desc: GpuPipelineDesc,
    ) -> GpuPipelineCacheResult<RenderPipelineHandle> {
        let handle = RenderPipelineHandle(gpu_pipeline_id("render", &desc));
        let cache_hit = upsert_pipeline_cache(&mut self.render_pipelines, handle, desc);
        GpuPipelineCacheResult { handle, cache_hit }
    }

    pub fn create_compute_pipeline(
        &mut self,
        desc: GpuPipelineDesc,
    ) -> GpuPipelineCacheResult<ComputePipelineHandle> {
        let handle = ComputePipelineHandle(gpu_pipeline_id("compute", &desc));
        let cache_hit = upsert_pipeline_cache(&mut self.compute_pipelines, handle, desc);
        GpuPipelineCacheResult { handle, cache_hit }
    }

    pub fn create_ray_pipeline(
        &mut self,
        desc: GpuPipelineDesc,
    ) -> GpuPipelineCacheResult<RayPipelineHandle> {
        let handle = RayPipelineHandle(gpu_pipeline_id("ray", &desc));
        let cache_hit = upsert_pipeline_cache(&mut self.ray_pipelines, handle, desc);
        GpuPipelineCacheResult { handle, cache_hit }
    }

    pub fn passes(&self) -> &[GpuPassDesc] {
        &self.passes
    }

    pub fn resource(&self, handle: GpuResourceHandle) -> Option<&GpuResourceRecord> {
        self.resources.get(&handle)
    }

    pub fn resources(&self) -> impl Iterator<Item = &GpuResourceRecord> {
        self.resources.values()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GpuFrameReport {
    pub frame_id: FrameId,
    pub backend: GpuBackend,
    pub capability_report: GpuCapabilityReport,
    pub passes: Vec<GpuPassDesc>,
    pub declared_resources: Vec<GpuResourceRecord>,
    pub barriers: Vec<GpuBarrier>,
    pub timing: Vec<GpuPassTiming>,
    pub validation: GpuGraphValidationReport,
    pub resource_usage: Vec<GpuResourceUsage>,
    pub registered_resource_usage: Vec<GpuRegisteredResourceUsage>,
    pub pipeline_report: GpuPipelineReport,
    pub descriptor_report: GpuDescriptorReport,
    pub memory_plan: GpuMemoryPlan,
    pub execution_plan: GpuExecutionPlan,
    pub queue_workloads: Vec<GpuQueueWorkload>,
    pub transient_memory_bytes: u64,
    pub total_gpu_milliseconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuBarrierReason {
    ReadAfterWrite,
    WriteAfterRead,
    WriteAfterWrite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuBarrier {
    pub resource: GpuResourceHandle,
    pub from_pass: String,
    pub to_pass: String,
    pub reason: GpuBarrierReason,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GpuPassTiming {
    pub pass_name: String,
    pub queue: GpuQueueKind,
    pub estimated_gpu_milliseconds: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GpuGraphValidationReport {
    pub passed: bool,
    pub issues: Vec<GpuGraphValidationIssue>,
}

impl Default for GpuGraphValidationReport {
    fn default() -> Self {
        Self {
            passed: true,
            issues: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuGraphValidationIssue {
    pub severity: GpuValidationSeverity,
    pub code: String,
    pub pass_name: Option<String>,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuValidationSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuResourceUsage {
    pub resource: GpuResourceHandle,
    pub readers: Vec<String>,
    pub writers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuRegisteredResourceUsage {
    pub resource: GpuResourceHandle,
    pub label: String,
    pub kind: GpuResourceKind,
    pub owner: Option<ModuleId>,
    pub lifetime: GpuResourceLifetime,
    pub bindless_index: Option<u32>,
    pub byte_len: u64,
    pub readers: Vec<String>,
    pub writers: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GpuPipelineKind {
    Render,
    Compute,
    RayTracing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuPipelineUsage {
    pub kind: GpuPipelineKind,
    pub handle: u128,
    pub label: String,
    pub shader_key: String,
    pub quality_tier: QualityTier,
    pub permutation_key: String,
    pub permutation_defines: Vec<String>,
    pub pass_names: Vec<String>,
    pub request_count: u64,
    pub cache_hit: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuPipelineReport {
    pub pipelines: Vec<GpuPipelineUsage>,
    pub total_pipeline_requests: usize,
    pub cache_hit_count: usize,
    pub unknown_pipeline_count: usize,
    pub shader_contracts: Vec<GpuShaderPipelineContract>,
    pub shader_variant_count: usize,
    pub shader_validation_error_count: usize,
    pub shader_validation_warning_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuShaderPipelineContract {
    pub kind: GpuPipelineKind,
    pub handle: u128,
    pub label: String,
    pub shader_key: String,
    pub permutation_key: String,
    pub contract: GpuShaderContract,
    pub validation: GpuShaderValidationReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuShaderValidationReport {
    pub passed: bool,
    pub issues: Vec<GpuShaderValidationIssue>,
}

impl Default for GpuShaderValidationReport {
    fn default() -> Self {
        Self {
            passed: true,
            issues: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuShaderValidationIssue {
    pub severity: GpuValidationSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuDescriptorKind {
    StorageBuffer,
    SampledImage,
    StorageImage,
    AccelerationStructure,
    External,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuDescriptorBinding {
    pub resource: GpuResourceHandle,
    pub label: String,
    pub owner: Option<ModuleId>,
    pub descriptor_kind: GpuDescriptorKind,
    pub binding_slot: u32,
    pub bindless_index: Option<u32>,
    pub pass_names: Vec<String>,
    pub read_only: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuDescriptorReport {
    pub bindings: Vec<GpuDescriptorBinding>,
    pub bindless_binding_count: usize,
    pub descriptor_set_count: usize,
    pub descriptor_table_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuMemoryPlan {
    pub transient_resource_count: usize,
    pub alias_group_count: usize,
    pub total_transient_bytes: u64,
    pub unaliased_transient_bytes: u64,
    pub aliased_bytes_saved: u64,
    pub allocations: Vec<GpuMemoryAllocation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuMemoryAllocation {
    pub resource: GpuResourceHandle,
    pub label: String,
    pub owner: Option<ModuleId>,
    pub byte_len: u64,
    pub first_pass_index: usize,
    pub last_pass_index: usize,
    pub first_pass_name: String,
    pub last_pass_name: String,
    pub alias_group: usize,
    pub offset_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuExecutionPlan {
    pub scheduled_passes: Vec<GpuScheduledPass>,
    pub total_work_milliseconds: f32,
    pub estimated_wall_milliseconds: f32,
    pub overlapped_work_milliseconds: f32,
    pub cross_queue_wait_count: usize,
    pub queue_idle_milliseconds: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GpuScheduledPass {
    pub pass_index: usize,
    pub pass_name: String,
    pub queue: GpuQueueKind,
    pub start_milliseconds: f32,
    pub end_milliseconds: f32,
    pub duration_milliseconds: f32,
    pub wait_dependencies: Vec<GpuPassDependency>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuPassDependency {
    pub from_pass: String,
    pub from_queue: GpuQueueKind,
    pub resource: GpuResourceHandle,
    pub reason: GpuBarrierReason,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GpuQueueWorkload {
    pub queue: GpuQueueKind,
    pub pass_count: usize,
    pub estimated_gpu_milliseconds: f32,
}

#[derive(Clone, Debug)]
pub struct GpuServices {
    capabilities: GpuCapabilities,
    resources: BTreeMap<GpuResourceHandle, GpuResourceRecord>,
    next_bindless_index: u32,
    render_pipelines: BTreeMap<RenderPipelineHandle, GpuPipelineCacheEntry<RenderPipelineHandle>>,
    compute_pipelines:
        BTreeMap<ComputePipelineHandle, GpuPipelineCacheEntry<ComputePipelineHandle>>,
    ray_pipelines: BTreeMap<RayPipelineHandle, GpuPipelineCacheEntry<RayPipelineHandle>>,
}

impl GpuServices {
    pub fn new_vulkan() -> Self {
        Self {
            capabilities: GpuCapabilities::vulkan_baseline(),
            resources: BTreeMap::new(),
            next_bindless_index: 0,
            render_pipelines: BTreeMap::new(),
            compute_pipelines: BTreeMap::new(),
            ray_pipelines: BTreeMap::new(),
        }
    }

    pub fn register_resource(&mut self, desc: GpuResourceDesc) -> GpuResourceHandle {
        let handle = GpuResourceHandle(gpu_resource_id(&desc));
        if let Some(existing) = self.resources.get_mut(&handle) {
            if desc.bindless && existing.bindless_index.is_none() {
                existing.bindless_index = Some(self.next_bindless_index);
                self.next_bindless_index = self.next_bindless_index.saturating_add(1);
            }
            existing.desc = desc;
            return handle;
        }

        let bindless_index = desc.bindless.then(|| {
            let index = self.next_bindless_index;
            self.next_bindless_index = self.next_bindless_index.saturating_add(1);
            index
        });
        self.resources.insert(
            handle,
            GpuResourceRecord {
                handle,
                desc,
                bindless_index,
            },
        );
        handle
    }

    pub fn resource(&self, handle: GpuResourceHandle) -> Option<&GpuResourceRecord> {
        self.resources.get(&handle)
    }

    pub fn resources(&self) -> impl Iterator<Item = &GpuResourceRecord> {
        self.resources.values()
    }

    pub fn create_render_pipeline(
        &mut self,
        desc: GpuPipelineDesc,
    ) -> GpuPipelineCacheResult<RenderPipelineHandle> {
        let handle = RenderPipelineHandle(gpu_pipeline_id("render", &desc));
        let cache_hit = upsert_pipeline_cache(&mut self.render_pipelines, handle, desc);
        GpuPipelineCacheResult { handle, cache_hit }
    }

    pub fn create_compute_pipeline(
        &mut self,
        desc: GpuPipelineDesc,
    ) -> GpuPipelineCacheResult<ComputePipelineHandle> {
        let handle = ComputePipelineHandle(gpu_pipeline_id("compute", &desc));
        let cache_hit = upsert_pipeline_cache(&mut self.compute_pipelines, handle, desc);
        GpuPipelineCacheResult { handle, cache_hit }
    }

    pub fn create_ray_pipeline(
        &mut self,
        desc: GpuPipelineDesc,
    ) -> GpuPipelineCacheResult<RayPipelineHandle> {
        let handle = RayPipelineHandle(gpu_pipeline_id("ray", &desc));
        let cache_hit = upsert_pipeline_cache(&mut self.ray_pipelines, handle, desc);
        GpuPipelineCacheResult { handle, cache_hit }
    }

    pub fn render_pipeline(
        &self,
        handle: RenderPipelineHandle,
    ) -> Option<&GpuPipelineCacheEntry<RenderPipelineHandle>> {
        self.render_pipelines.get(&handle)
    }

    pub fn compute_pipeline(
        &self,
        handle: ComputePipelineHandle,
    ) -> Option<&GpuPipelineCacheEntry<ComputePipelineHandle>> {
        self.compute_pipelines.get(&handle)
    }

    pub fn ray_pipeline(
        &self,
        handle: RayPipelineHandle,
    ) -> Option<&GpuPipelineCacheEntry<RayPipelineHandle>> {
        self.ray_pipelines.get(&handle)
    }

    pub fn begin_frame(&self, frame_id: FrameId) -> GpuGraphBuilder {
        GpuGraphBuilder {
            frame_id,
            capabilities: self.capabilities.clone(),
            active_owner: None,
            passes: Vec::new(),
            resources: BTreeMap::new(),
            next_bindless_index: self.next_bindless_index,
            render_pipelines: BTreeMap::new(),
            compute_pipelines: BTreeMap::new(),
            ray_pipelines: BTreeMap::new(),
        }
    }

    pub fn submit(&self, graph: GpuGraphBuilder) -> GpuFrameReport {
        let GpuGraphBuilder {
            frame_id,
            mut passes,
            resources,
            render_pipelines,
            compute_pipelines,
            ray_pipelines,
            ..
        } = graph;
        let mut known_resources = self.resources.clone();
        known_resources.extend(resources.clone());
        fill_missing_resource_accesses(&mut passes, &known_resources);
        let barriers = compile_barriers(&passes);
        apply_barrier_dependency_hints(&mut passes, &barriers);
        let validation = validate_graph(&self.capabilities, &passes);
        let resource_usage = collect_resource_usage(&passes);
        let registered_resource_usage =
            collect_registered_resource_usage(&resource_usage, &known_resources);
        let pipeline_report = collect_pipeline_report(
            &passes,
            PipelineCacheView {
                service_render: &self.render_pipelines,
                service_compute: &self.compute_pipelines,
                service_ray: &self.ray_pipelines,
                frame_render: &render_pipelines,
                frame_compute: &compute_pipelines,
                frame_ray: &ray_pipelines,
            },
        );
        let descriptor_report = build_descriptor_report(&registered_resource_usage);
        let memory_plan = plan_transient_memory(&passes, &known_resources);
        let timing = passes
            .iter()
            .map(|pass| GpuPassTiming {
                pass_name: pass.name.clone(),
                queue: pass.queue,
                estimated_gpu_milliseconds: pass
                    .expected_cost
                    .gpu_milliseconds
                    .unwrap_or_else(|| estimate_pass_milliseconds(pass)),
            })
            .collect::<Vec<_>>();
        let total_gpu_milliseconds = timing
            .iter()
            .map(|timing| timing.estimated_gpu_milliseconds)
            .sum();
        let queue_workloads = collect_queue_workloads(&timing);
        let execution_plan = build_execution_plan(&passes, &timing, &barriers);
        let transient_memory_bytes = if memory_plan.transient_resource_count > 0 {
            memory_plan.total_transient_bytes
        } else {
            estimate_transient_memory_bytes(&resource_usage)
        };

        GpuFrameReport {
            frame_id,
            backend: self.capabilities.backend,
            capability_report: self.capabilities.capability_report(),
            passes,
            declared_resources: resources.into_values().collect(),
            barriers,
            timing,
            validation,
            resource_usage,
            registered_resource_usage,
            pipeline_report,
            descriptor_report,
            memory_plan,
            execution_plan,
            queue_workloads,
            transient_memory_bytes,
            total_gpu_milliseconds,
        }
    }

    pub fn capabilities(&self) -> &GpuCapabilities {
        &self.capabilities
    }
}

fn compile_barriers(passes: &[GpuPassDesc]) -> Vec<GpuBarrier> {
    let mut barriers = Vec::new();
    let mut last_writer_by_resource: BTreeMap<GpuResourceHandle, usize> = BTreeMap::new();
    let mut readers_since_write_by_resource: BTreeMap<GpuResourceHandle, Vec<usize>> =
        BTreeMap::new();

    for (pass_index, pass) in passes.iter().enumerate() {
        for resource in &pass.reads {
            if let Some(writer_index) = last_writer_by_resource.get(resource) {
                let writer = &passes[*writer_index];
                barriers.push(GpuBarrier {
                    resource: *resource,
                    from_pass: writer.name.clone(),
                    to_pass: pass.name.clone(),
                    reason: GpuBarrierReason::ReadAfterWrite,
                });
            }
            readers_since_write_by_resource
                .entry(*resource)
                .or_default()
                .push(pass_index);
        }

        for resource in &pass.writes {
            if let Some(writer_index) = last_writer_by_resource.get(resource) {
                let writer = &passes[*writer_index];
                barriers.push(GpuBarrier {
                    resource: *resource,
                    from_pass: writer.name.clone(),
                    to_pass: pass.name.clone(),
                    reason: GpuBarrierReason::WriteAfterWrite,
                });
            }

            if let Some(reader_indices) = readers_since_write_by_resource.remove(resource) {
                for reader_index in reader_indices {
                    if reader_index == pass_index {
                        continue;
                    }
                    barriers.push(GpuBarrier {
                        resource: *resource,
                        from_pass: passes[reader_index].name.clone(),
                        to_pass: pass.name.clone(),
                        reason: GpuBarrierReason::WriteAfterRead,
                    });
                }
            }

            last_writer_by_resource.insert(*resource, pass_index);
        }
    }

    barriers
}

fn estimate_pass_milliseconds(pass: &GpuPassDesc) -> f32 {
    let dispatch_cost = match pass.dispatch {
        GpuDispatchKind::Graphics(_) => 0.45,
        GpuDispatchKind::Compute(_) => 0.28,
        GpuDispatchKind::RayTracing(_) => 1.4,
        GpuDispatchKind::Copy => 0.08,
    };
    let queue_cost = match pass.queue {
        GpuQueueKind::Graphics => 0.15,
        GpuQueueKind::Compute => 0.1,
        GpuQueueKind::Transfer => 0.05,
    };
    let resource_cost = (pass.reads.len() + pass.writes.len()) as f32 * 0.025;
    let quality_cost = match pass.budget_hint {
        QualityTier::Disabled => 0.0,
        QualityTier::BackgroundApproximation => 0.5,
        QualityTier::NormalRuntime => 1.0,
        QualityTier::HeroHighFidelityRuntime => 1.45,
        QualityTier::ReferenceOfflineValidation => 4.0,
    };

    ((dispatch_cost + queue_cost + resource_cost) * quality_cost * 100.0).round() / 100.0
}

fn estimate_pass_cpu_milliseconds(pass: &GpuPassDesc) -> f32 {
    round_milliseconds(estimate_pass_milliseconds(pass) * 0.08 + 0.01)
}

fn estimate_pass_transient_memory_bytes(pass: &GpuPassDesc) -> u64 {
    let resource_count = pass.reads.len().saturating_add(pass.writes.len()).max(1) as u64;
    let queue_factor = match pass.queue {
        GpuQueueKind::Graphics => 512 * 1024,
        GpuQueueKind::Compute => 384 * 1024,
        GpuQueueKind::Transfer => 128 * 1024,
    };
    resource_count.saturating_mul(queue_factor)
}

fn fill_missing_expected_cost(pass: &mut GpuPassDesc) {
    if pass.expected_cost.gpu_milliseconds.is_none() {
        pass.expected_cost.gpu_milliseconds = Some(estimate_pass_milliseconds(pass));
    }
    if pass.expected_cost.cpu_milliseconds.is_none() {
        pass.expected_cost.cpu_milliseconds = Some(estimate_pass_cpu_milliseconds(pass));
    }
    if pass.expected_cost.transient_memory_bytes.is_none() {
        pass.expected_cost.transient_memory_bytes =
            Some(estimate_pass_transient_memory_bytes(pass));
    }
}

fn fill_missing_resource_accesses(
    passes: &mut [GpuPassDesc],
    resources: &BTreeMap<GpuResourceHandle, GpuResourceRecord>,
) {
    for pass in passes {
        if !pass.resource_accesses.is_empty() {
            continue;
        }
        for resource in &pass.reads {
            pass.resource_accesses.push(GpuPassResourceAccess {
                resource: *resource,
                access: infer_resource_access_type(pass, *resource, false, resources),
            });
        }
        for resource in &pass.writes {
            pass.resource_accesses.push(GpuPassResourceAccess {
                resource: *resource,
                access: infer_resource_access_type(pass, *resource, true, resources),
            });
        }
    }
}

fn infer_resource_access_type(
    pass: &GpuPassDesc,
    resource: GpuResourceHandle,
    write: bool,
    resources: &BTreeMap<GpuResourceHandle, GpuResourceRecord>,
) -> GpuResourceAccessType {
    let kind = resources
        .get(&resource)
        .map(|record| record.desc.kind)
        .unwrap_or(GpuResourceKind::External);
    if matches!(pass.dispatch, GpuDispatchKind::Copy) {
        return if write {
            GpuResourceAccessType::CopyWrite
        } else {
            GpuResourceAccessType::CopyRead
        };
    }
    match (kind, write) {
        (GpuResourceKind::AccelerationStructure, false) => {
            GpuResourceAccessType::AccelerationStructureRead
        }
        (GpuResourceKind::AccelerationStructure, true) => {
            GpuResourceAccessType::AccelerationStructureWrite
        }
        (GpuResourceKind::External, false) => GpuResourceAccessType::ExternalRead,
        (GpuResourceKind::External, true) => GpuResourceAccessType::ExternalWrite,
        (GpuResourceKind::Image2D | GpuResourceKind::Image3D, false)
            if matches!(pass.dispatch, GpuDispatchKind::Graphics(_)) =>
        {
            GpuResourceAccessType::SampledRead
        }
        (_, false) => GpuResourceAccessType::StorageRead,
        (_, true) => GpuResourceAccessType::StorageWrite,
    }
}

fn apply_barrier_dependency_hints(passes: &mut [GpuPassDesc], barriers: &[GpuBarrier]) {
    for barrier in barriers {
        let Some(pass) = passes.iter_mut().find(|pass| pass.name == barrier.to_pass) else {
            continue;
        };
        if pass.dependency_hints.iter().any(|hint| {
            hint.kind == GpuPassDependencyHintKind::After && hint.pass_name == barrier.from_pass
        }) {
            continue;
        }
        pass.dependency_hints.push(GpuPassDependencyHint::after(
            barrier.from_pass.clone(),
            format!("{:?} on resource {:?}", barrier.reason, barrier.resource),
        ));
    }
}

fn validate_graph(
    capabilities: &GpuCapabilities,
    passes: &[GpuPassDesc],
) -> GpuGraphValidationReport {
    let mut issues = Vec::new();
    if passes.is_empty() {
        issues.push(validation_issue(
            GpuValidationSeverity::Warning,
            "empty_gpu_graph",
            None,
            "GPU graph contains no scheduled work",
        ));
    }

    for pass in passes {
        if matches!(pass.dispatch, GpuDispatchKind::RayTracing(_)) && !capabilities.ray_tracing {
            issues.push(validation_issue(
                GpuValidationSeverity::Error,
                "ray_tracing_not_supported",
                Some(pass.name.clone()),
                "ray tracing pass was scheduled on a Vulkan capability set without ray tracing",
            ));
        }
        if pass.queue == GpuQueueKind::Compute && !capabilities.async_compute {
            issues.push(validation_issue(
                GpuValidationSeverity::Warning,
                "async_compute_not_supported",
                Some(pass.name.clone()),
                "compute work will need to serialize with graphics on this GPU capability set",
            ));
        }
        if pass.reads.is_empty() && pass.writes.is_empty() {
            issues.push(validation_issue(
                GpuValidationSeverity::Info,
                "pass_without_resources",
                Some(pass.name.clone()),
                "GPU pass declares no resource reads or writes",
            ));
        }
        if pass.owner.is_none() {
            issues.push(validation_issue(
                GpuValidationSeverity::Warning,
                "pass_missing_owner",
                Some(pass.name.clone()),
                "GPU pass has no owning component metadata",
            ));
        }
        if !pass.expected_cost.is_declared() {
            issues.push(validation_issue(
                GpuValidationSeverity::Warning,
                "pass_missing_expected_cost",
                Some(pass.name.clone()),
                "GPU pass has incomplete expected cost metadata",
            ));
        }
        if (!pass.reads.is_empty() || !pass.writes.is_empty()) && pass.resource_accesses.is_empty()
        {
            issues.push(validation_issue(
                GpuValidationSeverity::Warning,
                "pass_missing_resource_access",
                Some(pass.name.clone()),
                "GPU pass declares resources without explicit or inferred access metadata",
            ));
        }
    }

    for usage in collect_resource_usage(passes) {
        if usage.writers.is_empty() {
            issues.push(validation_issue(
                GpuValidationSeverity::Info,
                "external_resource_read",
                usage.readers.first().cloned(),
                "resource is read from an external or imported producer",
            ));
        }
        if usage.writers.len() > 1 && usage.readers.is_empty() {
            issues.push(validation_issue(
                GpuValidationSeverity::Warning,
                "write_only_resource_chain",
                usage.writers.last().cloned(),
                "resource is written by multiple passes without a later read in this graph",
            ));
        }
    }

    GpuGraphValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == GpuValidationSeverity::Error),
        issues,
    }
}

fn collect_resource_usage(passes: &[GpuPassDesc]) -> Vec<GpuResourceUsage> {
    let mut usage_by_resource: BTreeMap<GpuResourceHandle, GpuResourceUsage> = BTreeMap::new();
    for pass in passes {
        for resource in &pass.reads {
            usage_by_resource
                .entry(*resource)
                .or_insert_with(|| GpuResourceUsage {
                    resource: *resource,
                    readers: Vec::new(),
                    writers: Vec::new(),
                })
                .readers
                .push(pass.name.clone());
        }
        for resource in &pass.writes {
            usage_by_resource
                .entry(*resource)
                .or_insert_with(|| GpuResourceUsage {
                    resource: *resource,
                    readers: Vec::new(),
                    writers: Vec::new(),
                })
                .writers
                .push(pass.name.clone());
        }
    }

    usage_by_resource.into_values().collect()
}

fn collect_registered_resource_usage(
    usage: &[GpuResourceUsage],
    resources: &BTreeMap<GpuResourceHandle, GpuResourceRecord>,
) -> Vec<GpuRegisteredResourceUsage> {
    usage
        .iter()
        .filter_map(|usage| {
            let record = resources.get(&usage.resource)?;
            Some(GpuRegisteredResourceUsage {
                resource: usage.resource,
                label: record.desc.label.clone(),
                kind: record.desc.kind,
                owner: record.desc.owner,
                lifetime: record.desc.lifetime,
                bindless_index: record.bindless_index,
                byte_len: record.desc.byte_len,
                readers: usage.readers.clone(),
                writers: usage.writers.clone(),
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
struct PipelineCacheView<'a> {
    service_render: &'a BTreeMap<RenderPipelineHandle, GpuPipelineCacheEntry<RenderPipelineHandle>>,
    service_compute:
        &'a BTreeMap<ComputePipelineHandle, GpuPipelineCacheEntry<ComputePipelineHandle>>,
    service_ray: &'a BTreeMap<RayPipelineHandle, GpuPipelineCacheEntry<RayPipelineHandle>>,
    frame_render: &'a BTreeMap<RenderPipelineHandle, GpuPipelineCacheEntry<RenderPipelineHandle>>,
    frame_compute:
        &'a BTreeMap<ComputePipelineHandle, GpuPipelineCacheEntry<ComputePipelineHandle>>,
    frame_ray: &'a BTreeMap<RayPipelineHandle, GpuPipelineCacheEntry<RayPipelineHandle>>,
}

fn collect_pipeline_report(
    passes: &[GpuPassDesc],
    caches: PipelineCacheView<'_>,
) -> GpuPipelineReport {
    let mut pass_names_by_pipeline: BTreeMap<(GpuPipelineKind, u128), Vec<String>> =
        BTreeMap::new();
    for pass in passes {
        let Some((kind, handle)) = pipeline_key_for_dispatch(pass.dispatch) else {
            continue;
        };
        pass_names_by_pipeline
            .entry((kind, handle))
            .or_default()
            .push(pass.name.clone());
    }

    let total_pipeline_requests = pass_names_by_pipeline.values().map(Vec::len).sum::<usize>();
    let mut unknown_pipeline_count = 0;
    let mut pipelines = Vec::new();
    let mut shader_contracts = Vec::new();
    for ((kind, handle), mut pass_names) in pass_names_by_pipeline {
        pass_names.sort();
        pass_names.dedup();
        let Some((desc, request_count, cache_hit)) = pipeline_lookup(kind, handle, caches) else {
            unknown_pipeline_count += 1;
            continue;
        };
        let validation = validate_shader_contract(desc);

        pipelines.push(GpuPipelineUsage {
            kind,
            handle,
            label: desc.label.clone(),
            shader_key: desc.shader_key.clone(),
            quality_tier: desc.quality_tier,
            permutation_key: desc.permutation.key(),
            permutation_defines: desc.permutation.defines.clone(),
            pass_names,
            request_count,
            cache_hit,
        });
        shader_contracts.push(GpuShaderPipelineContract {
            kind,
            handle,
            label: desc.label.clone(),
            shader_key: desc.shader_key.clone(),
            permutation_key: desc.permutation.key(),
            contract: desc.shader_contract.clone(),
            validation,
        });
    }

    let cache_hit_count = pipelines
        .iter()
        .filter(|pipeline| pipeline.cache_hit)
        .count();
    let shader_validation_error_count = shader_contracts
        .iter()
        .flat_map(|contract| contract.validation.issues.iter())
        .filter(|issue| issue.severity == GpuValidationSeverity::Error)
        .count();
    let shader_validation_warning_count = shader_contracts
        .iter()
        .flat_map(|contract| contract.validation.issues.iter())
        .filter(|issue| issue.severity == GpuValidationSeverity::Warning)
        .count();
    let shader_variant_count = shader_contracts
        .iter()
        .map(|contract| (&contract.shader_key, &contract.permutation_key))
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    GpuPipelineReport {
        pipelines,
        total_pipeline_requests,
        cache_hit_count,
        unknown_pipeline_count,
        shader_contracts,
        shader_variant_count,
        shader_validation_error_count,
        shader_validation_warning_count,
    }
}

fn validate_shader_contract(desc: &GpuPipelineDesc) -> GpuShaderValidationReport {
    let mut issues = Vec::new();
    if desc.shader_contract.debug_name.trim().is_empty() {
        issues.push(shader_validation_issue(
            GpuValidationSeverity::Error,
            "shader_missing_debug_name",
            "shader contract must declare a human-readable debug name",
        ));
    }
    if desc.shader_contract.resource_layout.is_empty() {
        issues.push(shader_validation_issue(
            GpuValidationSeverity::Error,
            "shader_missing_resource_layout",
            "shader contract must declare its resource layout",
        ));
    }
    if desc.shader_contract.specialization_constants.is_empty() {
        issues.push(shader_validation_issue(
            GpuValidationSeverity::Error,
            "shader_missing_specialization_constants",
            "shader contract must declare specialization constants",
        ));
    }
    if desc.shader_contract.supported_quality_tiers.is_empty()
        || !desc
            .shader_contract
            .supported_quality_tiers
            .contains(&desc.quality_tier)
    {
        issues.push(shader_validation_issue(
            GpuValidationSeverity::Error,
            "shader_quality_tier_not_supported",
            "shader contract must include the pipeline quality tier",
        ));
    }
    if desc.shader_contract.expected_subgroup_size.is_none() {
        issues.push(shader_validation_issue(
            GpuValidationSeverity::Warning,
            "shader_missing_subgroup_assumption",
            "shader contract should declare expected subgroup assumptions",
        ));
    }
    if desc.shader_contract.validation_tests.is_empty() {
        issues.push(shader_validation_issue(
            GpuValidationSeverity::Error,
            "shader_missing_validation_tests",
            "shader contract must name validation tests",
        ));
    }
    if desc.shader_contract.artifact == GpuShaderArtifactKind::Unknown {
        issues.push(shader_validation_issue(
            GpuValidationSeverity::Warning,
            "shader_artifact_unknown",
            "shader artifact kind could not be inferred from the shader key",
        ));
    }

    GpuShaderValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == GpuValidationSeverity::Error),
        issues,
    }
}

fn shader_validation_issue(
    severity: GpuValidationSeverity,
    code: &'static str,
    message: &'static str,
) -> GpuShaderValidationIssue {
    GpuShaderValidationIssue {
        severity,
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn pipeline_key_for_dispatch(dispatch: GpuDispatchKind) -> Option<(GpuPipelineKind, u128)> {
    match dispatch {
        GpuDispatchKind::Graphics(handle) => Some((GpuPipelineKind::Render, handle.0)),
        GpuDispatchKind::Compute(handle) => Some((GpuPipelineKind::Compute, handle.0)),
        GpuDispatchKind::RayTracing(handle) => Some((GpuPipelineKind::RayTracing, handle.0)),
        GpuDispatchKind::Copy => None,
    }
}

fn pipeline_lookup<'a>(
    kind: GpuPipelineKind,
    handle: u128,
    caches: PipelineCacheView<'a>,
) -> Option<(&'a GpuPipelineDesc, u64, bool)> {
    match kind {
        GpuPipelineKind::Render => {
            let handle = RenderPipelineHandle(handle);
            caches
                .frame_render
                .get(&handle)
                .map(|entry| (&entry.desc, entry.request_count, entry.request_count > 1))
                .or_else(|| {
                    caches
                        .service_render
                        .get(&handle)
                        .map(|entry| (&entry.desc, entry.request_count, true))
                })
        }
        GpuPipelineKind::Compute => {
            let handle = ComputePipelineHandle(handle);
            caches
                .frame_compute
                .get(&handle)
                .map(|entry| (&entry.desc, entry.request_count, entry.request_count > 1))
                .or_else(|| {
                    caches
                        .service_compute
                        .get(&handle)
                        .map(|entry| (&entry.desc, entry.request_count, true))
                })
        }
        GpuPipelineKind::RayTracing => {
            let handle = RayPipelineHandle(handle);
            caches
                .frame_ray
                .get(&handle)
                .map(|entry| (&entry.desc, entry.request_count, entry.request_count > 1))
                .or_else(|| {
                    caches
                        .service_ray
                        .get(&handle)
                        .map(|entry| (&entry.desc, entry.request_count, true))
                })
        }
    }
}

fn build_descriptor_report(usage: &[GpuRegisteredResourceUsage]) -> GpuDescriptorReport {
    let mut bindings = usage
        .iter()
        .map(|usage| GpuDescriptorBinding {
            resource: usage.resource,
            label: usage.label.clone(),
            owner: usage.owner,
            descriptor_kind: descriptor_kind_for_usage(usage),
            binding_slot: 0,
            bindless_index: usage.bindless_index,
            pass_names: descriptor_pass_names(usage),
            read_only: usage.writers.is_empty(),
        })
        .collect::<Vec<_>>();

    bindings.sort_by(|left, right| {
        left.bindless_index
            .is_none()
            .cmp(&right.bindless_index.is_none())
            .then_with(|| left.bindless_index.cmp(&right.bindless_index))
            .then_with(|| left.owner.cmp(&right.owner))
            .then_with(|| left.resource.cmp(&right.resource))
    });

    for (slot, binding) in bindings.iter_mut().enumerate() {
        binding.binding_slot = slot.min(u32::MAX as usize) as u32;
    }

    let bindless_binding_count = bindings
        .iter()
        .filter(|binding| binding.bindless_index.is_some())
        .count();
    let owner_set_count = bindings
        .iter()
        .filter(|binding| binding.descriptor_kind != GpuDescriptorKind::External)
        .map(|binding| binding.owner)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let descriptor_set_count = owner_set_count + usize::from(bindless_binding_count > 0);
    let descriptor_table_bytes = bindings
        .iter()
        .filter(|binding| binding.descriptor_kind != GpuDescriptorKind::External)
        .count()
        .saturating_mul(32) as u64;

    GpuDescriptorReport {
        bindings,
        bindless_binding_count,
        descriptor_set_count,
        descriptor_table_bytes,
    }
}

fn descriptor_kind_for_usage(usage: &GpuRegisteredResourceUsage) -> GpuDescriptorKind {
    match usage.kind {
        GpuResourceKind::Buffer => GpuDescriptorKind::StorageBuffer,
        GpuResourceKind::Image2D | GpuResourceKind::Image3D => {
            if usage.writers.is_empty() {
                GpuDescriptorKind::SampledImage
            } else {
                GpuDescriptorKind::StorageImage
            }
        }
        GpuResourceKind::AccelerationStructure => GpuDescriptorKind::AccelerationStructure,
        GpuResourceKind::External => GpuDescriptorKind::External,
    }
}

fn descriptor_pass_names(usage: &GpuRegisteredResourceUsage) -> Vec<String> {
    let mut pass_names = usage
        .readers
        .iter()
        .chain(usage.writers.iter())
        .cloned()
        .collect::<Vec<_>>();
    pass_names.sort();
    pass_names.dedup();
    pass_names
}

fn plan_transient_memory(
    passes: &[GpuPassDesc],
    resources: &BTreeMap<GpuResourceHandle, GpuResourceRecord>,
) -> GpuMemoryPlan {
    let mut lifetimes: BTreeMap<GpuResourceHandle, ResourcePassLifetime> = BTreeMap::new();
    for (pass_index, pass) in passes.iter().enumerate() {
        for resource in pass.reads.iter().chain(pass.writes.iter()) {
            lifetimes
                .entry(*resource)
                .and_modify(|lifetime| {
                    lifetime.first_pass_index = lifetime.first_pass_index.min(pass_index);
                    lifetime.last_pass_index = lifetime.last_pass_index.max(pass_index);
                })
                .or_insert(ResourcePassLifetime {
                    first_pass_index: pass_index,
                    last_pass_index: pass_index,
                });
        }
    }

    let mut candidates = lifetimes
        .into_iter()
        .filter_map(|(resource, lifetime)| {
            let record = resources.get(&resource)?;
            (record.desc.lifetime == GpuResourceLifetime::Transient && record.desc.byte_len > 0)
                .then(|| MemoryCandidate {
                    resource,
                    label: record.desc.label.clone(),
                    owner: record.desc.owner,
                    byte_len: record.desc.byte_len,
                    first_pass_index: lifetime.first_pass_index,
                    last_pass_index: lifetime.last_pass_index,
                })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.first_pass_index
            .cmp(&right.first_pass_index)
            .then_with(|| left.last_pass_index.cmp(&right.last_pass_index))
            .then_with(|| left.resource.cmp(&right.resource))
    });

    let unaliased_transient_bytes = candidates
        .iter()
        .map(|candidate| candidate.byte_len)
        .sum::<u64>();
    let mut groups: Vec<MemoryAliasGroup> = Vec::new();
    let mut pending_allocations = Vec::new();

    for candidate in candidates {
        let alias_group = groups
            .iter()
            .position(|group| group.last_pass_index < candidate.first_pass_index)
            .unwrap_or_else(|| {
                groups.push(MemoryAliasGroup {
                    last_pass_index: 0,
                    byte_len: 0,
                    offset_bytes: 0,
                });
                groups.len() - 1
            });

        let group = &mut groups[alias_group];
        group.last_pass_index = candidate.last_pass_index;
        group.byte_len = group.byte_len.max(candidate.byte_len);
        pending_allocations.push((
            alias_group,
            GpuMemoryAllocation {
                resource: candidate.resource,
                label: candidate.label,
                owner: candidate.owner,
                byte_len: candidate.byte_len,
                first_pass_index: candidate.first_pass_index,
                last_pass_index: candidate.last_pass_index,
                first_pass_name: passes[candidate.first_pass_index].name.clone(),
                last_pass_name: passes[candidate.last_pass_index].name.clone(),
                alias_group,
                offset_bytes: 0,
            },
        ));
    }

    let mut total_transient_bytes = 0;
    for group in &mut groups {
        group.offset_bytes = total_transient_bytes;
        total_transient_bytes = total_transient_bytes.saturating_add(group.byte_len);
    }

    let allocations = pending_allocations
        .into_iter()
        .map(|(alias_group, mut allocation)| {
            allocation.offset_bytes = groups[alias_group].offset_bytes;
            allocation
        })
        .collect::<Vec<_>>();

    GpuMemoryPlan {
        transient_resource_count: allocations.len(),
        alias_group_count: groups.len(),
        total_transient_bytes,
        unaliased_transient_bytes,
        aliased_bytes_saved: unaliased_transient_bytes.saturating_sub(total_transient_bytes),
        allocations,
    }
}

#[derive(Clone, Copy)]
struct ResourcePassLifetime {
    first_pass_index: usize,
    last_pass_index: usize,
}

struct MemoryCandidate {
    resource: GpuResourceHandle,
    label: String,
    owner: Option<ModuleId>,
    byte_len: u64,
    first_pass_index: usize,
    last_pass_index: usize,
}

struct MemoryAliasGroup {
    last_pass_index: usize,
    byte_len: u64,
    offset_bytes: u64,
}

fn collect_queue_workloads(timing: &[GpuPassTiming]) -> Vec<GpuQueueWorkload> {
    let mut workload_by_queue: BTreeMap<u8, GpuQueueWorkload> = BTreeMap::new();
    for timing in timing {
        let key = queue_index(timing.queue);
        let workload = workload_by_queue
            .entry(key)
            .or_insert_with(|| GpuQueueWorkload {
                queue: timing.queue,
                pass_count: 0,
                estimated_gpu_milliseconds: 0.0,
            });
        workload.pass_count += 1;
        workload.estimated_gpu_milliseconds += timing.estimated_gpu_milliseconds;
    }

    workload_by_queue
        .into_values()
        .map(|mut workload| {
            workload.estimated_gpu_milliseconds =
                (workload.estimated_gpu_milliseconds * 100.0).round() / 100.0;
            workload
        })
        .collect()
}

fn build_execution_plan(
    passes: &[GpuPassDesc],
    timing: &[GpuPassTiming],
    barriers: &[GpuBarrier],
) -> GpuExecutionPlan {
    let mut pass_index_by_name = BTreeMap::new();
    for (index, pass) in passes.iter().enumerate() {
        pass_index_by_name.insert(pass.name.clone(), index);
    }

    let mut queue_available = [0.0_f32; 3];
    let mut pass_end_times = vec![0.0; passes.len()];
    let mut scheduled_passes = Vec::new();
    let mut cross_queue_wait_count = 0;
    let mut queue_idle_milliseconds = 0.0;

    for (pass_index, pass) in passes.iter().enumerate() {
        let queue = queue_index(pass.queue) as usize;
        let duration = timing
            .get(pass_index)
            .map(|timing| timing.estimated_gpu_milliseconds)
            .unwrap_or_else(|| estimate_pass_milliseconds(pass));
        let dependencies = barriers
            .iter()
            .filter(|barrier| barrier.to_pass == pass.name)
            .filter_map(|barrier| {
                let from_pass_index = *pass_index_by_name.get(&barrier.from_pass)?;
                let from_queue = passes[from_pass_index].queue;
                Some((
                    from_pass_index,
                    GpuPassDependency {
                        from_pass: barrier.from_pass.clone(),
                        from_queue,
                        resource: barrier.resource,
                        reason: barrier.reason,
                    },
                ))
            })
            .collect::<Vec<_>>();

        cross_queue_wait_count += dependencies
            .iter()
            .filter(|(_, dependency)| dependency.from_queue != pass.queue)
            .count();
        let dependency_ready = dependencies
            .iter()
            .map(|(from_pass_index, _)| pass_end_times[*from_pass_index])
            .fold(0.0, f32::max);
        let queue_ready = queue_available[queue];
        let start_milliseconds = round_milliseconds(queue_ready.max(dependency_ready));
        if start_milliseconds > queue_ready {
            queue_idle_milliseconds += start_milliseconds - queue_ready;
        }
        let end_milliseconds = round_milliseconds(start_milliseconds + duration);
        queue_available[queue] = end_milliseconds;
        pass_end_times[pass_index] = end_milliseconds;

        scheduled_passes.push(GpuScheduledPass {
            pass_index,
            pass_name: pass.name.clone(),
            queue: pass.queue,
            start_milliseconds,
            end_milliseconds,
            duration_milliseconds: duration,
            wait_dependencies: dependencies
                .into_iter()
                .map(|(_, dependency)| dependency)
                .collect(),
        });
    }

    let total_work_milliseconds = timing
        .iter()
        .map(|timing| timing.estimated_gpu_milliseconds)
        .sum::<f32>();
    let estimated_wall_milliseconds =
        round_milliseconds(queue_available.into_iter().fold(0.0, f32::max));
    let overlapped_work_milliseconds =
        round_milliseconds((total_work_milliseconds - estimated_wall_milliseconds).max(0.0));

    GpuExecutionPlan {
        scheduled_passes,
        total_work_milliseconds: round_milliseconds(total_work_milliseconds),
        estimated_wall_milliseconds,
        overlapped_work_milliseconds,
        cross_queue_wait_count,
        queue_idle_milliseconds: round_milliseconds(queue_idle_milliseconds),
    }
}

fn estimate_transient_memory_bytes(resource_usage: &[GpuResourceUsage]) -> u64 {
    resource_usage
        .iter()
        .filter(|usage| !usage.writers.is_empty())
        .map(|usage| {
            let reader_factor = usage.readers.len().max(1) as u64;
            let writer_factor = usage.writers.len().max(1) as u64;
            512 * 1024 * (reader_factor + writer_factor)
        })
        .sum()
}

fn queue_index(queue: GpuQueueKind) -> u8 {
    match queue {
        GpuQueueKind::Graphics => 0,
        GpuQueueKind::Compute => 1,
        GpuQueueKind::Transfer => 2,
    }
}

fn round_milliseconds(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn validation_issue(
    severity: GpuValidationSeverity,
    code: &'static str,
    pass_name: Option<String>,
    message: &'static str,
) -> GpuGraphValidationIssue {
    GpuGraphValidationIssue {
        severity,
        code: code.to_string(),
        pass_name,
        message: message.to_string(),
    }
}

fn upsert_pipeline_cache<H>(
    cache: &mut BTreeMap<H, GpuPipelineCacheEntry<H>>,
    handle: H,
    desc: GpuPipelineDesc,
) -> bool
where
    H: Copy + Ord,
{
    if let Some(entry) = cache.get_mut(&handle) {
        entry.request_count = entry.request_count.saturating_add(1);
        entry.desc = desc;
        return true;
    }

    cache.insert(
        handle,
        GpuPipelineCacheEntry {
            handle,
            desc,
            request_count: 1,
        },
    );
    false
}

fn gpu_resource_id(desc: &GpuResourceDesc) -> u128 {
    let mut high = StableGpuHasher::new(0x6a09_e667_f3bc_c909);
    high.write_str("ashfall-gpu-resource-high");
    high.write_resource_kind(desc.kind);
    high.write_str(&desc.label);
    high.write_u64(desc.byte_len);
    high.write_optional_u64(desc.owner);
    high.write_resource_lifetime(desc.lifetime);
    high.write_resource_residency(desc.residency_policy);
    high.write_u8(u8::from(desc.bindless));

    let mut low = StableGpuHasher::new(0xbb67_ae85_84ca_a73b);
    low.write_str("ashfall-gpu-resource-low");
    low.write_str(&desc.label);
    low.write_resource_lifetime(desc.lifetime);
    low.write_resource_residency(desc.residency_policy);
    low.write_resource_kind(desc.kind);
    low.write_u64(desc.byte_len);
    low.write_optional_u64(desc.owner);

    ((high.finish() as u128) << 64) | low.finish() as u128
}

fn gpu_pipeline_id(kind: &str, desc: &GpuPipelineDesc) -> u128 {
    let mut high = StableGpuHasher::new(0x3c6e_f372_fe94_f82b);
    high.write_str("ashfall-gpu-pipeline-high");
    high.write_str(kind);
    high.write_str(&desc.label);
    high.write_str(&desc.shader_key);
    high.write_quality(desc.quality_tier);
    high.write_shader_permutation(&desc.permutation);
    high.write_shader_contract(&desc.shader_contract);

    let mut low = StableGpuHasher::new(0xa54f_f53a_5f1d_36f1);
    low.write_str("ashfall-gpu-pipeline-low");
    low.write_str(&desc.shader_key);
    low.write_quality(desc.quality_tier);
    low.write_shader_permutation(&desc.permutation);
    low.write_shader_contract(&desc.shader_contract);
    low.write_str(&desc.label);
    low.write_str(kind);

    ((high.finish() as u128) << 64) | low.finish() as u128
}

#[derive(Clone, Copy, Debug)]
struct StableGpuHasher {
    state: u64,
}

impl StableGpuHasher {
    const FNV_PRIME: u64 = 1_099_511_628_211;

    fn new(seed: u64) -> Self {
        Self {
            state: 14_695_981_039_346_656_037 ^ seed,
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.write_usize(bytes.len());
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(Self::FNV_PRIME);
        }
    }

    fn write_str(&mut self, value: &str) {
        self.write_bytes(value.as_bytes());
    }

    fn write_u8(&mut self, value: u8) {
        self.write_bytes(&[value]);
    }

    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.state ^= value as u64;
        self.state = self.state.wrapping_mul(Self::FNV_PRIME);
    }

    fn write_optional_u64(&mut self, value: Option<u64>) {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_u64(value);
            }
            None => self.write_u8(0),
        }
    }

    fn write_resource_kind(&mut self, kind: GpuResourceKind) {
        let value = match kind {
            GpuResourceKind::Buffer => 1,
            GpuResourceKind::Image2D => 2,
            GpuResourceKind::Image3D => 3,
            GpuResourceKind::AccelerationStructure => 4,
            GpuResourceKind::External => 5,
        };
        self.write_u8(value);
    }

    fn write_resource_lifetime(&mut self, lifetime: GpuResourceLifetime) {
        let value = match lifetime {
            GpuResourceLifetime::Imported => 1,
            GpuResourceLifetime::Persistent => 2,
            GpuResourceLifetime::Transient => 3,
        };
        self.write_u8(value);
    }

    fn write_resource_residency(&mut self, residency: GpuResourceResidencyPolicy) {
        let value = match residency {
            GpuResourceResidencyPolicy::StaticResident => 1,
            GpuResourceResidencyPolicy::StreamedResident => 2,
            GpuResourceResidencyPolicy::Transient => 3,
            GpuResourceResidencyPolicy::Readback => 4,
        };
        self.write_u8(value);
    }

    fn write_shader_permutation(&mut self, permutation: &GpuShaderPermutation) {
        self.write_usize(permutation.defines.len());
        for define in &permutation.defines {
            self.write_str(define);
        }
    }

    fn write_shader_contract(&mut self, contract: &GpuShaderContract) {
        self.write_shader_artifact(contract.artifact);
        self.write_usize(contract.resource_layout.len());
        for binding in &contract.resource_layout {
            self.write_u64(u64::from(binding.set));
            self.write_u64(u64::from(binding.binding));
            self.write_str(&binding.name);
            self.write_shader_binding_kind(binding.kind);
        }
        self.write_usize(contract.specialization_constants.len());
        for constant in &contract.specialization_constants {
            self.write_str(&constant.name);
            self.write_str(&constant.value);
        }
        self.write_usize(contract.supported_quality_tiers.len());
        for quality_tier in &contract.supported_quality_tiers {
            self.write_quality(*quality_tier);
        }
        self.write_optional_u64(contract.expected_subgroup_size.map(u64::from));
        self.write_shader_precision(contract.precision);
        self.write_str(&contract.debug_name);
        self.write_usize(contract.validation_tests.len());
        for validation_test in &contract.validation_tests {
            self.write_str(validation_test);
        }
        self.write_u8(u8::from(contract.hot_reload_allowed));
    }

    fn write_shader_artifact(&mut self, artifact: GpuShaderArtifactKind) {
        let value = match artifact {
            GpuShaderArtifactKind::SpirV => 1,
            GpuShaderArtifactKind::RustGpuSource => 2,
            GpuShaderArtifactKind::EngineShaderSource => 3,
            GpuShaderArtifactKind::Unknown => 4,
        };
        self.write_u8(value);
    }

    fn write_shader_binding_kind(&mut self, kind: GpuShaderResourceBindingKind) {
        let value = match kind {
            GpuShaderResourceBindingKind::UniformBuffer => 1,
            GpuShaderResourceBindingKind::StorageBuffer => 2,
            GpuShaderResourceBindingKind::SampledImage => 3,
            GpuShaderResourceBindingKind::StorageImage => 4,
            GpuShaderResourceBindingKind::AccelerationStructure => 5,
            GpuShaderResourceBindingKind::BindlessTable => 6,
            GpuShaderResourceBindingKind::PushConstants => 7,
        };
        self.write_u8(value);
    }

    fn write_shader_precision(&mut self, precision: GpuShaderPrecision) {
        let value = match precision {
            GpuShaderPrecision::Relaxed => 1,
            GpuShaderPrecision::Mixed => 2,
            GpuShaderPrecision::FullFloat => 3,
        };
        self.write_u8(value);
    }

    fn write_quality(&mut self, quality_tier: QualityTier) {
        let value = match quality_tier {
            QualityTier::Disabled => 0,
            QualityTier::BackgroundApproximation => 1,
            QualityTier::NormalRuntime => 2,
            QualityTier::HeroHighFidelityRuntime => 3,
            QualityTier::ReferenceOfflineValidation => 4,
        };
        self.write_u8(value);
    }

    fn finish(self) -> u64 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_validation_rejects_unsupported_ray_tracing() {
        let services = GpuServices::new_vulkan();
        let mut graph = services.begin_frame(1);
        let ray_pipeline = graph
            .create_ray_pipeline(GpuPipelineDesc::new(
                "rt reflections",
                "test/rt_reflections.rgen+rmiss+rhit",
                QualityTier::HeroHighFidelityRuntime,
            ))
            .handle;
        graph.add_pass(
            GpuPassDesc::new(
                "rt_reflections",
                GpuQueueKind::Compute,
                GpuDispatchKind::RayTracing(ray_pipeline),
                QualityTier::HeroHighFidelityRuntime,
            )
            .reads([GpuResourceHandle(1)])
            .writes([GpuResourceHandle(2)]),
        );

        let report = services.submit(graph);

        assert!(!report.validation.passed);
        assert!(report.validation.issues.iter().any(|issue| {
            issue.severity == GpuValidationSeverity::Error
                && issue.code == "ray_tracing_not_supported"
        }));
    }

    #[test]
    fn capability_report_surfaces_vulkan_limits_features_and_fallbacks() {
        let services = GpuServices::new_vulkan();

        let report = services.capabilities().capability_report();

        assert!(report.passed());
        assert_eq!(report.backend, GpuBackend::Vulkan);
        assert_eq!(report.vulkan_version.label(), "1.3.0");
        assert_eq!(
            report.selected_tier,
            GpuCapabilityTier::DescriptorIndexedCompute
        );
        assert!(report.total_device_memory_bytes() >= 6 * 1024 * 1024 * 1024);
        assert!(report.limits.max_descriptor_count > 0);
        assert!(report.limits.max_bindless_resources > 0);
        assert!(report.fallback_count >= 3);
        assert_eq!(report.missing_required_feature_count, 0);
        assert!(
            report
                .feature("ray_tracing")
                .is_some_and(|feature| !feature.supported
                    && feature.fallback == Some(GpuFeatureFallback::ComputeFallback))
        );
        assert!(
            report
                .feature("descriptor_indexing")
                .is_some_and(|feature| feature.supported)
        );
    }

    #[test]
    fn graph_builder_stamps_owner_cost_labels_and_dependency_hints() {
        let services = GpuServices::new_vulkan();
        let mut graph = services.begin_frame(7);
        let scratch = graph.declare_resource(
            GpuResourceDesc::new("component scratch", GpuResourceKind::Buffer, 4096)
                .owned_by(77)
                .with_lifetime(GpuResourceLifetime::Transient),
        );
        let previous_owner = graph.push_component_owner(77);
        graph.add_pass(
            GpuPassDesc::new(
                "upload_scratch",
                GpuQueueKind::Transfer,
                GpuDispatchKind::Copy,
                QualityTier::NormalRuntime,
            )
            .writes([scratch]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "consume_scratch",
                GpuQueueKind::Transfer,
                GpuDispatchKind::Copy,
                QualityTier::NormalRuntime,
            )
            .reads([scratch]),
        );
        graph.pop_component_owner(previous_owner);

        let report = services.submit(graph);

        assert!(report.validation.passed);
        assert_eq!(report.passes.len(), 2);
        assert!(
            report
                .passes
                .iter()
                .all(|pass| pass.owner == Some(77) && pass.expected_cost.is_declared())
        );
        assert!(report.declared_resources.iter().any(|resource| {
            resource.desc.residency_policy == GpuResourceResidencyPolicy::Transient
        }));
        assert_eq!(report.passes[0].debug_label, "upload_scratch");
        assert!(report.passes[0].resource_accesses.iter().any(|access| {
            access.resource == scratch && access.access == GpuResourceAccessType::CopyWrite
        }));
        let consume = report
            .passes
            .iter()
            .find(|pass| pass.name == "consume_scratch")
            .expect("consume pass should be reported");
        assert!(consume.resource_accesses.iter().any(|access| {
            access.resource == scratch && access.access == GpuResourceAccessType::CopyRead
        }));
        assert!(consume.dependency_hints.iter().any(|hint| {
            hint.kind == GpuPassDependencyHintKind::After && hint.pass_name == "upload_scratch"
        }));
    }

    #[test]
    fn frame_report_collects_resource_usage_and_queue_workloads() {
        let services = GpuServices::new_vulkan();
        let mut graph = services.begin_frame(2);
        let scene = graph.declare_resource(
            GpuResourceDesc::new("frame scene", GpuResourceKind::Buffer, 4096)
                .owned_by(10)
                .with_lifetime(GpuResourceLifetime::Imported),
        );
        let depth = graph.declare_resource(
            GpuResourceDesc::new("frame depth", GpuResourceKind::Image2D, 8192)
                .owned_by(10)
                .with_lifetime(GpuResourceLifetime::Transient),
        );
        let cull_results = graph.declare_resource(
            GpuResourceDesc::new("frame cull results", GpuResourceKind::Buffer, 2048)
                .owned_by(10)
                .with_lifetime(GpuResourceLifetime::Transient),
        );
        let depth_pipeline = graph
            .create_render_pipeline(GpuPipelineDesc::new(
                "depth",
                "test/depth.vert+frag",
                QualityTier::NormalRuntime,
            ))
            .handle;
        let cull_pipeline_desc =
            GpuPipelineDesc::new("cull", "test/cull.comp", QualityTier::NormalRuntime);
        let first_cull_pipeline = graph.create_compute_pipeline(cull_pipeline_desc.clone());
        let second_cull_pipeline = graph.create_compute_pipeline(cull_pipeline_desc);
        assert!(!first_cull_pipeline.cache_hit);
        assert!(second_cull_pipeline.cache_hit);
        graph.add_pass(
            GpuPassDesc::new(
                "depth",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(depth_pipeline),
                QualityTier::NormalRuntime,
            )
            .reads([scene])
            .writes([depth]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "cull",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(second_cull_pipeline.handle),
                QualityTier::NormalRuntime,
            )
            .reads([depth])
            .writes([cull_results]),
        );

        let report = services.submit(graph);

        assert!(report.validation.passed);
        assert!(
            report
                .resource_usage
                .iter()
                .any(|usage| usage.resource == depth
                    && usage.readers == vec!["cull".to_string()]
                    && usage.writers == vec!["depth".to_string()])
        );
        assert_eq!(report.declared_resources.len(), 3);
        assert!(
            report
                .registered_resource_usage
                .iter()
                .any(|usage| usage.resource == depth && usage.owner == Some(10))
        );
        assert_eq!(report.descriptor_report.bindings.len(), 3);
        assert_eq!(report.descriptor_report.descriptor_set_count, 1);
        assert_eq!(report.descriptor_report.bindless_binding_count, 0);
        assert!(
            report
                .descriptor_report
                .bindings
                .iter()
                .any(|binding| binding.resource == depth
                    && binding.descriptor_kind == GpuDescriptorKind::StorageImage
                    && binding.pass_names == vec!["cull".to_string(), "depth".to_string()])
        );
        assert_eq!(report.pipeline_report.pipelines.len(), 2);
        assert_eq!(report.pipeline_report.total_pipeline_requests, 2);
        assert_eq!(report.pipeline_report.cache_hit_count, 1);
        assert_eq!(report.pipeline_report.unknown_pipeline_count, 0);
        assert_eq!(report.pipeline_report.shader_contracts.len(), 2);
        assert_eq!(report.pipeline_report.shader_variant_count, 2);
        assert_eq!(report.pipeline_report.shader_validation_error_count, 0);
        assert!(
            report
                .pipeline_report
                .shader_contracts
                .iter()
                .all(|contract| contract.validation.passed
                    && !contract.contract.resource_layout.is_empty()
                    && !contract.contract.specialization_constants.is_empty()
                    && !contract.contract.validation_tests.is_empty())
        );
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "cull" && pipeline.cache_hit && pipeline.request_count == 2
        }));
        assert_eq!(report.memory_plan.transient_resource_count, 2);
        assert_eq!(report.memory_plan.unaliased_transient_bytes, 10_240);
        assert_eq!(report.memory_plan.total_transient_bytes, 10_240);
        assert_eq!(
            report.transient_memory_bytes,
            report.memory_plan.total_transient_bytes
        );
        assert!(
            report.queue_workloads.iter().any(
                |workload| workload.queue == GpuQueueKind::Graphics && workload.pass_count == 1
            )
        );
        assert_eq!(report.execution_plan.scheduled_passes.len(), 2);
        assert_eq!(report.execution_plan.cross_queue_wait_count, 1);
        let cull_schedule = report
            .execution_plan
            .scheduled_passes
            .iter()
            .find(|pass| pass.pass_name == "cull")
            .expect("cull pass should be scheduled");
        assert!(
            cull_schedule
                .wait_dependencies
                .iter()
                .any(|dependency| dependency.from_pass == "depth"
                    && dependency.from_queue == GpuQueueKind::Graphics)
        );
        assert!(report.transient_memory_bytes > 0);
    }

    #[test]
    fn execution_plan_overlaps_independent_queue_work() {
        let services = GpuServices::new_vulkan();
        let mut graph = services.begin_frame(5);
        let graphics_target = graph.declare_resource(
            GpuResourceDesc::new("graphics target", GpuResourceKind::Image2D, 4096)
                .with_lifetime(GpuResourceLifetime::Transient),
        );
        let compute_target = graph.declare_resource(
            GpuResourceDesc::new("compute target", GpuResourceKind::Buffer, 4096)
                .with_lifetime(GpuResourceLifetime::Transient),
        );
        let graphics_pipeline = graph
            .create_render_pipeline(GpuPipelineDesc::new(
                "independent graphics",
                "test/independent_graphics.vert+frag",
                QualityTier::NormalRuntime,
            ))
            .handle;
        let compute_pipeline = graph
            .create_compute_pipeline(GpuPipelineDesc::new(
                "independent compute",
                "test/independent_compute.comp",
                QualityTier::NormalRuntime,
            ))
            .handle;
        graph.add_pass(
            GpuPassDesc::new(
                "independent_graphics",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(graphics_pipeline),
                QualityTier::NormalRuntime,
            )
            .writes([graphics_target]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "independent_compute",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(compute_pipeline),
                QualityTier::NormalRuntime,
            )
            .writes([compute_target]),
        );

        let report = services.submit(graph);

        assert_eq!(report.execution_plan.cross_queue_wait_count, 0);
        assert!(report.execution_plan.estimated_wall_milliseconds < report.total_gpu_milliseconds);
        assert!(report.execution_plan.overlapped_work_milliseconds > 0.0);
        assert_eq!(report.pipeline_report.pipelines.len(), 2);
        assert_eq!(report.pipeline_report.unknown_pipeline_count, 0);
        assert!(
            report
                .execution_plan
                .scheduled_passes
                .iter()
                .all(|pass| pass.start_milliseconds == 0.0)
        );
    }

    #[test]
    fn memory_plan_aliases_non_overlapping_transient_resources() {
        let services = GpuServices::new_vulkan();
        let mut graph = services.begin_frame(4);
        let first = graph.declare_resource(
            GpuResourceDesc::new("first scratch", GpuResourceKind::Buffer, 1024)
                .with_lifetime(GpuResourceLifetime::Transient),
        );
        let second = graph.declare_resource(
            GpuResourceDesc::new("second scratch", GpuResourceKind::Buffer, 2048)
                .with_lifetime(GpuResourceLifetime::Transient),
        );
        let bridge = graph.declare_resource(
            GpuResourceDesc::new("bridge", GpuResourceKind::Buffer, 512)
                .with_lifetime(GpuResourceLifetime::Persistent),
        );
        let write_first_pipeline = graph
            .create_compute_pipeline(GpuPipelineDesc::new(
                "write first",
                "test/write_first.comp",
                QualityTier::NormalRuntime,
            ))
            .handle;
        let read_first_pipeline = graph
            .create_compute_pipeline(GpuPipelineDesc::new(
                "read first",
                "test/read_first.comp",
                QualityTier::NormalRuntime,
            ))
            .handle;
        let write_second_pipeline = graph
            .create_compute_pipeline(GpuPipelineDesc::new(
                "write second",
                "test/write_second.comp",
                QualityTier::NormalRuntime,
            ))
            .handle;
        graph.add_pass(
            GpuPassDesc::new(
                "write_first",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(write_first_pipeline),
                QualityTier::NormalRuntime,
            )
            .writes([first, bridge]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "read_first",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(read_first_pipeline),
                QualityTier::NormalRuntime,
            )
            .reads([first])
            .writes([bridge]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "write_second",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(write_second_pipeline),
                QualityTier::NormalRuntime,
            )
            .reads([bridge])
            .writes([second]),
        );

        let report = services.submit(graph);

        assert_eq!(report.memory_plan.transient_resource_count, 2);
        assert_eq!(report.pipeline_report.unknown_pipeline_count, 0);
        assert_eq!(report.memory_plan.alias_group_count, 1);
        assert_eq!(report.memory_plan.unaliased_transient_bytes, 3072);
        assert_eq!(report.memory_plan.total_transient_bytes, 2048);
        assert_eq!(report.memory_plan.aliased_bytes_saved, 1024);
        assert_eq!(
            report
                .memory_plan
                .allocations
                .iter()
                .map(|allocation| allocation.alias_group)
                .collect::<Vec<_>>(),
            vec![0, 0]
        );
    }

    #[test]
    fn services_register_resources_and_report_owned_usage() {
        let mut services = GpuServices::new_vulkan();
        let scene_buffer = services.register_resource(
            GpuResourceDesc::new("scene instances", GpuResourceKind::Buffer, 4096)
                .owned_by(10)
                .with_lifetime(GpuResourceLifetime::Persistent)
                .bindless(),
        );
        let lighting_target = services.register_resource(
            GpuResourceDesc::new("lighting target", GpuResourceKind::Image2D, 8192).owned_by(10),
        );
        let pipeline = services.create_render_pipeline(GpuPipelineDesc::new(
            "main lighting",
            "shaders/main_lighting.wgsl",
            QualityTier::NormalRuntime,
        ));

        let mut graph = services.begin_frame(3);
        graph.add_pass(
            GpuPassDesc::new(
                "main_lighting",
                GpuQueueKind::Graphics,
                GpuDispatchKind::Graphics(pipeline.handle),
                QualityTier::NormalRuntime,
            )
            .reads([scene_buffer])
            .writes([lighting_target]),
        );

        let report = services.submit(graph);

        assert!(report.validation.passed);
        assert_eq!(report.registered_resource_usage.len(), 2);
        assert_eq!(report.pipeline_report.pipelines.len(), 1);
        assert_eq!(report.pipeline_report.cache_hit_count, 1);
        assert_eq!(report.pipeline_report.unknown_pipeline_count, 0);
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "main lighting" && pipeline.cache_hit && pipeline.request_count == 1
        }));
        assert_eq!(report.descriptor_report.bindless_binding_count, 1);
        assert_eq!(report.descriptor_report.descriptor_set_count, 2);
        assert!(
            report
                .descriptor_report
                .bindings
                .iter()
                .any(|binding| binding.resource == scene_buffer
                    && binding.bindless_index == Some(0)
                    && binding.descriptor_kind == GpuDescriptorKind::StorageBuffer)
        );
        let scene_usage = report
            .registered_resource_usage
            .iter()
            .find(|usage| usage.resource == scene_buffer)
            .expect("scene buffer should be in registered usage");
        assert_eq!(scene_usage.label, "scene instances");
        assert_eq!(scene_usage.owner, Some(10));
        assert_eq!(scene_usage.bindless_index, Some(0));
        assert_eq!(scene_usage.readers, vec!["main_lighting".to_string()]);
        assert!(services.resource(scene_buffer).is_some());
        assert!(services.render_pipeline(pipeline.handle).is_some());
    }

    #[test]
    fn pipeline_cache_reuses_handles_for_identical_descriptors() {
        let mut services = GpuServices::new_vulkan();
        let desc = GpuPipelineDesc::new(
            "gpu culling",
            "shaders/gpu_culling.comp",
            QualityTier::NormalRuntime,
        );

        let first = services.create_compute_pipeline(desc.clone());
        let second = services.create_compute_pipeline(desc);
        let permuted = services.create_compute_pipeline(
            GpuPipelineDesc::new(
                "gpu culling",
                "shaders/gpu_culling.comp",
                QualityTier::NormalRuntime,
            )
            .with_permutation(GpuShaderPermutation::new([
                "SKINNING",
                "QUALITY_NORMAL",
                "SKINNING",
            ])),
        );
        let permuted_again = services.create_compute_pipeline(
            GpuPipelineDesc::new(
                "gpu culling",
                "shaders/gpu_culling.comp",
                QualityTier::NormalRuntime,
            )
            .with_permutation(GpuShaderPermutation::new(["QUALITY_NORMAL", "SKINNING"])),
        );

        assert!(!first.cache_hit);
        assert!(second.cache_hit);
        assert_eq!(first.handle, second.handle);
        assert!(!permuted.cache_hit);
        assert!(permuted_again.cache_hit);
        assert_ne!(first.handle, permuted.handle);
        assert_eq!(permuted.handle, permuted_again.handle);
        assert_eq!(
            services
                .compute_pipeline(first.handle)
                .expect("pipeline cache entry should exist")
                .request_count,
            2
        );
        let permuted_entry = services
            .compute_pipeline(permuted.handle)
            .expect("permuted pipeline cache entry should exist");
        assert_eq!(permuted_entry.request_count, 2);
        assert_eq!(
            permuted_entry.desc.permutation.key(),
            "QUALITY_NORMAL+SKINNING"
        );
    }
}

use std::fmt;

use crate::core::{EntityId, ForceCommand, FrameId, ModuleId, QualityTier, Vec3};
use crate::gpu::{
    ComputePipelineHandle, GpuDispatchKind, GpuGraphBuilder, GpuPassDesc, GpuPipelineDesc,
    GpuQueueKind, GpuResourceDesc, GpuResourceHandle, GpuResourceKind, GpuResourceLifetime,
    RayPipelineHandle, RenderPipelineHandle,
};
use crate::world::{CommandSink, WorldCommand, WorldEvent, WorldEventKind};

pub const ASHFALL_DYNAMIC_MODULE_API_VERSION: u32 = 1;
pub const DYNAMIC_MODULE_TEXT_BYTES: usize = 64;
pub const DYNAMIC_ASSET_MAX_DEPENDENCIES: usize = 8;
pub const DYNAMIC_GPU_MAX_PASS_RESOURCES: usize = 8;
pub const DYNAMIC_HOST_STATUS_OK: u32 = 0;
pub const DYNAMIC_HOST_STATUS_NULL_HOST: u32 = 1;
pub const DYNAMIC_HOST_STATUS_NULL_PAYLOAD: u32 = 2;
pub const DYNAMIC_HOST_STATUS_NULL_RESULT: u32 = 3;
pub const DYNAMIC_HOST_STATUS_INVALID_HANDLE: u32 = 4;
pub const DYNAMIC_HOST_STATUS_INVALID_TEXT: u32 = 5;
pub const DYNAMIC_HOST_STATUS_INVALID_ENUM: u32 = 6;
pub const DYNAMIC_HOST_STATUS_INVALID_VERSION: u32 = 7;
pub const DYNAMIC_HOST_STATUS_TOO_MANY_DEPENDENCIES: u32 = 8;
pub const DYNAMIC_SINK_STATUS_OK: u32 = 0;
pub const DYNAMIC_SINK_STATUS_NULL_SINK: u32 = 1;
pub const DYNAMIC_SINK_STATUS_NULL_PAYLOAD: u32 = 2;
pub const DYNAMIC_SINK_STATUS_INVALID_HANDLE: u32 = 3;
pub const DYNAMIC_SINK_STATUS_INVALID_TEXT: u32 = 4;
pub const DYNAMIC_GRAPH_STATUS_OK: u32 = 0;
pub const DYNAMIC_GRAPH_STATUS_NULL_GRAPH: u32 = 1;
pub const DYNAMIC_GRAPH_STATUS_NULL_PAYLOAD: u32 = 2;
pub const DYNAMIC_GRAPH_STATUS_INVALID_HANDLE: u32 = 3;
pub const DYNAMIC_GRAPH_STATUS_INVALID_TEXT: u32 = 4;
pub const DYNAMIC_GRAPH_STATUS_INVALID_ENUM: u32 = 5;
pub const DYNAMIC_GRAPH_STATUS_TOO_MANY_RESOURCES: u32 = 6;
pub const DYNAMIC_GRAPH_STATUS_FRAME_MISMATCH: u32 = 7;
pub const DYNAMIC_GRAPH_STATUS_NULL_RESULT: u32 = 8;
pub const DYNAMIC_GPU_QUEUE_GRAPHICS: u32 = 0;
pub const DYNAMIC_GPU_QUEUE_COMPUTE: u32 = 1;
pub const DYNAMIC_GPU_QUEUE_TRANSFER: u32 = 2;
pub const DYNAMIC_GPU_DISPATCH_GRAPHICS: u32 = 0;
pub const DYNAMIC_GPU_DISPATCH_COMPUTE: u32 = 1;
pub const DYNAMIC_GPU_DISPATCH_RAY_TRACING: u32 = 2;
pub const DYNAMIC_GPU_DISPATCH_COPY: u32 = 3;
pub const DYNAMIC_GPU_PIPELINE_GRAPHICS: u32 = 0;
pub const DYNAMIC_GPU_PIPELINE_COMPUTE: u32 = 1;
pub const DYNAMIC_GPU_PIPELINE_RAY_TRACING: u32 = 2;
pub const DYNAMIC_GPU_RESOURCE_KIND_BUFFER: u32 = 0;
pub const DYNAMIC_GPU_RESOURCE_KIND_IMAGE_2D: u32 = 1;
pub const DYNAMIC_GPU_RESOURCE_KIND_IMAGE_3D: u32 = 2;
pub const DYNAMIC_GPU_RESOURCE_KIND_ACCELERATION_STRUCTURE: u32 = 3;
pub const DYNAMIC_GPU_RESOURCE_KIND_EXTERNAL: u32 = 4;
pub const DYNAMIC_GPU_RESOURCE_LIFETIME_IMPORTED: u32 = 0;
pub const DYNAMIC_GPU_RESOURCE_LIFETIME_PERSISTENT: u32 = 1;
pub const DYNAMIC_GPU_RESOURCE_LIFETIME_TRANSIENT: u32 = 2;
pub const DYNAMIC_ASSET_KIND_MESH: u32 = 0;
pub const DYNAMIC_ASSET_KIND_TEXTURE: u32 = 1;
pub const DYNAMIC_ASSET_KIND_MATERIAL_GRAPH: u32 = 2;
pub const DYNAMIC_ASSET_KIND_AUDIO_CLIP: u32 = 3;
pub const DYNAMIC_ASSET_KIND_SHADER: u32 = 4;
pub const DYNAMIC_ASSET_KIND_WORLD_CHUNK: u32 = 5;
pub const DYNAMIC_ASSET_KIND_GENERATED_BUNDLE: u32 = 6;
pub const DYNAMIC_ASSET_KIND_OTHER: u32 = 255;
pub const DYNAMIC_ASSET_LOAD_UNLOADED: u32 = 0;
pub const DYNAMIC_ASSET_LOAD_RESIDENT: u32 = 1;
pub const DYNAMIC_ASSET_LOAD_FAILED: u32 = 2;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicModuleHandle(pub usize);

impl DynamicModuleHandle {
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DynamicEngineHostV1 {
    pub api_version: u32,
    pub host_handle: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicSchemaRegistrationV1 {
    pub name: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub version: u32,
}

impl Default for DynamicSchemaRegistrationV1 {
    fn default() -> Self {
        Self {
            name: [0; DYNAMIC_MODULE_TEXT_BYTES],
            version: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicSchemaMigrationStepV1 {
    pub schema_name: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub from_version: u32,
    pub to_version: u32,
    pub description: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub lossless: u8,
    pub _reserved: [u8; 7],
}

impl Default for DynamicSchemaMigrationStepV1 {
    fn default() -> Self {
        Self {
            schema_name: [0; DYNAMIC_MODULE_TEXT_BYTES],
            from_version: 0,
            to_version: 0,
            description: [0; DYNAMIC_MODULE_TEXT_BYTES],
            lossless: 0,
            _reserved: [0; 7],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicAssetRegistrationV1 {
    pub kind: u32,
    pub label: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub provenance: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub dependency_count: u32,
    pub dependencies: [DynamicU128V1; DYNAMIC_ASSET_MAX_DEPENDENCIES],
    pub quality_tier: u32,
    pub load_state: u32,
    pub byte_len: u64,
    pub has_byte_len: u8,
    pub _reserved: [u8; 7],
}

impl Default for DynamicAssetRegistrationV1 {
    fn default() -> Self {
        Self {
            kind: DYNAMIC_ASSET_KIND_OTHER,
            label: [0; DYNAMIC_MODULE_TEXT_BYTES],
            provenance: [0; DYNAMIC_MODULE_TEXT_BYTES],
            dependency_count: 0,
            dependencies: [DynamicU128V1::default(); DYNAMIC_ASSET_MAX_DEPENDENCIES],
            quality_tier: quality_tier_to_abi(QualityTier::NormalRuntime),
            load_state: DYNAMIC_ASSET_LOAD_UNLOADED,
            byte_len: 0,
            has_byte_len: 0,
            _reserved: [0; 7],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicGeneratedAssetRecipeV1 {
    pub kind: u32,
    pub label: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub recipe_schema: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub recipe_version: u32,
    pub recipe_key: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub source_asset_count: u32,
    pub source_assets: [DynamicU128V1; DYNAMIC_ASSET_MAX_DEPENDENCIES],
    pub quality_tier: u32,
    pub byte_len: u64,
    pub has_byte_len: u8,
    pub _reserved: [u8; 7],
}

impl Default for DynamicGeneratedAssetRecipeV1 {
    fn default() -> Self {
        Self {
            kind: DYNAMIC_ASSET_KIND_GENERATED_BUNDLE,
            label: [0; DYNAMIC_MODULE_TEXT_BYTES],
            recipe_schema: [0; DYNAMIC_MODULE_TEXT_BYTES],
            recipe_version: 0,
            recipe_key: [0; DYNAMIC_MODULE_TEXT_BYTES],
            source_asset_count: 0,
            source_assets: [DynamicU128V1::default(); DYNAMIC_ASSET_MAX_DEPENDENCIES],
            quality_tier: quality_tier_to_abi(QualityTier::NormalRuntime),
            byte_len: 0,
            has_byte_len: 0,
            _reserved: [0; 7],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicAssetRegistrationResultV1 {
    pub asset_id: DynamicU128V1,
    pub cache_hit: u8,
    pub _reserved: [u8; 7],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DynamicFrameInputV1 {
    pub frame_id: FrameId,
    pub tick: u64,
    pub dt_seconds: f32,
    pub snapshot_handle: usize,
    pub recent_event_count: u32,
    pub force_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicCommandSinkV1 {
    pub sink_handle: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DynamicVec3V1 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl DynamicVec3V1 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub const fn to_vec3(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicU128V1 {
    pub high: u64,
    pub low: u64,
}

impl DynamicU128V1 {
    pub const fn from_u128(value: u128) -> Self {
        Self {
            high: (value >> 64) as u64,
            low: value as u64,
        }
    }

    pub const fn to_u128(self) -> u128 {
        ((self.high as u128) << 64) | self.low as u128
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DynamicApplyForceCommandV1 {
    pub entity: EntityId,
    pub vector_newtons: DynamicVec3V1,
    pub impulse_newton_seconds: f32,
    pub source_entity: EntityId,
    pub has_source_entity: u8,
    pub _reserved: [u8; 7],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicCustomEventV1 {
    pub event_id: DynamicU128V1,
    pub tick: u64,
    pub location_meters: DynamicVec3V1,
    pub actor: EntityId,
    pub has_actor: u8,
    pub _reserved: [u8; 7],
    pub label: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub physical_evidence: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub narrative_tag: [u8; DYNAMIC_MODULE_TEXT_BYTES],
}

impl Default for DynamicCustomEventV1 {
    fn default() -> Self {
        Self {
            event_id: DynamicU128V1::default(),
            tick: 0,
            location_meters: DynamicVec3V1::default(),
            actor: 0,
            has_actor: 0,
            _reserved: [0; 7],
            label: [0; DYNAMIC_MODULE_TEXT_BYTES],
            physical_evidence: [0; DYNAMIC_MODULE_TEXT_BYTES],
            narrative_tag: [0; DYNAMIC_MODULE_TEXT_BYTES],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicGpuGraphBuilderV1 {
    pub frame_id: FrameId,
    pub graph_handle: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicGpuResourceV1 {
    pub label: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub kind: u32,
    pub byte_len: u64,
    pub owner: ModuleId,
    pub has_owner: u8,
    pub lifetime: u32,
    pub bindless: u8,
    pub _reserved: [u8; 2],
}

impl Default for DynamicGpuResourceV1 {
    fn default() -> Self {
        Self {
            label: [0; DYNAMIC_MODULE_TEXT_BYTES],
            kind: DYNAMIC_GPU_RESOURCE_KIND_BUFFER,
            byte_len: 0,
            owner: 0,
            has_owner: 0,
            lifetime: DYNAMIC_GPU_RESOURCE_LIFETIME_TRANSIENT,
            bindless: 0,
            _reserved: [0; 2],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicGpuResourceResultV1 {
    pub resource: DynamicU128V1,
    pub bindless_index: u32,
    pub has_bindless_index: u8,
    pub _reserved: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicGpuPipelineV1 {
    pub label: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub shader_key: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub kind: u32,
    pub quality_tier: u32,
}

impl Default for DynamicGpuPipelineV1 {
    fn default() -> Self {
        Self {
            label: [0; DYNAMIC_MODULE_TEXT_BYTES],
            shader_key: [0; DYNAMIC_MODULE_TEXT_BYTES],
            kind: DYNAMIC_GPU_PIPELINE_COMPUTE,
            quality_tier: quality_tier_to_abi(QualityTier::NormalRuntime),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicGpuPipelineResultV1 {
    pub pipeline: DynamicU128V1,
    pub cache_hit: u8,
    pub _reserved: [u8; 7],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicGpuPassV1 {
    pub name: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub queue: u32,
    pub dispatch_kind: u32,
    pub pipeline: DynamicU128V1,
    pub budget_hint: u32,
    pub read_count: u32,
    pub reads: [DynamicU128V1; DYNAMIC_GPU_MAX_PASS_RESOURCES],
    pub write_count: u32,
    pub writes: [DynamicU128V1; DYNAMIC_GPU_MAX_PASS_RESOURCES],
}

impl Default for DynamicGpuPassV1 {
    fn default() -> Self {
        Self {
            name: [0; DYNAMIC_MODULE_TEXT_BYTES],
            queue: DYNAMIC_GPU_QUEUE_GRAPHICS,
            dispatch_kind: DYNAMIC_GPU_DISPATCH_GRAPHICS,
            pipeline: DynamicU128V1::default(),
            budget_hint: quality_tier_to_abi(QualityTier::NormalRuntime),
            read_count: 0,
            reads: [DynamicU128V1::default(); DYNAMIC_GPU_MAX_PASS_RESOURCES],
            write_count: 0,
            writes: [DynamicU128V1::default(); DYNAMIC_GPU_MAX_PASS_RESOURCES],
        }
    }
}

pub type DynamicInitFn =
    unsafe extern "C" fn(host: *mut DynamicEngineHostV1) -> DynamicModuleHandle;
pub type DynamicTickFn = unsafe extern "C" fn(
    module: DynamicModuleHandle,
    frame: *const DynamicFrameInputV1,
    out: *mut DynamicCommandSinkV1,
);
pub type DynamicScheduleGpuFn =
    unsafe extern "C" fn(module: DynamicModuleHandle, graph: *mut DynamicGpuGraphBuilderV1);
pub type DynamicShutdownFn = unsafe extern "C" fn(module: DynamicModuleHandle);

/// Pushes an apply-force command into an opaque dynamic module command sink.
///
/// # Safety
///
/// `sink` must point to a `DynamicCommandSinkV1` created by the Ashfall runtime for the
/// current tick, and its `sink_handle` must still reference the live internal command sink.
/// `command` must point to a valid `DynamicApplyForceCommandV1` for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ashfall_dynamic_sink_push_apply_force_v1(
    sink: *mut DynamicCommandSinkV1,
    command: *const DynamicApplyForceCommandV1,
) -> u32 {
    let Some(sink) = (unsafe { sink.as_ref() }) else {
        return DYNAMIC_SINK_STATUS_NULL_SINK;
    };
    let Some(command) = (unsafe { command.as_ref() }) else {
        return DYNAMIC_SINK_STATUS_NULL_PAYLOAD;
    };
    let Some(out) = (unsafe { command_sink_from_handle(sink.sink_handle) }) else {
        return DYNAMIC_SINK_STATUS_INVALID_HANDLE;
    };

    out.command(WorldCommand::ApplyForce(ForceCommand {
        entity: command.entity,
        vector_newtons: command.vector_newtons.to_vec3(),
        impulse_newton_seconds: command.impulse_newton_seconds,
        source: (command.has_source_entity != 0).then_some(command.source_entity),
    }));

    DYNAMIC_SINK_STATUS_OK
}

/// Pushes a custom event into an opaque dynamic module command sink.
///
/// # Safety
///
/// `sink` must point to a `DynamicCommandSinkV1` created by the Ashfall runtime for the
/// current tick, and its `sink_handle` must still reference the live internal command sink.
/// `event` must point to a valid `DynamicCustomEventV1` for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ashfall_dynamic_sink_push_custom_event_v1(
    sink: *mut DynamicCommandSinkV1,
    event: *const DynamicCustomEventV1,
) -> u32 {
    let Some(sink) = (unsafe { sink.as_ref() }) else {
        return DYNAMIC_SINK_STATUS_NULL_SINK;
    };
    let Some(event) = (unsafe { event.as_ref() }) else {
        return DYNAMIC_SINK_STATUS_NULL_PAYLOAD;
    };
    let Some(out) = (unsafe { command_sink_from_handle(sink.sink_handle) }) else {
        return DYNAMIC_SINK_STATUS_INVALID_HANDLE;
    };
    let Ok(label) = decode_fixed_text(&event.label) else {
        return DYNAMIC_SINK_STATUS_INVALID_TEXT;
    };
    if label.trim().is_empty() {
        return DYNAMIC_SINK_STATUS_INVALID_TEXT;
    }
    let physical_evidence = optional_fixed_text(&event.physical_evidence);
    let narrative_tag = optional_fixed_text(&event.narrative_tag);
    let (Ok(physical_evidence), Ok(narrative_tag)) = (physical_evidence, narrative_tag) else {
        return DYNAMIC_SINK_STATUS_INVALID_TEXT;
    };

    out.event(WorldEvent {
        event_id: event.event_id.to_u128(),
        tick: event.tick,
        location_meters: event.location_meters.to_vec3(),
        actors: (event.has_actor != 0)
            .then_some(event.actor)
            .into_iter()
            .collect(),
        kind: WorldEventKind::Custom(label),
        physical_evidence: physical_evidence.into_iter().collect(),
        narrative_tags: narrative_tag.into_iter().collect(),
    });

    DYNAMIC_SINK_STATUS_OK
}

/// Declares a GPU resource on an opaque dynamic module graph builder.
///
/// # Safety
///
/// `graph` must point to a `DynamicGpuGraphBuilderV1` created by the Ashfall runtime for
/// the current GPU scheduling callback. `resource` and `out` must point to valid payload
/// and result storage for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ashfall_dynamic_gpu_graph_declare_resource_v1(
    graph: *mut DynamicGpuGraphBuilderV1,
    resource: *const DynamicGpuResourceV1,
    out: *mut DynamicGpuResourceResultV1,
) -> u32 {
    let Some(graph) = (unsafe { graph.as_ref() }) else {
        return DYNAMIC_GRAPH_STATUS_NULL_GRAPH;
    };
    let Some(resource) = (unsafe { resource.as_ref() }) else {
        return DYNAMIC_GRAPH_STATUS_NULL_PAYLOAD;
    };
    let Some(out) = (unsafe { out.as_mut() }) else {
        return DYNAMIC_GRAPH_STATUS_NULL_RESULT;
    };
    let Some(builder) = (unsafe { gpu_graph_builder_from_handle(graph.graph_handle) }) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_HANDLE;
    };
    if graph.frame_id != builder.frame_id {
        return DYNAMIC_GRAPH_STATUS_FRAME_MISMATCH;
    }
    let Ok(label) = decode_fixed_text(&resource.label) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_TEXT;
    };
    if label.trim().is_empty() {
        return DYNAMIC_GRAPH_STATUS_INVALID_TEXT;
    }
    let Some(kind) = dynamic_gpu_resource_kind_from_abi(resource.kind) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_ENUM;
    };
    let Some(lifetime) = dynamic_gpu_resource_lifetime_from_abi(resource.lifetime) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_ENUM;
    };

    let mut desc = GpuResourceDesc::new(label, kind, resource.byte_len).with_lifetime(lifetime);
    if resource.has_owner != 0 {
        desc = desc.owned_by(resource.owner);
    }
    if resource.bindless != 0 {
        desc = desc.bindless();
    }

    let handle = builder.declare_resource(desc);
    let bindless_index = builder
        .resource(handle)
        .and_then(|record| record.bindless_index);
    out.resource = DynamicU128V1::from_u128(handle.0);
    out.bindless_index = bindless_index.unwrap_or_default();
    out.has_bindless_index = u8::from(bindless_index.is_some());

    DYNAMIC_GRAPH_STATUS_OK
}

/// Creates or reuses a GPU pipeline on an opaque dynamic module graph builder.
///
/// # Safety
///
/// `graph` must point to a `DynamicGpuGraphBuilderV1` created by the Ashfall runtime for
/// the current GPU scheduling callback. `pipeline` and `out` must point to valid payload
/// and result storage for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ashfall_dynamic_gpu_graph_create_pipeline_v1(
    graph: *mut DynamicGpuGraphBuilderV1,
    pipeline: *const DynamicGpuPipelineV1,
    out: *mut DynamicGpuPipelineResultV1,
) -> u32 {
    let Some(graph) = (unsafe { graph.as_ref() }) else {
        return DYNAMIC_GRAPH_STATUS_NULL_GRAPH;
    };
    let Some(pipeline) = (unsafe { pipeline.as_ref() }) else {
        return DYNAMIC_GRAPH_STATUS_NULL_PAYLOAD;
    };
    let Some(out) = (unsafe { out.as_mut() }) else {
        return DYNAMIC_GRAPH_STATUS_NULL_RESULT;
    };
    let Some(builder) = (unsafe { gpu_graph_builder_from_handle(graph.graph_handle) }) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_HANDLE;
    };
    if graph.frame_id != builder.frame_id {
        return DYNAMIC_GRAPH_STATUS_FRAME_MISMATCH;
    }
    let Ok(label) = decode_fixed_text(&pipeline.label) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_TEXT;
    };
    let Ok(shader_key) = decode_fixed_text(&pipeline.shader_key) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_TEXT;
    };
    if label.trim().is_empty() || shader_key.trim().is_empty() {
        return DYNAMIC_GRAPH_STATUS_INVALID_TEXT;
    }
    let Some(quality_tier) = quality_tier_from_abi(pipeline.quality_tier) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_ENUM;
    };
    let desc = GpuPipelineDesc::new(label, shader_key, quality_tier);

    let (handle, cache_hit) = match pipeline.kind {
        DYNAMIC_GPU_PIPELINE_GRAPHICS => {
            let result = builder.create_render_pipeline(desc);
            (result.handle.0, result.cache_hit)
        }
        DYNAMIC_GPU_PIPELINE_COMPUTE => {
            let result = builder.create_compute_pipeline(desc);
            (result.handle.0, result.cache_hit)
        }
        DYNAMIC_GPU_PIPELINE_RAY_TRACING => {
            let result = builder.create_ray_pipeline(desc);
            (result.handle.0, result.cache_hit)
        }
        _ => return DYNAMIC_GRAPH_STATUS_INVALID_ENUM,
    };

    out.pipeline = DynamicU128V1::from_u128(handle);
    out.cache_hit = u8::from(cache_hit);

    DYNAMIC_GRAPH_STATUS_OK
}

/// Adds a GPU pass to an opaque dynamic module graph builder.
///
/// # Safety
///
/// `graph` must point to a `DynamicGpuGraphBuilderV1` created by the Ashfall runtime for
/// the current GPU scheduling callback, and its `graph_handle` must still reference the
/// live internal `GpuGraphBuilder`. `pass` must point to a valid `DynamicGpuPassV1`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ashfall_dynamic_gpu_graph_add_pass_v1(
    graph: *mut DynamicGpuGraphBuilderV1,
    pass: *const DynamicGpuPassV1,
) -> u32 {
    let Some(graph) = (unsafe { graph.as_ref() }) else {
        return DYNAMIC_GRAPH_STATUS_NULL_GRAPH;
    };
    let Some(pass) = (unsafe { pass.as_ref() }) else {
        return DYNAMIC_GRAPH_STATUS_NULL_PAYLOAD;
    };
    let Some(builder) = (unsafe { gpu_graph_builder_from_handle(graph.graph_handle) }) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_HANDLE;
    };
    if graph.frame_id != builder.frame_id {
        return DYNAMIC_GRAPH_STATUS_FRAME_MISMATCH;
    }
    let Ok(name) = decode_fixed_text(&pass.name) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_TEXT;
    };
    if name.trim().is_empty() {
        return DYNAMIC_GRAPH_STATUS_INVALID_TEXT;
    }
    let Some(queue) = dynamic_gpu_queue_from_abi(pass.queue) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_ENUM;
    };
    let Some(dispatch) = dynamic_gpu_dispatch_from_abi(pass.dispatch_kind, pass.pipeline) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_ENUM;
    };
    let Some(budget_hint) = quality_tier_from_abi(pass.budget_hint) else {
        return DYNAMIC_GRAPH_STATUS_INVALID_ENUM;
    };
    let read_count = pass.read_count as usize;
    let write_count = pass.write_count as usize;
    if read_count > DYNAMIC_GPU_MAX_PASS_RESOURCES || write_count > DYNAMIC_GPU_MAX_PASS_RESOURCES {
        return DYNAMIC_GRAPH_STATUS_TOO_MANY_RESOURCES;
    }

    let reads = pass.reads[..read_count]
        .iter()
        .map(|resource| GpuResourceHandle(resource.to_u128()));
    let writes = pass.writes[..write_count]
        .iter()
        .map(|resource| GpuResourceHandle(resource.to_u128()));
    builder.add_pass(
        GpuPassDesc::new(name, queue, dispatch, budget_hint)
            .reads(reads)
            .writes(writes),
    );

    DYNAMIC_GRAPH_STATUS_OK
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DynamicModuleApiV1 {
    pub api_version: u32,
    pub module_id: ModuleId,
    pub name: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub schema_name: [u8; DYNAMIC_MODULE_TEXT_BYTES],
    pub schema_version: u32,
    pub default_quality: u32,
    pub init: Option<DynamicInitFn>,
    pub tick: Option<DynamicTickFn>,
    pub schedule_gpu: Option<DynamicScheduleGpuFn>,
    pub shutdown: Option<DynamicShutdownFn>,
}

impl DynamicModuleApiV1 {
    pub fn from_parts(
        module_id: ModuleId,
        name: &str,
        schema_name: &str,
        schema_version: u32,
        default_quality: QualityTier,
        handlers: DynamicModuleHandlersV1,
    ) -> Self {
        Self {
            api_version: ASHFALL_DYNAMIC_MODULE_API_VERSION,
            module_id,
            name: encode_fixed_text(name),
            schema_name: encode_fixed_text(schema_name),
            schema_version,
            default_quality: quality_tier_to_abi(default_quality),
            init: handlers.init,
            tick: handlers.tick,
            schedule_gpu: handlers.schedule_gpu,
            shutdown: handlers.shutdown,
        }
    }

    pub fn validate(&self) -> DynamicModuleValidationReport {
        let mut issues = Vec::new();

        if self.api_version != ASHFALL_DYNAMIC_MODULE_API_VERSION {
            issues.push(DynamicModuleValidationIssue::error(
                "api_version_mismatch",
                format!(
                    "module exports API v{}, but runtime expects v{}",
                    self.api_version, ASHFALL_DYNAMIC_MODULE_API_VERSION
                ),
            ));
        }

        if self.module_id == 0 {
            issues.push(DynamicModuleValidationIssue::error(
                "missing_module_id",
                "module_id must be non-zero",
            ));
        }

        validate_fixed_text("name", &self.name, &mut issues);
        validate_fixed_text("schema_name", &self.schema_name, &mut issues);

        if self.schema_version == 0 {
            issues.push(DynamicModuleValidationIssue::error(
                "missing_schema_version",
                "schema_version must be non-zero",
            ));
        }

        if quality_tier_from_abi(self.default_quality).is_none() {
            issues.push(DynamicModuleValidationIssue::error(
                "invalid_quality_tier",
                format!(
                    "{} is not a known QualityTier ABI value",
                    self.default_quality
                ),
            ));
        }

        if self.init.is_none() {
            issues.push(DynamicModuleValidationIssue::error(
                "missing_init",
                "dynamic module must export init",
            ));
        }
        if self.tick.is_none() {
            issues.push(DynamicModuleValidationIssue::error(
                "missing_tick",
                "dynamic module must export tick",
            ));
        }
        if self.schedule_gpu.is_none() {
            issues.push(DynamicModuleValidationIssue::error(
                "missing_schedule_gpu",
                "dynamic module must export schedule_gpu",
            ));
        }
        if self.shutdown.is_none() {
            issues.push(DynamicModuleValidationIssue::error(
                "missing_shutdown",
                "dynamic module must export shutdown",
            ));
        }

        DynamicModuleValidationReport {
            passed: !issues
                .iter()
                .any(|issue| issue.severity == DynamicModuleValidationSeverity::Error),
            issues,
        }
    }

    pub fn descriptor(&self) -> Result<DynamicModuleDescriptor, DynamicModuleValidationReport> {
        let report = self.validate();
        if !report.passed {
            return Err(report);
        }

        Ok(DynamicModuleDescriptor {
            module_id: self.module_id,
            name: decode_fixed_text(&self.name).unwrap_or_default(),
            schema_name: decode_fixed_text(&self.schema_name).unwrap_or_default(),
            schema_version: self.schema_version,
            default_quality: quality_tier_from_abi(self.default_quality)
                .expect("quality tier was validated"),
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DynamicModuleHandlersV1 {
    pub init: Option<DynamicInitFn>,
    pub tick: Option<DynamicTickFn>,
    pub schedule_gpu: Option<DynamicScheduleGpuFn>,
    pub shutdown: Option<DynamicShutdownFn>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynamicModuleDescriptor {
    pub module_id: ModuleId,
    pub name: String,
    pub schema_name: String,
    pub schema_version: u32,
    pub default_quality: QualityTier,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynamicModuleValidationReport {
    pub passed: bool,
    pub issues: Vec<DynamicModuleValidationIssue>,
}

impl DynamicModuleValidationReport {
    pub fn has_errors(&self) -> bool {
        !self.passed
    }
}

impl fmt::Display for DynamicModuleValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.passed {
            write!(
                f,
                "dynamic module ABI validation passed with {} diagnostics",
                self.issues.len()
            )
        } else if let Some(issue) = self
            .issues
            .iter()
            .find(|issue| issue.severity == DynamicModuleValidationSeverity::Error)
        {
            write!(f, "dynamic module ABI validation failed: {}", issue.message)
        } else {
            write!(f, "dynamic module ABI validation failed")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynamicModuleValidationIssue {
    pub severity: DynamicModuleValidationSeverity,
    pub code: String,
    pub message: String,
}

impl DynamicModuleValidationIssue {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DynamicModuleValidationSeverity::Error,
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DynamicModuleValidationSeverity::Warning,
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicModuleValidationSeverity {
    Warning,
    Error,
}

pub fn quality_tier_to_abi(quality: QualityTier) -> u32 {
    match quality {
        QualityTier::Disabled => 0,
        QualityTier::BackgroundApproximation => 1,
        QualityTier::NormalRuntime => 2,
        QualityTier::HeroHighFidelityRuntime => 3,
        QualityTier::ReferenceOfflineValidation => 4,
    }
}

pub fn quality_tier_from_abi(value: u32) -> Option<QualityTier> {
    match value {
        0 => Some(QualityTier::Disabled),
        1 => Some(QualityTier::BackgroundApproximation),
        2 => Some(QualityTier::NormalRuntime),
        3 => Some(QualityTier::HeroHighFidelityRuntime),
        4 => Some(QualityTier::ReferenceOfflineValidation),
        _ => None,
    }
}

fn encode_fixed_text<const N: usize>(value: &str) -> [u8; N] {
    let mut out = [0; N];
    let max_len = N.saturating_sub(1);
    let mut copy_len = value.len().min(max_len);
    while !value.is_char_boundary(copy_len) {
        copy_len -= 1;
    }
    out[..copy_len].copy_from_slice(&value.as_bytes()[..copy_len]);
    out
}

fn validate_fixed_text(
    field: &'static str,
    bytes: &[u8],
    issues: &mut Vec<DynamicModuleValidationIssue>,
) {
    if bytes.first().copied().unwrap_or_default() == 0 {
        issues.push(DynamicModuleValidationIssue::error(
            format!("{field}_empty"),
            format!("{field} must not be empty"),
        ));
        return;
    }

    if !bytes.contains(&0) {
        issues.push(DynamicModuleValidationIssue::warning(
            format!("{field}_missing_terminator"),
            format!("{field} is not null terminated inside its fixed ABI buffer"),
        ));
    }

    if let Err(error) = decode_fixed_text(bytes) {
        issues.push(DynamicModuleValidationIssue::error(
            format!("{field}_invalid_utf8"),
            error,
        ));
    }
}

fn decode_fixed_text(bytes: &[u8]) -> Result<String, String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end])
        .map(str::to_string)
        .map_err(|error| format!("fixed text field is not valid UTF-8: {error}"))
}

fn optional_fixed_text(bytes: &[u8]) -> Result<Option<String>, String> {
    let value = decode_fixed_text(bytes)?;
    if value.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn dynamic_gpu_queue_from_abi(value: u32) -> Option<GpuQueueKind> {
    match value {
        DYNAMIC_GPU_QUEUE_GRAPHICS => Some(GpuQueueKind::Graphics),
        DYNAMIC_GPU_QUEUE_COMPUTE => Some(GpuQueueKind::Compute),
        DYNAMIC_GPU_QUEUE_TRANSFER => Some(GpuQueueKind::Transfer),
        _ => None,
    }
}

fn dynamic_gpu_dispatch_from_abi(
    dispatch_kind: u32,
    pipeline: DynamicU128V1,
) -> Option<GpuDispatchKind> {
    let pipeline = pipeline.to_u128();
    match dispatch_kind {
        DYNAMIC_GPU_DISPATCH_GRAPHICS => {
            Some(GpuDispatchKind::Graphics(RenderPipelineHandle(pipeline)))
        }
        DYNAMIC_GPU_DISPATCH_COMPUTE => {
            Some(GpuDispatchKind::Compute(ComputePipelineHandle(pipeline)))
        }
        DYNAMIC_GPU_DISPATCH_RAY_TRACING => {
            Some(GpuDispatchKind::RayTracing(RayPipelineHandle(pipeline)))
        }
        DYNAMIC_GPU_DISPATCH_COPY => Some(GpuDispatchKind::Copy),
        _ => None,
    }
}

fn dynamic_gpu_resource_kind_from_abi(value: u32) -> Option<GpuResourceKind> {
    match value {
        DYNAMIC_GPU_RESOURCE_KIND_BUFFER => Some(GpuResourceKind::Buffer),
        DYNAMIC_GPU_RESOURCE_KIND_IMAGE_2D => Some(GpuResourceKind::Image2D),
        DYNAMIC_GPU_RESOURCE_KIND_IMAGE_3D => Some(GpuResourceKind::Image3D),
        DYNAMIC_GPU_RESOURCE_KIND_ACCELERATION_STRUCTURE => {
            Some(GpuResourceKind::AccelerationStructure)
        }
        DYNAMIC_GPU_RESOURCE_KIND_EXTERNAL => Some(GpuResourceKind::External),
        _ => None,
    }
}

fn dynamic_gpu_resource_lifetime_from_abi(value: u32) -> Option<GpuResourceLifetime> {
    match value {
        DYNAMIC_GPU_RESOURCE_LIFETIME_IMPORTED => Some(GpuResourceLifetime::Imported),
        DYNAMIC_GPU_RESOURCE_LIFETIME_PERSISTENT => Some(GpuResourceLifetime::Persistent),
        DYNAMIC_GPU_RESOURCE_LIFETIME_TRANSIENT => Some(GpuResourceLifetime::Transient),
        _ => None,
    }
}

unsafe fn command_sink_from_handle<'a>(handle: usize) -> Option<&'a mut CommandSink> {
    if handle == 0 {
        return None;
    }

    let sink = handle as *mut CommandSink;
    unsafe { sink.as_mut() }
}

unsafe fn gpu_graph_builder_from_handle<'a>(handle: usize) -> Option<&'a mut GpuGraphBuilder> {
    if handle == 0 {
        return None;
    }

    let graph = handle as *mut GpuGraphBuilder;
    unsafe { graph.as_mut() }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn init(_: *mut DynamicEngineHostV1) -> DynamicModuleHandle {
        DynamicModuleHandle::new(11)
    }

    unsafe extern "C" fn tick(
        _: DynamicModuleHandle,
        _: *const DynamicFrameInputV1,
        _: *mut DynamicCommandSinkV1,
    ) {
    }

    unsafe extern "C" fn schedule_gpu(_: DynamicModuleHandle, _: *mut DynamicGpuGraphBuilderV1) {}

    unsafe extern "C" fn shutdown(_: DynamicModuleHandle) {}

    fn complete_handlers() -> DynamicModuleHandlersV1 {
        DynamicModuleHandlersV1 {
            init: Some(init),
            tick: Some(tick),
            schedule_gpu: Some(schedule_gpu),
            shutdown: Some(shutdown),
        }
    }

    #[test]
    fn dynamic_module_abi_accepts_complete_exports() {
        let api = DynamicModuleApiV1::from_parts(
            42,
            "test_renderer",
            "RenderPacket",
            1,
            QualityTier::NormalRuntime,
            complete_handlers(),
        );

        let report = api.validate();
        assert!(report.passed, "{report}");

        let descriptor = api.descriptor().expect("descriptor should decode");
        assert_eq!(descriptor.module_id, 42);
        assert_eq!(descriptor.name, "test_renderer");
        assert_eq!(descriptor.schema_name, "RenderPacket");
        assert_eq!(descriptor.default_quality, QualityTier::NormalRuntime);
    }

    #[test]
    fn dynamic_module_abi_rejects_missing_required_exports() {
        let api = DynamicModuleApiV1::from_parts(
            0,
            "",
            "BrokenOutput",
            0,
            QualityTier::NormalRuntime,
            DynamicModuleHandlersV1::default(),
        );

        let report = api.validate();

        assert!(!report.passed);
        assert!(report.has_errors());
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "missing_module_id")
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "missing_tick")
        );
    }

    #[test]
    fn dynamic_module_abi_rejects_unknown_quality_values() {
        let mut api = DynamicModuleApiV1::from_parts(
            7,
            "quality_test",
            "QualityOutput",
            1,
            QualityTier::NormalRuntime,
            complete_handlers(),
        );
        api.default_quality = 99;

        let report = api.validate();

        assert!(!report.passed);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "invalid_quality_tier")
        );
        assert!(api.descriptor().is_err());
    }

    #[test]
    fn dynamic_command_sink_helpers_push_plain_abi_payloads() {
        let mut out = CommandSink::default();
        let mut sink = DynamicCommandSinkV1 {
            sink_handle: &mut out as *mut CommandSink as usize,
        };
        let command = DynamicApplyForceCommandV1 {
            entity: 9,
            vector_newtons: DynamicVec3V1::new(1.0, 2.0, 3.0),
            impulse_newton_seconds: 0.5,
            source_entity: 7,
            has_source_entity: 1,
            _reserved: [0; 7],
        };
        let event = DynamicCustomEventV1 {
            event_id: DynamicU128V1::from_u128(99),
            tick: 4,
            location_meters: DynamicVec3V1::new(4.0, 5.0, 6.0),
            actor: 9,
            has_actor: 1,
            _reserved: [0; 7],
            label: encode_fixed_text("dynamic_probe_event"),
            physical_evidence: encode_fixed_text("dynamic_evidence"),
            narrative_tag: encode_fixed_text("dynamic_tag"),
        };

        let command_status =
            unsafe { ashfall_dynamic_sink_push_apply_force_v1(&mut sink, &command) };
        let event_status = unsafe { ashfall_dynamic_sink_push_custom_event_v1(&mut sink, &event) };

        assert_eq!(command_status, DYNAMIC_SINK_STATUS_OK);
        assert_eq!(event_status, DYNAMIC_SINK_STATUS_OK);
        assert_eq!(out.commands.len(), 1);
        assert!(matches!(
            out.commands[0],
            WorldCommand::ApplyForce(ForceCommand {
                entity: 9,
                source: Some(7),
                ..
            })
        ));
        assert_eq!(out.events.len(), 1);
        assert_eq!(out.events[0].actors, vec![9]);
        assert_eq!(out.events[0].physical_evidence, vec!["dynamic_evidence"]);
        assert_eq!(out.events[0].narrative_tags, vec!["dynamic_tag"]);
        assert!(matches!(
            &out.events[0].kind,
            WorldEventKind::Custom(label) if label == "dynamic_probe_event"
        ));
    }

    #[test]
    fn dynamic_command_sink_helpers_reject_null_and_invalid_payloads() {
        let mut out = CommandSink::default();
        let mut sink = DynamicCommandSinkV1 {
            sink_handle: &mut out as *mut CommandSink as usize,
        };
        let invalid_event = DynamicCustomEventV1 {
            label: [0; DYNAMIC_MODULE_TEXT_BYTES],
            ..DynamicCustomEventV1::default()
        };

        assert_eq!(
            unsafe {
                ashfall_dynamic_sink_push_apply_force_v1(std::ptr::null_mut(), std::ptr::null())
            },
            DYNAMIC_SINK_STATUS_NULL_SINK
        );
        assert_eq!(
            unsafe { ashfall_dynamic_sink_push_apply_force_v1(&mut sink, std::ptr::null()) },
            DYNAMIC_SINK_STATUS_NULL_PAYLOAD
        );
        assert_eq!(
            unsafe { ashfall_dynamic_sink_push_custom_event_v1(&mut sink, &invalid_event) },
            DYNAMIC_SINK_STATUS_INVALID_TEXT
        );
        assert!(out.commands.is_empty());
        assert!(out.events.is_empty());
    }

    #[test]
    fn dynamic_gpu_graph_helper_adds_plain_abi_passes() {
        let services = crate::gpu::GpuServices::new_vulkan();
        let mut graph = services.begin_frame(12);
        let mut bridge = DynamicGpuGraphBuilderV1 {
            frame_id: 12,
            graph_handle: &mut graph as *mut GpuGraphBuilder as usize,
        };
        let source = DynamicGpuResourceV1 {
            label: encode_fixed_text("dynamic source buffer"),
            kind: DYNAMIC_GPU_RESOURCE_KIND_BUFFER,
            byte_len: 4096,
            owner: 42,
            has_owner: 1,
            lifetime: DYNAMIC_GPU_RESOURCE_LIFETIME_IMPORTED,
            bindless: 1,
            _reserved: [0; 2],
        };
        let target = DynamicGpuResourceV1 {
            label: encode_fixed_text("dynamic target buffer"),
            kind: DYNAMIC_GPU_RESOURCE_KIND_BUFFER,
            byte_len: 8192,
            owner: 42,
            has_owner: 1,
            lifetime: DYNAMIC_GPU_RESOURCE_LIFETIME_TRANSIENT,
            bindless: 0,
            _reserved: [0; 2],
        };
        let mut source_result = DynamicGpuResourceResultV1::default();
        let mut target_result = DynamicGpuResourceResultV1::default();

        assert_eq!(
            unsafe {
                ashfall_dynamic_gpu_graph_declare_resource_v1(
                    &mut bridge,
                    &source,
                    &mut source_result,
                )
            },
            DYNAMIC_GRAPH_STATUS_OK
        );
        assert_eq!(
            unsafe {
                ashfall_dynamic_gpu_graph_declare_resource_v1(
                    &mut bridge,
                    &target,
                    &mut target_result,
                )
            },
            DYNAMIC_GRAPH_STATUS_OK
        );
        assert!(source_result.resource.to_u128() != 0);
        assert_eq!(source_result.has_bindless_index, 1);
        assert!(target_result.resource.to_u128() != 0);
        assert_eq!(target_result.has_bindless_index, 0);

        let pipeline = DynamicGpuPipelineV1 {
            label: encode_fixed_text("dynamic compute pipeline"),
            shader_key: encode_fixed_text("dynamic/compute.comp"),
            kind: DYNAMIC_GPU_PIPELINE_COMPUTE,
            quality_tier: quality_tier_to_abi(QualityTier::BackgroundApproximation),
        };
        let mut pipeline_result = DynamicGpuPipelineResultV1::default();
        let mut cached_pipeline_result = DynamicGpuPipelineResultV1::default();
        assert_eq!(
            unsafe {
                ashfall_dynamic_gpu_graph_create_pipeline_v1(
                    &mut bridge,
                    &pipeline,
                    &mut pipeline_result,
                )
            },
            DYNAMIC_GRAPH_STATUS_OK
        );
        assert_eq!(pipeline_result.cache_hit, 0);
        assert_eq!(
            unsafe {
                ashfall_dynamic_gpu_graph_create_pipeline_v1(
                    &mut bridge,
                    &pipeline,
                    &mut cached_pipeline_result,
                )
            },
            DYNAMIC_GRAPH_STATUS_OK
        );
        assert_eq!(cached_pipeline_result.cache_hit, 1);
        assert_eq!(cached_pipeline_result.pipeline, pipeline_result.pipeline);

        let mut reads = [DynamicU128V1::default(); DYNAMIC_GPU_MAX_PASS_RESOURCES];
        let mut writes = [DynamicU128V1::default(); DYNAMIC_GPU_MAX_PASS_RESOURCES];
        reads[0] = source_result.resource;
        writes[0] = target_result.resource;
        let pass = DynamicGpuPassV1 {
            name: encode_fixed_text("dynamic_compute"),
            queue: DYNAMIC_GPU_QUEUE_COMPUTE,
            dispatch_kind: DYNAMIC_GPU_DISPATCH_COMPUTE,
            pipeline: pipeline_result.pipeline,
            budget_hint: quality_tier_to_abi(QualityTier::BackgroundApproximation),
            read_count: 1,
            reads,
            write_count: 1,
            writes,
        };

        let status = unsafe { ashfall_dynamic_gpu_graph_add_pass_v1(&mut bridge, &pass) };

        assert_eq!(status, DYNAMIC_GRAPH_STATUS_OK);
        assert_eq!(graph.passes().len(), 1);
        let pass = &graph.passes()[0];
        assert_eq!(pass.name, "dynamic_compute");
        assert_eq!(pass.queue, GpuQueueKind::Compute);
        assert!(matches!(
            pass.dispatch,
            GpuDispatchKind::Compute(handle) if handle == ComputePipelineHandle(pipeline_result.pipeline.to_u128())
        ));
        assert_eq!(
            pass.reads,
            vec![GpuResourceHandle(source_result.resource.to_u128())]
        );
        assert_eq!(
            pass.writes,
            vec![GpuResourceHandle(target_result.resource.to_u128())]
        );
        assert_eq!(graph.resources().count(), 2);

        let report = services.submit(graph);
        assert_eq!(report.pipeline_report.pipelines.len(), 1);
        assert_eq!(report.pipeline_report.cache_hit_count, 1);
        assert_eq!(report.pipeline_report.unknown_pipeline_count, 0);
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "dynamic compute pipeline"
                && pipeline.shader_key == "dynamic/compute.comp"
                && pipeline.request_count == 2
                && pipeline.pass_names == vec!["dynamic_compute".to_string()]
        }));
    }

    #[test]
    fn dynamic_gpu_graph_helper_rejects_bad_payloads() {
        let services = crate::gpu::GpuServices::new_vulkan();
        let mut graph = services.begin_frame(3);
        let mut bridge = DynamicGpuGraphBuilderV1 {
            frame_id: 3,
            graph_handle: &mut graph as *mut GpuGraphBuilder as usize,
        };
        let invalid_name = DynamicGpuPassV1 {
            name: [0; DYNAMIC_MODULE_TEXT_BYTES],
            ..DynamicGpuPassV1::default()
        };
        let too_many_reads = DynamicGpuPassV1 {
            name: encode_fixed_text("too_many"),
            read_count: (DYNAMIC_GPU_MAX_PASS_RESOURCES + 1) as u32,
            ..DynamicGpuPassV1::default()
        };
        let valid_pipeline = DynamicGpuPipelineV1 {
            label: encode_fixed_text("valid pipeline"),
            shader_key: encode_fixed_text("dynamic/valid.comp"),
            kind: DYNAMIC_GPU_PIPELINE_COMPUTE,
            quality_tier: quality_tier_to_abi(QualityTier::NormalRuntime),
        };
        let invalid_pipeline_text = DynamicGpuPipelineV1 {
            label: [0; DYNAMIC_MODULE_TEXT_BYTES],
            shader_key: encode_fixed_text("dynamic/invalid.comp"),
            kind: DYNAMIC_GPU_PIPELINE_COMPUTE,
            quality_tier: quality_tier_to_abi(QualityTier::NormalRuntime),
        };
        let invalid_pipeline_kind = DynamicGpuPipelineV1 {
            label: encode_fixed_text("invalid kind"),
            shader_key: encode_fixed_text("dynamic/invalid_kind.comp"),
            kind: 99,
            quality_tier: quality_tier_to_abi(QualityTier::NormalRuntime),
        };
        let mut pipeline_result = DynamicGpuPipelineResultV1::default();

        assert_eq!(
            unsafe {
                ashfall_dynamic_gpu_graph_add_pass_v1(std::ptr::null_mut(), std::ptr::null())
            },
            DYNAMIC_GRAPH_STATUS_NULL_GRAPH
        );
        assert_eq!(
            unsafe { ashfall_dynamic_gpu_graph_add_pass_v1(&mut bridge, std::ptr::null()) },
            DYNAMIC_GRAPH_STATUS_NULL_PAYLOAD
        );
        assert_eq!(
            unsafe { ashfall_dynamic_gpu_graph_add_pass_v1(&mut bridge, &invalid_name) },
            DYNAMIC_GRAPH_STATUS_INVALID_TEXT
        );
        assert_eq!(
            unsafe { ashfall_dynamic_gpu_graph_add_pass_v1(&mut bridge, &too_many_reads) },
            DYNAMIC_GRAPH_STATUS_TOO_MANY_RESOURCES
        );
        assert_eq!(
            unsafe {
                ashfall_dynamic_gpu_graph_declare_resource_v1(
                    &mut bridge,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                )
            },
            DYNAMIC_GRAPH_STATUS_NULL_PAYLOAD
        );
        assert_eq!(
            unsafe {
                ashfall_dynamic_gpu_graph_create_pipeline_v1(
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    &mut pipeline_result,
                )
            },
            DYNAMIC_GRAPH_STATUS_NULL_GRAPH
        );
        assert_eq!(
            unsafe {
                ashfall_dynamic_gpu_graph_create_pipeline_v1(
                    &mut bridge,
                    std::ptr::null(),
                    &mut pipeline_result,
                )
            },
            DYNAMIC_GRAPH_STATUS_NULL_PAYLOAD
        );
        assert_eq!(
            unsafe {
                ashfall_dynamic_gpu_graph_create_pipeline_v1(
                    &mut bridge,
                    &valid_pipeline,
                    std::ptr::null_mut(),
                )
            },
            DYNAMIC_GRAPH_STATUS_NULL_RESULT
        );
        assert_eq!(
            unsafe {
                ashfall_dynamic_gpu_graph_create_pipeline_v1(
                    &mut bridge,
                    &invalid_pipeline_text,
                    &mut pipeline_result,
                )
            },
            DYNAMIC_GRAPH_STATUS_INVALID_TEXT
        );
        assert_eq!(
            unsafe {
                ashfall_dynamic_gpu_graph_create_pipeline_v1(
                    &mut bridge,
                    &invalid_pipeline_kind,
                    &mut pipeline_result,
                )
            },
            DYNAMIC_GRAPH_STATUS_INVALID_ENUM
        );
        assert!(graph.passes().is_empty());
    }
}

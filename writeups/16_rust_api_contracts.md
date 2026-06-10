# 16 — Rust API Contracts

These are Rust-style interface sketches, not final compilable code. They define component boundaries and data flow. V2 separates CPU authoring data from GPU ABI structs and makes long-running work job-based.

## ID types

```rust
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct AssetId(pub u128);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct MaterialId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct GeometryId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct RenderSceneId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct RenderFrameId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct RenderCaptureId(pub u64);
```

## Interface version

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterfaceVersion {
    pub name: &'static str,
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}
```

Rules:

```text
- changing field meaning requires a major bump
- adding an optional field requires a minor bump
- bug fixes with no schema change require a patch bump
- eval artifacts store all interface versions used for a run
```

## Engine module

```rust
pub trait EngineModule: Send {
    fn name(&self) -> &'static str;
    fn interface_version(&self) -> InterfaceVersion;
    fn register(&mut self, registry: &mut ModuleRegistry) -> anyhow::Result<()>;
    fn initialize(&mut self, ctx: &mut InitContext) -> anyhow::Result<()>;
    fn begin_frame(&mut self, frame: &FrameSnapshot) -> anyhow::Result<()>;
    fn update(&mut self, ctx: &mut UpdateContext) -> anyhow::Result<ModuleOutput>;
    fn end_frame(&mut self, frame: &FrameSnapshot) -> anyhow::Result<()>;
    fn telemetry(&self) -> ModuleTelemetry;
}
```

## GPU capabilities

```rust
pub struct GpuCapabilities {
    pub device_name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub api_version: VulkanApiVersion,
    pub driver_version: String,
    pub queue_families: QueueFamilyReport,
    pub memory: GpuMemoryReport,
    pub limits: GpuLimitReport,
    pub formats: GpuFormatReport,
    pub supports_dynamic_rendering: bool,
    pub supports_dynamic_rendering_local_read: bool,
    pub supports_synchronization2: bool,
    pub supports_timeline_semaphore: bool,
    pub supports_descriptor_indexing: bool,
    pub supports_descriptor_buffer: bool,
    pub ray_tracing_mode: RayTracingMode,
    pub timestamp_period_ns: f32,
}

pub enum RayTracingMode {
    Unsupported,
    RayQueryOnly,
    FullPipeline,
}
```

Capabilities must be emitted to `gpu_info.json` for every evaluation run.

## Renderer

```rust
pub trait Renderer: Send {
    fn capabilities(&self) -> &GpuCapabilities;

    fn create_scene_proxy(&mut self, scene: &SceneView) -> anyhow::Result<RenderSceneId>;

    fn update_scene_proxy(
        &mut self,
        scene_id: RenderSceneId,
        updates: &[RenderSceneUpdate],
    ) -> anyhow::Result<()>;

    fn render_frame(
        &mut self,
        frame: &FrameSnapshot,
        tasks: &[RenderTask],
    ) -> anyhow::Result<RenderFrameId>;

    fn capture(
        &mut self,
        frame: RenderFrameId,
        request: CaptureRequest,
    ) -> anyhow::Result<RenderCaptureId>;

    fn present(&mut self, frame: RenderFrameId) -> anyhow::Result<PresentStats>;
    fn telemetry(&self) -> RendererTelemetry;
}
```

The renderer owns GPU resources and exposes handles, captures, and telemetry only. No component outside `gfx_vk`, `render_graph`, or renderer internals receives raw Vulkan handles.

## Render feature

```rust
pub trait RenderFeature: Send {
    fn feature_id(&self) -> &'static str;
    fn required_gpu_features(&self) -> GpuFeatureMask;
    fn supported_quality_range(&self) -> QualityRange;

    fn prepare(
        &mut self,
        ctx: &mut RenderPrepareContext,
        frame: &FrameSnapshot,
    ) -> anyhow::Result<()>;

    fn record(
        &self,
        graph: &mut RenderGraph,
        frame: &FrameSnapshot,
    ) -> anyhow::Result<()>;

    fn telemetry(&self) -> FeatureTelemetry;
}
```

## Render graph

The render graph API can be ergonomic, but the implementation must avoid unbounded per-frame allocation after warmup.

```rust
pub struct RenderPassDesc {
    pub name: &'static str,
    pub queue: QueueClass,
    pub reads: Vec<ResourceUse>,
    pub writes: Vec<ResourceUse>,
    pub timing_scope: &'static str,
}

pub trait RenderGraphBuilder {
    fn create_image(&mut self, desc: ImageDesc) -> GraphImage;
    fn create_buffer(&mut self, desc: BufferDesc) -> GraphBuffer;
    fn import_swapchain_image(&mut self) -> GraphImage;
    fn import_external_image(&mut self, desc: ExternalImageDesc) -> GraphImage;
    fn add_pass<P>(&mut self, desc: RenderPassDesc, pass: P) -> GraphPassId
    where
        P: PassRecorder + 'static;
}

pub trait PassRecorder: Send + Sync {
    fn record(&self, cmd: &mut CommandRecorder) -> anyhow::Result<()>;
}
```

Implementation rule:

```text
- prototype builders may box pass recorders
- production renderer should reuse pass storage through arenas or feature-owned pass structs
- graph compile must validate resource hazards before command recording
- pass timing labels must be stable strings for telemetry comparison
```

## Sky renderer

```rust
pub struct SkySceneRequest {
    pub seed: u64,
    pub camera: CameraState,
    pub sun: SunParams,
    pub atmosphere: AtmosphereParams,
    pub clouds: Vec<CloudLayer>,
    pub quality: RenderQualityProfile,
    pub capture: Option<CaptureRequest>,
}

pub struct SkyRenderResult {
    pub frame: RenderFrameId,
    pub capture: Option<RenderCaptureId>,
    pub frame_stats: FrameTelemetry,
    pub feature_stats: Vec<FeatureTelemetry>,
}

pub trait SkyRenderer {
    fn render_sky(&mut self, request: SkySceneRequest) -> anyhow::Result<SkyRenderResult>;
}
```

## Material representation

CPU authoring data:

```rust
pub struct MaterialAuthoringDesc {
    pub material_id: MaterialId,
    pub name: String,
    pub semantic_class: MaterialClass,
    pub graph: Option<MaterialGraph>,
    pub source_images: Vec<ImageAssetId>,
    pub physical_hints: PhysicalMaterialHints,
    pub provenance: MaterialProvenance,
}
```

Renderer-resolved CPU material:

```rust
pub struct RuntimeMaterialDesc {
    pub material_id: MaterialId,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub transmission: f32,
    pub subsurface: f32,
    pub anisotropy: f32,
    pub clearcoat: f32,
    pub ior: f32,
    pub normal: MaterialInputRef,
    pub displacement: MaterialInputRef,
    pub procedural_seed: u64,
    pub flags: MaterialFlags,
}

pub enum MaterialInputRef {
    None,
    Texture(TextureId),
    ProceduralField(ProceduralFieldId),
    GeneratedCache(GeneratedTextureId),
}
```

Packed GPU material ABI:

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GpuMaterialPacked {
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub transmission: f32,
    pub subsurface: f32,
    pub anisotropy: f32,
    pub clearcoat: f32,
    pub ior: f32,
    pub flags: u32,
    pub normal_resource_index: u32,
    pub displacement_resource_index: u32,
    pub procedural_seed_low: u32,
    pub procedural_seed_high: u32,
    pub _pad0: [u32; 3],
}
```

GPU ABI rules:

```text
- no String, Vec, Box, Rust enum, trait object, or provider object in GPU structs
- resource references are integer indices into renderer-owned tables
- padding is explicit and initialized
- shader mirror structs are tested against CPU offsets
- use `bytemuck` or an equivalent internal check only after the dependency is deliberately accepted
- default resource index 0 should be a valid fallback texture/procedural field
```

## Material generator jobs

```rust
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct MaterialJobId(pub u64);

pub trait MaterialGenerator: Send {
    fn submit_generate(
        &mut self,
        request: MaterialGenerationRequest,
    ) -> anyhow::Result<MaterialJobId>;

    fn submit_refine(
        &mut self,
        material: MaterialId,
        feedback: MaterialEvalFeedback,
    ) -> anyhow::Result<MaterialJobId>;

    fn poll_job(&mut self, job: MaterialJobId) -> anyhow::Result<MaterialJobStatus>;
    fn cancel_job(&mut self, job: MaterialJobId) -> anyhow::Result<()>;
}
```

## Physics solver

```rust
pub trait PhysicsSolver: Send {
    fn solver_id(&self) -> &'static str;
    fn supports(&self, object: &PhysicalObjectDesc) -> bool;

    fn step(
        &mut self,
        dt_s: f32,
        world: &PhysicsWorldView,
        events: &[PhysicsEvent],
    ) -> anyhow::Result<PhysicsDelta>;

    fn telemetry(&self) -> SolverTelemetry;
}
```

## AI controller

```rust
pub trait AiCharacterController: Send {
    fn observe(
        &mut self,
        character: CharacterId,
        perception: PerceptionFrame,
    ) -> anyhow::Result<()>;

    fn decide(
        &mut self,
        character: CharacterId,
        dt_s: f32,
        world: &AiWorldView,
    ) -> anyhow::Result<Vec<AiIntent>>;

    fn receive_feedback(
        &mut self,
        character: CharacterId,
        feedback: ActionFeedback,
    ) -> anyhow::Result<()>;
}
```

AI provider adapters may use async runtimes internally, but the game loop consumes bounded `AiIntent` outputs and never blocks the render thread on a provider call.

## Voice runtime jobs and streams

```rust
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct SpeechJobId(pub u64);

pub trait VoiceRuntime: Send {
    fn submit_synthesis(
        &mut self,
        request: SpeechSynthesisRequest,
    ) -> anyhow::Result<SpeechJobId>;

    fn poll_synthesis(&mut self, job: SpeechJobId) -> anyhow::Result<SpeechJobStatus>;

    fn start_realtime_session(
        &mut self,
        session: VoiceSessionRequest,
    ) -> anyhow::Result<RealtimeVoiceSession>;

    fn lipsync(&mut self, audio: AudioStreamHandle) -> anyhow::Result<LipSyncTrack>;
}
```

## Visual evaluator jobs

```rust
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct EvalJobId(pub u64);

pub trait VisualEvaluator: Send {
    fn submit_eval(&mut self, request: EvalRequest) -> anyhow::Result<EvalJobId>;
    fn poll_eval(&mut self, job: EvalJobId) -> anyhow::Result<EvalJobStatus>;
}
```

Evaluator adapters may call external models, local Burn models, or deterministic image metrics. The engine stores the result schema, not provider-specific response objects.

## Safety note for `unsafe`

Use this crate-level pattern:

```rust
// Most crates:
#![forbid(unsafe_code)]

// gfx_vk and selected renderer internals:
#![deny(unsafe_op_in_unsafe_fn)]
```

Every unsafe/raw Vulkan block should include:

```text
- why Vulkano path was not sufficient
- Vulkan valid-usage assumptions
- lifetime/ownership assumptions
- validation/profiler coverage
```


## V3 GPU resource and job contracts

The following contracts should be added before descriptor-indexed materials or complex render features are implemented.

```rust
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct DescriptorEpoch(pub u64);

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct GpuSubmissionId {
    pub queue_class: QueueClass,
    pub timeline_value: u64,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct GpuResourceId {
    pub index: u32,
    pub generation: u32,
    pub kind: GpuResourceKind,
}

pub enum GpuResourceKind {
    Buffer,
    Image,
    ImageView,
    Sampler,
    AccelerationStructure,
    DescriptorTable,
    Pipeline,
}
```

A renderer implementation may keep these internal at first, but telemetry and eval artifacts should expose enough resource/descriptor metadata to debug lifetime problems.

Long-running tools should share a common job-status vocabulary, even if their concrete job IDs differ:

```rust
pub enum JobStatus<T> {
    Queued,
    Running { progress: Option<f32> },
    Complete(T),
    Failed(JobError),
    Cancelled,
}
```


## V3 presentation and lifetime contracts

```rust
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct PresentationSurfaceId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct SwapchainId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubmissionId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimelineValue(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct DescriptorEpoch(pub u64);

pub enum PresentationState {
    Unavailable,
    Available,
    Minimized,
    ResizePending,
    SurfaceLost,
    DeviceLost,
}

pub trait PresentationHost {
    fn presentation_state(&self) -> PresentationState;
    fn surface_id(&self) -> Option<PresentationSurfaceId>;
    fn current_extent(&self) -> Option<[u32; 2]>;
    fn request_redraw(&self);
}

pub struct ResourceRetirement {
    pub resource: GpuResourceId,
    pub retire_after: TimelineValue,
    pub descriptor_epoch: Option<DescriptorEpoch>,
}
```

These are still sketches. The first real Rust repository should make every public API example compile in a small `api_contracts` crate or doc-test crate once placeholder types are introduced.

# 03 — Core Interfaces

## Interface purpose

The interfaces must allow separately developed components to work together without sharing private implementation details. A renderer developer should be able to replace the cloud renderer without changing the evaluator. A physics developer should be able to change the fracture solver without changing AI. A voice provider should be replaceable without changing character logic.

## ID and handle policy

Use typed opaque IDs, not raw pointers or raw Vulkan handles.

```rust
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct AssetId(pub u128);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct MaterialId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct GeometryId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct SimObjectId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct CharacterId(pub u64);
```

Rules:

```text
- IDs are stable within an artifact run.
- Content assets should use content-addressed or manifest-addressed IDs.
- Runtime handles may be generational internally, but public IDs remain opaque.
- Raw GPU handles never cross Layer 0 interfaces.
```

## Interface versioning

Every public cross-crate interface should have a version.

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
- changing field meaning = major bump
- adding optional field = minor bump
- bug fix/no schema change = patch bump
- eval artifacts store interface versions
- old artifact manifests remain readable
```

## Frame snapshot

The frame snapshot is the read-only world view consumed by rendering, AI, evaluation, and audio systems.

```rust
pub struct FrameSnapshot {
    pub frame_index: u64,
    pub sim_time_s: f64,
    pub dt_s: f32,
    pub camera: CameraState,
    pub exposure: CameraExposure,
    pub weather: WeatherState,
    pub scene_id: SceneSnapshotId,
    pub physics_id: PhysicsSnapshotId,
    pub character_id: CharacterSnapshotId,
    pub render_quality: RenderQualityProfile,
}
```

Avoid long-lived borrowed snapshots across worker threads. Use stable snapshot IDs and immutable snapshot stores where async workers need access.

## Module lifecycle

```rust
pub trait EngineModule {
    fn name(&self) -> &'static str;
    fn interface_version(&self) -> InterfaceVersion;

    fn register(&mut self, registry: &mut ModuleRegistry) -> anyhow::Result<()>;
    fn initialize(&mut self, ctx: &mut InitContext) -> anyhow::Result<()>;
    fn begin_frame(&mut self, frame: &FrameSnapshot) -> anyhow::Result<()>;
    fn update(&mut self, ctx: &mut UpdateContext) -> anyhow::Result<ModuleOutput>;
    fn end_frame(&mut self, frame: &FrameSnapshot) -> anyhow::Result<()>;
    fn health(&self) -> ComponentHealth;
    fn telemetry(&self) -> ModuleTelemetry;
}
```

`ModuleOutput` may contain events, asset requests, render tasks, simulation deltas, and telemetry. It must not contain private component state.

## Event bus

Use typed events for component communication.

```rust
pub enum EngineEvent {
    AssetRequested(AssetRequest),
    AssetReady(AssetReady),
    PhysicsImpact(ImpactEvent),
    MaterialDamage(MaterialDamageEvent),
    FractureCreated(FractureEvent),
    FluidStateChanged(FluidEvent),
    GasStateChanged(GasEvent),
    AiIntent(AiIntentEvent),
    SpeechRequested(SpeechRequest),
    SpeechReady(SpeechReady),
    RenderCaptureReady(RenderCaptureId),
    EvalResult(EvalResult),
}
```

Event rules:

```text
- every event has frame index, timestamp, source component, and schema version
- events are append-only for a frame
- events should be replayable from artifact logs
- large binary payloads go through asset/capture stores, not inline events
```

## Asset request contract

```rust
pub struct AssetRequest {
    pub request_id: RequestId,
    pub semantic_class: AssetSemanticClass,
    pub constraints: AssetConstraints,
    pub priority: AssetPriority,
    pub deadline_frame: Option<u64>,
    pub provenance_required: bool,
}

pub struct AssetReady {
    pub request_id: RequestId,
    pub asset_id: AssetId,
    pub manifest_id: AssetManifestId,
    pub quality_report: Option<AssetQualityReport>,
}
```

Asset requests should be asynchronous. The frame loop should never block on asset generation.

## Render tasks

```rust
pub enum RenderTask {
    UpdateScene(RenderSceneUpdate),
    RenderMainView(MainViewRenderRequest),
    RenderCapture(CaptureRequest),
    BuildAccelerationStructure(AccelerationStructureRequest),
    BakeMaterial(MaterialBakeRequest),
}
```

Rendering consumes snapshots and tasks. It does not query physics, AI, voice, or asset internals directly.

## Telemetry contract

```rust
pub struct FrameTelemetry {
    pub frame_index: u64,
    pub cpu_frame_ms: f32,
    pub gpu_frame_ms: Option<f32>,
    pub gpu_passes: Vec<GpuPassTiming>,
    pub vram_used_mb: Option<u32>,
    pub transient_memory_mb: Option<u32>,
    pub draw_calls: u32,
    pub dispatch_calls: u32,
    pub validation_errors: u32,
    pub dropped_frames: u32,
}
```

Telemetry must be emitted even when a frame fails. Failed-frame telemetry is often the most useful data.

## Threading model

```text
Main thread:
  winit event loop, window events, app lifecycle

Render thread:
  renderer state, render graph compile/record, queue submission

Simulation thread(s):
  physics, animation, world state updates

Asset workers:
  imports, bakes, generated caches

AI/voice workers:
  external provider calls, local inference, speech synthesis
```

Rules:

```text
- no AI/voice provider call on render thread
- no blocking asset load on render thread
- no CPU readback except explicit capture/eval mode
- no Vulkan device wait in normal frame path
```

## Component health

```rust
pub enum ComponentHealth {
    Ok,
    Degraded { reason: String },
    Disabled { reason: String },
    Failed { reason: String },
}
```

This lets the engine degrade gracefully when ray tracing, cloud high quality, AI provider access, or voice synthesis is unavailable.

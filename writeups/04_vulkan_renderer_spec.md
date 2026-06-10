# 04 — Vulkan Renderer Specification

## Renderer goal

Build a high-performance real-time renderer that can grow from a sky-only milestone into a hybrid photoreal renderer for materials, humans, fluids, gases, and physical consequence.

Priority order:

```text
1. correctness and repeatability
2. frame-time stability
3. photoreal visual quality
4. scalable GPU utilization
5. inspectable artifacts and telemetry
```

## Vulkan baseline

Target Vulkan 1.3 as the practical baseline and feature-detect Vulkan 1.4 capabilities where available. Vulkan 1.4 is useful because it consolidates several previously optional features and raises minimum requirements, but it should not be the only path until the target hardware matrix supports it.

Use feature buckets:

```rust
pub enum RendererGpuTier {
    Vulkan13Portable,
    Vulkan13WithDescriptorIndexing,
    Vulkan13WithRayQuery,
    Vulkan13WithFullRayTracing,
    Vulkan14Preferred,
}
```

The engine must log a `GpuCapabilities` report at startup.


## GPU capability and limits report

`GpuCapabilities` must not be just booleans. It should capture the limits that change renderer decisions:

```text
- API version and enabled extensions
- queue family capabilities
- timestamp period and timestamp-valid bits
- subgroup support and subgroup size behavior
- maximum sampled/storage images and buffers
- descriptor indexing limits and partially-bound support
- descriptor buffer support and alignment requirements
- max image dimensions and supported formats
- depth/color/HDR format support
- ray query / acceleration structure / full ray pipeline support
- memory heaps, device-local budget, host-visible budget
- timeline semaphore support
- dynamic rendering / synchronization2 support
```

The startup report should be stored in every eval artifact bundle. Hardware tiers should be selected from this report rather than from GPU names alone.

## Vulkano strategy

Use Vulkano for:

```text
- instance/device/surface/swapchain bootstrap
- safe buffer/image abstractions where performance is acceptable
- descriptor and command-buffer scaffolding
- validation-friendly development
```

Retain a raw Vulkan path for:

```text
- extensions not exposed or awkward in Vulkano
- descriptor-buffer experiments
- ray-tracing pipeline work if needed
- vendor-specific profiling hooks
- performance-critical paths verified by captures
```

Raw Vulkan policy:

```text
- only inside gfx_vk or renderer internals
- all unsafe code documented
- validation and profiler capture required before merge
- no raw handles in core interfaces
```

## Renderer architecture

```text
Renderer frontend:
  consumes scene snapshots, render tasks, quality profile

Scene proxy:
  renderer-owned GPU representation of scene objects

Render feature system:
  atmosphere, sun, clouds, PBR, shadows, skin, hair, fluid, gas, postprocess

Render graph:
  compiles frame passes, resources, synchronization, and queue submissions

GPU backend:
  Vulkano/raw Vulkan device, queues, memory, descriptors, pipelines

Capture system:
  exports color, HDR, AOVs, debug buffers, and telemetry
```

## Renderer API

```rust
pub trait Renderer {
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

## Render feature API

```rust
pub trait RenderFeature {
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

Feature rules:

```text
- every feature owns its quality knobs
- every feature emits GPU pass timings
- every feature has debug visualizations
- every feature has at least one minimal regression scene
- every feature can be disabled or degraded by quality profile
```

## Render graph

The render graph is the only place where inter-pass resource use and synchronization are declared.

```rust
pub struct RenderPassDesc {
    pub name: &'static str,
    pub queue: QueueClass,
    pub reads: Vec<ResourceUse>,
    pub writes: Vec<ResourceUse>,
    pub timing_scope: &'static str,
    pub debug_color: Option<[f32; 4]>,
}
```

The graph compiler must:

```text
- validate read/write dependencies
- insert image/buffer barriers
- schedule graphics/compute/transfer work
- record GPU timestamps
- emit debug labels
- support transient resource aliasing later
- fail in debug/eval mode on undefined resource use
```

## Descriptor strategy

Start simple, but design for bindless-style scale.

Phase 1:

```text
- fixed descriptor set layouts
- per-frame uniform/storage buffers
- explicit sampled image arrays for sky/cloud LUTs
```

Phase 2:

```text
- descriptor indexing for material/texture arrays
- partially bound arrays where supported
- update-after-bind where useful
```

Phase 3:

```text
- descriptor buffer experiments on hardware that supports it
- renderer-owned fallback to descriptor indexing or normal sets
```

Do not make descriptor buffer a first-milestone requirement.


### Descriptor lifetime rules

Descriptor indexing is not accepted until the renderer has explicit descriptor lifetime accounting.

```text
- every descriptor write belongs to a descriptor epoch
- every submitted frame records the highest epoch it can access
- resources referenced by descriptors cannot be destroyed until all dependent GPU work is complete
- update-after-bind is allowed only for descriptor bindings marked for it and only under documented synchronization rules
- partially-bound arrays require shader-side validity rules and debug validation in eval builds
- descriptor-buffer experiments must have a descriptor-indexing fallback
```

The renderer should prefer predictable descriptor heaps and stable resource indices over frequent descriptor-set churn.

## Ray tracing strategy

Ray tracing is optional. The first sky milestone does not require hardware ray tracing.

Use modes:

```rust
pub enum RayTracingMode {
    Unsupported,
    RayQueryOnly,
    FullPipeline,
}
```

Potential uses later:

```text
- hard/soft shadow references
- reflections
- ambient occlusion
- material validation
- human eye/skin close-up references
- offline/oracle comparisons
```

## Color and camera pipeline

Use a physical-camera-inspired pipeline:

```text
- HDR internal color
- scene-linear lighting
- camera exposure value
- aperture/shutter/ISO metadata
- white balance
- tone mapping
- bloom from HDR luminance, not arbitrary glow
- optional lens effects as separate features
```

Required outputs:

```text
- SDR preview PNG
- HDR/EXR color capture
- luminance buffer
- depth
- normals where geometry exists
- motion vectors where temporal reconstruction exists
- object/material IDs where objects exist
- sky/cloud debug AOVs for first milestone
```

## Material model

Initial runtime material baseline:

```text
- glTF-style metallic/roughness PBR
- base color
- normal source
- roughness/metallic
- emissive
- occlusion
```

Future material extension:

```text
- transmission
- subsurface
- anisotropy
- clearcoat
- volume
- procedural fields
- MaterialX-inspired graph subset
```

Runtime material must be compact and GPU-friendly. Material graphs are authoring/generation-time structures that compile to runtime material data and optional generated texture caches.


## GPU material ABI rule

Do not copy rich Rust material structs directly into GPU buffers. The renderer should compile material assets through three levels:

```text
MaterialAuthoringDesc:
  source photos, procedural graph, semantic class, provenance, editor controls

RuntimeMaterialDesc:
  renderer-resolved material with texture/procedural handles and quality policy

GpuMaterialPacked:
  shader ABI with fixed-size scalar/vector fields, integer resource indices, flags, and explicit padding
```

`GpuMaterialPacked` is the only material struct that may be mapped into storage/uniform buffers. It must have explicit layout tests, shader-side mirror definitions, and validation that CPU and shader offsets match.

## Shader workflow

Shader development requirements:

```text
- shader source lives under /shaders
- shader compilation is reproducible
- SPIR-V output is cached
- shader variants are counted and budgeted
- hot reload only in dev builds
- shader compiler warnings fail CI in strict mode
- every shader pass has debug names and timestamp scopes
```

## Performance constraints

General hot-path rules:

```text
- no per-frame heap allocation in renderer hot paths after warmup
- no blocking GPU waits during normal frames
- no synchronous asset IO during frames
- no shader compilation during gameplay/release
- no CPU readback except explicit capture/eval mode
- no unbounded descriptor or pipeline growth
```

## Renderer definition of done

The renderer architecture is ready for the first milestone when:

```text
- desktop window clears and presents reliably
- headless render target produces image artifacts
- render graph records at least one pass with timing
- validation layers report no errors in smoke tests
- shader compilation is reproducible
- capture export works
- telemetry CSV/JSON output works
- sky renderer feature slots exist for sun, atmosphere, and clouds
```


## V4 backend boundary and resource-lifetime requirement

The renderer is not complete with only a render graph and material model. It must also implement explicit GPU lifetime tracking before descriptor-indexed materials or general bindless-style resources are enabled:

```text
- generational GPU resource IDs for stale-handle detection
- descriptor epochs for descriptor-table updates
- submitted-frame resource usage records
- timeline/fence-based retirement queues
- shader ABI versioning
- reflection tests for shader bindings and packed structs
- async capture/readback through staging resources
- validation/eval mode checks for stale or destroyed resources
- capture/readback resources isolated to explicit eval/dev modes
```

Descriptor buffer remains an optional backend experiment. It is not a milestone-1 requirement and cannot remove the need for resource lifetime tracking.

See `24_gpu_resource_lifetime_and_sync.md` and `40_shader_abi_and_gpu_data_layout.md` for the required policies.

## V4 raw interop rule

Vulkano-first does not mean raw Vulkan is forbidden. It means raw Vulkan is centralized, reviewed, and owned by the graphics backend. Raw Vulkan or `ash` code must remain inside `gfx_vk` or approved renderer backend internals. It must not mutate or destroy wrapper-owned resources unless that ownership path is explicitly documented, tested, and covered by validation/profiler evidence.

A raw path must include:

```text
- why the safe wrapper path is insufficient
- Vulkan valid-usage assumptions
- ownership and destruction-order assumptions
- synchronization assumptions
- validation coverage
- fallback or capability gate
```

## V4 presentation and backend boundary

The renderer must support two output modes:

```text
Presented mode:
  render graph imports the current swapchain image for one frame, then presents
  through gfx_vk/app_desktop coordination.

Offscreen mode:
  render graph writes to renderer-owned images, exports captures, and never
  requires a swapchain.
```

Imported swapchain images are transient graph resources. Renderer features must not retain them after the frame. Feature-level renderer APIs expose capabilities, resource handles, graph resources, and command abstractions rather than raw Vulkan handles.

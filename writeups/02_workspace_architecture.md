# 02 — Workspace Architecture

## Cargo workspace

Use a multi-crate Rust workspace. Crate boundaries should enforce component boundaries.

```text
game-engine/
  Cargo.toml
  AGENTS.md
  crates/
    app_desktop/
    app_headless/
    engine_core/
    telemetry/
    scene_schema/
    gfx_vk/
    render_graph/
    shader_lab/
    renderer_realtime/
    renderer_oracle/
    asset_pipeline/
    material_lab/
    physics_core/
    physics_gpu/
    human_gen/
    ai_world/
    voice_runtime/
    eval_lab/
    dev_web/
    tools_cli/
  assets/
  configs/
  docs/
  eval_runs/
  shaders/
  tests/
```

## Dependency layers

```text
Layer 0 — pure contracts
  engine_core
  telemetry
  scene_schema

Layer 1 — platform and storage infrastructure
  gfx_vk
  render_graph
  shader_lab
  asset_pipeline

Layer 2 — major runtime systems
  renderer_realtime
  renderer_oracle
  physics_core
  physics_gpu
  material_lab
  human_gen
  ai_world
  voice_runtime
  eval_lab

Layer 3 — applications and tools
  app_desktop
  app_headless
  dev_web
  tools_cli
```

Rules:

```text
- Layer 0 cannot depend on Vulkan, windowing, audio, AI providers, or web servers.
- Renderer internals may depend on gfx_vk and render_graph.
- Physics contracts live in physics_core; GPU kernels live in physics_gpu.
- AI and voice providers are adapter crates, not core types.
- app_desktop owns winit; the renderer does not own the OS event loop.
- app_headless exists for deterministic evaluation and CI-friendly rendering.
```

## Crate responsibilities

### `app_desktop`

Owns desktop application boot:

```text
- winit event loop
- window creation and resize
- swapchain surface creation request
- input forwarding
- dev/release mode selection
```

Winit 0.30 uses the `ApplicationHandler` / `run_app` model. The desktop app should create the window in the app handler lifecycle and pass raw-window-handle-compatible information to `gfx_vk`.

### `app_headless`

Owns offscreen evaluation and automation:

```text
- no visible window required
- fixed seeds
- fixed camera paths
- deterministic artifact output
- CI smoke tests where GPU access exists
```

This app is the preferred target for sky evaluation.

### `engine_core`

Contains runtime primitives:

```text
- frame clock
- module lifecycle
- typed event bus
- task/scheduler contracts
- handles and IDs
- error boundaries
- interface versioning
```

No Vulkan, AI, voice, windowing, or web dependencies.

### `telemetry`

Owns structured metrics:

```text
- CPU timings
- GPU timings
- memory telemetry
- frame statistics
- artifact manifests
- CSV/JSON output
- tracing integration
```

Telemetry must be lightweight enough for release builds, with detailed modes enabled in profiling/evaluation builds.

### `scene_schema`

Owns pure scene contracts:

```text
- transforms
- cameras
- exposure
- material descriptors
- geometry descriptors
- animation references
- semantic tags
- physical object descriptors
```

Scene schema is not a renderer or physics engine.

### `gfx_vk`

Owns Vulkan/Vulkano infrastructure:

```text
- Vulkan library and instance
- physical-device selection
- device and queues
- surface/swapchain creation
- memory allocation
- descriptor allocation
- pipeline cache
- debug labels
- validation setup
- feature detection
- raw Vulkan escape hatch
```

Policy:

```text
- Vulkano path first
- raw/ash path only inside gfx_vk or renderer internals
- no raw Vulkan handles across public component interfaces
```

### `render_graph`

Owns frame dependency scheduling:

```text
- render/compute/transfer pass declarations
- transient resources
- resource lifetimes
- synchronization barriers
- queue submissions
- GPU timestamp scopes
- graph validation
```

All rendering features express dependencies through the graph.

### `shader_lab`

Owns shader workflow:

```text
- shader compilation
- SPIR-V caching
- shader reflection
- hot reload in dev mode
- shader variant management
- shader tests
- debug naming
```

### `renderer_realtime`

Owns real-time rendering:

```text
- atmosphere
- sun
- volumetric clouds
- PBR material shading
- shadows
- volumetrics
- denoising
- temporal reconstruction
- tone mapping
- presentation-ready output
```

### `renderer_oracle`

Owns slow reference renders:

```text
- high-sample path-traced or high-quality captures
- material reference scenes
- non-realtime comparison images
```

The oracle may use Vulkan compute/ray tracing or CPU fallback, but it must not block the real-time renderer.

### `asset_pipeline`

Owns asset import/build/cache:

```text
- content-addressed asset IDs
- glTF import/export subset
- texture compression
- generated texture caches
- dependency graph
- immutable build artifacts
```

MaterialX and OpenUSD are later interchange paths, not first-milestone blockers.

### `material_lab`

Owns material generation and validation:

```text
- procedural fields
- photo-to-material tools
- material graph generation
- bake targets
- preview scenes
- material score reports
```

### `physics_core` and `physics_gpu`

`physics_core` owns contracts and CPU/reference solvers. `physics_gpu` owns Vulkan compute kernels and GPU interop buffers.

### `human_gen`

Owns generated human assets:

```text
- body/face parameters
- skin fields
- eye descriptors
- hair descriptors
- rig descriptors
- LOD tiers
- consent/provenance metadata
```

### `ai_world`

Owns AI character state and decision contracts:

```text
- perception frames
- memory records
- goal/state models
- action intents
- provider adapters
- local fallback policies
```

### `voice_runtime`

Owns voice generation and lip sync contracts:

```text
- TTS provider adapters
- realtime voice sessions
- audio streaming
- voice caching
- phoneme/viseme timing
- disclosure metadata
```

### `eval_lab`

Owns visual/performance feedback loops:

```text
- deterministic scene runner
- capture export
- AI image judge adapters
- artifact schema
- scoring/rubric engine
- regression gates
```

### `dev_web`

Optional dashboard. Hide concrete web framework choice behind internal traits.

### `tools_cli`

Owns commands:

```text
cargo run -p tools_cli -- eval sky --profile high --seed-set locked_001
cargo run -p tools_cli -- shader check
cargo run -p tools_cli -- assets build
cargo run -p tools_cli -- telemetry summarize eval_runs/latest
```

## Build targets

```text
dev_debug:
  validation, labels, hot reload, slow assertions

dev_profile:
  optimized, timestamps, profiler markers, optional validation

eval:
  deterministic, fixed seeds, capture all artifacts, no hot reload

release:
  no validation, no CPU readbacks except explicit capture, locked shader cache
```

## Repository testing layout

```text
tests/
  smoke/
  renderer/
  render_graph/
  shader/
  material/
  physics/
  eval/

configs/
  hardware_profiles/
  quality_profiles/
  eval_rubrics/
  seed_sets/
```

Every component should ship with both API-level tests and artifact-level evaluation where applicable.


## V3 architecture hardening

The workspace should also treat these as first-class architectural concerns, even if they are implemented as modules inside existing crates rather than new crates:

```text
runtime job system:
  submit/poll/cancel for material, AI/model, voice, eval, bake, and oracle jobs

gpu resource registry:
  generational resource handles, descriptor epochs, retirement queues, memory categories

artifact/schema registry:
  schema versions, compatibility checks, artifact validators, migration notes

provenance and rights registry:
  source data, generated data, consent, voice/persona metadata, allowed uses

security/dependency policy:
  secrets handling, dependency upgrade review, dashboard access controls, coding-agent boundaries
```

These concerns may live initially in `engine_core`, `gfx_vk`, `telemetry`, `asset_pipeline`, and `eval_lab`, but their public contracts should remain project-owned and provider-independent.


## V3 ownership clarifications

### Window/presentation ownership

```text
app_desktop:
  owns winit event loop, window creation, redraw requests, and OS lifecycle events

gfx_vk:
  owns Vulkan instance, physical/logical device, queues, surface, swapchain,
  memory allocation, descriptor infrastructure, and raw Vulkan escape hatches

renderer_realtime:
  owns render features, render graph, scene proxy, offscreen targets, captures,
  and presentation requests, but not the OS event loop
```

`app_headless` creates no window and no swapchain. It asks `gfx_vk` for an offscreen-capable device and render targets. It is valid for deterministic artifact generation; it is not automatically valid for final performance certification unless run on the named target hardware.

### Backend boundary

No crate outside `gfx_vk`, `render_graph`, `renderer_realtime`, `renderer_oracle`, and `shader_lab` should depend on Vulkan/Vulkano/ash types. No crate outside `app_desktop` should depend on winit types.

Raw Vulkan code belongs behind `gfx_vk` backend modules. Renderer features should call renderer/backend abstractions and should not pass raw handles across component interfaces.

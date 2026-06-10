# 28 — Dependency and Capability Matrix

## Purpose

This document turns the dependency choices into explicit roles, boundaries, and capability tiers. It should be updated whenever a dependency is added, upgraded, or replaced.

## Dependency rule

```text
Dependencies are implementation details of crates, not cross-project contracts.
```

A public interface may expose project-owned types such as `RenderFrameId`, `GpuCapabilities`, `SpeechRequest`, or `EvalReport`. It should not expose concrete provider/library types such as Vulkano command buffers, Burn tensors, OpenAI request structs, or dashboard framework request objects.

## Dependency matrix

| Dependency / tool | Primary owner crate | Role | Public-interface leakage allowed? | Notes |
|---|---|---|---|---|
| Vulkan | `gfx_vk`, `renderer_realtime`, `physics_gpu` | GPU graphics/compute API | No raw handles outside graphics internals | Runtime GPU foundation. |
| Vulkano | `gfx_vk` | Safe Rust Vulkan wrapper | No | Default path for bootstrap and safe abstractions. |
| `ash` or raw Vulkan calls | `gfx_vk`, selected renderer internals | Escape hatch | No | Only when Vulkano is insufficient or too costly. Requires unsafe review. |
| winit | `app_desktop` | Window/event loop | No | Renderer receives surface/swapchain requests, not event-loop ownership. |
| Burn | `eval_lab`, `ai_world`, `material_lab`, tooling | Local AI/model/tensor tooling | No | Worker-side only; do not couple to render thread. |
| OpenAI or other AI APIs | provider adapter crates | External evaluation/dialogue/voice services | No | Provider traits only. Store model/provider metadata in artifacts. |
| Dev web framework | `dev_web` | Dashboard/websocket/API | No | `gweb` unresolved; pick concrete crate after a spike. |
| glTF importer/exporter | `asset_pipeline` | Runtime asset interchange | Only through project asset schema | First asset interchange path. |
| MaterialX | `material_lab`, later `asset_pipeline` | Material/lookdev interchange | No direct dependency in runtime renderer | Later material graph source. |
| OpenUSD | later `asset_pipeline` / tools | Scene composition/interchange | No direct runtime dependency initially | Do not block first milestone. |
| RenderDoc/Nsight/RGP | external tools | Debug/profiling | N/A | Use markers/captures, not runtime dependency. |

## Version-pin policy

Use explicit dependency pins during milestone work.

```text
Cargo.toml policy:
- pin major/minor versions for milestone branches
- upgrade dependencies in dedicated PRs/tasks only
- rerun smoke tests and eval baselines after upgrades
- record dependency versions in eval artifact manifests
```

Example:

```toml
[workspace.dependencies]
anyhow = "1"
winit = "0.30"
vulkano = "0.35"
# Burn only in AI/eval/tool crates, not renderer core.
burn = { version = "0.21", optional = true }
```

The exact versions must be refreshed when implementation starts. This writeup captures the architecture, not a permanent lockfile.

## Feature flags

Use feature flags to avoid unnecessary dependency coupling.

```toml
[features]
default = ["desktop"]
desktop = ["dep:winit"]
headless = []
validation = []
renderdoc_markers = []
ray_tracing = []
ai_eval_openai = []
ai_eval_local_burn = ["dep:burn"]
dev_web = []
```

Crate rule:

```text
renderer_realtime must not require ai_eval_openai, voice providers, or dev_web.
app_headless must not require winit unless explicitly testing surface creation.
eval_lab may depend on AI adapters, but only behind features.
```

## Vulkan capability tiers

The renderer should detect capabilities once at startup and expose a stable `GpuCapabilities` report.

| Tier | Requirement | Intended use | First milestone required? |
|---|---|---|---|
| Tier 0 — Vulkan 1.3 portable | Vulkan 1.3, dynamic rendering, Synchronization2, timestamps | Clear, HDR target, basic render graph, sky fallback | Yes |
| Tier 1 — Descriptor indexing | Tier 0 + descriptor indexing features | Texture/material scale, bindless-like arrays | Not required for sky, design-ready |
| Tier 2 — Ray query | Tier 1 + ray query | Later shadows/reflections/reference checks | No |
| Tier 3 — Full ray tracing | Tier 1 + acceleration structures + RT pipeline | Later hybrid rendering and oracle paths | No |
| Tier 4 — Vulkan 1.4 preferred | Vulkan 1.4 features/limits where available | Cleaner advanced path and future performance | No, but preferred when present |

## GPU capability report schema

```rust
pub struct GpuCapabilities {
    pub device_name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub api_version: VulkanApiVersion,
    pub driver_version: String,
    pub queue_families: QueueFamilyReport,
    pub memory_heaps: Vec<MemoryHeapReport>,
    pub supports_dynamic_rendering: bool,
    pub supports_synchronization2: bool,
    pub supports_timeline_semaphores: bool,
    pub supports_descriptor_indexing: bool,
    pub supports_descriptor_buffer: bool,
    pub supports_buffer_device_address: bool,
    pub supports_ray_query: bool,
    pub supports_ray_tracing_pipeline: bool,
    pub supports_acceleration_structure: bool,
    pub max_image_dimension_2d: u32,
    pub max_storage_buffer_range: u64,
    pub timestamp_period_ns: f32,
    pub selected_tier: RendererGpuTier,
}
```

## Capability negotiation rules

```text
1. Detect instance extensions.
2. Select physical device using required baseline features.
3. Detect optional features.
4. Select queue families.
5. Build RendererGpuTier.
6. Load feature modules compatible with tier and quality profile.
7. Log all enabled features and rejected optional features.
8. Store report in every eval artifact bundle.
```

## Desktop versus headless GPU setup

### Desktop path

```text
app_desktop:
  owns winit EventLoop and Window
  requests surface creation
  handles resize/minimize/suspend/resume
  forwards input and redraw requests

gfx_vk:
  creates Vulkan surface/swapchain from window handles
  handles swapchain recreation
  reports presentation stats
```

### Headless path

```text
app_headless:
  creates fixed run context
  no visible window required
  fixed seeds and camera paths

gfx_vk:
  creates offscreen images
  renders into capture targets
  performs readback only in explicit capture/eval mode
```

## Raw Vulkan escape hatch requirements

A raw path requires this review block in code or nearby docs:

```text
Raw Vulkan justification:
- Safe wrapper limitation:
- Vulkan valid-usage assumptions:
- Ownership/lifetime assumptions:
- Synchronization assumptions:
- Validation coverage:
- Capture/profiler evidence:
- Fallback path:
```

## Burn integration boundary

Burn should run in workers, not on the render thread.

```rust
pub trait LocalModelRunner {
    fn enqueue(&self, job: ModelJob) -> anyhow::Result<ModelJobId>;
    fn try_poll(&self, id: ModelJobId) -> anyhow::Result<Option<ModelOutput>>;
    fn telemetry(&self) -> ModelRunnerTelemetry;
}
```

Rules:

```text
- no Burn tensors in renderer public APIs
- no renderer Vulkan command buffer waits on Burn jobs
- no model inference on the frame-critical render thread
- AI/model jobs must have deadlines and fallback results
```

## Dashboard boundary

Use a dashboard trait:

```rust
pub trait DevUiServer {
    fn start(&mut self, config: DevUiConfig) -> anyhow::Result<()>;
    fn publish_event(&self, event: DevUiEvent) -> anyhow::Result<()>;
    fn stop(&mut self) -> anyhow::Result<()>;
}
```

Do not choose the concrete framework until `eval_lab` can already emit artifact bundles.

## Dependency upgrade checklist

```text
- update Cargo.toml/Cargo.lock
- run cargo fmt/check/test/clippy
- run Vulkan smoke test
- run headless capture smoke test
- run sky eval mini seed set
- update dependency versions in docs/manifest if relevant
- store before/after telemetry for renderer-impacting upgrades
```

# 33 — First Epics and Backlog

## Purpose

This backlog converts the writeups into component-scoped implementation epics. It is designed for Codex/ChatGPT-assisted work where each task has narrow file boundaries, acceptance checks, and no hidden scope expansion.

## Backlog rules

```text
- one task modifies one crate or one tightly related crate pair
- every task states allowed paths
- every task states forbidden paths
- every task has acceptance checks
- no task may relax rubrics, thresholds, or milestone boundaries
- no task may add non-sky content before sky certification
```

## Epic E0 — Repository skeleton

### E0-001 Create workspace

Allowed paths:

```text
Cargo.toml
crates/*/Cargo.toml
crates/*/src/lib.rs
```

Tasks:

```text
- create workspace package layout
- add empty library crates
- add app crates
- add workspace dependencies but keep features minimal
```

Acceptance:

```bash
cargo check --workspace
cargo test --workspace
```

### E0-002 Add AGENTS and documentation structure

Allowed paths:

```text
AGENTS.md
docs/**
```

Tasks:

```text
- install project coding-agent rules
- add docs index
- add interface versioning policy
```

Acceptance:

```text
AGENTS.md exists at repo root.
Docs mention current milestone: sky only.
```

### E0-003 Add CI-style local check script

Allowed paths:

```text
scripts/**
justfile or Makefile
```

Tasks:

```text
- add commands for fmt/check/test/clippy
- add placeholder shader/eval commands
```

Acceptance:

```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Epic E1 — Core contracts

### E1-001 Add ID and version types

Crate:

```text
engine_core
```

Tasks:

```text
- add EntityId, AssetId, MaterialId, GeometryId, RenderFrameId, RenderCaptureId
- add InterfaceVersion
- add crate-level unsafe policy
```

Acceptance:

```text
Types are repr-transparent where appropriate.
No Vulkan, winit, AI, or voice dependencies.
```

### E1-002 Add frame snapshot and event bus skeleton

Crates:

```text
engine_core
scene_schema
```

Tasks:

```text
- define FrameSnapshot
- define EngineEvent enum
- define EventBus trait
- define minimal CameraState and exposure schema
```

Acceptance:

```text
Renderer and eval crates can depend on these contracts without circular dependency.
```

### E1-003 Add telemetry schema

Crate:

```text
telemetry
```

Tasks:

```text
- define FrameTelemetry
- define GpuPassTiming
- define MemoryTelemetry
- define artifact manifest structs
- add JSON/CSV writer helpers
```

Acceptance:

```text
A unit test writes and reads a sample manifest.
```

## Epic E2 — Desktop/headless app shells

### E2-001 winit desktop shell

Crate:

```text
app_desktop
```

Tasks:

```text
- implement ApplicationHandler skeleton
- create window on lifecycle event
- request redraws correctly
- send resize/close events into engine_core
```

Forbidden:

```text
- no Vulkan setup in app_desktop except calling gfx_vk API
- no renderer feature code
```

Acceptance:

```text
Desktop app opens, redraws a placeholder frame, and closes cleanly.
```

### E2-002 headless shell

Crate:

```text
app_headless
```

Tasks:

```text
- implement deterministic run loop
- accept seed/profile/output directory
- write placeholder artifact manifest
```

Acceptance:

```bash
cargo run -p app_headless -- --seed 1 --out target/eval_smoke
```

## Epic E3 — Vulkan bootstrap

### E3-001 Device creation and capability report

Crate:

```text
gfx_vk
```

Tasks:

```text
- load Vulkan library
- create instance
- enumerate physical devices
- select device based on Vulkan 1.3 baseline
- create logical device and queues
- fill GpuCapabilities
```

Acceptance:

```text
Headless app logs GpuCapabilities.
Validation enabled in dev/profile modes.
```

### E3-002 Desktop surface and swapchain

Crates:

```text
gfx_vk
app_desktop
```

Tasks:

```text
- create surface from window handle
- create swapchain
- handle resize and swapchain recreation
- present clear color
```

Acceptance:

```text
Desktop app shows a clear color.
No Vulkan validation errors in smoke test.
```

### E3-003 Headless offscreen target

Crate:

```text
gfx_vk
```

Tasks:

```text
- create offscreen color target
- clear it
- copy to readback only in capture mode
- save a placeholder image through eval_lab/tooling
```

Acceptance:

```text
Headless app exports a non-empty PNG.
```

## Epic E4 — Render graph skeleton

### E4-001 Graph resources and passes

Crate:

```text
render_graph
```

Tasks:

```text
- define GraphImage and GraphBuffer handles
- define ResourceUse
- define RenderPassDesc
- build pass DAG validation
```

Acceptance:

```text
Unit test: read-before-write dependency fails.
Unit test: simple write-then-read graph succeeds.
```

### E4-002 Vulkan command recording bridge

Crates:

```text
render_graph
gfx_vk
```

Tasks:

```text
- translate simple graph into command buffer work
- add GPU timestamp scopes
- emit debug labels
```

Acceptance:

```text
Clear-color pass reports GPU timing.
```

## Epic E5 — Shader lab

### E5-001 Shader compilation path

Crate:

```text
shader_lab
```

Tasks:

```text
- choose initial shader source language
- compile to SPIR-V reproducibly
- cache outputs
- report diagnostics
```

Acceptance:

```bash
cargo run -p tools_cli -- shader check
```

### E5-002 Shader reflection and pass validation

Tasks:

```text
- reflect descriptor/resource bindings
- compare reflection against render pass declarations
- fail strict mode on mismatch
```

Acceptance:

```text
A deliberately mismatched shader/resource test fails.
```

## Epic E6 — HDR capture and tone mapping

### E6-001 HDR render target and tone map

Crates:

```text
renderer_realtime
gfx_vk
render_graph
```

Tasks:

```text
- render into HDR color target
- tone map to SDR output
- expose camera exposure metadata
```

Acceptance:

```text
Headless capture includes SDR PNG and HDR metadata.
```

### E6-002 Capture artifact bundle

Crates:

```text
eval_lab
telemetry
tools_cli
```

Tasks:

```text
- create eval_runs directory
- write manifest.json
- write settings.json
- write telemetry CSVs
- link capture files
```

Acceptance:

```text
Artifact bundle validates against schema in 30_artifact_and_schema_contracts.md.
```

## Epic E7 — Sun and exposure

### E7-001 Sun feature

Crate:

```text
renderer_realtime
```

Tasks:

```text
- add SunFeature
- render HDR sun disc
- support explicit direction
- output sun metadata
```

Acceptance:

```text
Sun appears in final PNG and HDR capture.
Exposure changes do not cause unstable jumps.
```

### E7-002 Physical-camera-inspired exposure

Tasks:

```text
- add exposure parameters
- add fixed exposure mode for eval
- add optional auto exposure only after fixed mode works
```

Acceptance:

```text
Fixed exposure is deterministic across runs.
```

## Epic E8 — Atmosphere

### E8-001 Atmosphere parameters and LUTs

Tasks:

```text
- define AtmosphereParams
- create transmittance LUT or equivalent
- create sky-view LUT or equivalent
- update only when parameters change
```

Acceptance:

```text
Clear sky renders at sunrise, noon, sunset.
Atmosphere GPU pass timing exists.
```

### E8-002 Atmosphere eval scenes

Crate:

```text
eval_lab
```

Tasks:

```text
- add clear-sky seed cases
- add exposure ramp path
- add reference/human review placeholder
```

Acceptance:

```text
Sky smoke eval runs without clouds.
```

## Epic E9 — Volumetric clouds

### E9-001 Cloud density/weather fields

Tasks:

```text
- define CloudLayer
- generate deterministic weather/coverage fields
- generate density and detail noise
- expose debug AOV
```

Acceptance:

```text
Density debug output changes with seed and remains deterministic for same seed.
```

### E9-002 Cloud raymarch and lighting

Tasks:

```text
- reduced-resolution raymarch
- direct sun lighting
- approximate multiple scattering
- cloud transmittance
- step-count debug output
```

Acceptance:

```text
Sparse, broken, and overcast cases render visibly different clouds.
```

### E9-003 Temporal reprojection and upscale

Tasks:

```text
- deterministic jitter sequence
- history validity tracking
- temporal clamp/rejection
- upscale/composite
```

Acceptance:

```text
Temporal history validity AOV exists.
Slow-pan sequence has no severe smearing in manual review.
```

## Epic E10 — Sky evaluation and certification

### E10-001 Eval CLI

Command:

```bash
cargo run -p tools_cli -- eval sky --profile high --seed-set locked_smoke_001
```

Tasks:

```text
- load seed set
- run headless renders
- collect captures and telemetry
- write pass/fail JSON
```

Acceptance:

```text
Command creates complete artifact bundle.
```

### E10-002 AI visual judge adapter

Crate:

```text
eval_lab
```

Tasks:

```text
- implement VisualEvaluator provider trait
- send images only from artifact bundle
- require structured JSON
- store prompt/model/provider metadata
```

Acceptance:

```text
Invalid JSON fails eval.
Provider failure produces an explicit inconclusive result, not a silent pass.
```

### E10-003 Temporal metrics

Tasks:

```text
- compute frame-time p95/p99
- compute luminance flicker proxy
- compute history rejection statistics
- write temporal report
```

Acceptance:

```text
Temporal report exists for every camera path sequence.
```

### E10-004 Certification gate

Tasks:

```text
- implement certification decision logic
- require complete artifacts
- require human review file for promotion
- reject missing validation log or missing telemetry
```

Acceptance:

```text
locked_cert_001 can produce pass/fail decision.
```

## Post-sky backlog freeze

Do not start these until sky certification:

```text
- polished ceramic sphere
- general PBR material system
- terrain
- fracture
- liquid/gas outside cloud volume
- human body parts
- AI characters
- voice personas
```

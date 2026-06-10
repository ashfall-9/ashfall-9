# 42 — First 90 Days Execution Plan

## Purpose

The full engine vision is too large to start directly. The first 90 days should prove the development loop, not the whole game.

## North-star command

```bash
cargo run -p tools_cli -- eval sky --profile high --seed-set locked_001
```

By the end of this period, that command should produce a valid artifact bundle even if the sky is not yet beautiful.

## Day 0 rule

Do not implement terrain, props, humans, voice, AI characters, fracture, fluids, or general materials during this period. Only build the foundation needed for sun, atmosphere, clouds, capture, telemetry, and evaluation.

## Weeks 1–2 — repository and GPU bootstrap

Deliverables:

```text
- Cargo workspace
- AGENTS.md in repository root
- crates: engine_core, telemetry, gfx_vk, render_graph, renderer_realtime, app_desktop, app_headless, tools_cli
- winit desktop window opens and closes
- Vulkano instance/device/queue creation
- offscreen image allocation
- clear-color present path
- clear-color headless capture path
- basic telemetry JSON
```

Acceptance commands:

```bash
cargo check --workspace
cargo run -p app_desktop
cargo run -p tools_cli -- gpu smoke --headless
```

## Weeks 3–4 — render graph and shader lab

Deliverables:

```text
- first render graph resource model
- graph pass declarations with reads/writes
- GPU timestamp scopes
- validation/debug labels
- shader compilation pipeline
- shader ABI test harness
- PNG capture exporter
- artifact manifest skeleton
```

Acceptance commands:

```bash
cargo run -p tools_cli -- render-graph test
cargo run -p tools_cli -- shader check
cargo run -p tools_cli -- gpu smoke --capture
```

## Weeks 5–6 — HDR, camera, sun, atmosphere foundation

Deliverables:

```text
- physical camera/exposure struct
- HDR render target
- tone map pass
- sun disc pass
- initial atmosphere LUT or simplified sky approximation
- AOV/debug capture slots
- first sky smoke seeds
```

Acceptance command:

```bash
cargo run -p tools_cli -- eval sky --seed-set smoke --profile dev
```

Expected result: not final photorealism; valid deterministic artifacts and telemetry.

## Weeks 7–8 — evaluation harness

Deliverables:

```text
- eval artifact schema validation
- visual evaluator provider trait
- local placeholder evaluator
- optional external AI evaluator adapter behind feature flag
- rubric versioning
- calibration set structure
- performance report schema
- baseline/candidate comparison report
```

Acceptance command:

```bash
cargo run -p tools_cli -- eval sky --seed-set smoke --profile dev --compare baseline
```

## Weeks 9–10 — first volumetric cloud prototype

Deliverables:

```text
- deterministic cloud density/weather field
- low-resolution volumetric raymarch pass
- simple cloud lighting/transmittance
- temporal history buffer allocated but not necessarily perfect
- debug views: density, transmittance, history validity
```

Acceptance command:

```bash
cargo run -p tools_cli -- eval sky --seed-set cloud_smoke --profile dev
```

## Weeks 11–12 — performance and stability pass

Deliverables:

```text
- p50/p95/p99 frame timing
- per-pass GPU timings
- resource lifetime/retirement tests
- resize/swapchain recreation smoke test
- cloud pass quality knobs
- first locked_eval seed set
- first human review template
```

Acceptance command:

```bash
cargo run -p tools_cli -- eval sky --profile high --seed-set locked_001
```

The run may fail visual certification at this stage. It should not fail due to missing infrastructure.

## Roles/workstreams

```text
gfx/backend:
  Vulkan setup, surface, swapchain, resources, timestamps, validation

rendering:
  render graph, shaders, HDR, sun, atmosphere, clouds

eval/tools:
  captures, artifacts, schemas, AI adapter, reports

quality/perf:
  profiling, budgets, hardware matrix, regression gates
```

A solo developer can still follow these as time slices.

## Exit criteria for first 90 days

```text
- repository builds
- desktop and headless smoke paths work
- render graph exists
- shader ABI tests exist
- artifact bundle validates
- sun/atmosphere/cloud prototype renders
- telemetry includes per-pass GPU timings
- visual eval can produce structured JSON
- performance report includes p50/p95/p99
- no other object class has been added
```

## Non-goals during first 90 days

```text
- final cloud beauty
- humans
- terrain
- full PBR material system
- fracture/liquids/gases outside clouds
- AI NPC behavior
- production voice system
- multiplayer/networking
- asset marketplace/import pipeline beyond minimal shader/assets
```

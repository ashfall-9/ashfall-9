# 34 — Runtime Data Flow and Threading Model

## Purpose

This document makes the execution model explicit. Crate boundaries prevent compile-time coupling, but they do not automatically prevent render-thread stalls, hidden locks, provider calls, or uncontrolled mutation. The runtime must separate frame-critical work from slow or nondeterministic work.

## Core rule

```text
The render lane consumes immutable frame snapshots and renderer-owned GPU resources.
It never blocks on AI, voice, material generation, asset import, evaluation, network, or dashboard work.
```

## Runtime lanes

```text
Application lane:
  winit lifecycle, window events, resize, suspend/resume, input forwarding

Simulation lane:
  world state, physics, gameplay, character state, event production

Render lane:
  scene-proxy updates, render graph build, command recording, submit, present/capture

Asset lane:
  import, decode, cache lookup, bake, upload request production

AI/model lane:
  local Burn inference, external LLM calls, planning helpers, visual-eval adapters

Voice/audio lane:
  TTS/realtime streams, audio playback buffers, viseme/lip-sync extraction

Evaluation lane:
  capture export, temporal metrics, AI judging, report normalization

Dev-dashboard lane:
  local UI, telemetry streaming, artifact browsing
```

These lanes may be OS threads, async tasks, or worker pools. The ownership rules stay the same regardless of scheduling implementation.

## Frame flow

```text
1. app_desktop/app_headless produces platform timing and input.
2. engine_core advances the simulation clock.
3. simulation systems emit WorldDelta and EngineEvents.
4. scene_schema publishes immutable FrameSnapshot.
5. asset_pipeline provides ready SceneUpdateBatch entries.
6. renderer_realtime consumes FrameSnapshot + RenderTasks.
7. render_graph compiles resource dependencies and queues.
8. gfx_vk submits Vulkan work and records timeline/fence values.
9. telemetry stores CPU/GPU/memory timings.
10. eval_lab optionally consumes captures and telemetry after rendering.
```

## Snapshot rule

`FrameSnapshot` is immutable after publication. Do not put live mutable world references, mutex guards, or provider objects inside it. Components that need changes emit events or deltas for a later frame.

## Scene proxy update rule

The simulation and asset lanes produce project-owned updates. The renderer maps them into renderer-owned resources.

```text
Simulation/asset side:
  EntityId, GeometryId, MaterialId, Transform, SceneUpdateBatch

Renderer side:
  buffers, images, descriptors, pipelines, acceleration structures, history resources
```

The renderer may keep persistent scene proxies, but all mutation enters through explicit update batches.

## Job pattern

Long work uses submit/poll/cancel.

```rust
pub enum JobStatus<T> {
    Queued,
    Running { progress: Option<f32> },
    Complete(T),
    Failed(JobError),
    Cancelled,
}
```

Use this pattern for material generation, local model inference, remote AI calls, voice synthesis, reference rendering, texture baking, and visual evaluation.

## Backpressure

Every worker lane reports:

```text
- queued job count
- running job count
- average and p95 latency
- max queue age
- failed/cancelled count
- CPU/GPU memory used by worker
- provider/network errors where applicable
- fallback count
```

Frame-critical callers set deadlines and use cached/degraded output when jobs miss deadlines.

## Burn/model isolation

Burn may use WGPU/SPIR-V paths for local model work, but this is not renderer-owned Vulkan interop. Certification runs should record whether model workers shared the same GPU. Renderer FPS certification should run with model workers disabled or isolated unless the milestone explicitly includes those workers.

## Determinism levels

```text
DeterministicExact:
  byte-identical artifacts expected under identical build/hardware/driver

DeterministicStatistical:
  artifacts remain within tolerance; GPU/floating-point variance expected

NondeterministicProvider:
  external AI/voice/model provider behavior can vary; store metadata and calibration
```

Sky rendering should target `DeterministicStatistical`; external AI judging is `NondeterministicProvider` and must be calibrated.

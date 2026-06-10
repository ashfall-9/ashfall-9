# 12 — Performance and Development Environment

## Goal

Make performance and correctness visible from the first Vulkan frame.

## Required tooling

```text
Vulkan validation layers:
  correctness, object lifetime, synchronization, API misuse

RenderDoc:
  frame capture, pass inspection, textures, buffers, pipeline state

NVIDIA Nsight Graphics:
  NVIDIA GPU debugging/profiling, shader and ray-tracing analysis

AMD Radeon GPU Profiler:
  AMD hardware-level GPU profiling

Internal telemetry:
  always-on timing, memory, artifacts, pass stats
```

## Build modes

```text
dev_debug:
  validation layers
  debug names
  shader hot reload
  slow assertions
  detailed telemetry

dev_profile:
  optimized build
  timestamps
  profiler markers
  optional validation
  no artificial debug bottlenecks

eval:
  deterministic seeds
  fixed camera paths
  capture outputs
  no shader hot reload
  strict artifact manifest

release:
  validation disabled
  no debug UI unless enabled
  locked shader cache
  no CPU readbacks except explicit capture
```

## Hardware profiles

Create named profiles:

```yaml
id: desktop_high_001
resolution: [2560, 1440]
target_frame_ms: 16.6
gpu_name: TBD
driver_version: recorded_at_runtime
vulkan_version: recorded_at_runtime
features_required:
  - vulkan_1_3
features_optional:
  - descriptor_indexing
  - ray_query
  - vulkan_1_4
```

Certification cannot happen against unnamed hardware.

## Frame telemetry

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
    pub triangles: u64,
    pub meshlets: u64,
    pub validation_errors: u32,
}
```

## Hot-path rules

```text
- no heap allocation in hot render paths after warmup
- no blocking GPU waits in normal frames
- no synchronous asset loading during frames
- no shader compilation in release/eval
- no unbounded descriptor/pipeline creation
- no AI/voice provider calls on render thread
- no CPU readback except explicit capture/eval
```

## GPU timing rules

```text
- every render graph pass has a timestamp scope
- timestamps are collected asynchronously
- missing GPU timing in eval mode fails the run unless hardware lacks support
- p50/p95/p99 frame times are reported
- warmup frames are excluded but counted
```

## Validation policy

```text
Debug/eval:
  validation errors fail tests unless explicitly waived

Profile:
  validation optional; performance numbers tagged with validation on/off

Release:
  validation off, but release candidates must pass eval validation
```

## Memory policy

```text
- budget transient allocations
- recycle per-frame resources
- track descriptor and pipeline counts
- detect leaks between eval runs
- report high-water marks
```

## CI strategy

CI may not always have GPU access. Use levels:

```text
CI Level 0:
  cargo check, cargo test, clippy, formatting, schema validation

CI Level 1:
  shader compilation, SPIR-V reflection, render graph tests

CI Level 2:
  headless Vulkan smoke test on configured runner

CI Level 3:
  full eval on dedicated hardware
```

## Performance regression policy

```text
- any certified milestone has a locked performance baseline
- regressions over tolerance fail merge
- intentional regressions require explicit budget change and artifact note
- performance data is stored by hardware profile
```


## V3 certification hardware rule

CI smoke tests are useful for build health, validation basics, and artifact generation. They are not sufficient for final photoreal/performance certification unless the CI runner is one of the named hardware profiles.

Each certification report must include:

```text
- GPU model
- driver version
- Vulkan API version
- enabled Vulkan extensions/features
- OS and kernel/build version where relevant
- power/performance mode if configurable
- display mode for presented tests
- offscreen versus presented mode
```

Software Vulkan implementations may be used for smoke tests, but their timings must not be mixed with real GPU performance history.

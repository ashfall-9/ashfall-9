# 39 — Renderer Validation and Testing Plan

## Purpose

The renderer will fail in subtle ways unless validation is designed from the beginning. This document defines the minimum testing pyramid for Vulkan, shader, render-graph, capture, and performance work.

## Testing pyramid

```text
Level 0: Rust compile/style checks
Level 1: pure CPU unit tests
Level 2: shader ABI and reflection tests
Level 3: render-graph resource/hazard tests
Level 4: GPU smoke tests
Level 5: Vulkan validation and synchronization validation runs
Level 6: offscreen capture tests
Level 7: visual/eval regression tests
Level 8: named-hardware performance certification
```

No visual feature should skip directly to Level 7.

## Level 0 — Rust checks

Required for most code changes:

```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

GPU-only crates may need feature-gated test subsets, but failures should be explicit, not silently ignored.

## Level 1 — CPU unit tests

Examples:

```text
- handle generation and stale-handle rejection
- frame snapshot immutability
- event bus ordering
- asset ID hashing
- material parameter clamping
- color-space conversion functions
- sky parameter serialization
- eval schema validation
```

## Level 2 — shader ABI and reflection tests

Every shader-visible struct must have tests for:

```text
- size
- alignment
- field offsets
- explicit padding
- version number
- descriptor bindings
- specialization constants
- push-constant layout
```

The test should compare CPU-side packed structs against shader reflection output or a manually checked golden layout.

## Level 3 — render-graph tests

The render graph must be testable without a real scene.

Required cases:

```text
- read-after-write image transition
- write-after-read hazard
- write-after-write hazard
- transient resource aliasing disabled/enabled
- cross-queue transfer ownership
- missing writer for read resource
- invalid resource lifetime
- duplicate pass name diagnostics
- timestamp-scope creation
```

Debug/eval builds should fail loudly on undefined resource use.

## Level 4 — GPU smoke tests

Minimum GPU smoke tests:

```text
- create instance/device/queue
- create offscreen image
- clear image with compute or graphics pass
- copy image to staging
- write PNG artifact
- record GPU timestamp
- cleanly destroy resources through retirement queue
```

Desktop smoke tests:

```text
- create winit window
- create surface and swapchain
- present clear color
- resize window
- recreate swapchain
- minimize/restore or simulate zero-size handling
- shut down without validation errors
```

## Level 5 — Vulkan validation and synchronization validation

Validation runs are required for:

```text
- backend setup changes
- descriptor changes
- render-graph changes
- swapchain changes
- resource lifetime changes
- raw Vulkan/ash changes
```

Validation logs are artifacts. A validation warning in a certification run is a failed or inconclusive run unless explicitly waived with a decision record.

## Level 6 — offscreen capture tests

Offscreen capture tests must verify:

```text
- deterministic image dimensions
- deterministic metadata
- artifact manifest completeness
- EXR/HDR path where supported
- PNG preview path
- AOV/debug output paths
- no CPU readback on normal gameplay frames
- readback waits only after the recorded timeline/fence completion
```

## Level 7 — visual/eval regression tests

Visual tests should avoid brittle pixel equality except for tiny deterministic technical cases. Use layered checks:

```text
- exact metadata/schema validation
- histogram sanity checks
- luminance range checks
- perceptual/reference comparison where appropriate
- AI artifact tagging
- temporal metrics for sequences
- human review only at promotion gates
```

Visual diffs must store both baseline and candidate artifacts.

## Level 8 — named-hardware performance certification

Performance certification requires named hardware and driver/runtime metadata.

Required report fields:

```text
- CPU model
- GPU model
- driver version
- OS version
- Vulkan API version
- display mode or offscreen mode
- resolution
- quality profile
- build profile
- validation on/off
- p50/p95/p99 CPU frame time
- p50/p95/p99 GPU frame time
- per-pass GPU timings
- VRAM usage
- pipeline/descriptor/resource counts
```

Headless CI timing is not certification timing unless the CI runner is a named hardware profile.

## Test commands to add early

```bash
cargo run -p tools_cli -- gpu smoke --backend vulkano
cargo run -p tools_cli -- gpu smoke --headless
cargo run -p tools_cli -- shader check
cargo run -p tools_cli -- render-graph test
cargo run -p tools_cli -- eval sky --seed-set smoke
cargo run -p tools_cli -- eval sky --seed-set locked_001 --profile high
```

## Failure triage categories

Every failed render/eval run should be tagged:

```text
- build failure
- shader compilation failure
- shader ABI mismatch
- Vulkan validation failure
- synchronization hazard
- GPU device lost
- swapchain/surface lifecycle failure
- missing artifact
- visual regression
- temporal regression
- performance regression
- evaluator/provider failure
- infrastructure failure
```

This keeps automated iteration from treating all failures as “make the image prettier.”

## Merge rule

A change that modifies renderer output must include:

```text
- tests run
- artifacts produced
- validation status
- performance impact
- visual/eval impact
- whether baselines changed
- whether any thresholds changed
```

Threshold changes require a separate evaluation-infrastructure task.

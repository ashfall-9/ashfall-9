# 37 — Revision 4 Sanity Report

## Verdict

The V3 writeup set is directionally sound, but it still needed one more pass before it can be used as a serious engineering control document. V4 focuses on turning the specification from an aspirational architecture into a testable implementation program.

The most important V4 changes are:

```text
- corrected stale cross-file references
- collapsed duplicated renderer lifetime sections
- added a source-verified dependency note
- added explicit shader ABI / GPU data-layout rules
- added a renderer validation and testing plan
- added a statistical promotion protocol for visual and performance evals
- added a practical first-90-days execution plan
- added a stronger risk register with dependency pivot criteria
```

## Current-source sanity checks

These assumptions were rechecked against current public documentation on 2026-06-08 and should be refreshed when implementation begins:

| Area | Current sanity result | Architecture impact |
|---|---|---|
| Vulkano | Current docs describe Vulkano as a safe and rich Rust wrapper around Vulkan and note raw-window-handle compatibility. | Keep Vulkano as the default safe layer, but retain a reviewed raw Vulkan/ash backend escape hatch. |
| winit | Current docs show winit 0.30 using `ApplicationHandler` / `EventLoop::run_app`; docs also advise rendering in response to `WindowEvent::RedrawRequested`, not `AboutToWait`. | `app_desktop` owns the event loop and redraw scheduling; renderer features do not depend on winit. |
| Burn | Current Burn docs list the `vulkan` feature as WGPU backend with an alternative SPIR-V compiler, not direct Vulkan renderer interop. | Burn stays in worker/tool/eval/AI crates; no Burn tensors or waits on the render thread. |
| Vulkan 1.4 | Khronos documents Vulkan 1.4 as consolidating previously optional features and increasing minimum capabilities. | Treat Vulkan 1.4 as a preferred tier, not the first portability baseline. |
| Descriptor indexing | Khronos samples emphasize update-after-bind flexibility, but resource lifetime and synchronization remain application responsibilities. | Descriptor epochs and resource retirement are mandatory before bindless-style general material use. |
| OpenAI Evals | OpenAI has announced the Evals platform deprecation timeline for 2026. | `eval_lab` must own artifacts, schemas, rubrics, calibration records, and promotion decisions. |
| OpenAI voice/audio | Current docs distinguish realtime sessions for low-latency audio and request-based APIs for bounded/generated speech. | Voice runtime must expose both realtime and job/request paths behind provider adapters. |

## Concrete defects fixed in V4

### 1. Bad cross references

V3 had references to files that did not exist:

```text
29_gpu_resource_lifetime_and_abi (missing legacy reference)
22_artifact_and_schema_contracts (missing legacy reference)
```

These are now corrected to:

```text
24_gpu_resource_lifetime_and_sync.md
40_shader_abi_and_gpu_data_layout.md
30_artifact_and_schema_contracts.md
```

### 2. Duplicated renderer lifetime section

`04_vulkan_renderer_spec.md` contained duplicated V3 resource-lifetime/raw-interop sections. V4 replaces them with a single V4 backend boundary section.

### 3. Missing shader ABI policy

The previous docs warned not to put Rust enums in GPU structs, but that warning was scattered. V4 adds a dedicated shader ABI document with concrete rules for:

```text
- CPU authoring data vs runtime descriptors vs GPU-packed data
- `#[repr(C)]` use
- explicit padding
- no `bool`, `usize`, `Option`, `Vec`, strings, references, or rich enums in GPU ABI structs
- reflection checks
- packed-struct golden tests
- descriptor-index indirection
- shader ABI versioning
```

### 4. Evaluation math was too informal

V3 had good eval-rubric structure, but not enough statistical control. V4 adds a promotion protocol for:

```text
- paired baseline/candidate comparisons
- minimum sample counts
- confidence intervals or bootstrap summaries
- p50/p95/p99 frame-time reporting
- repeated run stability
- evaluator drift handling
- separation of dev, locked-eval, and certification datasets
```

### 5. Testing strategy needed a pyramid

V3 listed many gates, but the test levels were not integrated. V4 defines a testing pyramid from Rust compile checks through shader reflection, GPU validation, offscreen rendering, swapchain lifecycle, visual evaluation, and hardware performance certification.

## Important interpretation corrections

### “100% Rust/Vulkan”

Interpret this as:

```text
- engine runtime code is Rust-first
- GPU graphics/compute path is Vulkan-first
- renderer does not depend on DirectX/Metal/OpenGL/WebGPU as its core runtime path
- external operating systems, GPU drivers, tooling, AI providers, voice providers, and file formats remain outside this purity rule
```

It does not mean every tool in the ecosystem is written in Rust, nor that external model APIs or GPU drivers disappear.

### “Photorealistic”

Photorealistic is not a boolean. For this project, it means certified under:

```text
- named scenes
- named camera paths
- named hardware profiles
- named reference sets
- named rubrics
- named evaluator versions
- human promotion review
```

The phrase “perfected” should not be used in gates except as shorthand for `milestone-certified`.

### “Physical consequence engine”

Arbitrary fracture, liquids, gases, tissue, cloth, and interactions are not one system. They are a family of solvers that must share event contracts and LOD rules. The architecture should let a simple approximation emit the same event shape as a future high-fidelity solver.

### “AI image verification”

AI vision judging is useful, but it can drift, overfit, or reward the wrong artifacts. It must never be the only gate. The gate must combine:

```text
- AI visual analysis
- reference comparison
- physical plausibility checks
- temporal stability metrics
- performance measurements
- human signoff for certification
```

## V4 risk ranking

| Risk | Severity | Current mitigation | V4 addition |
|---|---:|---|---|
| Renderer becomes too abstract and slow | High | Render graph, profiles, telemetry | Hot-path testing plan and no-allocation checks. |
| Vulkan lifetime bugs | High | Resource retirement docs | Shader ABI and descriptor-index test requirements. |
| AI eval overfitting | High | Dataset tiers | Statistical promotion protocol and paired comparisons. |
| Dependency mismatch | Medium | Dependency matrix | Source-verified dependency note and pivot criteria. |
| winit/swapchain lifecycle bugs | Medium | Surface lifecycle doc | Explicit redraw and lifecycle test cases. |
| Burn/Vulkan misunderstanding | Medium | Worker boundary | Source-verified dependency note. |
| Voice/persona legal risk later | High | Governance doc | Provider abstraction/offline mode and consent policy retained as precondition. |
| Project scope explosion | High | One object class at a time | First-90-days plan and kill/pivot criteria. |

## V4 package acceptance check

A documentation package passes this pass only if:

```text
- all Markdown cross-file references resolve
- README and manifest describe the current revision
- renderer lifetime and shader ABI policies are centralized
- dependency assumptions are source-verified and dated
- eval promotion does not rely on a single AI score
- first milestone remains sun + atmosphere + clouds only
- ZIP integrity passes
```


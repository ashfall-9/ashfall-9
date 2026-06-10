# 00 — Sanity Check Report

## Verdict

The direction is technically coherent if treated as a **multi-year, milestone-gated R&D program**. The architecture should not promise a finished photorealistic world simulation up front. It should produce one certified object class at a time, starting with the sky renderer.

The original writeups were strongest in these areas:

```text
- component isolation through crates and interfaces
- artifact-driven evaluation
- first milestone discipline: sun + atmosphere + volumetric clouds only
- separation of renderer, physics, AI, voice, materials, humans, and evaluation
- performance and photorealism as measurable gates
```

The original writeups needed improvement in these areas:

```text
- Vulkan/Rust scope needed clearer boundaries
- Vulkano should have an explicit escape hatch for unsupported Vulkan features
- Burn should not be assumed to provide direct Vulkan renderer interop
- AI image verification needed stronger guardrails
- "perfected" needed to become a measurable milestone state
- physics/human generation needed more staged feasibility constraints
- OpenUSD/MaterialX should be positioned as interchange, not early runtime dependencies
- development workflow needed coding-agent rules and automated regression gates
```

## Corrected assumptions

### 1. “100% Vulkan and Rust” needs a practical definition

Use this definition:

```text
The game runtime, renderer, GPU simulation, asset runtime, and core engine are Rust-first and Vulkan-first.
```

Allow these controlled exceptions:

```text
- operating-system windowing through winit
- external AI/voice APIs behind provider traits
- optional dev dashboards over HTTP/WebSocket
- offline reference datasets and captured photography
- build tools and profilers outside the game binary
```

Do **not** allow provider-specific APIs to leak into core engine contracts.

### 2. Vulkano is a good starting layer, not the whole graphics strategy

Use Vulkano for safer device setup, swapchain management, buffers, images, descriptors, pipelines, command buffers, and validation-friendly development. Keep an explicit `gfx_vk::raw` module for Vulkan features or performance paths that need direct access through `ash` or Vulkano’s raw device functions.

Policy:

```text
- safe Vulkano path by default
- raw Vulkan path only inside gfx_vk or renderer internals
- no raw Vulkan handles across component interfaces
- every raw path requires a safety comment, validation coverage, and RenderDoc/Nsight/RGP capture coverage
```

### 3. Burn should be used for AI/model tooling, not renderer integration

Burn is useful for Rust-native tensor/model work, local evaluators, small perception models, ranking models, and offline tools. Do not design the renderer around sharing Vulkan memory directly with Burn. Treat Burn as an AI/tooling dependency behind async worker interfaces.

Use Burn where it fits:

```text
- local image artifact classifiers
- small agent state models
- material scoring helpers
- perception or animation helper models
- offline training/inference experiments
```

Do not assume:

```text
- zero-copy renderer tensor interop
- direct Vulkan command-buffer integration
- frame-critical AI inference on the render thread
```

### 4. `gweb` is unresolved

The exact Rust crate name `gweb` should not be hard-coded. Keep the dev dashboard behind:

```rust
pub trait DevUiServer { /* ... */ }
```

Then choose a concrete crate later after a small spike. Candidate crates may include Actix Web, Axum, Warp, or another framework, but the writeups should not depend on one until it is deliberately selected.

### 5. “Perfected” must mean “milestone-certified”

Replace this:

```text
This component is perfected.
```

With this:

```text
This component is milestone-certified under named test scenes, hardware profiles, image-evaluation rubrics, temporal stability checks, and performance budgets.
```

A certified component can still be improved later, but it becomes protected from regression.

### 6. AI visual judgment cannot be the only judge

AI image recognition is useful, but not sufficient. It can be inconsistent, overvalue superficial beauty, miss temporal artifacts, and reward plausible-looking but physically wrong output.

Use a panel:

```text
- AI image realism judge
- reference-image comparison
- artifact detector
- temporal flicker/ghosting checker
- physical plausibility checks
- GPU/CPU telemetry gates
- periodic human calibration review
```

### 7. Physical consequence must be staged by material class

“Breaking stuff into arbitrary pieces based on forces” is a research area, not a first implementation target. The engine should support the interface from day one but implement one material class at a time.

Staging:

```text
1. no physics in sky milestone
2. rigid ceramic sphere/block
3. ceramic fracture under impact
4. wood fracture/splintering
5. metal denting/plastic deformation
6. single liquid droplet/puddle
7. gas/smoke volume
8. coupled fracture + liquid/gas
```

### 8. Humans are not an early renderer feature

Photoreal humans combine skin optics, eyes, hair, anatomy, animation, expression, cloth, gaze, voice, and behavior. The human generator should be decomposed into testable body-part milestones.

Staging:

```text
1. skin material patch
2. eye close-up
3. hair lock
4. mouth/teeth/tongue close-up
5. face neutral pose
6. face expression loop
7. head with hair and eyes
8. torso/hands
9. full human near-camera
10. AI-driven speaking human
```

### 9. OpenUSD and MaterialX should not block the first milestone

Use glTF-style PBR as the first runtime material baseline. Add MaterialX subset support after material graphs exist. Add OpenUSD after scene composition and production interchange matter.

Initial scope:

```text
- internal runtime scene schema
- glTF import/export subset
- generated material cache
```

Later scope:

```text
- MaterialX graph subset
- OpenUSD export/import bridge
- Hydra-like diagnostic views if useful
```


## Second sanity pass — V2 hardening notes

This pass focused on issues that would hurt implementation rather than project vision.

### A. Separate CPU descriptors from GPU-packed structs

The earlier API sketch used `#[repr(C)] RuntimeMaterial` while still containing Rust-side concepts such as `TextureOrProcedural`. That is unsafe as a GPU ABI contract because Rust enums and rich handles are not automatically stable, POD-compatible shader data. V2 separates:

```text
MaterialAuthoringDesc:
  human/tool-friendly material intent, provenance, procedural graphs, source photos

RuntimeMaterialDesc:
  renderer-friendly but still CPU-side resolved material description

GpuMaterialPacked:
  tightly packed shader ABI with integer resource indices and explicit padding
```

Only `GpuMaterialPacked` should be copied directly to GPU buffers.

### B. Keep the render graph hot path allocation-aware

A boxed pass recorder is useful for object-safe prototypes, but a mature renderer should not allocate pass objects every frame. V2 clarifies that the public design may expose ergonomic builders while the renderer implementation should use arenas, stable pass IDs, reusable graph storage, or static feature-owned pass recorders.

### C. Treat descriptor indexing as a lifetime/synchronization feature, not just a bindless feature

Descriptor indexing and update-after-bind can reduce descriptor churn, but they also move complexity into synchronization and resource lifetime rules. V2 adds explicit rules for descriptor epochs, pending-frame fences/timeline values, and safe update points.

### D. Do not make numeric AI scores universal

An AI realism score such as `8.8/10` is meaningful only under a locked rubric, locked prompt, locked evaluator model, locked reference set, and calibration images. V2 changes the sky milestone language to use locked thresholds and percentile/report-based promotion rather than implying a universal photorealism number.

### E. Atmosphere/cloud simulation should be honest about physics scope

The first cloud system should be physically inspired, not a full meteorological solver. It should preserve plausible radiance, transmittance, scale, lighting, and temporal behavior, while treating weather maps and cloud layers as controllable artistic/physical approximations. Full fluid weather simulation is out of scope for the sky milestone.

### F. Long-running tools should use job handles

Material generation, AI evaluation, voice synthesis, and offline reference rendering should not be synchronous APIs in the runtime. V2 adds job-handle patterns for long work so the engine can schedule, cancel, cache, and isolate provider calls.

### G. OpenAI Evals should not be a dependency

The docs already recommended an internal `eval_lab`, and V2 makes this stronger. OpenAI has announced the Evals platform deprecation with read-only and shutdown dates in 2026, so this project should store its own artifacts and use model providers only through replaceable evaluator adapters.

## V2 corrected implementation risks

| Issue found in previous improved set | Severity | V2 change |
|---|---:|---|
| GPU material sketch mixed Rust enums with `repr(C)` | High | Added CPU/runtime/GPU material split and packed ABI rules. |
| Render-graph boxed pass sketch could imply per-frame allocation | Medium | Added arena/static-pass guidance and hot-path allocation rule. |
| Descriptor indexing was described mainly as scale feature | Medium | Added lifetime, pending-frame, and update-epoch constraints. |
| AI scores could be read as absolute truth | High | Added calibration, model drift, threshold versioning, and human promotion checks. |
| Cloud milestone might imply full weather simulation | Medium | Clarified physically inspired rendering vs meteorological simulation. |
| Synchronous generator APIs encouraged blocking architecture | Medium | Added async job/worker contracts for heavy work. |
| Eval infrastructure could accidentally depend on deprecated vendor product | High | Explicitly require internal artifact/eval storage. |

## Risk register

| Risk | Severity | Likelihood | Mitigation |
|---|---:|---:|---|
| Scope explosion | Critical | High | One object class at a time; milestone gates; no parallel feature creep. |
| Renderer hidden behind high-level wrapper limitations | High | Medium | Vulkano-first plus raw Vulkan escape hatch. |
| AI judge rewards attractive but physically wrong images | High | High | Multi-signal scoring and human calibration. |
| Clouds look good in stills but flicker in motion | High | High | Temporal test paths and p95/p99 stability metrics. |
| Physics promises exceed implementation | Critical | High | Material-by-material solver staging and LOD event contracts. |
| Humans become uncanny and too expensive | Critical | High | Body-part milestones, LOD budgets, and explicit acceptance gates. |
| External AI/voice providers leak into core runtime | High | Medium | Provider traits, event contracts, caching, offline fallback. |
| Unsafe Vulkan code spreads through project | High | Medium | Restrict unsafe/raw Vulkan to `gfx_vk` and reviewed renderer internals. |
| Shader iteration is slow | Medium | High | Shader hot reload, pipeline cache, minimized variants, shader tests. |
| Artifact results are not reproducible | High | Medium | Fixed seeds, fixed prompts, pinned models/providers, manifests, versioned rubrics. |

## Revised immediate target

The first deliverable should be a headless-or-windowed sky evaluation runner:

```bash
cargo run -p tools_cli -- eval sky --profile high --seed-set locked_001
```

It should output:

```text
- PNG screenshot for quick review
- EXR/HDR capture for analysis
- AOV captures: depth, luminance, transmittance, cloud density debug, motion
- GPU pass timing CSV
- CPU timing CSV
- memory telemetry
- Vulkan validation summary
- AI visual-evaluation JSON
- temporal-stability report
- final pass/fail report
```

Do not start terrain, material objects, humans, AI characters, voice, or fracture until this pipeline works end-to-end.


## Third sanity pass — V3 hardening notes

V3 treats the previous documents as architecturally sound but not yet strict enough for implementation. The third pass adds controls around the places where a real Vulkan/Rust renderer is most likely to fail.

### A. Runtime data flow must be lane-based

The previous interface sketches separated crates, but crate boundaries alone do not prevent frame stalls. V3 adds a runtime-lane model:

```text
application lane
simulation lane
render lane
asset lane
AI/model lane
voice/audio lane
evaluation lane
dev-dashboard lane
```

The render lane consumes immutable snapshots and renderer-owned scene proxies. It never blocks on material generation, Burn inference, voice synthesis, provider APIs, asset import, or AI evaluation.

### B. GPU resource lifetime needs explicit accounting

High-level handles are not enough. V3 adds generational GPU resource IDs, resource lifecycle states, descriptor epochs, timeline/fence-based retirement, and explicit readback/capture rules. This prevents common Vulkan bugs such as destroying textures still referenced by pending command buffers or updating descriptor-indexed resources without proving pending-frame safety.

### C. Shader ABI must be tested, not trusted

V2 introduced the authoring/runtime/GPU material split. V3 makes this operational: shader-facing structs must be POD-style packed data, with explicit padding, ABI version fields, generated offset tests, and shader reflection checks.

### D. Raw Vulkan and Vulkano interop needs an ownership boundary

A raw Vulkan escape hatch is still required, but V3 clarifies that wrapper-created resources must not be mutated or destroyed through raw calls unless the wrapper owner explicitly owns that path. Raw paths require valid-usage notes, synchronization notes, destruction-order notes, validation coverage, and a fallback or capability gate.

### E. Evaluation datasets can be overfit

AI visual judging can be gamed accidentally by repeatedly tuning against the same seeds and prompt. V3 adds smoke, dev, locked-eval, locked-certification, and calibration datasets. The certification set should not be used as a constant iteration target.

### F. Certification needs a decision record

A pass/fail result should be stored as a decision record that names the git revision, dataset IDs, hardware profiles, prompt/rubric versions, evaluator configuration, validation status, performance status, human review, and certification state.

### G. Rights, consent, and provenance must start before humans and voice

Sky rendering is low-risk, but the project will later handle photos, human generation, skin references, voices, and AI-generated assets. V3 adds data-governance rules now so the process is ready before high-risk assets appear.

### H. Dependency and supply-chain changes must be separate tasks

Adding or upgrading dependencies should not be hidden inside renderer fixes. Dependency changes require their own review, version recording, cargo checks, security/audit review where practical, and eval-baseline reruns.

## V3 corrected implementation risks

| Issue | Severity | V3 change |
|---|---:|---|
| Render thread can stall on workers/providers | High | Added lane-based runtime data-flow and job/deadline rules. |
| GPU resources can be destroyed while still referenced | High | Added generational handles, retirement queue, descriptor epochs, and submission tracking. |
| Raw Vulkan escape hatch can violate wrapper ownership | High | Added raw interop ownership and review requirements. |
| Shader ABI drift can silently corrupt rendering | High | Added POD-packed ABI rule, explicit padding, ABI versions, and reflection tests. |
| AI/eval loop can overfit to fixed seeds or prompts | High | Added dataset tiers, calibration, anti-overfitting rules, and decision records. |
| Reference photos/materials/humans/voices can lack rights | High | Added provenance, consent, and data-governance documents. |
| Dependency upgrades can invalidate baselines | Medium | Added dependency matrix, feature flags, and upgrade checklist. |
| Artifact format can drift without migration | Medium | Added artifact/schema contracts and schema-versioning rules. |

## Revised V3 immediate target

The first target is still deliberately narrow:

```bash
cargo run -p tools_cli -- eval sky --profile high --seed-set locked_001
```

For V3, this command should eventually emit or validate:

```text
- sky captures and HDR/debug AOVs
- GPU resource and descriptor epoch telemetry
- Vulkan validation and synchronization-validation logs
- artifact manifest with schema versions
- visual judge JSON with calibration result
- temporal metrics
- performance gate result
- decision record
```


## Third sanity pass — V3 hardening notes

V3 does not change the project direction. It adds implementation constraints that prevent common engine-startup failures.

### H. Headless evaluation is offscreen rendering, not guaranteed CI performance

`app_headless` should avoid a visible window and swapchain, but it still needs a Vulkan-capable device. CI can run smoke tests with a software Vulkan implementation or limited runner, but milestone performance certification must happen on named hardware and driver versions.

### I. Window, surface, and swapchain lifecycle must be first-class

The desktop app owns winit and the event loop. `gfx_vk` owns Vulkan surfaces and swapchains. Renderer features should never cache swapchain images across frames; imported swapchain images are one-frame graph resources. Resize, minimize, suspend, surface-lost, and device-lost states are normal lifecycle states.

### J. Raw Vulkan is backend-internal only

The raw escape hatch remains necessary, but V3 narrows it: raw Vulkan/ash usage should live behind `gfx_vk` backend modules. Renderer features may request capabilities and command abstractions, not random raw handles.

### K. Descriptor buffer is not the first scalable binding path

Descriptor indexing is the first scalable path. Descriptor buffer is an optional backend experiment with fallback, separate validation, and dedicated rollback notes.

### L. Long-running jobs need budget metadata

Material generation, visual eval, voice synthesis, offline oracle rendering, and external AI planning jobs need priority, cancellation, provider identity, cache policy, retry policy, and artifact/provenance output.


## Revision 4 sanity-check addendum

V4 keeps the V3 architecture but fixes implementation-readiness gaps:

```text
- stale Markdown references were corrected
- duplicated renderer backend/lifetime sections were collapsed
- a source-verified dependency note was added
- shader ABI and GPU data layout rules were centralized
- renderer validation/testing was turned into a test pyramid
- eval promotion now includes statistical and paired-comparison rules
- the first 90 days are scoped to proving the sky render/eval loop
- risk and pivot criteria were added for dependencies, evals, and scope creep
```

The first milestone remains sun + atmosphere + volumetric clouds only.

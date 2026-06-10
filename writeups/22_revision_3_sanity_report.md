# 22 — Revision 3 Sanity Report

## Purpose

This third pass does not change the project vision. It hardens the parts that would most likely break during real implementation:

```text
- Vulkan/Vulkano/raw-handle ownership
- shader ABI stability
- GPU resource lifetimes
- descriptor indexing safety
- strict provider boundaries for AI, voice, and evaluation
- evaluator drift and benchmark reproducibility
- first-milestone task decomposition
```

The earlier V2 set was already directionally correct. The main V3 correction is that a high-level architecture alone is not enough for a Vulkan project with AI-assisted iteration. The project needs explicit ownership rules, deletion rules, ABI rules, and review gates before Codex or any coding agent is allowed to produce substantial implementation changes.

## Sanity-check conclusion

The project is feasible as a multi-year R&D program if the first deliverable remains narrow:

```bash
cargo run -p tools_cli -- eval sky --profile high --seed-set locked_001
```

The project is not feasible if the team attempts to build clouds, humans, fracture, fluids, gases, voice, agentic AI, and material generation in parallel before the render/eval/profiling loop is stable.

## V3 corrections and improvements

### 1. “100% Rust/Vulkan” must be interpreted as an engine-core rule

The real-time renderer, runtime simulation code, asset schemas, tooling, and engine orchestration should be Rust-first and Vulkan-first. However, the project also explicitly wants Codex, ChatGPT Pro, AI-based image verification, AI characters, and voice personas. Those services are not Vulkan subsystems and may involve external APIs.

V3 therefore defines this boundary:

```text
Engine core:
  Rust + Vulkan/Vulkano/raw Vulkan where needed

Provider adapters:
  replaceable external boundaries for AI vision, LLMs, voice, cloud tools, and dashboards

Artifacts:
  provider-neutral JSON, images, audio, telemetry, and manifests stored by eval_lab
```

No external AI, voice, or dashboard SDK type may appear in `engine_core`, `scene_schema`, `gfx_vk`, `render_graph`, or the hot renderer path.

### 2. Vulkano and raw Vulkan must not mix ownership casually

Vulkano remains useful for safe Rust Vulkan ergonomics, but raw Vulkan/ash escape hatches create ownership hazards. V3 adds an explicit ownership policy:

```text
- Vulkano-created objects stay owned by Vulkano wrappers.
- Raw Vulkan-created objects stay owned by gfx_vk RAII wrappers.
- Cross-boundary raw handles are borrowed, not destroyed by the borrower.
- Raw-handle borrowing requires a lifetime comment and validation capture.
- No raw handle may cross into scene, AI, voice, material, or physics schemas.
```

### 3. Shader ABI must be a first-class contract

V2 separated CPU material descriptors from GPU-packed data. V3 generalizes this into a full shader ABI rule:

```text
- every shader-visible struct has a versioned ABI ID
- every shader-visible struct is plain data only
- no Rust enum, Vec, String, Option, trait object, reference, or pointer appears in GPU ABI structs
- booleans are u32 flags unless a backend-specific layout is formally validated
- host-side structs and shader-side structs are validated by reflection or compile-time layout tests
- ABI changes require a manifest bump and shader test update
```

This is essential because visual bugs from mismatched GPU layouts can look like lighting, material, or cloud problems, wasting AI-iteration cycles.

### 4. Burn is useful, but it is not the renderer backend

Burn can be used for local AI/model tooling, evaluator helpers, ranking, image-feature extraction, and offline model experiments. The Burn Vulkan feature is exposed through the WGPU/SPIR-V path, so do not design the Vulkan renderer around sharing a single device, queue, descriptor heap, or image memory with Burn.

Recommended V3 policy:

```text
- Burn work runs in ai_worker, eval_worker, or tools_cli.
- Burn never executes on the render thread.
- Burn output crosses into the engine as files, tensors serialized to owned buffers, or provider-neutral JSON.
- If WGPU is used, force the intended backend in worker configuration and record it in the artifact.
- No Burn tensor owns renderer images or descriptors.
```

### 5. AI image judging must be calibration-based, not score-worship

A numeric realism score is only valid inside a locked rubric. V3 requires:

```text
- locked evaluator model identifier
- locked prompt/rubric version
- locked reference set
- locked negative/challenge set
- human calibration panel for milestone promotion
- drift test before changing model/provider/prompt
- no auto-merge solely from AI visual score
```

The AI judge should be treated as a defect detector and consistency assistant, not as the final authority on photorealism.

### 6. Clouds are the right first milestone, but the milestone needs smaller gates

The sky milestone is still correct because it avoids geometry, materials, animation, humans, terrain, and gameplay. However, “clouds and sun” is still too large as a single task. V3 splits it into gated slices:

```text
S0 clear color + capture + timestamps
S1 physical camera + HDR sun disc
S2 analytic/precomputed atmosphere
S3 static single-layer cloud density
S4 lit volumetric cloud raymarch
S5 temporal reprojection and upscale
S6 weather/coverage variants
S7 locked sky eval certification
```

Each slice has a visual, performance, and artifact gate.

### 7. Performance needs p95/p99 and stability, not just average FPS

Frame averages are easy to game. V3 requires:

```text
- p50, p95, p99 CPU frame time
- p50, p95, p99 GPU frame time
- per-pass p95 GPU time
- 1% low equivalent for interactive preview
- VRAM peak and steady-state plateau
- shader compilation/cache miss count
- descriptor/pipeline/resource count growth over time
```

### 8. Provider and data safety must be designed now

The project may use photography material, human references, voice personas, and AI evaluation. That creates provenance and privacy issues. V3 adds a provider-boundary specification:

```text
- no human likeness/voice generation without provenance metadata
- no sending private captures or reference photos to external providers unless the asset manifest allows it
- model/provider outputs are cached with provenance and terms-of-use metadata
- all external calls are reproducible at the prompt/request level, but not assumed deterministic
- runtime has offline fallbacks for provider failure
```

## Revised risk register

| Risk | Severity | Likelihood | V3 mitigation |
|---|---:|---:|---|
| Raw Vulkan and Vulkano ownership conflict | High | Medium | Single `gfx_vk` ownership policy, RAII wrappers, no raw handles in schemas. |
| Shader ABI mismatch causes subtle visual defects | High | High | Versioned ABI IDs, layout tests, reflection manifests, packed structs only. |
| AI judge optimizes toward pretty but physically wrong images | High | High | Multi-signal eval, locked negative set, human calibration. |
| Descriptor-indexed resources are destroyed while still in flight | High | Medium | Descriptor epochs, timeline retire queues, per-frame resource references. |
| Burn/WGPU accidentally creates hidden non-Vulkan dependency in runtime | Medium | Medium | AI workers only, backend recorded, no renderer interop assumption. |
| First milestone remains too broad | Medium | High | S0-S7 gated sky backlog. |
| External provider changes break eval comparability | Medium | High | Provider-neutral artifacts, drift tests, pinned model/rubric versions. |
| Humans/voice raise identity and consent problems | High | Medium | Consent/provenance metadata and external-call allowlists. |
| Average FPS hides stutter | High | High | p95/p99 frame-time budgets and long soak tests. |
| Codex changes rubrics to make tests pass | High | Medium | AGENTS.md forbids threshold edits in feature tasks; rubric changes are separate review tasks. |

## Recommendation

The next actual code task should be no bigger than this:

```text
Create the Rust workspace skeleton, AGENTS.md, tools_cli placeholder, telemetry crate, and a no-GPU eval artifact writer.
```

Then add Vulkan initialization, then winit, then swapchain, then capture, then timestamps. Do not start clouds until the artifact and telemetry loop exists.

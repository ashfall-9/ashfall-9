# AGENTS.md — Project Instructions for Coding Agents

## Project identity

This repository is a Rust-first, Vulkan-first photorealistic game-engine R&D project. The current milestone is **sun + atmosphere + volumetric clouds only**.

## Core rules

- Preserve crate boundaries.
- Edit only files explicitly allowed by the task.
- Do not introduce new dependencies unless the task asks for them or you document why they are necessary.
- Do not change public interfaces without updating interface versions and documentation.
- Do not relax tests, rubrics, or performance thresholds to make a task pass.
- Do not add terrain, objects, humans, physics, AI characters, or voice to the sky milestone.
- Do not call external AI/voice APIs from the renderer or render thread.
- Do not block the render thread on asset loading, model inference, or voice synthesis.

## Unsafe code policy

Most crates should use:

```rust
#![forbid(unsafe_code)]
```

Unsafe/raw Vulkan code is allowed only in `gfx_vk` and selected renderer internals. Every unsafe block must explain:

- why the safe path was insufficient
- Vulkan valid-usage assumptions
- ownership/lifetime assumptions
- validation or capture coverage

## Required checks

Prefer these checks after changes:

```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

For renderer/shader work, also run or update the relevant shader/eval command:

```bash
cargo run -p tools_cli -- shader check
cargo run -p tools_cli -- eval sky --profile high --seed-set locked_001
```

If a command cannot run in the current environment, report that honestly and explain what was run instead.

## Documentation rule

If a change modifies an interface, artifact schema, evaluation rubric, or milestone gate, update the relevant Markdown writeup and manifest/schema file.

## Current milestone boundaries

Allowed visual content:

```text
- camera
- sun
- atmosphere
- volumetric clouds
```

Forbidden until sky certification:

```text
- ground
- terrain
- props
- buildings
- characters
- humans
- materials beyond sky/cloud internals
- gameplay
- fracture/liquids/gases outside cloud volume implementation
```

## V2 hardening rules

- Do not place Rust enums, strings, `Vec`, trait objects, or provider-specific objects into GPU-facing `#[repr(C)]` structs.
- Separate authoring data, runtime CPU descriptors, and packed GPU ABI structs.
- Do not allocate render-graph pass objects every frame in production paths; use reusable storage after warmup.
- Do not update descriptor-indexed resources without proving pending-frame lifetime safety.
- Do not change AI/eval prompts, rubrics, score weights, or thresholds in the same task as a renderer fix.
- Long-running material, AI, voice, and eval work must use job handles or worker queues; never block the render thread.


## V3 hardening rules

- Do not change GPU-facing `#[repr(C)]` structs without updating shader ABI tests, reflection checks, and documentation.
- Do not introduce raw Vulkan or `ash` calls outside `gfx_vk` or approved renderer internals.
- Every raw Vulkan path must document valid-usage assumptions, ownership/lifetime assumptions, synchronization assumptions, validation coverage, and fallback behavior.
- Do not destroy or recycle renderer resources without using the resource-retirement/timeline-fence policy.
- Do not change descriptor-indexed resource update behavior without updating descriptor epoch tests or docs.
- Do not block the render lane on AI/model, voice, material generation, asset import, evaluation, network, or dashboard work.
- Do not tune renderer changes against the certification dataset as a normal iteration loop.
- Do not modify prompts, rubrics, thresholds, seed sets, and renderer code in the same task unless the task is explicitly an evaluation-infrastructure change.
- Do not add or upgrade dependencies inside unrelated feature work. Use a dependency-specific task and record the reason.
- Do not add reference images, human data, material photos, or voice samples without provenance/rights metadata.
- Do not store secrets, provider API keys, auth headers, or private datasets in source files, examples, eval reports, or artifacts.
- For any task that touches `eval_lab`, preserve old artifact readability or provide an explicit schema migration note.


## V3 additional agent rules

- Do not expose winit types outside `app_desktop` boundary code.
- Do not expose raw Vulkan/ash/Vulkano handles outside `gfx_vk` or renderer backend internals.
- Do not retain swapchain images across frames; imported swapchain images are frame-graph resources only.
- Do not implement descriptor indexing without descriptor epoch and resource retirement tests.
- Do not use headless CI timing as certification timing unless the CI runner is a named hardware profile.
- Do not change dependency versions without updating the relevant dependency decision record.
- Do not mix evaluator prompt/rubric/model changes with renderer-output changes in one task.


## V4 hardening rules

- Do not leave Markdown cross-file references stale. Run or update a documentation-link check when adding/removing docs.
- Do not modify GPU-packed structs without updating `40_shader_abi_and_gpu_data_layout.md`, shader reflection tests, and ABI version notes.
- Do not promote visual changes from a single screenshot or single AI score. Use the statistical promotion protocol.
- Do not cite certification performance from validation-enabled runs unless the report clearly labels them as correctness runs, not performance runs.
- Do not add dashboard/web work before the artifact bundle and CLI eval path work.
- Do not choose or upgrade the dev web framework as part of renderer work; use a dependency spike.
- Do not introduce a new object class in the first 90-day plan.
- When creating new docs, update `README.md`, `manifest.json`, and the reading order if appropriate.

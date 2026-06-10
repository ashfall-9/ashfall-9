# AGENTS.md

## Project Identity

This repository is a Rust-first, Vulkan-first photorealistic game-engine R&D project.
The active milestone is sky rendering only:

- camera
- sun
- atmosphere
- volumetric clouds

Do not add terrain, props, buildings, gameplay, humans, AI characters, voice, material labs, or physics content before sky certification.

## Coding Rules

- Preserve crate boundaries from `writeups/02_workspace_architecture.md`.
- Keep `winit` inside `app_desktop`.
- Keep Vulkano/Vulkan implementation details inside `gfx_vk` and renderer backend internals.
- Do not expose raw Vulkan handles, Vulkano types, or winit types through `engine_core`, `scene_schema`, or public renderer contracts.
- Most crates use `#![forbid(unsafe_code)]`.
- Raw Vulkan or unsafe code must stay in approved graphics internals and include the review notes from the writeups.
- Long-running work must use submit/poll/cancel or worker queues; never block the render lane on AI, voice, asset import, eval, or network work.

## Checks

Prefer these checks after changes:

```powershell
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Renderer/shader/eval work should also keep the placeholder commands working until their real implementations replace them:

```powershell
cargo run -p tools_cli -- shader check
cargo run -p app_headless -- --seed 1 --out target/eval_smoke
```

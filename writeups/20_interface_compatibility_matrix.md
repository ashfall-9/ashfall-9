# 20 — Interface Compatibility Matrix

## Purpose

This matrix prevents accidental coupling between separately developed components.

## Allowed dependencies

| Consumer | May depend on | Must not depend on |
|---|---|---|
| `engine_core` | none of the feature crates | Vulkan, winit, AI providers, voice providers, web frameworks |
| `scene_schema` | `engine_core` handle types | renderer internals, physics solver internals, provider SDKs |
| `telemetry` | `engine_core` IDs and simple serialization | renderer feature internals, AI provider objects |
| `gfx_vk` | Vulkan/Vulkano, platform surface adapters | AI, voice, material generation, humans, physics logic |
| `render_graph` | `gfx_vk`, telemetry labels | scene authoring, AI, voice, gameplay logic |
| `renderer_realtime` | scene snapshots, render graph, gfx_vk, asset runtime | physics solver internals, AI provider SDKs, voice SDKs |
| `renderer_oracle` | scene snapshots, renderer-independent material/geometry schema | gameplay runtime assumptions |
| `physics_core` | scene/physical material contracts | renderer GPU internals |
| `physics_gpu` | `physics_core`, `gfx_vk` compute resources | renderer private pass state |
| `material_lab` | scene/material schema, asset pipeline, eval adapters | render thread, swapchain, voice/AI character internals |
| `human_gen` | material/schema/animation contracts | renderer private GPU handles, voice provider objects |
| `ai_world` | world snapshots, event bus, provider traits | renderer private state, raw audio provider objects |
| `voice_runtime` | voice persona schema, audio stream handles | AI private memory, renderer private state |
| `eval_lab` | captures, telemetry, provider adapters | ability to mutate renderer/eval thresholds during a renderer fix |
| `dev_web` | telemetry and artifact readers | engine-core ownership of runtime state |
| `tools_cli` | public crate APIs | private module internals |

## Interface style rules

```text
- Cross-crate data uses handles, snapshots, descriptors, and events.
- No public interface exposes raw Vulkan handles.
- No public interface exposes provider SDK objects.
- Long-running work uses job IDs or streams.
- GPU ABI structs are renderer-owned packed data, not general scene schema.
- Eval artifacts store interface versions and schema versions.
```

## Common coupling mistakes to reject

```text
- physics directly spawning render meshes instead of emitting fracture events
- AI reading renderer visibility buffers directly instead of using perception frames
- material generator writing descriptor sets directly
- voice runtime mutating AI memory directly
- eval lab changing score thresholds to pass a renderer patch
- app_desktop owning renderer internals because it owns winit
- using GPU material structs as editor/material-authoring structs
```

## Versioning requirements

A public interface change must include:

```text
- version bump
- changelog entry
- migration note
- updated API contract sketch
- updated artifact schema if eval output changes
- regression test or schema validation change
```

# 38 — Source-Verified Dependency Notes

## Purpose

This document records dependency assumptions that were checked against current public documentation. It is not a lockfile. It is a decision aid and a reminder that implementation must refresh versions before coding begins.

## Verification date

```text
Checked: 2026-06-08
Package revision: V4
```

## Current dependency snapshot

| Dependency / platform | Checked status | V4 policy |
|---|---|---|
| Vulkano | Current docs describe `vulkano` 0.35.2 as a safe and rich Rust wrapper around Vulkan. The docs also note raw-window-handle compatibility and currently mention `rwh_06`. | Use as the safe default Vulkan wrapper. Keep raw/ash escape hatches only behind `gfx_vk` backend modules. |
| winit | Current docs describe `winit` 0.30.13 as cross-platform window creation/event-loop management. Event handling uses `EventLoop::run_app` and `ApplicationHandler`. | `app_desktop` owns the event loop. Create windows in the lifecycle handler. Render in response to `RedrawRequested`. |
| Burn | Current docs describe Burn 0.21.0 as a Rust deep-learning/tensor framework. The `vulkan` feature makes the WGPU backend available with an alternative SPIR-V compiler path. | Use Burn for local model/eval/tool workers. Do not design direct Vulkan memory interop with the renderer. |
| Vulkan | Khronos current spec line is Vulkan 1.4. Vulkan 1.4 consolidates features such as dynamic rendering local read and scalar block layout. | Baseline Vulkan 1.3 for first implementation. Treat Vulkan 1.4 as a preferred advanced tier after hardware verification. |
| Descriptor indexing | Khronos samples describe update-after-bind as flexible, but descriptors can be updated only when they are not actually accessed by the GPU. | Descriptor epochs, resource-retirement queues, and per-submission usage tracking are mandatory before broad use. |
| Descriptor buffer | Khronos docs describe descriptor buffers as a more direct descriptor-management model. | Optional backend experiment. Not a milestone-1 requirement. Do not use it to avoid lifetime tracking. |
| OpenAI Realtime/audio | Current docs distinguish realtime sessions for live low-latency audio and request-based APIs for files/bounded/generated speech. | Voice runtime exposes both streaming and job/request paths through provider traits. |
| OpenAI Responses/vision | Current docs support image inputs and tool/function workflows through API surfaces. | Use as one evaluator/provider option, not as the artifact source of truth. |
| OpenAI Evals | Official deprecation docs say the Evals platform becomes read-only on 2026-10-31 and shuts down on 2026-11-30. | Do not build the engine’s eval infrastructure on the Evals platform. Own eval schemas and artifacts. |
| `gweb` | Ambiguous as a concrete Rust dependency. | Keep `DevUiServer` as an internal trait. Choose concrete web framework only after a spike. |

## Dependency boundary rules

```text
1. Dependencies are owned by crates, not by the entire architecture.
2. Public interfaces expose project-owned types only.
3. Dependency upgrades happen in dedicated tasks.
4. Provider APIs stay behind adapter crates.
5. No dependency type crosses the Layer 0 core boundary.
6. Renderer hot paths do not call AI, voice, web, model, or asset-import code.
```

## Version refresh checklist

Before starting implementation, run a dependency refresh task:

```bash
cargo search vulkano
cargo search winit
cargo search burn
cargo search raw-window-handle
cargo info vulkano
cargo info winit
cargo info burn
```

Then update:

```text
- Cargo.toml workspace dependency versions
- dependency decision records
- README dependency summary
- eval artifact dependency metadata schema
- AGENTS.md if any agent rule changes
```

## Dependency adoption record template

Every major dependency should have a short decision record:

```markdown
# Dependency Decision: <name>

Date:
Owner crate:
Version considered:
Purpose:
Alternatives:
Why accepted:
Why rejected alternatives were not chosen:
Public API exposure:
Feature flags:
Security/licensing review:
Performance notes:
Upgrade policy:
Rollback plan:
```

## Specific V4 dependency decisions

### Vulkano

Accepted as the first Vulkan access layer because it improves Rust ergonomics and safety. Not accepted as the only possible route for every feature. Raw Vulkan/ash remains allowed only in reviewed backend modules.

### winit

Accepted for desktop window/event loop handling. It is not an engine dependency. It belongs to `app_desktop` only.

### Burn

Accepted for AI/model/eval/tooling research. It is not a graphics interop layer for the renderer.

### OpenAI APIs

Allowed as replaceable provider adapters for visual evaluation, coding assistance, dialogue, and voice. Not allowed as core runtime types. Secrets and raw provider payloads require artifact and governance controls.

### Dev web framework

Deferred. Do not block renderer/eval milestones on dashboard technology.

## What must be rechecked regularly

```text
- winit event-loop API changes
- Vulkano raw-window-handle compatibility
- Vulkano support for new Vulkan features used by the renderer
- Burn backend naming and GPU backend behavior
- OpenAI API surface changes, model names, pricing, rate limits, and deprecations
- Vulkan SDK validation/synchronization validation behavior
- GPU driver support for Vulkan 1.4 features
```

## Dependency failure response

If a dependency blocks the milestone:

```text
1. Write a minimal reproduction.
2. Check whether the issue is misuse, version mismatch, wrapper limitation, or upstream bug.
3. Add a workaround only inside the owning crate.
4. Do not leak workaround types into public interfaces.
5. Record the decision.
6. Re-run smoke, eval, and artifact-schema checks.
```

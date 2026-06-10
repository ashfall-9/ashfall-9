# 25 — Dependency Spike Plan

## Purpose

Dependencies should not be accepted because they look plausible in a specification. Each important dependency must pass a small proof spike with documented results.

## Decision record format

Create one Markdown file per accepted dependency:

```text
docs/decisions/DDR-0001-vulkano.md
```

Template:

```text
# DDR-0001 — Dependency Name

Status: proposed | accepted | rejected | replaced
Date:
Owner:

Problem:
Options considered:
Selected option:
Version pinned:
Feature flags:
Why accepted:
Why not alternatives:
Risks:
License notes:
Build/platform notes:
Minimal spike command:
Rollback plan:
```

## Mandatory first spikes

### Spike 1 — winit + Vulkan surface

Goal:

```text
Create a window, create a Vulkan surface, create a swapchain, clear, present,
resize repeatedly, close cleanly.
```

Acceptance:

```text
- no validation errors
- no leaks observed in logs
- resize/suspend/minimize path handled
- crate versions recorded
```

### Spike 2 — offscreen/headless capture

Goal:

```text
Create no window, render to offscreen image, copy to staging, export PNG/HDR artifact.
```

Acceptance:

```text
- works without swapchain
- stores image plus telemetry JSON
- no CPU readback in normal frame path, only capture path
```

### Spike 3 — GPU timestamps and telemetry

Goal:

```text
Record per-pass timestamps and write a frame telemetry file.
```

Acceptance:

```text
- timestamp period applied correctly
- one clear-color pass has CPU and GPU timing
- telemetry schema versioned
```

### Spike 4 — shader compilation path

Goal:

```text
Choose and prove the shader compilation/reflection route.
```

Options may include GLSL-to-SPIR-V, HLSL-to-SPIR-V, Slang, or another deliberately chosen path. The decision must document why the path is suitable for Vulkan, hot reload, CI, and shader reflection.

Acceptance:

```text
- shader check command works
- SPIR-V output is cached or reproducible
- resource bindings can be reflected or manually verified
- warnings fail strict mode
```

### Spike 5 — eval artifact bundle

Goal:

```text
Run a fake sky evaluation that exports images, telemetry, rubric JSON, and pass/fail JSON.
```

Acceptance:

```text
- artifact directory matches schema
- rerun with same seed overwrites nothing; creates a new immutable run
- manifest includes build and interface versions
```

### Spike 6 — AI evaluator adapter

Goal:

```text
Implement a fake evaluator and one real provider adapter behind the same VisualEvaluator trait.
```

Acceptance:

```text
- provider response is converted to project-owned EvalReport
- raw provider response is stored as optional provenance, not core state
- prompt, model/provider, rubric, calibration set, and schema versions are stored
```

### Spike 7 — Burn worker isolation

Goal:

```text
Run a tiny Burn inference/evaluation worker outside the renderer hot path.
```

Acceptance:

```text
- renderer can run with Burn disabled
- worker communicates through jobs/results
- no direct Vulkan resource sharing with renderer
- GPU contention is measured if both use the same physical GPU
```

## Dependency adoption rules

```text
- no new dependency in engine_core without architecture review
- no provider SDK types in core interfaces
- no graphics dependency outside app_desktop/gfx_vk/renderer/shader_lab unless justified
- no async runtime requirement in renderer hot path
- no dependency accepted without license and platform notes
- optional dependencies must compile-disabled by default where practical
```

## Rejection is a valid outcome

A spike can conclude that a dependency should not be used. Record the reason and fallback path. This is preferable to forcing the architecture around a poor fit.

# 01 — Project Vision

## Goal

Create a Rust-first, Vulkan-first game-engine project focused on photorealism, physical consequence, measurable performance, and AI-assisted development.

The engine should eventually support realistic materials, physically meaningful object consequences, generated humans, AI-driven characters, and comfortable synthetic voices. The development process must remain incremental: only one object class is introduced at a time, and each object class is protected by regression tests before the next class begins.

## Project law

```text
No component is accepted because it looked good once.
A component is accepted when it passes locked visual, temporal, physical, and performance gates.
```

## Runtime scope

The runtime engine should be:

```text
- written primarily in Rust
- using Vulkan as the explicit GPU API
- built around stable component interfaces
- deterministic where simulation correctness matters
- able to run visual evaluation without gameplay
- able to run renderer development without AI, voice, humans, or physics
```

External AI and voice services may exist behind adapters, but the simulation and renderer should never depend on provider-specific types.

## Non-goals for the first year of development

```text
- full open-world game production
- arbitrary materials and arbitrary fracture
- full photoreal human generation
- complete fluid/gas/material coupling
- fully autonomous AI story generation
- production-grade voice acting replacement
- broad platform support beyond initial development hardware
```

These are long-term ambitions. The near-term goal is the infrastructure that makes those ambitions testable.

## Core principles

### 1. Photorealism must be measured

Every visual feature should expose:

```text
- capture outputs
- image-evaluation scores
- reference comparisons
- temporal stability metrics
- physical plausibility checks
- GPU/CPU timing
- memory usage
- regression history
```

A visual feature without captured artifacts and telemetry is not done.

### 2. Performance budgets exist before implementation

Every feature must declare:

```text
- target hardware profile
- resolution profile
- frame-time budget
- GPU pass budget
- CPU budget
- VRAM budget
- shader/pipeline budget
- accepted fallback modes
```

This avoids building beautiful features that cannot run.

### 3. Components are replaceable

Components communicate through:

```text
- typed handles
- immutable frame snapshots
- event streams
- asset descriptors
- telemetry reports
- artifact manifests
```

Components must not share private memory layout, private caches, Vulkan objects, model-provider objects, or audio-provider objects.

### 4. Add one object class at a time

Recommended milestone order:

```text
1. sun + atmosphere + volumetric clouds
2. ceramic sphere
3. glass sphere
4. wood block
5. metal block
6. fracturable ceramic
7. liquid droplet/puddle
8. gas/smoke volume
9. skin patch
10. eye close-up
11. hair lock
12. face close-up
13. full human
14. AI-driven speaking character
```

Each milestone should be certified before the next one begins.

### 5. Generated materials should reduce texture dependence

The engine should minimize manually authored texture sets by supporting:

```text
- physical material parameters
- procedural fields
- generated cache textures
- reference photography
- optional imported texture maps
```

For skin and natural materials, the source of truth should be procedural/material state where possible, with texture caches generated as runtime or build-time acceleration structures.

### 6. AI is a system, not a script substitute

AI characters should consume perception frames and emit action intents. They should not directly modify the world. This ensures AI-generated behavior remains compatible with deterministic simulation, networking/replay possibilities, and safety filters.

### 7. Artifacts are the project memory

Each milestone run should produce an immutable artifact bundle:

```text
- input settings
- seeds
- captures
- AOVs
- telemetry
- validation logs
- evaluator reports
- model/provider versions
- pass/fail decision
```

Without artifacts, automated improvement becomes guesswork.

## First non-negotiable milestone

The first milestone contains only:

```text
- camera
- sun
- atmosphere
- volumetric clouds
```

It explicitly excludes:

```text
- ground
- terrain
- props
- buildings
- characters
- particles outside the cloud implementation
- gameplay
```

This exposes sky, exposure, scattering, cloud lighting, cloud shape, temporal stability, and performance without hiding errors behind foreground content.

## Definition of architectural success

The architecture is successful when:

```text
- each major system is a separate crate or crate group
- public interfaces compile without circular dependencies
- renderer runs without AI, voice, humans, or physics
- evaluator runs renderer captures automatically
- telemetry is emitted for every frame
- artifacts are stored in a stable format
- Codex can work on one component without editing unrelated systems
- the sky milestone can be certified from CLI
```

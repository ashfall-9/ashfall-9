# 19 — Revision 2 Change Log

## Purpose

This file records the second sanity-check pass applied to the improved writeups.

The first improved set made the project feasible as a staged R&D program. Revision 2 focuses on implementation hazards that would create bugs once the project turns into real Rust/Vulkan code.

## Changes by area

### Renderer and Vulkan

```text
- Added a fuller GPU capabilities report.
- Added descriptor-indexing lifetime and epoch rules.
- Clarified descriptor buffer as an optional experiment with fallback.
- Clarified that render graph APIs may be ergonomic while implementation must reuse storage.
- Added stronger startup/eval requirement for storing GPU limits and driver/runtime versions.
```

### Material system

```text
- Replaced ambiguous RuntimeMaterial layer with three explicit representations:
  MaterialAuthoringDesc, RuntimeMaterialDesc, GpuMaterialPacked.
- Removed rich Rust enums from GPU-facing ABI examples.
- Added explicit GPU ABI layout validation requirements.
- Changed material generation API to job-based submit/poll/cancel pattern.
```

### Clouds and sun

```text
- Added physical scope boundary for the first sky milestone.
- Clarified that cloud parameters are renderer controls, not full meteorological simulation state.
- Added AI-score calibration language.
```

### Evaluation

```text
- Made evaluator infrastructure explicitly project-owned.
- Changed visual evaluator API to submit/poll job pattern.
- Added model drift and calibration requirements.
- Strengthened the rule that external model providers are adapters, not artifact authorities.
```

### Codex / agent workflow

```text
- Added better task-packet guidance.
- Strengthened AGENTS.md with V2 rules against unsafe GPU ABI changes, descriptor lifetime bugs, and eval threshold edits.
```

## Sanity-check verdict after V2

The writeups are now stronger as a technical starting specification. The remaining uncertainty is not whether the architecture is coherent; it is whether the team can keep scope under control and produce high-quality renderer milestones one object class at a time.

The most important next step is still small:

```bash
cargo run -p tools_cli -- eval sky --profile high --seed-set locked_001
```

Before any humans, fracture, liquids, AI agents, or voice systems are built, the project should prove that it can render, capture, evaluate, profile, and improve one sky milestone repeatedly.

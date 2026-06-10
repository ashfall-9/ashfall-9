# 17 — Codex and ChatGPT Workflow

## Goal

Use Codex and ChatGPT Pro to accelerate development without allowing AI-generated changes to destroy component boundaries, regress performance, or silently change milestone criteria.

## Coding-agent operating model

```text
ChatGPT:
  architecture reasoning, research, rubrics, review, prompt design, component specs

Codex:
  repository-local edits, tests, refactors, shader/test scaffolding, bug fixes

Eval lab:
  objective artifact generation and pass/fail decisions

Human owner:
  milestone promotion, scope decisions, safety decisions, final reviews
```

## Durable project instructions

Keep `AGENTS.md` at the repository root. Codex should read it before tasks. The included `AGENTS.md` in this ZIP is a starting point.

Important rules for the agent:

```text
- edit only the component named in the task
- do not change public interfaces without updating version and docs
- do not add dependencies without justification
- do not introduce unsafe outside allowed crates
- run relevant tests before presenting diff
- include telemetry/eval changes for renderer work
```


## V2 coding-agent guardrails

Use Codex as a component-scoped contributor, not as an unconstrained project manager.

```text
Good task:
  "In renderer_realtime and shaders/clouds only, reduce cloud edge ghosting on
   locked sky seed set 001. Do not edit eval thresholds. Run shader check and
   sky eval if available."

Bad task:
  "Make the sky photorealistic. Change whatever is necessary."
```

Recommended task packet:

```text
- goal
- allowed files/crates
- forbidden files/crates
- baseline artifact ID
- failure report excerpt
- required commands
- allowed performance regression tolerance
- required documentation updates
```

Codex may propose changes to interfaces or rubrics, but those should be separate review tasks, not bundled into rendering fixes.

## Component-scoped task template

```text
Task: Improve volumetric cloud edge stability in renderer_realtime.

Allowed crates:
- renderer_realtime
- shaders/clouds
- tests/renderer/clouds

Forbidden:
- engine_core public interfaces
- physics crates
- AI/voice crates
- unrelated material/human code

Acceptance:
- cargo test -p renderer_realtime
- cargo run -p tools_cli -- shader check
- cargo run -p tools_cli -- eval sky --profile desktop_mid_001 --seed-set locked_001
- temporal ghosting score improves or stays neutral
- cloud GPU time regresses by < 5%
- artifact bundle complete
```

## AI improvement loop

```text
1. Eval lab produces failure report.
2. ChatGPT summarizes likely causes and proposes scoped tasks.
3. Human selects one task.
4. Codex edits only allowed files.
5. Tests and eval run.
6. Human or CI reviews diff and artifact delta.
7. Merge only if gates pass.
```

## Do not use AI for uncontrolled auto-patching

Avoid loops like:

```text
AI sees bad image -> AI edits arbitrary files -> AI re-runs -> AI repeats until pass
```

Use bounded loops:

```text
AI sees bad image -> AI proposes one defect hypothesis -> scoped patch -> regression gate
```

## Review checklist for Codex output

```text
- Did it edit only allowed files?
- Did it preserve public interfaces or bump versions?
- Did it add dependencies?
- Did it introduce unsafe code?
- Did it add/update tests?
- Did it update telemetry or artifacts if needed?
- Did performance improve or remain within tolerance?
- Did it hide failures with relaxed thresholds?
```

## Prompt and rubric versioning

AI eval prompts are source files, not throwaway text.

```text
configs/eval_rubrics/sky_photorealism_v001.md
configs/eval_rubrics/sky_artifact_detection_v001.md
configs/eval_rubrics/sky_temporal_v001.md
```

Changing a prompt changes the eval baseline and requires an explicit artifact note.

## Recommended Codex first tasks

```text
1. Create workspace skeleton and AGENTS.md.
2. Add engine_core handle and interface-version types.
3. Add telemetry crate with JSON/CSV writer.
4. Add app_headless fake-frame artifact output.
5. Add gfx_vk smoke app with validation.
6. Add render_graph pass/resource skeleton.
7. Add shader_lab check command.
8. Add sky eval artifact manifest.
```

Do not ask Codex to “build the engine.” Ask it to complete one small work package with explicit allowed files and acceptance commands.


## V3 dependency and lifecycle task rules

Coding-agent tasks that touch windowing, Vulkan backend setup, descriptors, resource lifetime, shader compilation, or eval scoring must include a short decision note in the commit or PR summary.

Do not ask an agent to "make the renderer better". Use bounded tasks such as:

```text
- add surface-lost handling to app_desktop without touching cloud shaders
- add descriptor epoch accounting without changing material generation
- add sky temporal-flicker metric without changing evaluator prompt weights
- add an offscreen capture smoke test without changing presentation code
```

A task that changes both renderer output and the evaluation rubric should be rejected unless it is explicitly a rubric-migration task.

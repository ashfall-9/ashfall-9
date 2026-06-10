# 27 — Release Gate Playbook

## Purpose

This playbook defines how a component goes from local experiment to milestone-certified. It is especially important when Codex or other coding agents are making patches.

## Gate levels

```text
Gate 0 — compile and schema safety
Gate 1 — unit and shader checks
Gate 2 — artifact completeness
Gate 3 — visual/performance eval
Gate 4 — regression comparison
Gate 5 — human milestone review
```

## Gate 0 — compile and schema safety

Required checks:

```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
```

Required review:

```text
- crate boundaries preserved
- no forbidden dependency added
- no unsafe outside approved crates
- schema versions updated if schemas changed
```

## Gate 1 — unit and shader checks

Renderer/shader changes require:

```bash
cargo run -p tools_cli -- shader check
```

Expected coverage:

```text
- shader compiles
- reflected descriptors match expectations
- GPU-packed CPU structs match shader ABI assumptions
- render graph hazard tests pass
```

## Gate 2 — artifact completeness

The eval command must create:

```text
- manifest.json
- build_info.json
- gpu_info.json or structured no-GPU report
- settings.json
- captures or explicit dry-run placeholders
- telemetry files
- pass_fail.json
```

No complete artifact bundle, no certification.

## Gate 3 — visual/performance eval

Run the locked milestone eval:

```bash
cargo run -p tools_cli -- eval sky --profile desktop_mid_001 --seed-set locked_001
```

Evaluation must check:

```text
- visual score/rubric
- deterministic image metrics
- temporal sequence metrics where applicable
- GPU/CPU frame-time budgets
- validation errors
- memory high-water marks
```

## Gate 4 — regression comparison

Compare against the latest certified baseline for the same hardware profile.

Allowed outcomes:

```text
Pass:
  visual improves or stays neutral, performance within tolerance, no new severe artifacts

Needs review:
  visual improves but performance regresses within human-review range

Fail:
  previous milestone regresses, validation breaks, artifacts missing, or thresholds were changed inside the patch
```

## Gate 5 — human milestone review

Required only for milestone promotion, not every patch.

Review questions:

```text
- Does the output look plausible without knowing implementation details?
- Are temporal artifacts visible in motion?
- Did the AI evaluator miss anything obvious?
- Are performance numbers credible and repeatable?
- Were reference/counterexample calibration images scored correctly?
- Is the milestone scope still clean?
```

## Threshold-change procedure

Threshold changes must be separate from renderer patches.

A threshold-change proposal requires:

```text
- reason for change
- affected rubric/schema version
- before/after comparison on calibration set
- human approval
- migration note for old artifacts
```

## Provider/model-change procedure

Changing an AI model, voice provider, or evaluator provider requires:

```text
- adapter update
- calibration rerun
- provider/model metadata capture
- score comparability note
- rollback path
```

Do not compare old and new model scores as if they are the same scale unless calibration proves it.

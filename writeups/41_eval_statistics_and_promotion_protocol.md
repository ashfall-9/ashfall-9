# 41 — Evaluation Statistics and Promotion Protocol

## Purpose

The project needs AI image recognition, but it also needs measurement discipline. This document defines how a component is promoted without overfitting to one pretty screenshot, one judge prompt, or one lucky frame-time run.

## Principle

```text
Promote components by comparing a candidate against a locked baseline across many seeds, camera paths, and measured frames.
```

Do not promote from a single image or single average score.

## Dataset split

```text
smoke:
  tiny, used constantly, catches catastrophic failures

dev:
  used during iteration; may evolve with review

locked_eval:
  stable pre-merge comparison set

locked_certification:
  promotion-only set; not used as a normal tuning target

calibration:
  known-bad, threshold, and excellent examples for evaluator sanity
```

## Paired visual comparison

For renderer changes, use paired comparisons:

```text
same seed
same camera path
same quality profile
same resolution
same hardware profile when performance is measured
baseline artifact vs candidate artifact
```

The report should show per-case deltas, not only a global mean.

## Minimum visual report

```json
{
  "schema_version": "visual_promotion_v001",
  "baseline_run_id": "sky_baseline_2026_06_08",
  "candidate_run_id": "sky_candidate_2026_06_08",
  "dataset_id": "sky_locked_eval_001",
  "case_count": 0,
  "mean_delta": 0.0,
  "median_delta": 0.0,
  "worst_case_delta": 0.0,
  "cases_regressed": 0,
  "cases_improved": 0,
  "artifact_regressions": [],
  "requires_human_review": true
}
```

## Confidence summaries

Use simple statistics first:

```text
- mean and median score
- worst-case score
- count of cases below threshold
- count of artifact-tag regressions
- bootstrap confidence interval for mean delta when sample size is large enough
```

The engine does not need academic-level statistics on day one, but it does need to avoid promoting based on noise.

## Performance measurement protocol

Performance numbers must separate correctness runs from performance runs.

```text
validation run:
  validation layers on, synchronization validation on, debug labels on
  used for correctness, not final timing

profile run:
  optimized build, validation off or minimal, timestamps on, profiler labels on
  used for performance gating

eval run:
  deterministic seeds, captures/AOVs on, artifact output on
  used for visual and artifact gates
```

Performance report fields:

```text
- warmup frame count
- measured frame count
- p50 CPU frame time
- p95 CPU frame time
- p99 CPU frame time
- p50 GPU frame time
- p95 GPU frame time
- p99 GPU frame time
- per-pass p50/p95 GPU time
- VRAM peak
- transient allocation peak
- shader/pipeline count
- descriptor/resource count
```

## Repeated-run rule

A promotion candidate should pass repeated runs:

```text
- at least 3 profile runs on the same named hardware profile
- at least 2 visual/eval runs if provider variability is expected
- zero validation errors in validation run
- no missing artifacts
```

The exact repeat counts can be tuned, but they must be part of the rubric, not an ad hoc decision.

## AI evaluator drift rule

Changing any of these creates a new evaluator series:

```text
- provider
- model name/version
- prompt text
- rubric text
- output schema
- image preprocessing
- score normalization
```

Do not compare a V4 evaluator score to a V3 evaluator score unless a calibration bridge was run and recorded.

## Human review role

Human review is not a replacement for telemetry. It is a final sanity gate for promotion.

Human reviewer should check:

```text
- does the image look plausible without knowing implementation details?
- are there obvious repeated noise motifs?
- does motion reveal temporal artifacts?
- did the AI judge miss an obvious failure?
- does the component still match milestone scope?
- were performance budgets met on named hardware?
```

## Promotion pass rule for sky milestone

A sky candidate passes only when:

```text
- smoke set passes
- locked_eval set passes
- locked_certification set passes for promotion
- validation run has zero unwaived validation errors
- performance p95 and p99 are within budget
- worst-case visual score is above minimum floor
- no severe artifact tags are present
- artifact bundle validates
- dependency versions and GPU profile are recorded
- human promotion review approves
```

## Automatic iteration rule

A coding agent may propose fixes based on eval output, but it must not:

```text
- change eval thresholds while fixing renderer code
- remove hard seeds or hard camera paths
- tune on certification data as a normal loop
- accept lower performance for prettier screenshots without an explicit budget change
- hide failed artifacts
```

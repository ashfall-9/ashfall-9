# 11 — Evaluation and Feedback Loop

## Goal

The development loop should make photorealism and performance measurable.

```text
generate scene -> render -> capture -> evaluate -> profile -> decide -> patch -> rerender
```

AI assistance is useful, but evaluation must be multi-signal and artifact-backed.

## Evaluation principles

```text
- fixed seeds
- fixed camera paths
- fixed quality profiles
- fixed prompts/rubrics
- stored model/provider versions
- immutable artifacts
- no auto-merge based on AI judgment alone
```


## Evaluation infrastructure ownership

Do not make the project dependent on a vendor-hosted eval product as the artifact source of truth. The engine owns:

```text
- artifact manifests
- run history
- pass/fail decisions
- rubric files
- prompt files
- reference datasets
- score normalization
- regression comparisons
```

External model providers are replaceable evaluator adapters. Their raw responses may be stored for audit, but the project must store normalized evaluator JSON in its own schema.

## Visual evaluator API

```rust
pub trait VisualEvaluator {
    fn submit_eval(&mut self, request: EvalRequest) -> anyhow::Result<EvalJobId>;
    fn poll_eval(&mut self, job: EvalJobId) -> anyhow::Result<EvalJobStatus>;
}

pub enum EvalJobStatus {
    Queued,
    Running { progress: f32 },
    Complete(EvalReport),
    Failed(EvalError),
    Cancelled,
}

pub struct EvalRequest {
    pub component: ComponentId,
    pub milestone: MilestoneId,
    pub captures: Vec<RenderCaptureId>,
    pub references: Vec<ReferenceImageId>,
    pub telemetry: RendererTelemetry,
    pub rubric: EvalRubric,
}
```

## Eval report

```rust
pub struct EvalReport {
    pub score_total: f32,
    pub visual_score: f32,
    pub physical_score: f32,
    pub temporal_score: f32,
    pub performance_score: f32,
    pub artifacts: Vec<DetectedArtifact>,
    pub recommendations: Vec<EvalRecommendation>,
    pub pass: bool,
}
```

## Artifact bundle

```text
eval_runs/
  2026-06-08T000000Z_sky_locked_001/
    manifest.json
    build_info.json
    git_info.json
    gpu_info.json
    settings.json
    captures/
      final_sdr.png
      final_hdr.exr
      luminance.exr
      cloud_density_debug.exr
      cloud_transmittance.exr
      temporal_history_validity.png
    telemetry/
      frame_times.csv
      gpu_pass_times.csv
      memory.csv
      validation.log
    ai_reports/
      visual_judge.json
      artifact_detector.json
      suggested_fixes.md
    decision/
      pass_fail.json
      human_review.md
```

## Scoring model

Example sky score:

```text
VisualScore =
  0.30 * AI realism judgment
+ 0.20 * reference plausibility/comparison
+ 0.15 * artifact absence
+ 0.15 * lighting/exposure plausibility
+ 0.10 * cloud-shape plausibility
+ 0.10 * human calibration sample
```

Example performance score:

```text
PerformanceScore =
  0.40 * frame-time target
+ 0.20 * p95/p99 stability
+ 0.15 * GPU memory target
+ 0.10 * CPU frame cost
+ 0.10 * shader/pass budget
+ 0.05 * validation-clean result
```

Do not collapse everything to one number internally. Keep sub-scores visible.

## AI judge controls

```text
- prompt text is versioned
- rubric is versioned
- model/provider name is stored
- temperature/settings are stored when available
- all images and AOVs are linked by artifact ID
- evaluator output must be structured JSON
- failed JSON schema means failed eval
```

AI judge prompt should ask for specific artifacts, not vague beauty judgments.


## Model drift and calibration

AI evaluator results are not absolute. Every visual-evaluation run should store:

```text
- provider name
- model name or version string
- request parameters available to the caller
- prompt/rubric version
- calibration-image scores from the same run
- structured output schema version
```

A milestone threshold is valid only for the evaluator configuration it was calibrated with. If the evaluator model changes, rerun calibration captures before comparing scores to old baselines.

## Temporal evaluation

For sequences, compute:

```text
- luminance flicker
- edge ghosting
- history rejection rate
- temporal variance after motion compensation
- p95/p99 frame time
- reprojection artifact tags from AI sequence review if available
```

## Performance telemetry

Required per eval run:

```text
- CPU frame time
- GPU frame time
- per-pass GPU time
- draw/dispatch count
- memory usage
- transient allocation usage
- shader/pipeline count
- validation errors
- hardware profile
- driver/runtime versions
```

## Automated iteration policy

```text
1. Lock milestone.
2. Lock test set and hardware profile.
3. Run baseline.
4. Identify top visual defects and top performance bottleneck.
5. Create a component-scoped Codex task.
6. Run code changes.
7. Run tests and eval again.
8. Accept only if visual score improves and performance does not regress beyond allowed tolerance.
9. Store artifact bundle.
10. Promote only after repeated passing runs.
```

## Regression gate

A component passes only if:

```text
- unit tests pass
- shader tests pass where applicable
- render/eval tests pass
- performance thresholds pass
- validation logs are clean
- previous certified milestones do not regress
- artifacts are complete
```

## Human review

Human review is not required for every iteration, but it is required for milestone promotion. Store the review as an artifact.

Human reviewer checklist:

```text
- does the image look plausible without knowing the implementation?
- are artifacts visible in motion?
- does the AI judge miss anything obvious?
- are the performance numbers credible?
- should this milestone be frozen?
```


## V3 dataset and certification discipline

Evaluation must avoid overfitting to a single seed set or prompt. Use separate smoke, dev, locked-eval, locked-certification, and calibration datasets. Do not repeatedly tune renderer code against the certification set as the normal development loop.

Every milestone promotion attempt should produce a decision record naming:

```text
- candidate git revision
- dataset IDs
- hardware profiles
- evaluator provider/model metadata
- prompt/rubric/schema versions
- visual, temporal, performance, validation, and artifact gates
- human review result
- final certification decision
```

See `35_eval_dataset_and_judge_protocol.md` for the full protocol.


## V3 evaluator governance

Every evaluator report must include:

```text
- evaluator adapter name
- provider name, if external
- model name/version, if available
- prompt version
- rubric version
- calibration set version
- reference set version
- raw response provenance path, if allowed
- normalized project-owned EvalReport schema version
```

Model or prompt changes cannot be mixed into a renderer-fix pull request. They require a separate calibration run and a new baseline decision.

Promotion should prefer pairwise comparisons against the current certified baseline in addition to absolute scores. Pairwise review asks: did this candidate improve, regress, or trade one artifact for another?

## V4 statistical promotion link

Use this document for the runtime feedback loop. Use `41_eval_statistics_and_promotion_protocol.md` for promotion math, paired baseline/candidate comparisons, repeated-run requirements, and performance-report rules.

A passing `EvalReport` from one model call is not a certification result. Certification requires the dataset, artifact, performance, validation, and human-review gates described in the promotion protocol.

# 35 — Evaluation Dataset and Judge Protocol

## Purpose

This document prevents the project from treating an AI image score as an absolute truth. Certification is only meaningful under named datasets, rubrics, evaluator configurations, hardware profiles, and artifact schemas.

## Principle

```text
A component is certified against a named test set and named measurement process.
It is not declared universally perfect.
```

## Dataset tiers

```text
smoke set:
  tiny, runs often, catches obvious failures

dev set:
  larger, used for daily iteration, can evolve

locked eval set:
  stable pre-merge gate, reviewed changes only

locked certification set:
  larger promotion set, not used for constant tuning

calibration set:
  known bad / acceptable / excellent examples for evaluator sanity
```

Do not tune every shader change against the certification set. That creates overfitting.

## Reference manifest

Every reference image or sequence must include source and rights metadata.

```json
{
  "reference_id": "sky_ref_owned_0001",
  "media_type": "image_sequence",
  "source_type": "owned_photography",
  "license": "project-owned",
  "allowed_uses": ["visual_reference", "eval_comparison"],
  "disallowed_uses": ["model_training", "redistribution"],
  "contains_people": false,
  "capture_metadata": {
    "weather": "broken_cumulus",
    "time_of_day": "sunset"
  }
}
```

A visually useful reference is invalid if the project lacks rights to use it.

## Sky judge dimensions

Score separately:

```text
- atmosphere gradient and horizon behavior
- sun angular size, HDR intensity, bloom/glare restraint
- cloud shape, scale, type identity, and absence of repeated motifs
- cloud lighting, self-shadowing, transmittance, and silver lining
- camera exposure and tone mapping
- temporal stability under motion and exposure changes
- performance and memory under named hardware profile
```

## Evaluator output

AI judge output must be structured JSON.

```json
{
  "schema_version": "visual_eval_sky_v003",
  "overall_visual_score": 0.0,
  "atmosphere_score": 0.0,
  "sun_score": 0.0,
  "cloud_shape_score": 0.0,
  "cloud_lighting_score": 0.0,
  "camera_exposure_score": 0.0,
  "temporal_score": 0.0,
  "artifact_tags": [],
  "top_defects": [],
  "recommendations": [],
  "confidence": 0.0,
  "requires_human_review": true
}
```

Schema failure is eval failure. Free-form comments can be stored, but gates consume normalized fields.

## Calibration

Before judging a new candidate, run calibration examples through the evaluator. Calibration should show that known-bad examples fail, acceptable examples cluster near threshold, and excellent examples score high. If calibration fails, the evaluator configuration is invalid.

## Drift control

Store:

```text
- provider and model name/version
- prompt version
- rubric version
- schema version
- request settings
- calibration results
- raw response hash
- normalized JSON
```

Changing any of these creates a new eval series.

## Promotion rule

A sky milestone promotion requires:

```text
- smoke set passing on relevant PRs
- locked eval set passing on candidate branch
- locked certification set passing for promotion
- p95/p99 frame time within budget
- no validation errors
- no missing artifacts
- human promotion review attached
- previous certified milestones still pass
```

## Anti-overfitting rules

```text
- do not change renderer code and eval thresholds in the same task
- do not remove hard scenes just because they fail
- do not tune only one seed or one beautiful screenshot
- keep failed runs in artifact history
- keep certification data use limited and reviewed
```

# 21 — Sky Eval Rubric Templates

## Purpose

The sky milestone should not rely on a vague prompt such as “does this look photorealistic?” This file defines structured rubric templates for the first object class: sun, atmosphere, and volumetric clouds.

## Visual judge JSON schema sketch

```json
{
  "schema_version": "sky_visual_eval_v001",
  "component": "sky",
  "milestone": "sun_atmosphere_clouds_v001",
  "evaluator": {
    "provider": "string",
    "model": "string",
    "prompt_version": "string",
    "rubric_version": "string"
  },
  "scores": {
    "overall_visual_plausibility": 0.0,
    "cloud_shape_plausibility": 0.0,
    "sun_lighting_plausibility": 0.0,
    "atmosphere_color_plausibility": 0.0,
    "exposure_plausibility": 0.0,
    "artifact_absence": 0.0,
    "temporal_plausibility": 0.0
  },
  "artifacts": [
    {
      "type": "banding | tiling | ghosting | over_bloom | flat_lighting | wrong_scale | horizon_color | other",
      "severity": "none | minor | moderate | severe",
      "frame_or_capture_id": "string",
      "explanation": "string"
    }
  ],
  "top_three_fixes": [
    {
      "rank": 1,
      "component_hint": "atmosphere | clouds | sun | exposure | temporal | postprocess",
      "fix_hypothesis": "string",
      "risk": "string"
    }
  ],
  "pass_recommendation": false
}
```

## Visual prompt template

```text
You are evaluating rendered sky images from a Vulkan/Rust engine milestone.
The scene is allowed to contain only camera, sun, atmosphere, and volumetric clouds.
Do not reward the image for missing terrain, buildings, birds, aircraft, mountains, water, or characters.

Judge whether the sky itself is plausible under the provided time of day, sun direction,
exposure settings, cloud type, and weather parameters.

Look specifically for:
- repeated procedural noise motifs
- visible raymarch bands or stair steps
- cloud edges that look like cotton balls, smoke puffs, blobs, or plastic
- implausible silver lining or cloud self-shadowing
- incorrect sun/cloud transmittance
- overdone bloom or glare
- sky colors that are implausible near horizon/twilight
- exposure jumps or tone-mapping artifacts
- temporal ghosting or smearing if frame sequences are provided

Return only structured JSON matching the schema.
```

## Calibration set

Keep a calibration set with known examples:

```text
calibration/bad:
  obvious tiling, banding, flat clouds, wrong exposure

calibration/acceptable:
  plausible stills with minor temporal or lighting defects

calibration/excellent:
  strong reference captures or locked renderer outputs approved by human review
```

Every evaluator model or prompt change should score the calibration set before it scores new candidate captures.

## Numeric threshold rule

A threshold such as `visual_score >= 0.88` is valid only for:

```text
- the same rubric version
- the same prompt version
- the same evaluator model/provider family
- the same calibration set
- the same reference scene category
```

Changing any of these requires a new baseline.

## Pass/fail example

```json
{
  "schema_version": "sky_pass_fail_v001",
  "run_id": "2026-06-08T000000Z_sky_locked_001",
  "visual_gate": "pass",
  "temporal_gate": "pass",
  "performance_gate": "pass",
  "validation_gate": "pass",
  "artifact_bundle_complete": true,
  "human_promotion_review_required": true,
  "certified": false
}
```

`certified` remains false until the human promotion review is attached and previous certified milestones have not regressed.

# 29 — Sky Certification Rubric

## Purpose

This document defines the concrete pass/fail process for the first object class: **sun + atmosphere + volumetric clouds only**.

The goal is not literal perfection. The goal is a reproducible certification decision that protects the sky renderer from regression.

## Certified content boundary

Allowed:

```text
- camera
- sun
- atmosphere
- volumetric clouds
- sky/cloud debug visualizations
```

Forbidden until certification:

```text
- ground
- terrain
- water
- props
- buildings
- characters
- humans
- gameplay
- fracture
- non-cloud liquid/gas systems
```

## Seed-set structure

Create three seed sets.

```text
locked_smoke_001:
  5 scenes, used on every renderer PR

locked_eval_001:
  50 scenes, used for pre-merge sky feature work

locked_cert_001:
  150+ scenes, used for milestone promotion
```

Each scene record should include:

```json
{
  "scene_id": "sky_sunset_broken_cumulus_0007",
  "seed": 918273645,
  "camera_path": "slow_pan_15s",
  "sun": {
    "mode": "explicit_vector",
    "direction": [0.14, 0.32, 0.94],
    "angular_radius_deg": 0.266
  },
  "atmosphere": {
    "rayleigh_scale": 1.0,
    "mie_scale": 1.0,
    "ozone_scale": 1.0
  },
  "clouds": [
    {
      "cloud_type": "BrokenCumulus",
      "base_altitude_m": 1400.0,
      "thickness_m": 1900.0,
      "coverage": 0.47,
      "density_scale": 0.83,
      "wind_mps": [8.0, 0.0, 2.0]
    }
  ],
  "quality_profile": "high"
}
```

## Required scene categories

```text
Time of day:
- sunrise
- morning
- noon
- late afternoon
- sunset
- twilight

Cloud/weather cases:
- clear sky
- sparse cumulus
- broken cumulus
- overcast stratus
- towering cumulonimbus
- high cirrus
- mixed layer

Camera paths:
- static wide shot
- slow pan
- fast pan
- upward tilt
- exposure ramp
- sun entering cloud edge
- sun exiting cloud edge
- cloud-edge close framing
```

## Capture requirements

Every eval scene must produce:

```text
captures/final_sdr.png
captures/final_hdr.exr or documented HDR fallback
captures/luminance.exr
captures/cloud_density_debug.exr or cloud_density_debug.png
captures/cloud_transmittance.exr
captures/cloud_step_count.exr or cloud_step_count.png
captures/temporal_history_validity.png
telemetry/gpu_pass_times.csv
telemetry/frame_times.csv
telemetry/memory.csv
telemetry/validation.log
```

## AI visual judge JSON schema

The AI judge must return structured JSON matching this shape.

```json
{
  "schema_version": "sky_ai_judge.v1",
  "scene_id": "sky_sunset_broken_cumulus_0007",
  "model": "provider/model/version",
  "scores": {
    "photorealism": 0.0,
    "cloud_shape_plausibility": 0.0,
    "lighting_plausibility": 0.0,
    "exposure_plausibility": 0.0,
    "color_plausibility": 0.0,
    "artifact_absence": 0.0,
    "temporal_plausibility": 0.0
  },
  "detected_artifacts": [
    {
      "type": "raymarch_banding",
      "severity": "medium",
      "region": "upper_left",
      "evidence": "visible parallel bands in low-density cloud"
    }
  ],
  "top_three_fixes": [
    {
      "component": "VolumetricCloudFeature",
      "hypothesis": "increase jitter quality or adjust temporal resolve clamp",
      "expected_effect": "reduce banding without changing cloud shape"
    }
  ],
  "confidence": 0.0
}
```

Invalid JSON is an evaluation failure, not a warning.

## AI judge prompt template

The prompt should be versioned. Example:

```text
You are evaluating a real-time sky renderer. Judge only the sky, sun, atmosphere, and volumetric clouds. Do not reward fantasy stylization, overdone bloom, painterly colors, or dramatic composition unless they are physically plausible.

Return JSON matching sky_ai_judge.v1.

Score each category from 0.0 to 10.0:
- photorealism
- cloud_shape_plausibility
- lighting_plausibility
- exposure_plausibility
- color_plausibility
- artifact_absence
- temporal_plausibility, if a sequence is provided

Look specifically for:
- repeated noise patterns
- visible raymarch bands
- incorrect silver lining
- wrong sun/cloud transmittance
- overdone bloom
- cotton/plastic cloud appearance
- flat lighting
- temporal ghosting
- exposure jumps
- implausible horizon/twilight colors

Name the top three likely fixes. Do not suggest adding terrain, objects, or composition tricks.
```

## Defect taxonomy

Use stable defect names so regression reports can be grouped.

```text
raymarch_banding
visible_noise_repetition
temporal_ghosting
history_smear
edge_halo
sun_transmittance_wrong
silver_lining_wrong
overdone_bloom
underexposed_sun
overexposed_clouds
flat_cloud_lighting
implausible_horizon_color
implausible_twilight_color
cloud_shape_cotton_ball
cloud_shape_plastic
cloud_scale_wrong
cloud_motion_wrong
upscale_artifact
validation_error
performance_regression
```

## Temporal metrics

For each sequence:

```text
luminance_flicker_score:
  normalized frame-to-frame luminance instability after exposure normalization

edge_ghosting_score:
  cloud-edge mismatch between current frame and reprojected history

history_rejection_rate:
  percentage of pixels rejecting temporal history

camera_motion_stability:
  visual stability under slow pan, fast pan, and upward tilt

p95_frame_ms and p99_frame_ms:
  frame-time stability under the same sequence
```

## Performance scoring

Use per-profile budgets. Initial placeholders:

```text
High desktop:
  resolution: 2560x1440
  total_gpu_ms_p95 <= 16.6
  cloud_gpu_ms_p95 <= 2.5
  atmosphere_gpu_ms_p95 <= 0.8
  post_gpu_ms_p95 <= 0.5

Mid desktop:
  resolution: 1920x1080
  total_gpu_ms_p95 <= 16.6
  cloud_gpu_ms_p95 <= 4.0
  atmosphere_gpu_ms_p95 <= 1.0
  post_gpu_ms_p95 <= 0.8

Fallback:
  resolution: 1920x1080
  total_gpu_ms_p95 <= 33.3
  cloud_gpu_ms_p95 <= 7.0
```

Replace placeholders once hardware profiles are selected.

## Aggregate score

Keep sub-scores visible. A combined score is allowed only for dashboard sorting.

```text
VisualScore =
  0.25 * AI photorealism
+ 0.15 * cloud shape plausibility
+ 0.15 * lighting/transmittance plausibility
+ 0.10 * exposure/color plausibility
+ 0.15 * artifact absence
+ 0.10 * reference-set comparison
+ 0.10 * human calibration sample

TemporalScore =
  0.35 * low flicker
+ 0.25 * low ghosting
+ 0.20 * stable history rejection
+ 0.20 * stable motion sequences

PerformanceScore =
  0.45 * total frame budget
+ 0.25 * cloud pass budget
+ 0.15 * memory budget
+ 0.10 * CPU overhead budget
+ 0.05 * validation clean
```

## Certification pass/fail rule

Certification passes only if all are true:

```text
- smoke, eval, and cert seed sets complete
- no Vulkan validation errors
- no missing required artifacts
- no severe visual artifact in locked_cert_001
- VisualScore average >= locked threshold
- TemporalScore average >= locked threshold
- PerformanceScore average >= locked threshold on named hardware
- p99 frame time does not exceed emergency limit
- human reviewer approves milestone promotion
- previous certified milestones, if any, do not regress
```

## Anti-gaming rules

The eval should reject these shortcuts:

```text
- adding foreground objects to hide sky issues
- over-blurring clouds to hide banding
- excessive bloom to hide sun/cloud transmittance errors
- lowering resolution without updating quality profile
- relaxing AI prompts after failures
- changing seed set without a documented rubric version bump
- accepting a score improvement with a major performance regression
```

## Promotion artifact

Milestone promotion creates:

```text
decisions/sky_milestone_certification.md
```

Required contents:

```text
- source revision
- dependency versions
- hardware profiles
- driver versions
- seed sets
- quality profiles
- score summary
- top remaining known defects
- human reviewer notes
- signed pass/fail decision
```

# 26 — Runtime Quality Profiles and Budgets

## Purpose

Photorealism and performance need shared knobs. A renderer feature should not invent private settings that bypass milestone gates.

## Quality profile contract

```rust
pub struct RenderQualityProfile {
    pub profile_id: QualityProfileId,
    pub target_resolution: [u32; 2],
    pub target_frame_ms: f32,
    pub target_gpu_memory_mb: u32,
    pub allow_temporal_accumulation: bool,
    pub allow_ray_tracing: bool,
    pub allow_async_compute: bool,
    pub capture_aovs: AovCapturePolicy,
    pub feature_budgets: Vec<FeatureBudget>,
}

pub struct FeatureBudget {
    pub feature_id: &'static str,
    pub max_gpu_ms_p50: f32,
    pub max_gpu_ms_p95: f32,
    pub max_vram_mb: u32,
    pub max_dispatches: u32,
    pub max_draws: u32,
}
```

## Initial profiles

### `dev_fast`

```text
Purpose: quick iteration
Validation: optional
Resolution: low/medium
AOVs: minimal
AI eval: optional/fake
Certification: no
```

### `dev_visual`

```text
Purpose: artist/developer visual iteration
Validation: optional
Resolution: target or near-target
AOVs: selected debug outputs
AI eval: optional
Certification: no
```

### `eval_locked_high`

```text
Purpose: milestone certification on high profile
Validation: enabled or logged where practical
Resolution: locked
AOVs: required sky/debug outputs
AI eval: required
Temporal tests: required
Certification: yes, on named hardware only
```

### `eval_locked_mid`

```text
Purpose: scalability certification
Validation: configured by run type
Resolution: locked lower target
AOVs: required outputs
AI eval: required but weighted against budget
Certification: yes, on named hardware only
```

### `oracle_reference`

```text
Purpose: slow high-quality comparison output
Realtime budget: none
AOVs: maximum useful outputs
Used for: reference images, defect analysis, material validation
Certification: cannot replace realtime eval
```

## Sky feature knobs

Allowed sky knobs:

```text
Atmosphere:
  LUT resolution
  LUT update cadence
  scattering approximation level
  aerial perspective toggle/fidelity

Sun:
  sun-disc angular size
  HDR intensity calibration
  glare/bloom input threshold
  limb-darkening approximation toggle

Clouds:
  render resolution scale
  primary raymarch steps
  light/shadow ray steps
  temporal reprojection frames
  history rejection sensitivity
  weather map resolution
  density volume resolution
  erosion/detail noise octaves
  shadow/transmittance map resolution
  upsample quality
```

Forbidden certification shortcuts:

```text
- hiding artifacts with excessive bloom
- reducing exposure to conceal banding
- changing camera paths between candidate and baseline
- changing AI prompts or rubrics during a renderer fix
- disabling temporal motion tests
- passing a profile by silently lowering resolution
```

## Performance reporting

Every eval run should report:

```text
- p50, p95, p99 CPU frame time
- p50, p95, p99 GPU frame time
- per-pass GPU timings
- VRAM peak and steady-state
- descriptor count and pipeline count
- shader variant count
- captures exported and readback time
- validation errors/warnings
- dropped or skipped frames
```

## Promotion rule

A component can be milestone-certified only for the exact profile and hardware class that passed.

Example:

```text
clouds/sun certified for eval_locked_high on RTX-class desktop profile
```

does not imply:

```text
clouds/sun certified for laptop iGPU
clouds/sun certified for VR
clouds/sun certified with ray tracing disabled/enabled variants not tested
```

Each certification claim must name:

```text
- profile id
- hardware profile
- driver version
- build hash
- evaluator/rubric version
- seed set
- camera path set
- artifact bundle id
```

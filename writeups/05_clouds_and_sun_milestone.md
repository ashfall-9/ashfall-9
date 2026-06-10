# 05 — Clouds and Sun Milestone

## Purpose

The first certified object class is the sky:

```text
- camera
- sun
- atmosphere
- volumetric clouds
```

No ground, terrain, props, characters, buildings, water, or gameplay should exist in this milestone. This prevents hiding atmospheric and cloud errors behind foreground content.


## Physical scope boundary

This milestone is a physically inspired sky renderer, not a full weather simulator.

In scope:

```text
- plausible sun/sky radiance relationships
- plausible cloud scale, density, transmittance, and self-shadowing
- deterministic procedural cloud evolution
- physically coherent exposure and tone mapping
- temporal stability under camera motion
```

Out of scope for certification:

```text
- Navier-Stokes weather simulation
- precipitation microphysics
- storm lifecycle prediction
- coupling clouds to terrain or ocean evaporation
- global weather systems
```

The cloud layer parameters may use meteorological names, but they are renderer controls unless a future physics/weather module explicitly owns them.

## Required features

### Camera

```text
- physical-camera metadata
- stable exposure controls
- fixed camera paths for evaluation
- deterministic jitter sequence for temporal reconstruction
```

### Sun

```text
- direction from date/time or explicit vector
- physical angular size approximation
- HDR intensity input to exposure/tone mapping
- sun disc visible when unobscured
- cloud transmittance affects sun visibility
- bloom/glare only after HDR thresholding
```

### Atmosphere

```text
- Rayleigh scattering approximation
- Mie/aerosol approximation
- ozone/absorption approximation if feasible
- transmittance LUT
- sky-view LUT or equivalent
- aerial perspective groundwork for later terrain
- time-of-day support
```

### Volumetric clouds

```text
- 3D density field
- weather/coverage map
- cloud type parameters
- erosion/detail noise
- direct sun lighting
- approximate multiple scattering
- cloud self-shadowing/transmittance
- wind evolution
- temporal reprojection
- low-resolution render + upscale
- debug AOVs
```

## Cloud layer schema

```rust
pub struct CloudLayer {
    pub base_altitude_m: f32,
    pub thickness_m: f32,
    pub coverage: f32,
    pub cloud_type: CloudType,
    pub density_scale: f32,
    pub wind_mps: Vec3,
    pub humidity: f32,
    pub precipitation_potential: f32,
    pub seed: u64,
}

pub enum CloudType {
    Cumulus,
    Cumulonimbus,
    Stratus,
    Stratocumulus,
    Cirrus,
    Mixed,
}
```

## Sky render pass plan

```text
1. Frame constants upload
2. Atmosphere LUT update if parameters changed
3. Weather/cloud density field update if parameters changed
4. Cloud shadow/transmittance pass
5. Volumetric cloud raymarch at reduced resolution
6. Temporal reprojection
7. Upscale/composite
8. Atmosphere composite
9. Sun disc and HDR glare input
10. Tone map
11. Capture/export AOVs
```

## Debug outputs

Required captures:

```text
- final SDR PNG
- final HDR/EXR
- sky luminance
- cloud density slice or projected density
- cloud transmittance
- cloud step count
- temporal history validity
- exposure value
- GPU pass timing CSV
```

## Evaluation scene set

Create a locked seed set with at least:

```text
Time of day:
  - sunrise
  - morning
  - noon
  - late afternoon
  - sunset
  - twilight

Weather/cloud cases:
  - clear sky
  - sparse cumulus
  - broken cumulus
  - overcast stratus
  - towering cumulonimbus
  - high cirrus
  - mixed layers

Camera paths:
  - static wide shot
  - slow pan
  - fast pan
  - upward tilt
  - exposure ramp
  - sun entering/exiting cloud edge
```

## AI visual rubric

The AI judge should return structured JSON, not prose only.

```json
{
  "photorealism": 0.0,
  "cloud_shape_plausibility": 0.0,
  "lighting_plausibility": 0.0,
  "exposure_plausibility": 0.0,
  "artifact_score": 0.0,
  "detected_artifacts": [],
  "top_three_fixes": []
}
```

AI scores are relative to a locked evaluator configuration. Store calibration images for bad, acceptable, and excellent skies, and evaluate new captures against that calibration set before trusting a numeric threshold.

The prompt must ask the judge to look specifically for:

```text
- repeated noise patterns
- visible raymarch bands
- incorrect silver lining
- wrong sun/cloud transmittance
- overdone bloom
- plastic/cotton-like clouds
- flat lighting
- temporal ghosting in frame sequences
- exposure jumps
- unrealistic sky color at horizon or twilight
```

## Temporal stability checks

Still images are not enough. Evaluate frame sequences.

Metrics:

```text
- per-pixel luminance variance after motion compensation
- history rejection percentage
- ghosting score near cloud edges
- flicker score in low-density wisps
- p95/p99 frame time during camera motion
```

## Performance profiles

Do not certify performance without named hardware. Initial placeholders:

```text
High desktop target:
  resolution: 2560x1440
  total frame: <= 16.6 ms
  clouds: <= 2.5 ms
  atmosphere: <= 0.8 ms
  post/tone: <= 0.5 ms

Mid desktop target:
  resolution: 1920x1080
  total frame: <= 16.6 ms
  clouds: <= 4.0 ms
  atmosphere: <= 1.0 ms
  post/tone: <= 0.8 ms

Fallback target:
  resolution: 1920x1080
  total frame: <= 33.3 ms
  clouds: <= 7.0 ms
```

These are provisional and should be replaced with actual hardware profiles.

## Certification criteria

The sky milestone is certified only if:

```text
Visual:
  - average AI realism score >= locked threshold
  - no severe artifact in locked test set
  - human spot check accepts representative captures

Physical:
  - sun direction matches illumination
  - cloud transmittance affects sun visibility plausibly
  - exposure behaves coherently across brightness changes

Temporal:
  - no unacceptable flicker or ghosting in locked camera paths
  - temporal reprojection does not smear cloud edges severely

Performance:
  - GPU/CPU budgets pass on named hardware profile
  - p95 and p99 frame times pass
  - VRAM stable over 10-minute loop

Engineering:
  - Vulkan validation clean in eval profile
  - render graph has no undefined resource hazards
  - no shader hot reload or dynamic compilation in eval/release
  - artifacts are stored with manifest and source versions
```

## Iteration policy

```text
1. Run locked sky eval.
2. Collect artifacts and telemetry.
3. AI judge identifies top 1-3 visual defects.
4. Performance gate identifies top bottleneck.
5. Codex is assigned one component-scoped task.
6. Only the sky/cloud/atmosphere feature may be edited.
7. Run unit tests, shader checks, and sky eval again.
8. Accept only if score improves and performance does not regress beyond threshold.
9. Store artifact bundle.
10. Promote after multiple consecutive passing runs.
```

Do not allow an AI agent to modify unrelated systems during this milestone.


## V3 quality-profile rule for sky certification

Sky certification must name the exact quality profile, hardware profile, driver, seed set, camera-path set, evaluator/rubric version, and artifact bundle.

Allowed cloud/sun quality knobs are centralized in `26_runtime_quality_profiles_and_budgets.md`. A run may not pass by silently lowering resolution, changing camera paths, changing evaluator prompts, hiding artifacts with bloom, or disabling temporal tests.

## V3 offscreen capture requirement

The sky milestone must work in offscreen mode before it is promoted:

```text
cargo run -p tools_cli -- eval sky --profile eval_locked_high --seed-set locked_001
```

This command should not require a visible window. A separate desktop smoke test should prove swapchain presentation, resize, and surface-lifecycle handling.

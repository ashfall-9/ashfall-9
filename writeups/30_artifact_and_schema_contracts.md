# 30 — Artifact and Schema Contracts

## Purpose

The engine’s development process depends on reproducible artifacts. This document defines the minimum artifact bundle, telemetry schemas, and decision files for evaluation runs.

## Artifact principles

```text
- every eval run is immutable
- every capture links to source revision and settings
- every AI report stores prompt/model/provider metadata
- every performance result stores hardware and driver metadata
- missing artifacts cause failed or inconclusive evals, never silent passes
```

## Directory layout

```text
eval_runs/
  2026-06-08T153000Z_sky_locked_smoke_001_a1b2c3d/
    manifest.json
    build_info.json
    git_info.json
    dependency_info.json
    gpu_info.json
    settings.json
    seed_set.json
    captures/
      final_sdr.png
      final_hdr.exr
      luminance.exr
      cloud_density_debug.exr
      cloud_transmittance.exr
      cloud_step_count.exr
      temporal_history_validity.png
    telemetry/
      frame_times.csv
      gpu_pass_times.csv
      memory.csv
      render_graph.json
      validation.log
    ai_reports/
      prompt_sky_v1.txt
      visual_judge.json
      artifact_detector.json
      suggested_fixes.md
    temporal/
      temporal_metrics.json
      sequence_summary.csv
    decision/
      pass_fail.json
      human_review.md
```

## Manifest schema

```json
{
  "schema_version": "eval_manifest.v1",
  "run_id": "2026-06-08T153000Z_sky_locked_smoke_001_a1b2c3d",
  "created_utc": "2026-06-08T15:30:00Z",
  "milestone": "sky",
  "component": "renderer_realtime.sky",
  "seed_set": "locked_smoke_001",
  "quality_profile": "high",
  "source_revision": "git-sha-or-dirty-marker",
  "interface_versions": [
    { "name": "renderer", "major": 0, "minor": 1, "patch": 0 }
  ],
  "files": [
    {
      "path": "captures/final_sdr.png",
      "kind": "sdr_preview",
      "sha256": "..."
    }
  ],
  "status": "complete"
}
```

## Build info schema

```json
{
  "schema_version": "build_info.v1",
  "rustc_version": "rustc ...",
  "cargo_profile": "eval",
  "target_triple": "x86_64-unknown-linux-gnu",
  "features": ["headless", "validation"],
  "opt_level": 3,
  "debug_assertions": false
}
```

## Dependency info schema

```json
{
  "schema_version": "dependency_info.v1",
  "cargo_lock_sha256": "...",
  "key_dependencies": {
    "vulkano": "0.35.x",
    "winit": "0.30.x",
    "burn": "0.21.x"
  }
}
```

Use actual locked versions from Cargo.lock, not the example strings above.

## GPU info schema

```json
{
  "schema_version": "gpu_info.v1",
  "device_name": "GPU name",
  "vendor_id": 0,
  "device_id": 0,
  "api_version": "1.3.x",
  "driver_version": "driver string",
  "selected_tier": "Vulkan13Portable",
  "features": {
    "dynamic_rendering": true,
    "synchronization2": true,
    "descriptor_indexing": false,
    "descriptor_buffer": false,
    "ray_query": false,
    "ray_tracing_pipeline": false
  },
  "memory_heaps": [
    { "size_mb": 8192, "device_local": true }
  ],
  "timestamp_period_ns": 1.0
}
```

## Settings schema

```json
{
  "schema_version": "sky_settings.v1",
  "resolution": [2560, 1440],
  "quality_profile": "high",
  "fixed_exposure": true,
  "camera": {
    "path_id": "slow_pan_15s",
    "fov_degrees": 60.0,
    "exposure_value": 13.5
  },
  "sun": {
    "direction": [0.14, 0.32, 0.94],
    "angular_radius_deg": 0.266
  },
  "atmosphere": {},
  "clouds": []
}
```

## Frame telemetry CSV

File:

```text
telemetry/frame_times.csv
```

Columns:

```text
frame_index,cpu_frame_ms,gpu_frame_ms,present_ms,draw_calls,dispatch_calls,validation_errors
```

Rules:

```text
- one row per frame
- use dot decimal separator
- no localized units
- frame_index starts at 0 for sequence captures
```

## GPU pass timing CSV

File:

```text
telemetry/gpu_pass_times.csv
```

Columns:

```text
frame_index,pass_name,queue_class,start_timestamp,end_timestamp,duration_ms
```

Required pass names for sky milestone:

```text
frame_constants
atmosphere_lut
cloud_density_update
cloud_shadow_transmittance
cloud_raymarch
temporal_reprojection
cloud_upscale_composite
sun_disc
tone_map
capture_copy
```

If a pass is skipped because parameters did not change, record either a zero/absent pass consistently and document the policy in `render_graph.json`.

## Memory CSV

File:

```text
telemetry/memory.csv
```

Columns:

```text
frame_index,vram_used_mb,transient_mb,upload_mb,readback_mb,descriptor_sets,pipelines
```

## Render graph report

File:

```text
telemetry/render_graph.json
```

```json
{
  "schema_version": "render_graph_report.v1",
  "passes": [
    {
      "name": "cloud_raymarch",
      "queue": "compute",
      "reads": ["cloud_density", "atmosphere_lut"],
      "writes": ["cloud_color_lowres", "cloud_transmittance"],
      "barriers_inserted": 3
    }
  ],
  "validation": {
    "undefined_resource_use": 0,
    "cycles": 0
  }
}
```

## Visual report schema

```json
{
  "schema_version": "visual_eval_report.v1",
  "evaluator": "ai_provider_or_local_model",
  "model": "model/version",
  "prompt_version": "sky_prompt.v1",
  "rubric_version": "sky_rubric.v1",
  "scores": {
    "visual": 0.0,
    "physical": 0.0,
    "temporal": 0.0,
    "performance": 0.0
  },
  "artifacts": [],
  "recommendations": [],
  "pass": false
}
```

## Pass/fail decision schema

```json
{
  "schema_version": "milestone_decision.v1",
  "milestone": "sky",
  "run_id": "...",
  "decision": "fail",
  "reasons": [
    "cloud_raymarch p95 exceeded high profile budget",
    "medium temporal ghosting detected in sun-edge sequence"
  ],
  "scores": {
    "visual": 7.8,
    "temporal": 6.9,
    "performance": 8.3
  },
  "required_next_action": "Create one component-scoped task for temporal reprojection edge ghosting."
}
```

Decision values:

```text
pass
fail
inconclusive
```

Use `inconclusive` for missing AI provider response, missing human review, missing hardware profile, or incomplete artifacts.

## Artifact validation command

Target command:

```bash
cargo run -p tools_cli -- artifact validate eval_runs/<run_id>
```

Validation checks:

```text
- required files exist
- JSON schemas parse
- CSV headers match
- referenced files exist
- checksums match
- hardware profile exists
- prompt/rubric versions exist
- pass/fail decision is compatible with missing/present artifacts
```

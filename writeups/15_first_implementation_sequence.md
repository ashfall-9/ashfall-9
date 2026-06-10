# 15 — First Implementation Sequence

## Phase 0 — Repository and agent setup

```text
1. create Cargo workspace
2. add AGENTS.md
3. add formatting/clippy/test commands
4. create docs/ and configs/ directories
5. create initial manifest schema for eval artifacts
```

Acceptance:

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

## Phase 1 — Desktop and headless shells

```text
1. app_desktop creates winit event loop and window
2. app_headless creates offscreen run context
3. engine_core exposes app lifecycle
4. telemetry writes a simple JSON/CSV frame log
```

Acceptance:

```text
- desktop app opens and closes cleanly
- headless app runs one fake frame
- telemetry artifact exists
```

## Phase 2 — Vulkan bootstrap

```text
1. load Vulkan library
2. create instance
3. select physical device
4. create logical device and queues
5. create surface/swapchain for desktop
6. create offscreen image target for headless
7. enable debug labels and validation in dev
```

Acceptance:

```text
- desktop clear-color frame presents
- headless clear-color image exports
- validation clean
- GPU/device capabilities logged
```

## Phase 3 — Render graph skeleton

```text
1. define graph images/buffers
2. define pass descriptions
3. record a clear/composite pass
4. insert minimal barriers
5. collect GPU timestamps
6. export render graph debug report
```

Acceptance:

```text
- one pass records and runs
- timestamps appear in telemetry
- invalid read/write dependency fails debug test
```

## Phase 4 — Shader lab

```text
1. choose initial shader source workflow
2. compile to SPIR-V
3. cache compiled outputs
4. reflect descriptor/resource use
5. add shader check command
```

Acceptance:

```bash
cargo run -p tools_cli -- shader check
```

## Phase 5 — HDR color and capture

```text
1. render to HDR image
2. tone map to SDR
3. export PNG
4. export HDR/EXR if supported by chosen image path
5. write capture manifest
```

Acceptance:

```text
- output image exists
- capture manifest links settings and telemetry
```

## Phase 6 — Sun and exposure

```text
1. add camera exposure parameters
2. render sun disc in HDR
3. tone map correctly
4. add sun-direction debug overlay or metadata
```

Acceptance:

```text
- sun visible at multiple exposures
- exposure changes are stable
- capture includes exposure metadata
```

## Phase 7 — Atmosphere

```text
1. add atmosphere parameters
2. add transmittance/sky LUTs or equivalent
3. render clear sky by time of day
4. compare against reference captures/rubric
```

Acceptance:

```text
- sunrise/noon/sunset cases render plausibly
- no validation errors
- atmosphere GPU timing recorded
```

## Phase 8 — Volumetric clouds

```text
1. create procedural density/weather fields
2. add reduced-resolution cloud raymarch
3. add cloud lighting and transmittance
4. add temporal reprojection
5. add upscale/composite
6. export cloud debug AOVs
```

Acceptance:

```text
- sparse/broken/overcast cloud cases render
- debug AOVs exist
- cloud GPU timing recorded
```

## Phase 9 — Sky eval loop

```text
1. lock seed set
2. lock camera paths
3. lock hardware profile
4. lock AI visual prompt/rubric
5. run full sky eval
6. store artifact bundle
7. generate pass/fail report
```

Acceptance:

```bash
cargo run -p tools_cli -- eval sky --profile high --seed-set locked_001
```

Output:

```text
- captures
- AOVs
- telemetry CSVs
- validation log
- AI visual report
- temporal report
- pass/fail JSON
```

## Phase 10 — Certification

Certification requires repeated passing runs.

```text
- no severe visual defects
- temporal stability passes
- performance budget passes
- validation clean
- artifacts complete
- human spot check complete
```

Only after certification should the project add the first material object.

## First post-sky object

Add one polished ceramic sphere.

```text
- simple geometry
- one generated material
- no fracture
- no AI
- no human
- no terrain unless required as a neutral test plane later
```

This tests PBR material, exposure, reflection, and shadow behavior without exploding scope.

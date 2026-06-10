# 07 — Material and Texture Generator

## Goal

Reduce manually authored texture dependence by representing materials as physical parameters, procedural fields, and generated caches. Traditional texture maps remain supported, but they should not be the only route to photorealism.

## Sanity constraint

Photo-to-material and AI-generated textures are not guaranteed to produce physically plausible runtime materials. The material generator must validate outputs with rendering tests, parameter bounds, tiling checks, and performance budgets.

## Material representation layers

Use three explicitly different representations.

```text
Layer 0 — MaterialAuthoringDesc:
  tool/user-friendly material intent, source photos, semantic class, provenance,
  procedural graph, physical hints, and editable parameters

Layer 1 — RuntimeMaterialDesc:
  renderer-friendly CPU-side material resolved from authoring data, with stable
  texture/procedural handles, quality policy, and fallback strategy

Layer 2 — GpuMaterialPacked:
  fixed-layout shader ABI with scalar/vector values, resource table indices,
  flags, and explicit padding
```

Only `GpuMaterialPacked` may be copied directly into GPU buffers. Rich Rust enums, strings, graph nodes, and provider-specific metadata must never appear in GPU ABI structs.

## Initial material baseline

Use glTF-style metallic/roughness PBR first:

```text
- base color
- metallic
- roughness
- normal
- occlusion
- emissive
```

Then extend to:

```text
- transmission
- subsurface
- anisotropy
- clearcoat
- sheen
- volume
- displacement
- procedural fields
```

## Material generator API

Long-running material work should be job-based. A synchronous helper may exist for tools, but the runtime-facing contract should not block the render thread.

```rust
pub trait MaterialGenerator {
    fn submit_generate(
        &mut self,
        request: MaterialGenerationRequest,
    ) -> anyhow::Result<MaterialJobId>;

    fn submit_refine(
        &mut self,
        material: MaterialId,
        feedback: MaterialEvalFeedback,
    ) -> anyhow::Result<MaterialJobId>;

    fn poll_job(&mut self, job: MaterialJobId) -> anyhow::Result<MaterialJobStatus>;

    fn cancel_job(&mut self, job: MaterialJobId) -> anyhow::Result<()>;
}
```

```rust
pub enum MaterialJobStatus {
    Queued,
    Running { progress: f32, stage: MaterialJobStage },
    Complete(GeneratedMaterial),
    Failed(MaterialGenerationError),
    Cancelled,
}
```

```rust
pub struct MaterialGenerationRequest {
    pub name: String,
    pub semantic_class: MaterialClass,
    pub reference_images: Vec<ImageAssetId>,
    pub physical_hints: PhysicalMaterialHints,
    pub target_shader_model: ShaderModelTarget,
    pub max_texture_budget_mb: u32,
    pub allow_runtime_procedural: bool,
    pub seed: u64,
}
```

## Generated output

```rust
pub struct GeneratedMaterial {
    pub material_id: MaterialId,
    pub authoring: MaterialAuthoringDesc,
    pub runtime: RuntimeMaterialDesc,
    pub physical: Option<PhysicalMaterial>,
    pub preview_scenes: Vec<PreviewSceneId>,
    pub confidence: MaterialConfidence,
    pub provenance: MaterialProvenance,
}
```

The renderer then compiles `RuntimeMaterialDesc` into one or more `GpuMaterialPacked` records appropriate for the current GPU tier and quality profile.

## Procedural fields

Procedural fields should support multiple evaluation targets:

```text
- scalar field: roughness, density, thickness
- vector field: normal/displacement direction
- color field: pigmentation, stains, variation
- temporal field: wetness, dirt accumulation, bruising, drying
```

Cache strategy:

```text
Close camera:
  high-resolution generated tiles + procedural detail

Mid distance:
  generated texture cache + reduced procedural evaluation

Far distance:
  averaged material parameters + stochastic variation
```

## Photo-to-material workflow

```text
1. ingest reference photos with provenance
2. estimate lighting/camera conditions if possible
3. remove lighting/shading where feasible
4. infer material parameters and procedural variation
5. generate preview material
6. render standardized preview scenes
7. run visual/physical/performance evaluation
8. bake runtime caches
9. store material manifest
```

## Validation scenes

Every generated material should be previewed under:

```text
- neutral studio lighting
- direct sun
- cloudy sky
- grazing angle
- macro close-up
- mid-distance game camera
- wet/dry variants when applicable
```

## Material evaluation metrics

```text
Visual:
  - photorealism score
  - reference similarity
  - tiling/repetition score
  - normal/displacement plausibility
  - scale plausibility

Physical:
  - parameter bounds
  - energy conservation checks
  - plausible roughness/metallic ranges
  - plausible subsurface/transmission ranges

Performance:
  - texture memory
  - shader instruction count estimate
  - number of texture samples
  - procedural evaluation cost
  - cache build time
```

## GPU ABI validation

Every packed material layout should have tests that verify:

```text
- CPU struct size and alignment
- shader-side offset assumptions
- no non-POD Rust enum/string/Vec in GPU struct
- default/fallback resource indices are valid
- material flags match shader definitions
- padding is explicit and initialized
```

## First material milestone

After sky certification, add a polished ceramic sphere.

Why:

```text
- simple geometry
- clear PBR behavior
- visible roughness/reflection behavior
- useful exposure/shadow test
- no fracture/fluid/human complexity
```

Acceptance:

```text
- no texture required for base case
- generated material has plausible roughness and base color
- render matches reference sphere scenes within rubric
- material shader cost is under budget
```

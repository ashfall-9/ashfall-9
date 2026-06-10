# 13 — Asset and Scene Interchange

## Goal

Use a compact internal runtime scene schema for performance while supporting common interchange formats for assets, materials, and production workflows.

## Strategy

```text
Runtime:
  custom compact schema optimized for frame use

Initial import/export:
  glTF subset for geometry and PBR materials

Material/lookdev interchange:
  MaterialX subset after material graphs exist

Large scene/pipeline interchange:
  OpenUSD bridge after the engine has scene composition needs
```

Do not block the first sky milestone on glTF, MaterialX, or OpenUSD.

## Internal scene object

```rust
pub struct SceneObject {
    pub entity_id: EntityId,
    pub name: String,
    pub transform: Transform,
    pub geometry: GeometryRef,
    pub material: MaterialRef,
    pub physics: Option<PhysicalObjectDesc>,
    pub ai_tags: Vec<AiTag>,
    pub render_flags: RenderFlags,
}
```

## Geometry representations

```text
TriangleMesh:
  general rendering

MeshletMesh:
  GPU culling and LOD later

SignedDistanceField:
  procedural geometry, collision, material fields

TetrahedralMesh:
  deformable/fracture simulation later

VoxelGrid:
  clouds, gases, fluids, destruction volumes

ParticleSet:
  fluids, spray, dust, debris

CurveSet:
  hair, fibers, vegetation later
```

## Asset IDs

```text
- source asset ID: based on imported/generated source
- build artifact ID: based on processed output and settings
- runtime handle: renderer/engine-local handle
```

Do not confuse source assets with runtime GPU handles.

## Asset manifest

```json
{
  "asset_id": "...",
  "source": "generated_or_imported",
  "schema_version": "1.0.0",
  "inputs": [],
  "build_settings": {},
  "outputs": [],
  "provenance": {},
  "quality_report": {}
}
```

## Streaming strategy

```text
Phase 1:
  no streaming required for sky milestone

Phase 2:
  simple asset preload for first material objects

Phase 3:
  async asset loading and generated material caches

Phase 4:
  GPU streaming, descriptor indexing, residency budgets
```

## glTF subset

Initial support:

```text
- meshes
- transforms
- cameras if useful
- PBR metallic/roughness materials
- normal/occlusion/emissive textures
```

Defer:

```text
- full animation complexity
- all extensions
- skinning until human/animation milestone
```

## MaterialX subset

Use after material graphs exist:

```text
- import/generate limited Standard Surface/OpenPBR-like graphs
- map supported nodes to internal MaterialGraph
- bake unsupported nodes to textures or reject with clear diagnostics
```

## OpenUSD bridge

Use after the engine needs large scene composition or production-tool interchange.

Initial bridge goals:

```text
- export internal scenes for inspection
- import basic transforms/meshes/material references
- map render outputs/AOV concepts where useful
- do not make OpenUSD the runtime scene graph
```

## Provenance

Generated and imported assets should store:

```text
- source files or prompts
- reference image/license data
- consent metadata for humans/voices
- generation model/provider version where relevant
- build settings
- evaluator reports
```

# 40 — Shader ABI and GPU Data Layout

## Purpose

A Vulkan renderer can look architecturally clean while silently corrupting shader data. This document defines the CPU-to-GPU data-layout policy.

## Core rule

```text
Authoring data is not GPU ABI data.
Runtime CPU descriptors are not GPU ABI data.
GPU-packed structs are the only structs copied directly into shader-visible buffers.
```

## Three material layers

```text
MaterialAuthoringDesc:
  human/tool-friendly source data, procedural graphs, provenance, references, rich enums

RuntimeMaterialDesc:
  renderer-owned CPU descriptor with validated numeric fields and project handles

GpuMaterialPacked:
  fixed-layout, shader-visible data with integer indices, scalar fields, explicit padding, and ABI version
```

Only the packed layer may be copied into Vulkan buffers.

## Forbidden in GPU-packed structs

Do not put these in shader-visible structs:

```text
- String
- Vec<T>
- Box<T>
- trait objects
- references
- Option<T>
- Result<T, E>
- Rust enums unless manually represented as fixed-width integers
- bool
- usize/isize
- raw pointers
- provider/library types
- non-POD math types with unclear alignment
```

Use fixed-width scalars and explicit wire types.

## Recommended wire types

```rust
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GpuVec2 {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GpuVec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}
```

Use `u32` for table indices and packed flags. Use sentinel values instead of `Option`:

```rust
pub const GPU_INVALID_INDEX: u32 = u32::MAX;
```

## Example packed material

```rust
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GpuMaterialPacked {
    pub abi_version: u32,
    pub flags: u32,
    pub base_color_texture_index: u32,
    pub normal_texture_index: u32,

    pub base_color_factor: GpuVec4,

    pub metallic: f32,
    pub roughness: f32,
    pub transmission: f32,
    pub ior: f32,

    pub procedural_seed_lo: u32,
    pub procedural_seed_hi: u32,
    pub material_class: u32,
    pub _pad0: u32,
}
```

This struct is intentionally boring. Boring GPU ABI is good.

## ABI versioning

Every GPU buffer layout family must have:

```rust
pub const GPU_MATERIAL_ABI_VERSION: u32 = 1;
```

Changing field order, field meaning, alignment, padding, descriptor binding, or shader interpretation requires an ABI version bump and a migration note.

## Reflection checks

Shader lab must verify:

```text
- descriptor set numbers
- descriptor binding numbers
- descriptor array sizes
- push-constant ranges
- storage/uniform buffer struct sizes
- specialization constants
- entry points
```

A mismatch between Rust-side declarations and shader reflection fails `tools_cli -- shader check`.

## Layout tests

Use tests like:

```rust
#[test]
fn gpu_material_layout_is_stable() {
    assert_eq!(std::mem::size_of::<GpuMaterialPacked>(), 64);
    assert_eq!(std::mem::align_of::<GpuMaterialPacked>(), 4);
    // Add offset checks using memoffset or equivalent.
}
```

If using a POD helper crate, derive POD traits only for structs that truly satisfy the layout contract. Do not use derives to paper over uncertain layout.

## Shader language policy

The project should not choose shader language by taste alone. `shader_lab` should run a spike for:

```text
- GLSL -> SPIR-V
- HLSL -> SPIR-V
- WGSL/Naga path if useful for tooling
- hand-written SPIR-V only for tiny tests, not normal development
```

Selection criteria:

```text
- Vulkan feature coverage
- reflection quality
- CI reproducibility
- include/module support
- error message quality
- compatibility with debug/profiler tooling
- ability to express descriptor indexing and future ray tracing paths
```

## Push constants

Push constants are for small, frequently changing data. Do not use them as a workaround for missing buffer layout discipline.

Rules:

```text
- keep push constants small
- version their layout
- test reflection ranges
- do not put variable-length data in push constants
```

## Descriptor indices

Shader-visible references to resources should be integer indices into renderer-owned descriptor tables:

```text
base_color_texture_index: u32
normal_texture_index: u32
sampler_index: u32
material_parameter_index: u32
```

The descriptor table owner is responsible for:

```text
- descriptor epoch
- resource lifetime
- pending-frame safety
- fallback/default resources
- invalid-index handling
```

## Color and unit conventions

Shader ABI fields must state units and color space:

```text
- colors in linear space unless explicitly marked otherwise
- distances in meters
- angles in radians
- radiance/luminance/exposure fields documented by physical or engine units
- texture indices point to resources with declared color-space metadata
```

## Merge gate

A change to GPU ABI must include:

```text
- ABI version note
- Rust layout test update
- shader reflection test update
- affected shader list
- artifact/eval rerun if visual output changes
- migration note for old cached artifacts/assets if needed
```

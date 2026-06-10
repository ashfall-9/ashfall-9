# 06 — Physics and Physical-Consequence Engine

## Goal

The long-term goal is a physical-consequence engine, not a simple rigid-body system with visual effects. The system should eventually answer:

```text
- what is this object made of?
- how does it deform?
- when does it fracture?
- what fragments are created?
- what new internal surfaces appear?
- do fluids or gases escape?
- how does the renderer show the consequence?
- how do AI characters perceive and react to it?
```

## Sanity constraint

Do not implement this as “arbitrary physically correct destruction.” That is too broad. Implement one material family and one interaction type at a time.

Recommended staging:

```text
1. rigid-body baseline for one simple object
2. ceramic fracture under impact
3. fracture interior material generation
4. wood splintering approximation
5. metal denting/plastic deformation
6. single liquid emission
7. gas/smoke emission
8. coupled fracture + liquid/gas
```

## Physics object descriptor

```rust
pub struct PhysicalObjectDesc {
    pub object_id: SimObjectId,
    pub entity_id: EntityId,
    pub transform: Transform,
    pub geometry: CollisionGeometry,
    pub material: PhysicalMaterial,
    pub simulation_mode: SimulationMode,
    pub fracture: Option<FractureModel>,
    pub fluid: Option<FluidModel>,
    pub gas: Option<GasModel>,
}
```

## Physical material

```rust
pub struct PhysicalMaterial {
    pub density_kg_m3: f32,
    pub youngs_modulus_pa: Option<f32>,
    pub poisson_ratio: Option<f32>,
    pub yield_strength_pa: Option<f32>,
    pub fracture_toughness: Option<f32>,
    pub restitution: f32,
    pub friction_static: f32,
    pub friction_dynamic: f32,
    pub viscosity_pa_s: Option<f32>,
    pub thermal: Option<ThermalParams>,
    pub acoustic: Option<AcousticParams>,
}
```

Use `Option` for parameters that do not apply to every material class. Unknown physical data should be represented explicitly rather than faked silently.

## Solver architecture

```rust
pub trait PhysicsSolver {
    fn solver_id(&self) -> &'static str;
    fn supports(&self, object: &PhysicalObjectDesc) -> bool;

    fn step(
        &mut self,
        dt_s: f32,
        world: &PhysicsWorldView,
        events: &[PhysicsEvent],
    ) -> anyhow::Result<PhysicsDelta>;

    fn telemetry(&self) -> SolverTelemetry;
}
```

Solver families:

```text
Rigid solver:
  broadphase, narrowphase, contacts, constraints, sleeping, islands

Deformable solver:
  limited FEM/XPBD-style deformation for specific classes

Fracture solver:
  damage accumulation, crack/fracture event creation, procedural fragments

Fluid solver:
  particle/grid hybrid depending on scale; surface reconstruction for visible liquid

Gas solver:
  voxel/grid advection, buoyancy, density/temperature fields

Coupling layer:
  event-based links between solvers and renderer/AI/audio
```

## Consequence event model

```rust
pub enum PhysicsEvent {
    Impact {
        a: SimObjectId,
        b: SimObjectId,
        point: Vec3,
        normal: Vec3,
        impulse_n_s: f32,
        relative_velocity_mps: Vec3,
    },
    MaterialDamage {
        object: SimObjectId,
        damage_region: Bounds3,
        damage_value: f32,
    },
    Fracture {
        source: SimObjectId,
        fragments: Vec<FragmentDesc>,
        energy_absorbed_j: f32,
    },
    FluidEmission {
        source: SimObjectId,
        volume_m3: f32,
        fluid: FluidId,
        initial_velocity_mps: Vec3,
    },
    GasEmission {
        source: SimObjectId,
        mass_kg: f32,
        gas: GasId,
        temperature_k: f32,
    },
}
```

The renderer consumes events to create/update render geometry, show interior surfaces, spawn visible fluids/gases, and update acceleration structures where needed.

## Simulation LOD

```text
LOD 0 — full simulation:
  close, player-critical, currently interacting

LOD 1 — simplified simulation:
  nearby and gameplay-relevant

LOD 2 — procedural/cached consequence:
  mid-distance, plausible events, fewer solver details

LOD 3 — visual proxy:
  far distance, renderer-only update

LOD 4 — statistical consequence:
  world state and AI memory only
```

Every LOD emits the same event types. Consumers should not care whether a fracture came from a detailed solver or a cached pattern.

## Renderer coupling

Renderer-visible outputs:

```text
- updated transforms
- new fragment geometry
- interior material descriptors
- debris particle descriptors
- liquid surface descriptors
- gas volume descriptors
- decal/material-state changes
```

Do not let physics directly record render commands. It emits data and events.

## Determinism and replay

Physics should support deterministic replay for evaluation scenes where possible:

```text
- fixed timestep for test runs
- fixed seeds
- recorded input events
- stable artifact output
- versioned solver settings
```

GPU solvers may be nondeterministic across vendors. Store tolerances and hardware metadata in artifacts.

## First physics milestone after sky

A good first physics object is a rigid ceramic sphere or block:

```text
- one mesh
- one material
- one collision shape
- no fracture yet
- stable transform and collision telemetry
```

Only after that should ceramic fracture be attempted.

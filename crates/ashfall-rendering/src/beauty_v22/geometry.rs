//! Shared V22 geometry contracts.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2V22 {
    pub x: f32,
    pub y: f32,
}

impl Vec2V22 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3V22 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3V22 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn distance_xy(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds3V22 {
    pub min: Vec3V22,
    pub max: Vec3V22,
}

impl Bounds3V22 {
    pub fn new(min: Vec3V22, max: Vec3V22) -> Self {
        Self { min, max }
    }

    pub fn is_plausible(self) -> bool {
        self.max.x > self.min.x && self.max.y > self.min.y && self.max.z >= self.min.z
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautySurfaceIdV22(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautyMaterialIdV22(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautyMeshAssetIdV22(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeshSourceKindV22 {
    GeneratedRoadStrip,
    GeneratedTerrainPatch,
    GeneratedCurb,
    GeneratedFacade,
    GeneratedCurveTube,
    GeneratedPlantCluster,
    GeneratedStoneCluster,
    GeneratedLandfillProp,
    CoherentHumanProxy,
    CoherentVehicleProxy,
    ImportedAsset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VisualMeshRefV22 {
    pub mesh_id: BeautyMeshAssetIdV22,
    pub source_kind: MeshSourceKindV22,
    pub coherent_single_mesh: bool,
}

impl VisualMeshRefV22 {
    pub const fn generated(mesh_id: u64, source_kind: MeshSourceKindV22) -> Self {
        Self {
            mesh_id: BeautyMeshAssetIdV22(mesh_id),
            source_kind,
            coherent_single_mesh: true,
        }
    }

    pub fn visually_valid(self) -> bool {
        self.mesh_id.0 != 0 && self.coherent_single_mesh
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadPatchV22 {
    pub id: u64,
    pub centerline: Vec<Vec3V22>,
    pub width_meters: f32,
    pub unevenness_0_to_1: f32,
    pub camber_0_to_1: f32,
    pub mesh: VisualMeshRefV22,
    pub surface: BeautySurfaceIdV22,
    pub material: BeautyMaterialIdV22,
}

impl RoadPatchV22 {
    pub fn visually_valid(&self) -> bool {
        self.centerline.len() >= 2
            && self.width_meters >= 2.0
            && self.unevenness_0_to_1 >= 0.02
            && self.mesh.visually_valid()
            && self.surface.0 != 0
            && self.material.0 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPatchV22 {
    pub id: u64,
    pub bounds: Bounds3V22,
    pub height_variation_meters: f32,
    pub mesh: VisualMeshRefV22,
    pub soil_surface: BeautySurfaceIdV22,
    pub soil_material: BeautyMaterialIdV22,
    pub stone_density_0_to_1: f32,
    pub plant_density_0_to_1: f32,
}

impl TerrainPatchV22 {
    pub fn visually_valid(&self) -> bool {
        self.bounds.is_plausible()
            && self.height_variation_meters >= 0.05
            && self.mesh.visually_valid()
            && self.soil_surface.0 != 0
            && self.soil_material.0 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurbSegmentV22 {
    pub id: u64,
    pub start: Vec3V22,
    pub end: Vec3V22,
    pub height_meters: f32,
    pub bevel_radius_meters: f32,
    pub chip_density_0_to_1: f32,
    pub mesh: VisualMeshRefV22,
    pub surface: BeautySurfaceIdV22,
    pub material: BeautyMaterialIdV22,
}

impl CurbSegmentV22 {
    pub fn visually_valid(&self) -> bool {
        self.start.distance_xy(self.end) > 0.3
            && self.bevel_radius_meters > 0.006
            && self.mesh.visually_valid()
            && self.surface.0 != 0
            && self.material.0 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FacadeModuleV22 {
    pub id: u64,
    pub bounds: Bounds3V22,
    pub inset_depth_meters: f32,
    pub bevel_radius_meters: f32,
    pub window_count: u16,
    pub door_count: u16,
    pub pipe_count: u16,
    pub dirt_0_to_1: f32,
    pub mesh: VisualMeshRefV22,
    pub surface: BeautySurfaceIdV22,
    pub material: BeautyMaterialIdV22,
}

impl FacadeModuleV22 {
    pub fn visually_valid(&self) -> bool {
        self.bounds.is_plausible()
            && self.inset_depth_meters >= 0.04
            && self.bevel_radius_meters >= 0.006
            && (self.window_count > 0 || self.door_count > 0 || self.pipe_count > 0)
            && self.mesh.visually_valid()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CurveObjectKindV22 {
    Pipe,
    Cable,
    Root,
    Hose,
    FenceWire,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveObjectV22 {
    pub id: u64,
    pub kind: CurveObjectKindV22,
    pub points: Vec<Vec3V22>,
    pub radius_meters: f32,
    pub sag_0_to_1: f32,
    pub mesh: VisualMeshRefV22,
    pub surface: BeautySurfaceIdV22,
    pub material: BeautyMaterialIdV22,
}

impl CurveObjectV22 {
    pub fn visually_valid(&self) -> bool {
        self.points.len() >= 2
            && self.radius_meters > 0.005
            && self.mesh.visually_valid()
            && self.surface.0 != 0
            && self.material.0 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroundedWaterFilmV22 {
    pub id: u64,
    pub receiver_surface: BeautySurfaceIdV22,
    pub polygon: Vec<Vec3V22>,
    pub normal: Vec3V22,
    pub depth_meters: f32,
    pub edge_softness_0_to_1: f32,
    pub roughness_0_to_1: f32,
    pub screen_space_or_billboard: bool,
}

impl GroundedWaterFilmV22 {
    pub fn visually_valid(&self) -> bool {
        self.receiver_surface.0 != 0
            && self.polygon.len() >= 3
            && self.normal.z > 0.35
            && self.depth_meters >= 0.0
            && self.depth_meters <= 0.08
            && !self.screen_space_or_billboard
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlantInstanceV22 {
    pub id: u64,
    pub position: Vec3V22,
    pub height_meters: f32,
    pub radius_meters: f32,
    pub cluster_mesh: VisualMeshRefV22,
    pub leaf_surface: BeautySurfaceIdV22,
    pub leaf_material: BeautyMaterialIdV22,
    pub wind_response_0_to_1: f32,
    pub clump_seed: u64,
}

impl PlantInstanceV22 {
    pub fn visually_valid(&self) -> bool {
        self.height_meters > 0.03
            && self.radius_meters > 0.005
            && self.cluster_mesh.visually_valid()
            && self.leaf_surface.0 != 0
            && self.leaf_material.0 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoneInstanceV22 {
    pub id: u64,
    pub position: Vec3V22,
    pub radius_meters: f32,
    pub irregularity_0_to_1: f32,
    pub mesh: VisualMeshRefV22,
    pub surface: BeautySurfaceIdV22,
    pub material: BeautyMaterialIdV22,
}

impl StoneInstanceV22 {
    pub fn visually_valid(&self) -> bool {
        self.radius_meters > 0.015
            && self.irregularity_0_to_1 > 0.10
            && self.mesh.visually_valid()
            && self.surface.0 != 0
            && self.material.0 != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LandfillPropKindV22 {
    PlasticSheet,
    Cardboard,
    RustedMetalPanel,
    BrokenGlass,
    FabricBundle,
    Tire,
    Crate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LandfillPropV22 {
    pub id: u64,
    pub kind: LandfillPropKindV22,
    pub position: Vec3V22,
    pub bounds: Bounds3V22,
    pub deformation_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub mesh: VisualMeshRefV22,
    pub surface: BeautySurfaceIdV22,
    pub material: BeautyMaterialIdV22,
}

impl LandfillPropV22 {
    pub fn visually_valid(&self) -> bool {
        self.bounds.is_plausible()
            && self.deformation_0_to_1 > 0.05
            && self.mesh.visually_valid()
            && self.surface.0 != 0
            && self.material.0 != 0
    }
}

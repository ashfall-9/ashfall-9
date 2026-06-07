//! Shared V21 geometry contracts.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2V21 {
    pub x: f32,
    pub y: f32,
}

impl Vec2V21 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3V21 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3V21 {
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
pub struct Bounds3V21 {
    pub min: Vec3V21,
    pub max: Vec3V21,
}

impl Bounds3V21 {
    pub fn new(min: Vec3V21, max: Vec3V21) -> Self {
        Self { min, max }
    }
    pub fn center(self) -> Vec3V21 {
        Vec3V21::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }
    pub fn is_plausible(self) -> bool {
        self.max.x > self.min.x && self.max.y > self.min.y && self.max.z >= self.min.z
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautySurfaceIdV21(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautyMaterialIdV21(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct RoadPatchV21 {
    pub id: u64,
    pub centerline: Vec<Vec3V21>,
    pub width_meters: f32,
    pub unevenness_0_to_1: f32,
    pub camber_0_to_1: f32,
    pub surface: BeautySurfaceIdV21,
    pub material: BeautyMaterialIdV21,
}

impl RoadPatchV21 {
    pub fn visually_valid(&self) -> bool {
        self.centerline.len() >= 2
            && self.width_meters >= 2.0
            && self.surface.0 != 0
            && self.material.0 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPatchV21 {
    pub id: u64,
    pub bounds: Bounds3V21,
    pub height_variation_meters: f32,
    pub soil_surface: BeautySurfaceIdV21,
    pub soil_material: BeautyMaterialIdV21,
    pub stone_density_0_to_1: f32,
    pub plant_density_0_to_1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurbSegmentV21 {
    pub id: u64,
    pub start: Vec3V21,
    pub end: Vec3V21,
    pub height_meters: f32,
    pub bevel_radius_meters: f32,
    pub chip_density_0_to_1: f32,
    pub surface: BeautySurfaceIdV21,
    pub material: BeautyMaterialIdV21,
}

impl CurbSegmentV21 {
    pub fn visually_valid(&self) -> bool {
        self.start.distance_xy(self.end) > 0.3
            && self.bevel_radius_meters > 0.005
            && self.surface.0 != 0
            && self.material.0 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FacadeModuleV21 {
    pub id: u64,
    pub bounds: Bounds3V21,
    pub inset_depth_meters: f32,
    pub bevel_radius_meters: f32,
    pub window_count: u16,
    pub door_count: u16,
    pub pipe_count: u16,
    pub dirt_0_to_1: f32,
    pub surface: BeautySurfaceIdV21,
    pub material: BeautyMaterialIdV21,
}

impl FacadeModuleV21 {
    pub fn visually_valid(&self) -> bool {
        self.bounds.is_plausible()
            && self.inset_depth_meters >= 0.03
            && self.bevel_radius_meters >= 0.004
            && (self.window_count > 0 || self.door_count > 0 || self.pipe_count > 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CurveObjectKindV21 {
    Pipe,
    Cable,
    Root,
    Hose,
    FenceWire,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveObjectV21 {
    pub id: u64,
    pub kind: CurveObjectKindV21,
    pub points: Vec<Vec3V21>,
    pub radius_meters: f32,
    pub sag_0_to_1: f32,
    pub surface: BeautySurfaceIdV21,
    pub material: BeautyMaterialIdV21,
}

impl CurveObjectV21 {
    pub fn visually_valid(&self) -> bool {
        self.points.len() >= 2
            && self.radius_meters > 0.005
            && self.surface.0 != 0
            && self.material.0 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroundedWaterFilmV21 {
    pub id: u64,
    pub receiver_surface: BeautySurfaceIdV21,
    pub polygon: Vec<Vec3V21>,
    pub normal: Vec3V21,
    pub depth_meters: f32,
    pub edge_softness_0_to_1: f32,
    pub roughness_0_to_1: f32,
}

impl GroundedWaterFilmV21 {
    pub fn visually_valid(&self) -> bool {
        self.receiver_surface.0 != 0
            && self.polygon.len() >= 3
            && self.normal.z > 0.35
            && self.depth_meters >= 0.0
            && self.depth_meters <= 0.08
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlantInstanceV21 {
    pub id: u64,
    pub position: Vec3V21,
    pub height_meters: f32,
    pub radius_meters: f32,
    pub leaf_surface: BeautySurfaceIdV21,
    pub leaf_material: BeautyMaterialIdV21,
    pub wind_response_0_to_1: f32,
    pub clump_seed: u64,
}

impl PlantInstanceV21 {
    pub fn visually_valid(&self) -> bool {
        self.height_meters > 0.03
            && self.radius_meters > 0.005
            && self.leaf_surface.0 != 0
            && self.leaf_material.0 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoneInstanceV21 {
    pub id: u64,
    pub position: Vec3V21,
    pub radius_meters: f32,
    pub irregularity_0_to_1: f32,
    pub surface: BeautySurfaceIdV21,
    pub material: BeautyMaterialIdV21,
}

impl StoneInstanceV21 {
    pub fn visually_valid(&self) -> bool {
        self.radius_meters > 0.015
            && self.irregularity_0_to_1 > 0.10
            && self.surface.0 != 0
            && self.material.0 != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LandfillPropKindV21 {
    PlasticSheet,
    Cardboard,
    RustedMetalPanel,
    BrokenGlass,
    FabricBundle,
    Tire,
    Crate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LandfillPropV21 {
    pub id: u64,
    pub kind: LandfillPropKindV21,
    pub position: Vec3V21,
    pub bounds: Bounds3V21,
    pub deformation_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub surface: BeautySurfaceIdV21,
    pub material: BeautyMaterialIdV21,
}

impl LandfillPropV21 {
    pub fn visually_valid(&self) -> bool {
        self.bounds.is_plausible()
            && self.deformation_0_to_1 > 0.05
            && self.surface.0 != 0
            && self.material.0 != 0
    }
}

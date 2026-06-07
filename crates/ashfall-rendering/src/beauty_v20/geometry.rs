//! World-space geometry contracts for non-blocky Beauty Mode.

use super::material_pages::{BeautyMaterialIdV20, BeautySurfaceIdV20};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3V20 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3V20 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundsV20 {
    pub min: Vec3V20,
    pub max: Vec3V20,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadPatchV20 {
    pub surface_id: BeautySurfaceIdV20,
    pub material_id: BeautyMaterialIdV20,
    pub control_points: Vec<Vec3V20>,
    pub width_meters: f32,
    pub camber_0_to_1: f32,
    pub unevenness_0_to_1: f32,
    pub crack_density_0_to_1: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurbSegmentV20 {
    pub surface_id: BeautySurfaceIdV20,
    pub material_id: BeautyMaterialIdV20,
    pub start: Vec3V20,
    pub end: Vec3V20,
    pub radius_meters: f32,
    pub chip_density_0_to_1: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FacadeModuleV20 {
    pub surface_id: BeautySurfaceIdV20,
    pub material_id: BeautyMaterialIdV20,
    pub origin: Vec3V20,
    pub width_meters: f32,
    pub height_meters: f32,
    pub depth_meters: f32,
    pub bevel_radius_meters: f32,
    pub inset_window_count: u32,
    pub grime_0_to_1: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CurveObjectKindV20 {
    Pipe,
    Cable,
    Root,
    Hose,
    Rail,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveObjectV20 {
    pub surface_id: BeautySurfaceIdV20,
    pub material_id: BeautyMaterialIdV20,
    pub kind: CurveObjectKindV20,
    pub points: Vec<Vec3V20>,
    pub radius_meters: f32,
    pub sag_0_to_1: f32,
    pub dirt_0_to_1: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainPatchV20 {
    pub surface_id: BeautySurfaceIdV20,
    pub material_id: BeautyMaterialIdV20,
    pub center: Vec3V20,
    pub size_meters: [f32; 2],
    pub unevenness_0_to_1: f32,
    pub moisture_0_to_1: f32,
    pub vegetation_coverage_0_to_1: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlantInstanceV20 {
    pub surface_id: BeautySurfaceIdV20,
    pub material_id: BeautyMaterialIdV20,
    pub root_position: Vec3V20,
    pub height_meters: f32,
    pub radius_meters: f32,
    pub bend_0_to_1: f32,
    pub leaf_density_0_to_1: f32,
    pub variation_seed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoneInstanceV20 {
    pub surface_id: BeautySurfaceIdV20,
    pub material_id: BeautyMaterialIdV20,
    pub center: Vec3V20,
    pub radius_meters: f32,
    pub angularity_0_to_1: f32,
    pub chip_density_0_to_1: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandfillPropV20 {
    pub surface_id: BeautySurfaceIdV20,
    pub material_id: BeautyMaterialIdV20,
    pub center: Vec3V20,
    pub size_meters: [f32; 3],
    pub deformation_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub rust_or_stain_0_to_1: f32,
    pub sharp_edges_0_to_1: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundedWaterFilmV20 {
    pub surface_id: BeautySurfaceIdV20,
    pub receiver_surface_id: BeautySurfaceIdV20,
    pub center: Vec3V20,
    pub normal: Vec3V20,
    pub radius_meters: f32,
    pub depth_meters: f32,
    pub edge_irregularity_0_to_1: f32,
    pub reflection_quality_0_to_1: f32,
}

impl GroundedWaterFilmV20 {
    pub fn is_grounded(&self) -> bool {
        self.radius_meters > 0.02
            && self.depth_meters >= 0.0
            && self.normal.z.abs() >= 0.5
            && self.surface_id != self.receiver_surface_id
    }
}

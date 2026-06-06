//! Beauty geometry primitives.
//!
//! These records describe what Beauty Mode is allowed to draw. They are not raw
//! mesh data; they are contracts that builders and GPU services can translate
//! into retained buffers, mesh clusters, curve meshes, or procedural pages.

use super::irregularity::{BeautyObjectClassV15, IrregularityRecipeV15};
use super::material_pages::BeautyMaterialIdV15;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautySurfaceIdV15(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautyObjectIdV15(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautyBoundsV15 {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl BeautyBoundsV15 {
    pub fn center(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadStripV15 {
    pub surface_id: BeautySurfaceIdV15,
    pub centerline: Vec<[f32; 3]>,
    pub width_meters: f32,
    pub crown_height_meters: f32,
    pub edge_noise_meters: f32,
    pub material_id: BeautyMaterialIdV15,
    pub irregularity: IrregularityRecipeV15,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurbSegmentV15 {
    pub object_id: BeautyObjectIdV15,
    pub path: Vec<[f32; 3]>,
    pub height_meters: f32,
    pub width_meters: f32,
    pub bevel_radius_meters: f32,
    pub material_id: BeautyMaterialIdV15,
    pub irregularity: IrregularityRecipeV15,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FacadeModuleV15 {
    pub object_id: BeautyObjectIdV15,
    pub bounds: BeautyBoundsV15,
    pub floors: u32,
    pub facade_depth_meters: f32,
    pub bevel_radius_meters: f32,
    pub material_id: BeautyMaterialIdV15,
    pub irregularity: IrregularityRecipeV15,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveObjectV15 {
    pub object_id: BeautyObjectIdV15,
    pub object_class: BeautyObjectClassV15,
    pub points: Vec<[f32; 3]>,
    pub radius_meters: f32,
    pub material_id: BeautyMaterialIdV15,
    pub irregularity: IrregularityRecipeV15,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScatterFieldV15 {
    pub field_id: u64,
    pub bounds: BeautyBoundsV15,
    pub object_class: BeautyObjectClassV15,
    pub density_per_square_meter: f32,
    pub min_radius_meters: f32,
    pub max_radius_meters: f32,
    pub deterministic_seed: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnchoredPuddleV15 {
    pub object_id: BeautyObjectIdV15,
    pub receiver_surface_id: BeautySurfaceIdV15,
    pub center_world: [f32; 3],
    pub normal_world: [f32; 3],
    pub radius_x_meters: f32,
    pub radius_y_meters: f32,
    pub water_depth_meters: f32,
    pub edge_feather_meters: f32,
    pub z_bias_meters: f32,
    pub material_id: BeautyMaterialIdV15,
}

impl AnchoredPuddleV15 {
    pub fn is_ground_anchored(&self) -> bool {
        self.receiver_surface_id.0 != 0
            && self.water_depth_meters >= 0.0
            && self.z_bias_meters.abs() <= 0.02
            && self.normal_world[2] > 0.35
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanProxyV15 {
    pub object_id: BeautyObjectIdV15,
    pub root_position: [f32; 3],
    pub height_meters: f32,
    pub shoulder_width_meters: f32,
    pub hip_width_meters: f32,
    pub head_radius_meters: f32,
    pub clothing_material_id: BeautyMaterialIdV15,
    pub skin_material_id: BeautyMaterialIdV15,
    pub irregularity: IrregularityRecipeV15,
}

impl HumanProxyV15 {
    pub fn is_proportionate_non_rod(&self) -> bool {
        self.height_meters >= 1.35
            && self.height_meters <= 2.15
            && self.shoulder_width_meters >= 0.28
            && self.hip_width_meters >= 0.22
            && self.head_radius_meters >= 0.08
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VehicleProxyV15 {
    pub object_id: BeautyObjectIdV15,
    pub bounds: BeautyBoundsV15,
    pub wheel_count: u8,
    pub cabin_glass_ratio: f32,
    pub panel_seam_density: f32,
    pub body_material_id: BeautyMaterialIdV15,
    pub glass_material_id: BeautyMaterialIdV15,
    pub rubber_material_id: BeautyMaterialIdV15,
    pub irregularity: IrregularityRecipeV15,
}

impl VehicleProxyV15 {
    pub fn is_proportionate_non_box(&self) -> bool {
        let sx = self.bounds.max[0] - self.bounds.min[0];
        let sy = self.bounds.max[1] - self.bounds.min[1];
        let sz = self.bounds.max[2] - self.bounds.min[2];
        sx >= 2.2
            && sy >= 1.1
            && sz >= 1.0
            && self.wheel_count >= 2
            && self.cabin_glass_ratio > 0.05
    }
}

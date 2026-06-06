//! World-generation side of the Beauty Mode pivot.
//!
//! Streaming cells and chunks remain useful for simulation and diagnostics. The
//! player-facing city should resolve them into beauty packages containing roads,
//! curbs, facades, props, proxies, material requests, scatter, and lights.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BeautyAssetKindV1 {
    TerrainPatch,
    RoadSpline,
    CurbProfile,
    BuildingFacade,
    RoofDetail,
    Door,
    WindowFrame,
    PipeCurve,
    CableCurve,
    PropMesh,
    HumanProxy,
    VehicleProxy,
    DecalField,
    ScatterField,
    LocalLight,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautyBoundsV1 {
    pub center_meters: [f32; 3],
    pub half_extents_meters: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautyInstanceV1 {
    pub instance_id: u64,
    pub asset_kind: BeautyAssetKindV1,
    pub asset_id: u64,
    pub material_id: u64,
    pub transform_translation_meters: [f32; 3],
    pub transform_rotation_yaw_radians: f32,
    pub transform_scale: [f32; 3],
    pub bounds: BeautyBoundsV1,
    pub irregularity_seed: u64,
    pub lod_importance_0_to_1: f32,
    pub story_importance_0_to_1: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoadSplinePointV1 {
    pub position_meters: [f32; 3],
    pub width_meters: f32,
    pub crown_height_meters: f32,
    pub pothole_bias_0_to_1: f32,
    pub wetness_0_to_1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadSplineV1 {
    pub road_id: u64,
    pub points: Vec<RoadSplinePointV1>,
    pub material_id: u64,
    pub curb_material_id: u64,
    pub irregularity_seed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScatterFieldV1 {
    pub field_id: u64,
    pub bounds: BeautyBoundsV1,
    pub seed: u64,
    pub density_0_to_1: f32,
    pub trash_bias_0_to_1: f32,
    pub stone_bias_0_to_1: f32,
    pub paper_bias_0_to_1: f32,
    pub cable_bias_0_to_1: f32,
    pub max_visible_instances: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialPageRequestV1 {
    pub request_id: u64,
    pub material_id: u64,
    pub bounds: BeautyBoundsV1,
    pub desired_resolution_log2: u8,
    pub require_normal: bool,
    pub require_height: bool,
    pub require_roughness: bool,
    pub require_dirt: bool,
    pub require_wetness: bool,
    pub require_cracks: bool,
    pub require_soot_or_oil: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautyLocalLightV1 {
    pub light_id: u64,
    pub position_meters: [f32; 3],
    pub color_linear: [f32; 3],
    pub intensity_lumens: f32,
    pub radius_meters: f32,
    pub is_neon_accent: bool,
    pub casts_shadow: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CityCellBeautyPackageV1 {
    pub cell_id: u64,
    pub bounds: BeautyBoundsV1,
    pub quality_seed: u64,
    pub roads: Vec<RoadSplineV1>,
    pub instances: Vec<BeautyInstanceV1>,
    pub scatter_fields: Vec<ScatterFieldV1>,
    pub material_page_requests: Vec<MaterialPageRequestV1>,
    pub local_lights: Vec<BeautyLocalLightV1>,
    pub expected_triangle_budget_near: u32,
    pub expected_instance_budget_near: u32,
    pub expected_material_page_budget: u32,
}

impl CityCellBeautyPackageV1 {
    pub fn empty(cell_id: u64, center_meters: [f32; 3], half_extents_meters: [f32; 3]) -> Self {
        Self {
            cell_id,
            bounds: BeautyBoundsV1 {
                center_meters,
                half_extents_meters,
            },
            quality_seed: cell_id ^ 0xA5A5_9E37_79B9_7F4A,
            roads: Vec::new(),
            instances: Vec::new(),
            scatter_fields: Vec::new(),
            material_page_requests: Vec::new(),
            local_lights: Vec::new(),
            expected_triangle_budget_near: 0,
            expected_instance_budget_near: 0,
            expected_material_page_budget: 0,
        }
    }

    pub fn has_player_facing_content(&self) -> bool {
        !self.roads.is_empty()
            || self.instances.iter().any(|instance| {
                !matches!(
                    instance.asset_kind,
                    BeautyAssetKindV1::ScatterField | BeautyAssetKindV1::DecalField
                )
            })
    }
}

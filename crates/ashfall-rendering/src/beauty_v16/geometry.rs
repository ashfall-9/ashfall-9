//! Beauty geometry records.

use super::irregularity::IrregularityRecipeV16;
use super::material_pages::{BeautyMaterialIdV16, BeautySurfaceIdV16};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BeautyObjectIdV16(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautyBoundsV16 {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl BeautyBoundsV16 {
    pub fn non_degenerate(&self) -> bool {
        self.max[0] > self.min[0] && self.max[1] > self.min[1] && self.max[2] > self.min[2]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadSplineV16 {
    pub surface_id: BeautySurfaceIdV16,
    pub centerline_world: Vec<[f32; 3]>,
    pub width_meters: f32,
    pub crown_height_meters: f32,
    pub edge_noise_meters: f32,
    pub material_id: BeautyMaterialIdV16,
    pub irregularity: IrregularityRecipeV16,
}

impl RoadSplineV16 {
    pub fn is_real_road(&self) -> bool {
        self.centerline_world.len() >= 3
            && self.width_meters >= 2.5
            && self.edge_noise_meters > 0.01
            && self.irregularity.bevel_radius_meters_max > 0.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurbSegmentV16 {
    pub object_id: BeautyObjectIdV16,
    pub start_world: [f32; 3],
    pub end_world: [f32; 3],
    pub height_meters: f32,
    pub bevel_radius_meters: f32,
    pub chip_density_0_to_1: f32,
    pub material_id: BeautyMaterialIdV16,
    pub irregularity: IrregularityRecipeV16,
}

impl CurbSegmentV16 {
    pub fn is_rounded(&self) -> bool {
        self.height_meters > 0.05 && self.bevel_radius_meters > 0.01
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FacadeModuleV16 {
    pub object_id: BeautyObjectIdV16,
    pub bounds: BeautyBoundsV16,
    pub material_id: BeautyMaterialIdV16,
    pub window_count: u16,
    pub door_count: u16,
    pub vent_count: u16,
    pub inset_depth_meters: f32,
    pub bevel_radius_meters: f32,
    pub dirt_0_to_1: f32,
    pub irregularity: IrregularityRecipeV16,
}

impl FacadeModuleV16 {
    pub fn is_not_lego_block(&self) -> bool {
        self.bounds.non_degenerate()
            && self.bevel_radius_meters > 0.01
            && self.inset_depth_meters > 0.02
            && (self.window_count > 0 || self.door_count > 0 || self.vent_count > 0)
            && self.dirt_0_to_1 > 0.05
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveObjectV16 {
    pub object_id: BeautyObjectIdV16,
    pub points_world: Vec<[f32; 3]>,
    pub radius_meters: f32,
    pub material_id: BeautyMaterialIdV16,
    pub sag_meters: f32,
}

impl CurveObjectV16 {
    pub fn is_pipe_or_cable(&self) -> bool {
        self.points_world.len() >= 2 && self.radius_meters > 0.005
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScatterFieldV16 {
    pub object_id: BeautyObjectIdV16,
    pub bounds: BeautyBoundsV16,
    pub density_0_to_1: f32,
    pub item_count_budget: u16,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnchoredPuddleV16 {
    pub object_id: BeautyObjectIdV16,
    pub receiver_surface_id: BeautySurfaceIdV16,
    pub center_world: [f32; 3],
    pub receiver_normal_world: [f32; 3],
    pub radius_meters: f32,
    pub max_depth_meters: f32,
    pub edge_softness_meters: f32,
    pub material_id: BeautyMaterialIdV16,
}

impl AnchoredPuddleV16 {
    pub fn is_ground_anchored(&self) -> bool {
        self.receiver_surface_id.0 != 0
            && self.center_world[2].is_finite()
            && self.receiver_normal_world[2] > 0.65
            && self.radius_meters > 0.05
            && self.max_depth_meters > 0.001
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanProxyV16 {
    pub entity_id: u64,
    pub position_world: [f32; 3],
    pub height_meters: f32,
    pub shoulder_width_meters: f32,
    pub hip_width_meters: f32,
    pub head_radius_meters: f32,
    pub has_torso: bool,
    pub has_pelvis: bool,
    pub has_arms: bool,
    pub has_hands: bool,
    pub has_legs: bool,
    pub has_feet: bool,
    pub has_clothing_silhouette: bool,
    pub has_hair_or_head_covering: bool,
    pub skin_material_id: BeautyMaterialIdV16,
    pub clothing_material_id: BeautyMaterialIdV16,
    pub hair_material_id: BeautyMaterialIdV16,
    pub animation_phase_0_to_1: f32,
    pub breathing_weight_0_to_1: f32,
    pub lod_identity_locked: bool,
    pub irregularity: IrregularityRecipeV16,
}

impl HumanProxyV16 {
    pub fn default_adult(entity_id: u64, position_world: [f32; 3], seed: u64) -> Self {
        Self {
            entity_id,
            position_world,
            height_meters: 1.74,
            shoulder_width_meters: 0.46,
            hip_width_meters: 0.34,
            head_radius_meters: 0.105,
            has_torso: true,
            has_pelvis: true,
            has_arms: true,
            has_hands: true,
            has_legs: true,
            has_feet: true,
            has_clothing_silhouette: true,
            has_hair_or_head_covering: true,
            skin_material_id: BeautyMaterialIdV16(0x5151_5151),
            clothing_material_id: BeautyMaterialIdV16(0x000C_107A),
            hair_material_id: BeautyMaterialIdV16(0x0A11_A111),
            animation_phase_0_to_1: 0.0,
            breathing_weight_0_to_1: 0.35,
            lod_identity_locked: true,
            irregularity: IrregularityRecipeV16::human(seed),
        }
    }

    pub fn is_proportionate_non_rod(&self) -> bool {
        self.height_meters >= 1.45
            && self.height_meters <= 2.10
            && self.shoulder_width_meters >= 0.32
            && self.hip_width_meters >= 0.25
            && self.head_radius_meters >= 0.07
            && self.has_torso
            && self.has_pelvis
            && self.has_arms
            && self.has_hands
            && self.has_legs
            && self.has_feet
            && self.has_clothing_silhouette
            && self.lod_identity_locked
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VehicleProxyV16 {
    pub entity_id: u64,
    pub position_world: [f32; 3],
    pub length_meters: f32,
    pub width_meters: f32,
    pub height_meters: f32,
    pub wheel_radius_meters: f32,
    pub has_cabin: bool,
    pub has_wheels_or_hover_equivalent: bool,
    pub has_windshield: bool,
    pub has_headlights: bool,
    pub has_taillights: bool,
    pub has_panel_seams: bool,
    pub body_material_id: BeautyMaterialIdV16,
    pub glass_material_id: BeautyMaterialIdV16,
    pub tire_material_id: BeautyMaterialIdV16,
    pub wetness_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub lod_identity_locked: bool,
    pub irregularity: IrregularityRecipeV16,
}

impl VehicleProxyV16 {
    pub fn compact_car_default(entity_id: u64, position_world: [f32; 3], seed: u64) -> Self {
        Self {
            entity_id,
            position_world,
            length_meters: 4.35,
            width_meters: 1.82,
            height_meters: 1.44,
            wheel_radius_meters: 0.31,
            has_cabin: true,
            has_wheels_or_hover_equivalent: true,
            has_windshield: true,
            has_headlights: true,
            has_taillights: true,
            has_panel_seams: true,
            body_material_id: BeautyMaterialIdV16(0xCA9_B0D7),
            glass_material_id: BeautyMaterialIdV16(0x0009_1A55),
            tire_material_id: BeautyMaterialIdV16(0x0009_BB3B),
            wetness_0_to_1: 0.48,
            dirt_0_to_1: 0.32,
            lod_identity_locked: true,
            irregularity: IrregularityRecipeV16::factory_vehicle(seed),
        }
    }

    pub fn is_proportionate_non_box(&self) -> bool {
        self.length_meters >= 2.4
            && self.width_meters >= 1.2
            && self.height_meters >= 0.9
            && self.wheel_radius_meters >= 0.15
            && self.has_cabin
            && self.has_wheels_or_hover_equivalent
            && self.has_windshield
            && self.has_headlights
            && self.has_taillights
            && self.has_panel_seams
            && self.lod_identity_locked
    }
}

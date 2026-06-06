//! V17 Beauty geometry records.
//!
//! These records describe real visible content. Chunk bounds, material volumes,
//! nav nodes, and debug markers are not Beauty geometry.

use super::irregularity::IrregularityRecipeV17;
use super::material_pages::{BeautyMaterialIdV17, BeautySurfaceIdV17};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BeautyObjectIdV17(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BeautyCellIdV17(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautyBoundsV17 {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl BeautyBoundsV17 {
    pub fn non_degenerate(&self) -> bool {
        self.max[0] > self.min[0]
            && self.max[1] > self.min[1]
            && self.max[2] > self.min[2]
            && self.min.iter().all(|v| v.is_finite())
            && self.max.iter().all(|v| v.is_finite())
    }

    pub fn center(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadPatchV17 {
    pub object_id: BeautyObjectIdV17,
    pub surface_id: BeautySurfaceIdV17,
    pub centerline_world: Vec<[f32; 3]>,
    pub width_meters: f32,
    pub crown_height_meters: f32,
    pub edge_noise_meters: f32,
    pub puddle_basin_count: u16,
    pub pothole_count: u16,
    pub material_id: BeautyMaterialIdV17,
    pub irregularity: IrregularityRecipeV17,
}

impl RoadPatchV17 {
    pub fn is_real_road(&self) -> bool {
        self.centerline_world.len() >= 3
            && self.width_meters >= 2.5
            && self.crown_height_meters > 0.005
            && self.edge_noise_meters > 0.01
            && self.material_id.0 != 0
            && self.irregularity.is_realistic_not_lego()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurbSegmentV17 {
    pub object_id: BeautyObjectIdV17,
    pub start_world: [f32; 3],
    pub end_world: [f32; 3],
    pub height_meters: f32,
    pub width_meters: f32,
    pub bevel_radius_meters: f32,
    pub chip_density_0_to_1: f32,
    pub material_id: BeautyMaterialIdV17,
    pub irregularity: IrregularityRecipeV17,
}

impl CurbSegmentV17 {
    pub fn is_rounded_and_chipped(&self) -> bool {
        self.height_meters > 0.05
            && self.width_meters > 0.08
            && self.bevel_radius_meters > 0.01
            && self.chip_density_0_to_1 > 0.02
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FacadeModuleV17 {
    pub object_id: BeautyObjectIdV17,
    pub bounds: BeautyBoundsV17,
    pub material_id: BeautyMaterialIdV17,
    pub window_count: u16,
    pub door_count: u16,
    pub vent_count: u16,
    pub pipe_mount_count: u16,
    pub sign_mount_count: u16,
    pub inset_depth_meters: f32,
    pub bevel_radius_meters: f32,
    pub facade_warp_meters: f32,
    pub dirt_0_to_1: f32,
    pub poster_or_stain_count: u16,
    pub irregularity: IrregularityRecipeV17,
}

impl FacadeModuleV17 {
    pub fn is_not_lego_block(&self) -> bool {
        self.bounds.non_degenerate()
            && self.bevel_radius_meters > 0.01
            && self.inset_depth_meters > 0.02
            && self.facade_warp_meters >= 0.0
            && (self.window_count > 0 || self.door_count > 0 || self.vent_count > 0)
            && (self.dirt_0_to_1 > 0.05 || self.poster_or_stain_count > 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveObjectKindV17 {
    Pipe,
    Cable,
    Hose,
    Rail,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveObjectV17 {
    pub object_id: BeautyObjectIdV17,
    pub kind: CurveObjectKindV17,
    pub points_world: Vec<[f32; 3]>,
    pub radius_meters: f32,
    pub material_id: BeautyMaterialIdV17,
    pub sag_meters: f32,
    pub surface_dirt_0_to_1: f32,
}

impl CurveObjectV17 {
    pub fn is_pipe_or_cable(&self) -> bool {
        self.points_world.len() >= 2 && self.radius_meters > 0.005 && self.material_id.0 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScatterFieldV17 {
    pub object_id: BeautyObjectIdV17,
    pub bounds: BeautyBoundsV17,
    pub density_0_to_1: f32,
    pub item_count_budget: u16,
    pub has_trash: bool,
    pub has_stones: bool,
    pub has_paper: bool,
    pub has_broken_glass: bool,
    pub has_cable_clutter: bool,
    pub seed: u64,
}

impl ScatterFieldV17 {
    pub fn is_lived_in(&self) -> bool {
        self.bounds.non_degenerate()
            && self.density_0_to_1 > 0.02
            && self.item_count_budget > 0
            && (self.has_trash
                || self.has_stones
                || self.has_paper
                || self.has_broken_glass
                || self.has_cable_clutter)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroundedPuddleV17 {
    pub object_id: BeautyObjectIdV17,
    pub receiver_surface_id: BeautySurfaceIdV17,
    pub center_world: [f32; 3],
    pub receiver_normal_world: [f32; 3],
    pub radius_meters: f32,
    pub max_depth_meters: f32,
    pub edge_softness_meters: f32,
    pub material_id: BeautyMaterialIdV17,
}

impl GroundedPuddleV17 {
    pub fn is_ground_anchored(&self) -> bool {
        self.receiver_surface_id.0 != 0
            && self.center_world.iter().all(|v| v.is_finite())
            && self.center_world[2] >= -0.05
            && self.center_world[2] <= 0.12
            && self.receiver_normal_world[2] > 0.65
            && self.radius_meters > 0.05
            && self.max_depth_meters > 0.001
            && self.max_depth_meters < 0.12
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeautyAnchorModeV17 {
    WorldCell(BeautyCellIdV17),
    Entity(u64),
    CameraRelative,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanProxyV17 {
    pub entity_id: u64,
    pub anchor_mode: BeautyAnchorModeV17,
    pub position_world: [f32; 3],
    pub facing_yaw_radians: f32,
    pub height_meters: f32,
    pub shoulder_width_meters: f32,
    pub hip_width_meters: f32,
    pub head_radius_meters: f32,
    pub has_head: bool,
    pub has_neck: bool,
    pub has_torso: bool,
    pub has_pelvis: bool,
    pub has_arms: bool,
    pub has_hands: bool,
    pub has_legs: bool,
    pub has_feet: bool,
    pub has_clothing_silhouette: bool,
    pub has_hair_or_head_covering: bool,
    pub skin_material_id: BeautyMaterialIdV17,
    pub clothing_material_id: BeautyMaterialIdV17,
    pub hair_material_id: BeautyMaterialIdV17,
    pub animation_phase_0_to_1: f32,
    pub breathing_weight_0_to_1: f32,
    pub walk_cycle_weight_0_to_1: f32,
    pub lod_identity_locked: bool,
    pub irregularity: IrregularityRecipeV17,
}

impl HumanProxyV17 {
    pub fn adult_world_anchored(
        entity_id: u64,
        cell_id: BeautyCellIdV17,
        position_world: [f32; 3],
        seed: u64,
    ) -> Self {
        Self {
            entity_id,
            anchor_mode: BeautyAnchorModeV17::WorldCell(cell_id),
            position_world,
            facing_yaw_radians: seeded_angle(seed),
            height_meters: 1.66 + seeded01(seed, 1) * 0.20,
            shoulder_width_meters: 0.43 + seeded01(seed, 2) * 0.08,
            hip_width_meters: 0.31 + seeded01(seed, 3) * 0.08,
            head_radius_meters: 0.095 + seeded01(seed, 4) * 0.018,
            has_head: true,
            has_neck: true,
            has_torso: true,
            has_pelvis: true,
            has_arms: true,
            has_hands: true,
            has_legs: true,
            has_feet: true,
            has_clothing_silhouette: true,
            has_hair_or_head_covering: true,
            skin_material_id: BeautyMaterialIdV17(0x5151_5151),
            clothing_material_id: BeautyMaterialIdV17(0x000C_107A),
            hair_material_id: BeautyMaterialIdV17(0x0A11_A111),
            animation_phase_0_to_1: seeded01(seed, 5),
            breathing_weight_0_to_1: 0.30,
            walk_cycle_weight_0_to_1: 0.15 + seeded01(seed, 6) * 0.30,
            lod_identity_locked: true,
            irregularity: IrregularityRecipeV17::human(seed),
        }
    }

    pub fn is_proportionate_non_rod(&self) -> bool {
        self.anchor_mode != BeautyAnchorModeV17::CameraRelative
            && self.height_meters >= 1.45
            && self.height_meters <= 2.10
            && self.shoulder_width_meters >= 0.32
            && self.hip_width_meters >= 0.25
            && self.head_radius_meters >= 0.07
            && self.has_head
            && self.has_neck
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
pub struct VehicleProxyV17 {
    pub entity_id: u64,
    pub anchor_mode: BeautyAnchorModeV17,
    pub position_world: [f32; 3],
    pub facing_yaw_radians: f32,
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
    pub has_mirrors_or_sensors: bool,
    pub body_material_id: BeautyMaterialIdV17,
    pub glass_material_id: BeautyMaterialIdV17,
    pub tire_material_id: BeautyMaterialIdV17,
    pub wetness_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub lod_identity_locked: bool,
    pub irregularity: IrregularityRecipeV17,
}

impl VehicleProxyV17 {
    pub fn compact_car_world_anchored(
        entity_id: u64,
        cell_id: BeautyCellIdV17,
        position_world: [f32; 3],
        seed: u64,
    ) -> Self {
        Self {
            entity_id,
            anchor_mode: BeautyAnchorModeV17::WorldCell(cell_id),
            position_world,
            facing_yaw_radians: seeded_angle(seed ^ 0xCA9),
            length_meters: 4.25 + seeded01(seed, 1) * 0.35,
            width_meters: 1.78 + seeded01(seed, 2) * 0.12,
            height_meters: 1.38 + seeded01(seed, 3) * 0.16,
            wheel_radius_meters: 0.29 + seeded01(seed, 4) * 0.04,
            has_cabin: true,
            has_wheels_or_hover_equivalent: true,
            has_windshield: true,
            has_headlights: true,
            has_taillights: true,
            has_panel_seams: true,
            has_mirrors_or_sensors: true,
            body_material_id: BeautyMaterialIdV17(0xCA9_B0D7),
            glass_material_id: BeautyMaterialIdV17(0x0009_1A55),
            tire_material_id: BeautyMaterialIdV17(0x0009_BB3B),
            wetness_0_to_1: 0.48,
            dirt_0_to_1: 0.32,
            lod_identity_locked: true,
            irregularity: IrregularityRecipeV17::factory_vehicle(seed),
        }
    }

    pub fn is_proportionate_non_box(&self) -> bool {
        self.anchor_mode != BeautyAnchorModeV17::CameraRelative
            && self.length_meters >= 2.4
            && self.width_meters >= 1.2
            && self.height_meters >= 0.9
            && self.wheel_radius_meters >= 0.15
            && self.has_cabin
            && self.has_wheels_or_hover_equivalent
            && self.has_windshield
            && self.has_headlights
            && self.has_taillights
            && self.has_panel_seams
            && self.has_mirrors_or_sensors
            && self.lod_identity_locked
    }
}

fn seeded01(seed: u64, salt: u64) -> f32 {
    let mut x = seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    ((x >> 40) as f32) / ((1u64 << 24) as f32)
}

fn seeded_angle(seed: u64) -> f32 {
    seeded01(seed, 99) * std::f32::consts::TAU
}

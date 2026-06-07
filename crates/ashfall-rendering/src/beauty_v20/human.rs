//! Proportionate world-anchored human proxy contract.

use super::geometry::Vec3V20;
use super::material_pages::{BeautyMaterialIdV20, BeautySurfaceIdV20};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HumanPoseStateV20 {
    Idle,
    Walking,
    Running,
    Crouched,
    Sitting,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanProportionsV20 {
    pub height_meters: f32,
    pub head_radius_meters: f32,
    pub shoulder_width_meters: f32,
    pub torso_depth_meters: f32,
    pub pelvis_width_meters: f32,
    pub arm_length_meters: f32,
    pub leg_length_meters: f32,
    pub foot_length_meters: f32,
}

impl HumanProportionsV20 {
    pub fn adult_average() -> Self {
        Self {
            height_meters: 1.74,
            head_radius_meters: 0.115,
            shoulder_width_meters: 0.46,
            torso_depth_meters: 0.22,
            pelvis_width_meters: 0.34,
            arm_length_meters: 0.72,
            leg_length_meters: 0.88,
            foot_length_meters: 0.25,
        }
    }

    pub fn plausible(&self) -> bool {
        (1.35..=2.15).contains(&self.height_meters)
            && (0.075..=0.16).contains(&self.head_radius_meters)
            && (0.30..=0.70).contains(&self.shoulder_width_meters)
            && self.arm_length_meters > 0.40
            && self.leg_length_meters > 0.55
            && self.foot_length_meters > 0.12
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanMaterialSlotsV20 {
    pub skin_surface: BeautySurfaceIdV20,
    pub skin_material: BeautyMaterialIdV20,
    pub clothing_surface: BeautySurfaceIdV20,
    pub clothing_material: BeautyMaterialIdV20,
    pub hair_surface: BeautySurfaceIdV20,
    pub hair_material: BeautyMaterialIdV20,
    pub shoe_surface: BeautySurfaceIdV20,
    pub shoe_material: BeautyMaterialIdV20,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanProxyV20 {
    pub entity_id: u64,
    pub world_position: Vec3V20,
    pub facing_yaw_radians: f32,
    pub proportions: HumanProportionsV20,
    pub materials: HumanMaterialSlotsV20,
    pub pose: HumanPoseStateV20,
    pub has_head: bool,
    pub has_torso: bool,
    pub has_pelvis: bool,
    pub has_arms: bool,
    pub has_hands: bool,
    pub has_legs: bool,
    pub has_feet: bool,
    pub has_clothing_silhouette: bool,
    pub has_hair_or_hat_silhouette: bool,
    pub camera_relative: bool,
    pub rod_placeholder: bool,
}

impl HumanProxyV20 {
    pub fn city_pedestrian(entity_id: u64, world_position: Vec3V20) -> Self {
        Self {
            entity_id,
            world_position,
            facing_yaw_radians: 0.0,
            proportions: HumanProportionsV20::adult_average(),
            materials: HumanMaterialSlotsV20 {
                skin_surface: BeautySurfaceIdV20(40_001),
                skin_material: BeautyMaterialIdV20(0x5A1E_2020),
                clothing_surface: BeautySurfaceIdV20(40_002),
                clothing_material: BeautyMaterialIdV20(0xC107_2020),
                hair_surface: BeautySurfaceIdV20(40_003),
                hair_material: BeautyMaterialIdV20(0x0A17_2020),
                shoe_surface: BeautySurfaceIdV20(40_004),
                shoe_material: BeautyMaterialIdV20(0x500E_2020),
            },
            pose: HumanPoseStateV20::Idle,
            has_head: true,
            has_torso: true,
            has_pelvis: true,
            has_arms: true,
            has_hands: true,
            has_legs: true,
            has_feet: true,
            has_clothing_silhouette: true,
            has_hair_or_hat_silhouette: true,
            camera_relative: false,
            rod_placeholder: false,
        }
    }

    pub fn visually_valid(&self) -> bool {
        !self.camera_relative
            && !self.rod_placeholder
            && self.proportions.plausible()
            && self.has_head
            && self.has_torso
            && self.has_pelvis
            && self.has_arms
            && self.has_hands
            && self.has_legs
            && self.has_feet
            && self.has_clothing_silhouette
    }
}

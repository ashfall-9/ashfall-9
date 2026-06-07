//! Non-cartoon world-anchored human proxy contract.

use super::geometry::{BeautyMaterialIdV21, BeautySurfaceIdV21, Vec3V21};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HumanPoseStateV21 {
    Idle,
    Walking,
    Running,
    Crouched,
    Sitting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HumanProxyQualityV21 {
    FarSilhouette,
    MidBodyProxy,
    NearAnatomicalProxy,
    HeroRigged,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanProportionsV21 {
    pub height_meters: f32,
    pub head_height_meters: f32,
    pub neck_height_meters: f32,
    pub shoulder_width_meters: f32,
    pub torso_height_meters: f32,
    pub torso_depth_meters: f32,
    pub pelvis_width_meters: f32,
    pub arm_length_meters: f32,
    pub hand_length_meters: f32,
    pub leg_length_meters: f32,
    pub foot_length_meters: f32,
}

impl HumanProportionsV21 {
    pub fn adult_average() -> Self {
        Self {
            height_meters: 1.74,
            head_height_meters: 0.225,
            neck_height_meters: 0.10,
            shoulder_width_meters: 0.46,
            torso_height_meters: 0.58,
            torso_depth_meters: 0.22,
            pelvis_width_meters: 0.34,
            arm_length_meters: 0.72,
            hand_length_meters: 0.18,
            leg_length_meters: 0.88,
            foot_length_meters: 0.25,
        }
    }

    pub fn plausible(&self) -> bool {
        (1.35..=2.15).contains(&self.height_meters)
            && (0.16..=0.32).contains(&self.head_height_meters)
            && (0.30..=0.70).contains(&self.shoulder_width_meters)
            && (0.40..=0.80).contains(&self.torso_height_meters)
            && self.arm_length_meters > 0.40
            && self.leg_length_meters > 0.55
            && self.hand_length_meters > 0.10
            && self.foot_length_meters > 0.12
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanMaterialSlotsV21 {
    pub skin_surface: BeautySurfaceIdV21,
    pub skin_material: BeautyMaterialIdV21,
    pub clothing_surface: BeautySurfaceIdV21,
    pub clothing_material: BeautyMaterialIdV21,
    pub hair_surface: BeautySurfaceIdV21,
    pub hair_material: BeautyMaterialIdV21,
    pub shoe_surface: BeautySurfaceIdV21,
    pub shoe_material: BeautyMaterialIdV21,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanProxyV21 {
    pub entity_id: u64,
    pub world_position: Vec3V21,
    pub ground_z_meters: f32,
    pub facing_yaw_radians: f32,
    pub proportions: HumanProportionsV21,
    pub materials: HumanMaterialSlotsV21,
    pub pose: HumanPoseStateV21,
    pub quality: HumanProxyQualityV21,
    pub has_head: bool,
    pub has_neck: bool,
    pub has_torso: bool,
    pub has_pelvis: bool,
    pub has_arms: bool,
    pub has_hands: bool,
    pub has_legs: bool,
    pub has_feet: bool,
    pub has_clothing_silhouette: bool,
    pub has_hair_or_hat_silhouette: bool,
    pub has_face_proxy: bool,
    pub has_skin_material_pages: bool,
    pub has_clothing_material_pages: bool,
    pub camera_relative: bool,
    pub rod_placeholder: bool,
    pub cartoon_placeholder: bool,
}

impl HumanProxyV21 {
    pub fn city_pedestrian(entity_id: u64, world_position: Vec3V21) -> Self {
        Self {
            entity_id,
            ground_z_meters: world_position.z,
            world_position,
            facing_yaw_radians: 0.0,
            proportions: HumanProportionsV21::adult_average(),
            materials: HumanMaterialSlotsV21 {
                skin_surface: BeautySurfaceIdV21(40_001),
                skin_material: BeautyMaterialIdV21(0x5A1E_2021),
                clothing_surface: BeautySurfaceIdV21(40_002),
                clothing_material: BeautyMaterialIdV21(0xC107_2021),
                hair_surface: BeautySurfaceIdV21(40_003),
                hair_material: BeautyMaterialIdV21(0x0A17_2021),
                shoe_surface: BeautySurfaceIdV21(40_004),
                shoe_material: BeautyMaterialIdV21(0x500E_2021),
            },
            pose: HumanPoseStateV21::Idle,
            quality: HumanProxyQualityV21::MidBodyProxy,
            has_head: true,
            has_neck: true,
            has_torso: true,
            has_pelvis: true,
            has_arms: true,
            has_hands: true,
            has_legs: true,
            has_feet: true,
            has_clothing_silhouette: true,
            has_hair_or_hat_silhouette: true,
            has_face_proxy: true,
            has_skin_material_pages: true,
            has_clothing_material_pages: true,
            camera_relative: false,
            rod_placeholder: false,
            cartoon_placeholder: false,
        }
    }

    pub fn visually_valid(&self) -> bool {
        !self.camera_relative
            && !self.rod_placeholder
            && !self.cartoon_placeholder
            && self.proportions.plausible()
            && (self.world_position.z - self.ground_z_meters).abs() <= 0.08
            && self.has_head
            && self.has_neck
            && self.has_torso
            && self.has_pelvis
            && self.has_arms
            && self.has_hands
            && self.has_legs
            && self.has_feet
            && self.has_clothing_silhouette
            && self.has_hair_or_hat_silhouette
            && self.has_face_proxy
            && self.has_skin_material_pages
            && self.has_clothing_material_pages
    }
}

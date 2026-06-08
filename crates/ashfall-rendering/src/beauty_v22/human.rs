//! Coherent, non-cartoon, non-fractured human proxy contract.

use super::geometry::{BeautyMaterialIdV22, BeautyMeshAssetIdV22, BeautySurfaceIdV22, Vec3V22};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HumanPoseStateV22 {
    Idle,
    Walking,
    Running,
    Crouched,
    Sitting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HumanVisualQualityV22 {
    FarCoherentImpostor,
    MidCoherentMesh,
    NearSkinnedProxy,
    HeroRigged,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanProportionsV22 {
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

impl HumanProportionsV22 {
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
pub struct HumanMaterialSlotsV22 {
    pub skin_surface: BeautySurfaceIdV22,
    pub skin_material: BeautyMaterialIdV22,
    pub clothing_surface: BeautySurfaceIdV22,
    pub clothing_material: BeautyMaterialIdV22,
    pub hair_surface: BeautySurfaceIdV22,
    pub hair_material: BeautyMaterialIdV22,
    pub shoe_surface: BeautySurfaceIdV22,
    pub shoe_material: BeautyMaterialIdV22,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HumanMeshBindingV22 {
    pub coherent_body_mesh: BeautyMeshAssetIdV22,
    pub optional_hair_mesh: Option<BeautyMeshAssetIdV22>,
    pub optional_clothing_mesh: Option<BeautyMeshAssetIdV22>,
    pub single_coherent_body_mesh: bool,
    pub draw_as_disconnected_parts: bool,
}

impl HumanMeshBindingV22 {
    pub const fn generated_coherent_proxy(mesh_id: u64) -> Self {
        Self {
            coherent_body_mesh: BeautyMeshAssetIdV22(mesh_id),
            optional_hair_mesh: None,
            optional_clothing_mesh: None,
            single_coherent_body_mesh: true,
            draw_as_disconnected_parts: false,
        }
    }

    pub fn visually_valid(self) -> bool {
        self.coherent_body_mesh.0 != 0
            && self.single_coherent_body_mesh
            && !self.draw_as_disconnected_parts
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanProxyV22 {
    pub entity_id: u64,
    pub world_position: Vec3V22,
    pub ground_z_meters: f32,
    pub facing_yaw_radians: f32,
    pub proportions: HumanProportionsV22,
    pub materials: HumanMaterialSlotsV22,
    pub pose: HumanPoseStateV22,
    pub quality: HumanVisualQualityV22,
    pub mesh_binding: HumanMeshBindingV22,

    pub has_head_volume: bool,
    pub has_neck_connection: bool,
    pub has_torso_volume: bool,
    pub has_pelvis_volume: bool,
    pub has_arm_silhouette: bool,
    pub has_hand_silhouette: bool,
    pub has_leg_silhouette: bool,
    pub has_foot_silhouette: bool,
    pub has_clothing_silhouette: bool,
    pub has_hair_or_hat_silhouette: bool,
    pub has_face_proxy: bool,

    pub has_skin_material_pages: bool,
    pub has_clothing_material_pages: bool,
    pub has_hair_material_pages: bool,

    pub camera_relative: bool,
    pub rod_placeholder: bool,
    pub cartoon_placeholder: bool,
    pub fractured_placeholder: bool,
    pub fracture_piece_count: u32,
    pub disconnected_part_count: u32,
}

impl HumanProxyV22 {
    pub fn world_anchored_coherent_pedestrian(entity_id: u64, world_position: Vec3V22) -> Self {
        Self {
            entity_id,
            ground_z_meters: world_position.z,
            world_position,
            facing_yaw_radians: 0.0,
            proportions: HumanProportionsV22::adult_average(),
            materials: HumanMaterialSlotsV22 {
                skin_surface: BeautySurfaceIdV22(40_001),
                skin_material: BeautyMaterialIdV22(0x5A1E_2022),
                clothing_surface: BeautySurfaceIdV22(40_002),
                clothing_material: BeautyMaterialIdV22(0xC107_2022),
                hair_surface: BeautySurfaceIdV22(40_003),
                hair_material: BeautyMaterialIdV22(0x0A17_2022),
                shoe_surface: BeautySurfaceIdV22(40_004),
                shoe_material: BeautyMaterialIdV22(0x500E_2022),
            },
            pose: HumanPoseStateV22::Idle,
            quality: HumanVisualQualityV22::MidCoherentMesh,
            mesh_binding: HumanMeshBindingV22::generated_coherent_proxy(0x4800_2022),

            has_head_volume: true,
            has_neck_connection: true,
            has_torso_volume: true,
            has_pelvis_volume: true,
            has_arm_silhouette: true,
            has_hand_silhouette: true,
            has_leg_silhouette: true,
            has_foot_silhouette: true,
            has_clothing_silhouette: true,
            has_hair_or_hat_silhouette: true,
            has_face_proxy: true,

            has_skin_material_pages: true,
            has_clothing_material_pages: true,
            has_hair_material_pages: true,

            camera_relative: false,
            rod_placeholder: false,
            cartoon_placeholder: false,
            fractured_placeholder: false,
            fracture_piece_count: 0,
            disconnected_part_count: 0,
        }
    }

    pub fn visually_valid(&self) -> bool {
        !self.camera_relative
            && !self.rod_placeholder
            && !self.cartoon_placeholder
            && !self.fractured_placeholder
            && self.fracture_piece_count == 0
            && self.disconnected_part_count == 0
            && self.mesh_binding.visually_valid()
            && self.proportions.plausible()
            && (self.world_position.z - self.ground_z_meters).abs() <= 0.08
            && self.has_head_volume
            && self.has_neck_connection
            && self.has_torso_volume
            && self.has_pelvis_volume
            && self.has_arm_silhouette
            && self.has_hand_silhouette
            && self.has_leg_silhouette
            && self.has_foot_silhouette
            && self.has_clothing_silhouette
            && self.has_hair_or_hat_silhouette
            && self.has_face_proxy
            && self.has_skin_material_pages
            && self.has_clothing_material_pages
            && self.has_hair_material_pages
    }
}

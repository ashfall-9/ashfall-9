//! Coherent vehicle/machine proxy contract.

use super::geometry::{BeautyMaterialIdV22, BeautyMeshAssetIdV22, BeautySurfaceIdV22, Vec3V22};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VehicleKindV22 {
    CompactCar,
    DeliveryVan,
    LandfillLoader,
    Motorcycle,
    UtilityTruck,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VehicleMeshBindingV22 {
    pub body_mesh: BeautyMeshAssetIdV22,
    pub coherent_single_mesh_or_rig: bool,
    pub box_placeholder: bool,
}

impl VehicleMeshBindingV22 {
    pub const fn generated_vehicle(mesh_id: u64) -> Self {
        Self {
            body_mesh: BeautyMeshAssetIdV22(mesh_id),
            coherent_single_mesh_or_rig: true,
            box_placeholder: false,
        }
    }

    pub fn visually_valid(self) -> bool {
        self.body_mesh.0 != 0 && self.coherent_single_mesh_or_rig && !self.box_placeholder
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VehicleProxyV22 {
    pub entity_id: u64,
    pub kind: VehicleKindV22,
    pub world_position: Vec3V22,
    pub ground_z_meters: f32,
    pub facing_yaw_radians: f32,
    pub length_meters: f32,
    pub width_meters: f32,
    pub height_meters: f32,
    pub mesh_binding: VehicleMeshBindingV22,
    pub body_surface: BeautySurfaceIdV22,
    pub body_material: BeautyMaterialIdV22,
    pub glass_surface: BeautySurfaceIdV22,
    pub rubber_surface: BeautySurfaceIdV22,
    pub has_wheels_or_equivalent: bool,
    pub has_cabin_or_operator_space: bool,
    pub has_glass_or_viewport: bool,
    pub has_headlights_or_work_lamps: bool,
    pub has_panel_seams: bool,
    pub has_dirt_wetness_response: bool,
    pub camera_relative: bool,
    pub box_placeholder: bool,
}

impl VehicleProxyV22 {
    pub fn compact_car(entity_id: u64, world_position: Vec3V22) -> Self {
        Self {
            entity_id,
            kind: VehicleKindV22::CompactCar,
            ground_z_meters: world_position.z,
            world_position,
            facing_yaw_radians: 0.0,
            length_meters: 4.25,
            width_meters: 1.82,
            height_meters: 1.48,
            mesh_binding: VehicleMeshBindingV22::generated_vehicle(0xCA22_0001),
            body_surface: BeautySurfaceIdV22(50_001),
            body_material: BeautyMaterialIdV22(9),
            glass_surface: BeautySurfaceIdV22(50_002),
            rubber_surface: BeautySurfaceIdV22(50_003),
            has_wheels_or_equivalent: true,
            has_cabin_or_operator_space: true,
            has_glass_or_viewport: true,
            has_headlights_or_work_lamps: true,
            has_panel_seams: true,
            has_dirt_wetness_response: true,
            camera_relative: false,
            box_placeholder: false,
        }
    }

    pub fn landfill_loader(entity_id: u64, world_position: Vec3V22) -> Self {
        Self {
            entity_id,
            kind: VehicleKindV22::LandfillLoader,
            ground_z_meters: world_position.z,
            world_position,
            facing_yaw_radians: 0.2,
            length_meters: 6.3,
            width_meters: 2.6,
            height_meters: 3.1,
            mesh_binding: VehicleMeshBindingV22::generated_vehicle(0x10AD_2022),
            body_surface: BeautySurfaceIdV22(50_004),
            body_material: BeautyMaterialIdV22(9),
            glass_surface: BeautySurfaceIdV22(50_002),
            rubber_surface: BeautySurfaceIdV22(50_003),
            has_wheels_or_equivalent: true,
            has_cabin_or_operator_space: true,
            has_glass_or_viewport: true,
            has_headlights_or_work_lamps: true,
            has_panel_seams: true,
            has_dirt_wetness_response: true,
            camera_relative: false,
            box_placeholder: false,
        }
    }

    pub fn visually_valid(&self) -> bool {
        !self.camera_relative
            && !self.box_placeholder
            && self.mesh_binding.visually_valid()
            && self.length_meters >= 1.4
            && self.width_meters >= 0.6
            && self.height_meters >= 0.7
            && (self.world_position.z - self.ground_z_meters).abs() <= 0.10
            && self.has_wheels_or_equivalent
            && self.has_cabin_or_operator_space
            && self.has_glass_or_viewport
            && self.has_headlights_or_work_lamps
            && self.has_panel_seams
            && self.has_dirt_wetness_response
    }
}

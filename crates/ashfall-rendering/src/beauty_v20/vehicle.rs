//! Proportionate world-anchored vehicle/machine proxy contract.

use super::geometry::Vec3V20;
use super::material_pages::{BeautyMaterialIdV20, BeautySurfaceIdV20};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VehicleKindV20 {
    Sedan,
    Van,
    UtilityTruck,
    Motorcycle,
    CyberpunkHoverCar,
    LandfillMachine,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleMaterialSlotsV20 {
    pub body_surface: BeautySurfaceIdV20,
    pub body_material: BeautyMaterialIdV20,
    pub glass_surface: BeautySurfaceIdV20,
    pub glass_material: BeautyMaterialIdV20,
    pub tire_surface: BeautySurfaceIdV20,
    pub tire_material: BeautyMaterialIdV20,
    pub dirt_surface: BeautySurfaceIdV20,
    pub dirt_material: BeautyMaterialIdV20,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VehicleProxyV20 {
    pub entity_id: u64,
    pub kind: VehicleKindV20,
    pub world_position: Vec3V20,
    pub facing_yaw_radians: f32,
    pub length_meters: f32,
    pub width_meters: f32,
    pub height_meters: f32,
    pub materials: VehicleMaterialSlotsV20,
    pub has_curved_body: bool,
    pub has_wheels_or_hover_equivalent: bool,
    pub has_cabin: bool,
    pub has_glass: bool,
    pub has_lights: bool,
    pub has_panel_seams: bool,
    pub has_dirty_lower_body: bool,
    pub box_placeholder: bool,
}

impl VehicleProxyV20 {
    pub fn parked_sedan(entity_id: u64, world_position: Vec3V20) -> Self {
        Self {
            entity_id,
            kind: VehicleKindV20::Sedan,
            world_position,
            facing_yaw_radians: 0.12,
            length_meters: 4.45,
            width_meters: 1.86,
            height_meters: 1.43,
            materials: VehicleMaterialSlotsV20 {
                body_surface: BeautySurfaceIdV20(50_001),
                body_material: BeautyMaterialIdV20(0xCA9_2020),
                glass_surface: BeautySurfaceIdV20(50_002),
                glass_material: BeautyMaterialIdV20(0x61A5_2020),
                tire_surface: BeautySurfaceIdV20(50_003),
                tire_material: BeautyMaterialIdV20(0x700E_2020),
                dirt_surface: BeautySurfaceIdV20(50_004),
                dirt_material: BeautyMaterialIdV20(0xD127_2020),
            },
            has_curved_body: true,
            has_wheels_or_hover_equivalent: true,
            has_cabin: true,
            has_glass: true,
            has_lights: true,
            has_panel_seams: true,
            has_dirty_lower_body: true,
            box_placeholder: false,
        }
    }

    pub fn landfill_machine(entity_id: u64, world_position: Vec3V20) -> Self {
        let mut machine = Self::parked_sedan(entity_id, world_position);
        machine.kind = VehicleKindV20::LandfillMachine;
        machine.length_meters = 5.4;
        machine.width_meters = 2.2;
        machine.height_meters = 2.35;
        machine
    }

    pub fn plausible_dimensions(&self) -> bool {
        (1.5..=8.0).contains(&self.length_meters)
            && (0.6..=3.5).contains(&self.width_meters)
            && (0.6..=4.0).contains(&self.height_meters)
    }

    pub fn visually_valid(&self) -> bool {
        !self.box_placeholder
            && self.plausible_dimensions()
            && self.has_curved_body
            && self.has_wheels_or_hover_equivalent
            && self.has_cabin
            && self.has_glass
            && self.has_lights
            && self.has_panel_seams
    }
}

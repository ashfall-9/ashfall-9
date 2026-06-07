//! Proportionate world-anchored vehicle contract.

use super::geometry::{BeautyMaterialIdV21, BeautySurfaceIdV21, Vec3V21};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VehicleKindV21 {
    CompactCar,
    DeliveryVan,
    UtilityTruck,
    LandfillLoader,
    Motorcycle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleProportionsV21 {
    pub length_meters: f32,
    pub width_meters: f32,
    pub height_meters: f32,
    pub wheel_radius_meters: f32,
    pub cabin_height_meters: f32,
}

impl VehicleProportionsV21 {
    pub fn compact_car() -> Self {
        Self {
            length_meters: 4.35,
            width_meters: 1.82,
            height_meters: 1.45,
            wheel_radius_meters: 0.32,
            cabin_height_meters: 0.92,
        }
    }
    pub fn landfill_loader() -> Self {
        Self {
            length_meters: 6.20,
            width_meters: 2.45,
            height_meters: 2.85,
            wheel_radius_meters: 0.62,
            cabin_height_meters: 1.38,
        }
    }
    pub fn plausible(&self) -> bool {
        self.length_meters >= 1.8
            && self.width_meters >= 0.55
            && self.height_meters >= 0.6
            && self.wheel_radius_meters >= 0.12
            && self.cabin_height_meters >= 0.35
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleMaterialSlotsV21 {
    pub body_surface: BeautySurfaceIdV21,
    pub body_material: BeautyMaterialIdV21,
    pub glass_surface: BeautySurfaceIdV21,
    pub glass_material: BeautyMaterialIdV21,
    pub tire_surface: BeautySurfaceIdV21,
    pub tire_material: BeautyMaterialIdV21,
    pub light_surface: BeautySurfaceIdV21,
    pub light_material: BeautyMaterialIdV21,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VehicleProxyV21 {
    pub entity_id: u64,
    pub kind: VehicleKindV21,
    pub world_position: Vec3V21,
    pub facing_yaw_radians: f32,
    pub proportions: VehicleProportionsV21,
    pub materials: VehicleMaterialSlotsV21,
    pub has_curved_body: bool,
    pub has_cabin: bool,
    pub has_glass: bool,
    pub has_wheels_or_equivalent: bool,
    pub has_headlights: bool,
    pub has_taillights: bool,
    pub has_panel_seams: bool,
    pub has_dirt_and_wetness: bool,
    pub box_placeholder: bool,
}

impl VehicleProxyV21 {
    pub fn compact_car(entity_id: u64, world_position: Vec3V21) -> Self {
        Self {
            entity_id,
            kind: VehicleKindV21::CompactCar,
            world_position,
            facing_yaw_radians: 0.0,
            proportions: VehicleProportionsV21::compact_car(),
            materials: Self::default_materials(),
            has_curved_body: true,
            has_cabin: true,
            has_glass: true,
            has_wheels_or_equivalent: true,
            has_headlights: true,
            has_taillights: true,
            has_panel_seams: true,
            has_dirt_and_wetness: true,
            box_placeholder: false,
        }
    }

    pub fn landfill_loader(entity_id: u64, world_position: Vec3V21) -> Self {
        Self {
            kind: VehicleKindV21::LandfillLoader,
            proportions: VehicleProportionsV21::landfill_loader(),
            ..Self::compact_car(entity_id, world_position)
        }
    }

    fn default_materials() -> VehicleMaterialSlotsV21 {
        VehicleMaterialSlotsV21 {
            body_surface: BeautySurfaceIdV21(50_001),
            body_material: BeautyMaterialIdV21(0xCA12_2021),
            glass_surface: BeautySurfaceIdV21(50_002),
            glass_material: BeautyMaterialIdV21(0x61A5_2021),
            tire_surface: BeautySurfaceIdV21(50_003),
            tire_material: BeautyMaterialIdV21(0x71BE_2021),
            light_surface: BeautySurfaceIdV21(50_004),
            light_material: BeautyMaterialIdV21(0x1167_2021),
        }
    }

    pub fn visually_valid(&self) -> bool {
        !self.box_placeholder
            && self.proportions.plausible()
            && self.has_curved_body
            && self.has_cabin
            && self.has_glass
            && self.has_wheels_or_equivalent
            && self.has_headlights
            && self.has_taillights
            && self.has_panel_seams
            && self.has_dirt_and_wetness
    }
}

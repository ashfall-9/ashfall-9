//! Irregularity recipes define how objects stop looking like perfect blocks.
//! Factory-made objects such as cars may be smoother and more regular, but they
//! still need bevels, seams, dirt, wetness, and correct proportions.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeautyObjectClassV16 {
    Terrain,
    Road,
    Curb,
    BuildingFacade,
    Pipe,
    Cable,
    Prop,
    Scatter,
    Puddle,
    Human,
    VehicleFactoryMade,
    VehicleDamaged,
    Signage,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularityRecipeV16 {
    pub seed: u64,
    pub object_class: BeautyObjectClassV16,
    pub factory_made: bool,
    pub silhouette_variation_0_to_1: f32,
    pub bevel_radius_meters_min: f32,
    pub bevel_radius_meters_max: f32,
    pub surface_warp_strength_meters: f32,
    pub dirt_density_0_to_1: f32,
    pub chip_density_0_to_1: f32,
    pub crack_density_0_to_1: f32,
    pub decal_density_0_to_1: f32,
    pub scatter_density_0_to_1: f32,
    pub wetness_bias_0_to_1: f32,
}

impl IrregularityRecipeV16 {
    pub const fn road(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV16::Road,
            factory_made: false,
            silhouette_variation_0_to_1: 0.22,
            bevel_radius_meters_min: 0.015,
            bevel_radius_meters_max: 0.080,
            surface_warp_strength_meters: 0.055,
            dirt_density_0_to_1: 0.62,
            chip_density_0_to_1: 0.30,
            crack_density_0_to_1: 0.32,
            decal_density_0_to_1: 0.48,
            scatter_density_0_to_1: 0.30,
            wetness_bias_0_to_1: 0.74,
        }
    }

    pub const fn curb(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV16::Curb,
            factory_made: false,
            silhouette_variation_0_to_1: 0.20,
            bevel_radius_meters_min: 0.035,
            bevel_radius_meters_max: 0.110,
            surface_warp_strength_meters: 0.020,
            dirt_density_0_to_1: 0.55,
            chip_density_0_to_1: 0.38,
            crack_density_0_to_1: 0.24,
            decal_density_0_to_1: 0.34,
            scatter_density_0_to_1: 0.12,
            wetness_bias_0_to_1: 0.62,
        }
    }

    pub const fn dirty_facade(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV16::BuildingFacade,
            factory_made: false,
            silhouette_variation_0_to_1: 0.12,
            bevel_radius_meters_min: 0.025,
            bevel_radius_meters_max: 0.100,
            surface_warp_strength_meters: 0.018,
            dirt_density_0_to_1: 0.70,
            chip_density_0_to_1: 0.24,
            crack_density_0_to_1: 0.30,
            decal_density_0_to_1: 0.55,
            scatter_density_0_to_1: 0.05,
            wetness_bias_0_to_1: 0.38,
        }
    }

    pub const fn human(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV16::Human,
            factory_made: false,
            silhouette_variation_0_to_1: 0.08,
            bevel_radius_meters_min: 0.010,
            bevel_radius_meters_max: 0.040,
            surface_warp_strength_meters: 0.006,
            dirt_density_0_to_1: 0.18,
            chip_density_0_to_1: 0.0,
            crack_density_0_to_1: 0.0,
            decal_density_0_to_1: 0.08,
            scatter_density_0_to_1: 0.0,
            wetness_bias_0_to_1: 0.24,
        }
    }

    pub const fn factory_vehicle(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV16::VehicleFactoryMade,
            factory_made: true,
            silhouette_variation_0_to_1: 0.035,
            bevel_radius_meters_min: 0.025,
            bevel_radius_meters_max: 0.130,
            surface_warp_strength_meters: 0.006,
            dirt_density_0_to_1: 0.42,
            chip_density_0_to_1: 0.12,
            crack_density_0_to_1: 0.03,
            decal_density_0_to_1: 0.22,
            scatter_density_0_to_1: 0.0,
            wetness_bias_0_to_1: 0.46,
        }
    }
}

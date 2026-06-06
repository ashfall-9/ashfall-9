//! Irregularity recipes.
//!
//! Realism is not random noise everywhere. Concrete, asphalt, dirt, posters,
//! trash, cable sag, chipped curbs, and old walls should be irregular. Factory
//! objects such as cars should be smoother and more regular, but still need
//! bevels, seams, dirt, wetness, glass, tires, and panel variation.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BeautyObjectClassV17 {
    Road,
    Curb,
    DirtyFacade,
    Pipe,
    Cable,
    Scatter,
    Puddle,
    Human,
    FactoryVehicle,
    Prop,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularityRecipeV17 {
    pub seed: u64,
    pub object_class: BeautyObjectClassV17,
    pub silhouette_variation_meters: f32,
    pub bevel_radius_meters_min: f32,
    pub bevel_radius_meters_max: f32,
    pub surface_warp_meters: f32,
    pub dirt_density_0_to_1: f32,
    pub chip_density_0_to_1: f32,
    pub crack_density_0_to_1: f32,
    pub decal_density_0_to_1: f32,
    pub wetness_bias_0_to_1: f32,
    pub factory_regular_0_to_1: f32,
}

impl IrregularityRecipeV17 {
    pub fn road(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV17::Road,
            silhouette_variation_meters: 0.22,
            bevel_radius_meters_min: 0.015,
            bevel_radius_meters_max: 0.055,
            surface_warp_meters: 0.045,
            dirt_density_0_to_1: 0.64,
            chip_density_0_to_1: 0.18,
            crack_density_0_to_1: 0.36,
            decal_density_0_to_1: 0.52,
            wetness_bias_0_to_1: 0.78,
            factory_regular_0_to_1: 0.0,
        }
    }

    pub fn curb(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV17::Curb,
            silhouette_variation_meters: 0.06,
            bevel_radius_meters_min: 0.025,
            bevel_radius_meters_max: 0.08,
            surface_warp_meters: 0.018,
            dirt_density_0_to_1: 0.58,
            chip_density_0_to_1: 0.42,
            crack_density_0_to_1: 0.22,
            decal_density_0_to_1: 0.18,
            wetness_bias_0_to_1: 0.44,
            factory_regular_0_to_1: 0.0,
        }
    }

    pub fn dirty_facade(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV17::DirtyFacade,
            silhouette_variation_meters: 0.10,
            bevel_radius_meters_min: 0.025,
            bevel_radius_meters_max: 0.095,
            surface_warp_meters: 0.024,
            dirt_density_0_to_1: 0.74,
            chip_density_0_to_1: 0.30,
            crack_density_0_to_1: 0.28,
            decal_density_0_to_1: 0.62,
            wetness_bias_0_to_1: 0.34,
            factory_regular_0_to_1: 0.0,
        }
    }

    pub fn human(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV17::Human,
            silhouette_variation_meters: 0.025,
            bevel_radius_meters_min: 0.0,
            bevel_radius_meters_max: 0.0,
            surface_warp_meters: 0.0,
            dirt_density_0_to_1: 0.18,
            chip_density_0_to_1: 0.0,
            crack_density_0_to_1: 0.0,
            decal_density_0_to_1: 0.12,
            wetness_bias_0_to_1: 0.18,
            factory_regular_0_to_1: 0.15,
        }
    }

    pub fn factory_vehicle(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV17::FactoryVehicle,
            silhouette_variation_meters: 0.012,
            bevel_radius_meters_min: 0.025,
            bevel_radius_meters_max: 0.075,
            surface_warp_meters: 0.004,
            dirt_density_0_to_1: 0.28,
            chip_density_0_to_1: 0.08,
            crack_density_0_to_1: 0.0,
            decal_density_0_to_1: 0.18,
            wetness_bias_0_to_1: 0.45,
            factory_regular_0_to_1: 0.88,
        }
    }

    pub fn is_realistic_not_lego(&self) -> bool {
        self.bevel_radius_meters_max > 0.0
            || self.object_class == BeautyObjectClassV17::Human
            || self.object_class == BeautyObjectClassV17::Puddle
    }
}

//! Realistic irregularity recipes.
//!
//! Most real-world objects are not perfect boxes. Factory-made objects may be
//! clean and symmetrical, but even they need bevels, material variation, dirt,
//! seams, scratches, and correct scale.

use super::detail_budget::BeautyDetailTierV15;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BeautyObjectClassV15 {
    Road,
    Sidewalk,
    Curb,
    BuildingFacade,
    Door,
    Window,
    Pipe,
    Cable,
    Drain,
    Trash,
    Puddle,
    Human,
    Vehicle,
    FactoryMadeObject,
    DebugOnly,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailLodRulesV15 {
    pub preserve_silhouette_until: BeautyDetailTierV15,
    pub min_bevel_visible_meters: f32,
    pub min_material_page_scale: f32,
    pub may_drop_scatter: bool,
    pub may_drop_decals: bool,
    pub may_be_impostor: bool,
}

impl Default for DetailLodRulesV15 {
    fn default() -> Self {
        Self {
            preserve_silhouette_until: BeautyDetailTierV15::Mid,
            min_bevel_visible_meters: 0.01,
            min_material_page_scale: 0.25,
            may_drop_scatter: true,
            may_drop_decals: true,
            may_be_impostor: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularityRecipeV15 {
    pub seed: u64,
    pub object_class: BeautyObjectClassV15,
    pub factory_made: bool,
    pub silhouette_variation: f32,
    pub bevel_radius_min_meters: f32,
    pub bevel_radius_max_meters: f32,
    pub surface_warp_strength: f32,
    pub dirt_density: f32,
    pub chip_density: f32,
    pub crack_density: f32,
    pub decal_density: f32,
    pub scatter_density: f32,
    pub wetness_bias: f32,
    pub lod_rules: DetailLodRulesV15,
}

impl IrregularityRecipeV15 {
    pub fn road(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV15::Road,
            factory_made: false,
            silhouette_variation: 0.16,
            bevel_radius_min_meters: 0.005,
            bevel_radius_max_meters: 0.04,
            surface_warp_strength: 0.045,
            dirt_density: 0.75,
            chip_density: 0.28,
            crack_density: 0.4,
            decal_density: 0.7,
            scatter_density: 0.45,
            wetness_bias: 0.85,
            lod_rules: DetailLodRulesV15::default(),
        }
    }

    pub fn curb(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV15::Curb,
            factory_made: false,
            silhouette_variation: 0.1,
            bevel_radius_min_meters: 0.035,
            bevel_radius_max_meters: 0.12,
            surface_warp_strength: 0.018,
            dirt_density: 0.62,
            chip_density: 0.42,
            crack_density: 0.25,
            decal_density: 0.35,
            scatter_density: 0.2,
            wetness_bias: 0.7,
            lod_rules: DetailLodRulesV15::default(),
        }
    }

    pub fn facade(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV15::BuildingFacade,
            factory_made: false,
            silhouette_variation: 0.04,
            bevel_radius_min_meters: 0.015,
            bevel_radius_max_meters: 0.08,
            surface_warp_strength: 0.012,
            dirt_density: 0.68,
            chip_density: 0.24,
            crack_density: 0.22,
            decal_density: 0.6,
            scatter_density: 0.1,
            wetness_bias: 0.45,
            lod_rules: DetailLodRulesV15::default(),
        }
    }

    pub fn human(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV15::Human,
            factory_made: false,
            silhouette_variation: 0.08,
            bevel_radius_min_meters: 0.0,
            bevel_radius_max_meters: 0.0,
            surface_warp_strength: 0.0,
            dirt_density: 0.15,
            chip_density: 0.0,
            crack_density: 0.0,
            decal_density: 0.1,
            scatter_density: 0.0,
            wetness_bias: 0.25,
            lod_rules: DetailLodRulesV15 {
                preserve_silhouette_until: BeautyDetailTierV15::Impostor,
                min_bevel_visible_meters: 0.0,
                min_material_page_scale: 0.35,
                may_drop_scatter: true,
                may_drop_decals: true,
                may_be_impostor: true,
            },
        }
    }

    pub fn vehicle(seed: u64) -> Self {
        Self {
            seed,
            object_class: BeautyObjectClassV15::Vehicle,
            factory_made: true,
            silhouette_variation: 0.015,
            bevel_radius_min_meters: 0.02,
            bevel_radius_max_meters: 0.12,
            surface_warp_strength: 0.003,
            dirt_density: 0.35,
            chip_density: 0.08,
            crack_density: 0.03,
            decal_density: 0.25,
            scatter_density: 0.0,
            wetness_bias: 0.55,
            lod_rules: DetailLodRulesV15 {
                preserve_silhouette_until: BeautyDetailTierV15::Impostor,
                min_bevel_visible_meters: 0.02,
                min_material_page_scale: 0.3,
                may_drop_scatter: true,
                may_drop_decals: true,
                may_be_impostor: true,
            },
        }
    }
}

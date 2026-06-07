//! World-space Beauty scene geometry contract.

use super::artifact_policy::BeautyArtifactCountersV19;
use super::environment::{NaturalEnvironmentV19, WorldBiomeV19};
use super::frame_budget::FrameBudgetConfigV19;
use super::materials::{BeautyMaterialIdV19, BeautySurfaceIdV19, SurfaceTextureRecipeV19};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3V19 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3V19 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundsV19 {
    pub min: Vec3V19,
    pub max: Vec3V19,
}

impl BoundsV19 {
    pub fn contains_xy(&self, p: Vec3V19) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautySceneV19 {
    pub version: u32,
    pub seed: u64,
    pub environment: NaturalEnvironmentV19,
    pub frame_budget: FrameBudgetConfigV19,
    pub cells: Vec<BeautyCellPackageV19>,
    pub material_recipes: Vec<SurfaceTextureRecipeV19>,
    pub artifact_counters: BeautyArtifactCountersV19,
}

impl BeautySceneV19 {
    pub fn empty(seed: u64) -> Self {
        Self {
            version: 19,
            seed,
            environment: NaturalEnvironmentV19::overcast_city_nature_landfill(),
            frame_budget: FrameBudgetConfigV19::smooth_60hz(),
            cells: Vec::new(),
            material_recipes: Vec::new(),
            artifact_counters: BeautyArtifactCountersV19::default(),
        }
    }

    pub fn biome_count(&self, biome: WorldBiomeV19) -> usize {
        self.cells.iter().filter(|cell| cell.biome == biome).count()
    }

    pub fn human_count(&self) -> usize {
        self.cells.iter().map(|cell| cell.humans.len()).sum()
    }

    pub fn vehicle_count(&self) -> usize {
        self.cells.iter().map(|cell| cell.vehicles.len()).sum()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyCellPackageV19 {
    pub cell_id: u64,
    pub biome: WorldBiomeV19,
    pub bounds: BoundsV19,
    pub terrain: Vec<TerrainPatchV19>,
    pub roads: Vec<RoadPatchV19>,
    pub curbs: Vec<CurbSegmentV19>,
    pub facades: Vec<FacadeModuleV19>,
    pub plants: Vec<PlantInstanceV19>,
    pub stones: Vec<StoneInstanceV19>,
    pub landfill_props: Vec<LandfillPropV19>,
    pub water_films: Vec<GroundedWaterFilmV19>,
    pub humans: Vec<HumanProxyV19>,
    pub vehicles: Vec<VehicleProxyV19>,
}

impl BeautyCellPackageV19 {
    pub fn new(cell_id: u64, biome: WorldBiomeV19, bounds: BoundsV19) -> Self {
        Self {
            cell_id,
            biome,
            bounds,
            terrain: Vec::new(),
            roads: Vec::new(),
            curbs: Vec::new(),
            facades: Vec::new(),
            plants: Vec::new(),
            stones: Vec::new(),
            landfill_props: Vec::new(),
            water_films: Vec::new(),
            humans: Vec::new(),
            vehicles: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPatchV19 {
    pub surface_id: BeautySurfaceIdV19,
    pub material_id: BeautyMaterialIdV19,
    pub center: Vec3V19,
    pub size_meters: [f32; 2],
    pub unevenness_0_to_1: f32,
    pub moisture_0_to_1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadPatchV19 {
    pub surface_id: BeautySurfaceIdV19,
    pub material_id: BeautyMaterialIdV19,
    pub control_points: Vec<Vec3V19>,
    pub width_meters: f32,
    pub camber_0_to_1: f32,
    pub crack_density_0_to_1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurbSegmentV19 {
    pub surface_id: BeautySurfaceIdV19,
    pub material_id: BeautyMaterialIdV19,
    pub start: Vec3V19,
    pub end: Vec3V19,
    pub radius_meters: f32,
    pub chip_density_0_to_1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FacadeModuleV19 {
    pub surface_id: BeautySurfaceIdV19,
    pub material_id: BeautyMaterialIdV19,
    pub origin: Vec3V19,
    pub width_meters: f32,
    pub height_meters: f32,
    pub depth_meters: f32,
    pub bevel_radius_meters: f32,
    pub grime_0_to_1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlantInstanceV19 {
    pub surface_id: BeautySurfaceIdV19,
    pub material_id: BeautyMaterialIdV19,
    pub root_position: Vec3V19,
    pub height_meters: f32,
    pub bend_0_to_1: f32,
    pub leaf_density_0_to_1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoneInstanceV19 {
    pub surface_id: BeautySurfaceIdV19,
    pub material_id: BeautyMaterialIdV19,
    pub center: Vec3V19,
    pub radius_meters: f32,
    pub angularity_0_to_1: f32,
    pub chip_density_0_to_1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LandfillPropV19 {
    pub surface_id: BeautySurfaceIdV19,
    pub material_id: BeautyMaterialIdV19,
    pub center: Vec3V19,
    pub size_meters: [f32; 3],
    pub deformation_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub rust_or_stain_0_to_1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroundedWaterFilmV19 {
    pub surface_id: BeautySurfaceIdV19,
    pub receiver_surface_id: BeautySurfaceIdV19,
    pub center: Vec3V19,
    pub normal: Vec3V19,
    pub radius_meters: f32,
    pub depth_meters: f32,
    pub edge_irregularity_0_to_1: f32,
}

impl GroundedWaterFilmV19 {
    pub fn is_grounded(&self) -> bool {
        self.receiver_surface_id.0 != 0
            && self.normal.z > 0.65
            && self.depth_meters >= 0.0
            && self.depth_meters <= 0.08
            && self.radius_meters > 0.05
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanProxyV19 {
    pub entity_id: u64,
    pub world_position: Vec3V19,
    pub height_meters: f32,
    pub shoulder_width_meters: f32,
    pub has_head: bool,
    pub has_torso: bool,
    pub has_limbs: bool,
    pub has_clothing_material: bool,
    pub has_skin_material: bool,
    pub camera_relative: bool,
}

impl HumanProxyV19 {
    pub fn is_believable_proxy(&self) -> bool {
        !self.camera_relative
            && self.height_meters >= 1.35
            && self.height_meters <= 2.15
            && self.shoulder_width_meters >= 0.28
            && self.shoulder_width_meters <= 0.72
            && self.has_head
            && self.has_torso
            && self.has_limbs
            && self.has_clothing_material
            && self.has_skin_material
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VehicleProxyV19 {
    pub entity_id: u64,
    pub world_position: Vec3V19,
    pub length_meters: f32,
    pub width_meters: f32,
    pub height_meters: f32,
    pub has_wheels_or_hover_equivalent: bool,
    pub has_cabin: bool,
    pub has_glass: bool,
    pub has_lights: bool,
    pub has_panel_seams: bool,
    pub box_placeholder: bool,
}

impl VehicleProxyV19 {
    pub fn is_believable_proxy(&self) -> bool {
        !self.box_placeholder
            && self.length_meters >= 2.2
            && self.length_meters <= 7.0
            && self.width_meters >= 1.2
            && self.width_meters <= 3.2
            && self.height_meters >= 0.9
            && self.height_meters <= 3.2
            && self.has_wheels_or_hover_equivalent
            && self.has_cabin
            && self.has_glass
            && self.has_lights
            && self.has_panel_seams
    }
}

//! V21 player-facing Beauty scene contract.

use super::artifacts::{BeautyModeQuarantineV21, VisualArtifactCountersV21};
use super::environment::NaturalEnvironmentV21;
use super::frame_budget::FrameBudgetConfigV21;
use super::materials::SurfaceTextureRecipeV21;
use super::world::{BeautyCellPackageV21, WorldBiomeV21};

#[derive(Clone, Debug, PartialEq)]
pub struct BeautySceneV21 {
    pub scene_id: u64,
    pub environment: NaturalEnvironmentV21,
    pub quarantine: BeautyModeQuarantineV21,
    pub artifact_counters: VisualArtifactCountersV21,
    pub frame_budget: FrameBudgetConfigV21,
    pub material_recipes: Vec<SurfaceTextureRecipeV21>,
    pub cells: Vec<BeautyCellPackageV21>,
    pub previous_versions_quarantined: bool,
    pub pure_beauty_debug_overlays_disabled: bool,
}

impl BeautySceneV21 {
    pub fn empty(scene_id: u64) -> Self {
        Self {
            scene_id,
            environment: NaturalEnvironmentV21::city_nature_landfill_day(),
            quarantine: BeautyModeQuarantineV21::strict_beauty(),
            artifact_counters: VisualArtifactCountersV21::default(),
            frame_budget: FrameBudgetConfigV21::smooth_60hz(),
            material_recipes: Vec::new(),
            cells: Vec::new(),
            previous_versions_quarantined: true,
            pure_beauty_debug_overlays_disabled: true,
        }
    }

    pub fn has_biome(&self, biome: WorldBiomeV21) -> bool {
        self.cells.iter().any(|cell| cell.biome == biome)
    }
    pub fn visible_content_count(&self) -> usize {
        self.cells
            .iter()
            .map(BeautyCellPackageV21::visible_content_count)
            .sum()
    }
    pub fn human_count(&self) -> usize {
        self.cells.iter().map(|cell| cell.humans.len()).sum()
    }
    pub fn vehicle_count(&self) -> usize {
        self.cells.iter().map(|cell| cell.vehicles.len()).sum()
    }
    pub fn water_film_count(&self) -> usize {
        self.cells.iter().map(|cell| cell.water_films.len()).sum()
    }
    pub fn plant_count(&self) -> usize {
        self.cells.iter().map(|cell| cell.plants.len()).sum()
    }
    pub fn stone_count(&self) -> usize {
        self.cells.iter().map(|cell| cell.stones.len()).sum()
    }
    pub fn landfill_prop_count(&self) -> usize {
        self.cells
            .iter()
            .map(|cell| cell.landfill_props.len())
            .sum()
    }
}

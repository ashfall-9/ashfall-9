//! V20 player-facing Beauty Scene contract.

use super::artifact_quarantine::{BeautyModeQuarantineV20, VisualArtifactCountersV20};
use super::environment::NaturalEnvironmentV20;
use super::frame_budget::FrameBudgetConfigV20;
use super::material_pages::SurfaceTextureRecipeV20;
use super::world::{BeautyCellPackageV20, WorldBiomeV20};

#[derive(Clone, Debug, PartialEq)]
pub struct BeautySceneV20 {
    pub scene_id: u64,
    pub environment: NaturalEnvironmentV20,
    pub quarantine: BeautyModeQuarantineV20,
    pub artifact_counters: VisualArtifactCountersV20,
    pub frame_budget: FrameBudgetConfigV20,
    pub material_recipes: Vec<SurfaceTextureRecipeV20>,
    pub cells: Vec<BeautyCellPackageV20>,
}

impl BeautySceneV20 {
    pub fn empty(scene_id: u64) -> Self {
        Self {
            scene_id,
            environment: NaturalEnvironmentV20::city_nature_landfill_day(),
            quarantine: BeautyModeQuarantineV20::strict_beauty(),
            artifact_counters: VisualArtifactCountersV20::default(),
            frame_budget: FrameBudgetConfigV20::smooth_60hz(),
            material_recipes: Vec::new(),
            cells: Vec::new(),
        }
    }

    pub fn has_biome(&self, biome: WorldBiomeV20) -> bool {
        self.cells.iter().any(|cell| cell.biome == biome)
    }

    pub fn visible_content_count(&self) -> usize {
        self.cells
            .iter()
            .map(BeautyCellPackageV20::visible_content_count)
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
}

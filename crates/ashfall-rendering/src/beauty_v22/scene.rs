//! V22 player-facing Beauty scene contract.

use super::artifacts::{
    BeautyModeQuarantineV22, LegacyBeautyPathFlagsV22, VisualArtifactCountersV22,
};
use super::environment::NaturalEnvironmentV22;
use super::frame_budget::FrameBudgetConfigV22;
use super::materials::SurfaceTextureRecipeV22;
use super::world::{BeautyCellPackageV22, WorldBiomeV22};

#[derive(Clone, Debug, PartialEq)]
pub struct BeautySceneV22 {
    pub scene_id: u64,
    pub environment: NaturalEnvironmentV22,
    pub quarantine: BeautyModeQuarantineV22,
    pub artifact_counters: VisualArtifactCountersV22,
    pub legacy_path_flags: LegacyBeautyPathFlagsV22,
    pub frame_budget: FrameBudgetConfigV22,
    pub material_recipes: Vec<SurfaceTextureRecipeV22>,
    pub cells: Vec<BeautyCellPackageV22>,
    pub previous_versions_quarantined: bool,
    pub pure_beauty_debug_overlays_disabled: bool,
    pub pure_beauty_uses_only_v22: bool,
}

impl BeautySceneV22 {
    pub fn empty(scene_id: u64) -> Self {
        Self {
            scene_id,
            environment: NaturalEnvironmentV22::city_nature_landfill_overcast_day(),
            quarantine: BeautyModeQuarantineV22::strict_beauty(),
            artifact_counters: VisualArtifactCountersV22::default(),
            legacy_path_flags: LegacyBeautyPathFlagsV22::default(),
            frame_budget: FrameBudgetConfigV22::smooth_60hz(),
            material_recipes: Vec::new(),
            cells: Vec::new(),
            previous_versions_quarantined: true,
            pure_beauty_debug_overlays_disabled: true,
            pure_beauty_uses_only_v22: true,
        }
    }

    pub fn has_biome(&self, biome: WorldBiomeV22) -> bool {
        self.cells.iter().any(|cell| cell.biome == biome)
    }

    pub fn visible_content_count(&self) -> usize {
        self.cells
            .iter()
            .map(BeautyCellPackageV22::visible_content_count)
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

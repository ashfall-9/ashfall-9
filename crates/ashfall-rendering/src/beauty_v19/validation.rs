//! V19 Beauty validation.

use super::artifact_policy::{BeautyArtifactCountersV19, BeautyArtifactPolicyV19};
use super::draw_contract::BeautyDrawListV19;
use super::environment::WorldBiomeV19;
use super::geometry::BeautySceneV19;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeautyValidationReportV19 {
    pub passed: bool,
    pub errors: Vec<String>,
}

impl BeautyValidationReportV19 {
    pub fn pass() -> Self {
        Self {
            passed: true,
            errors: Vec::new(),
        }
    }

    pub fn fail(errors: Vec<String>) -> Self {
        Self {
            passed: false,
            errors,
        }
    }
}

pub fn validate_scene_v19(scene: &BeautySceneV19) -> BeautyValidationReportV19 {
    let mut errors = Vec::new();
    let policy = BeautyArtifactPolicyV19::strict_beauty();
    if !policy.accepts(scene.artifact_counters) {
        errors.push(format!(
            "Beauty artifacts are present: {:?}",
            scene.artifact_counters
        ));
    }
    if !scene.environment.has_natural_readability() {
        errors.push("Scene has no natural sun/moon readability".to_string());
    }
    if !scene.environment.bloom_is_restrained() {
        errors.push("Bloom/glare is too strong for V19 artifact policy".to_string());
    }
    if scene.biome_count(WorldBiomeV19::City) == 0 {
        errors.push("Missing city biome cell".to_string());
    }
    if scene.biome_count(WorldBiomeV19::NatureReserve) == 0 {
        errors.push("Missing nature biome cell".to_string());
    }
    if scene.biome_count(WorldBiomeV19::Landfill) == 0 {
        errors.push("Missing landfill biome cell".to_string());
    }
    if scene.human_count() == 0 {
        errors.push("Missing world-anchored human proxy".to_string());
    }
    if scene.vehicle_count() == 0 {
        errors.push("Missing world-anchored vehicle proxy".to_string());
    }
    for cell in &scene.cells {
        for water in &cell.water_films {
            if !water.is_grounded() {
                errors.push(format!("Cell {} has ungrounded water film", cell.cell_id));
            }
        }
        for human in &cell.humans {
            if !human.is_believable_proxy() {
                errors.push(format!(
                    "Cell {} has invalid human proxy {}",
                    cell.cell_id, human.entity_id
                ));
            }
        }
        for vehicle in &cell.vehicles {
            if !vehicle.is_believable_proxy() {
                errors.push(format!(
                    "Cell {} has invalid vehicle proxy {}",
                    cell.cell_id, vehicle.entity_id
                ));
            }
        }
    }
    for recipe in &scene.material_recipes {
        if !recipe.is_structured_enough() {
            errors.push(format!(
                "Material recipe {:?}/{:?} is not structured enough or risks smearing",
                recipe.surface_id, recipe.material_id
            ));
        }
    }
    if errors.is_empty() {
        BeautyValidationReportV19::pass()
    } else {
        BeautyValidationReportV19::fail(errors)
    }
}

pub fn validate_draw_list_v19(
    draw_list: &BeautyDrawListV19,
    counters: BeautyArtifactCountersV19,
) -> BeautyValidationReportV19 {
    let mut errors = Vec::new();
    if !BeautyArtifactPolicyV19::strict_beauty().accepts(counters) {
        errors.push("Artifact counters reject this draw list".to_string());
    }
    let forbidden = draw_list.forbidden_item_count();
    if forbidden > 0 {
        errors.push(format!(
            "Draw list contains {forbidden} forbidden Beauty primitives"
        ));
    }
    if errors.is_empty() {
        BeautyValidationReportV19::pass()
    } else {
        BeautyValidationReportV19::fail(errors)
    }
}

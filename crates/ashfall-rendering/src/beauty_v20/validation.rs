//! V20 Beauty validation.

use super::material_pages::{SurfaceTextureRecipeV20, TextureChannelV20};
use super::scene::BeautySceneV20;
use super::world::WorldBiomeV20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeautyValidationIssueV20 {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeautyValidationReportV20 {
    pub passed: bool,
    pub issues: Vec<BeautyValidationIssueV20>,
}

impl BeautyValidationReportV20 {
    pub fn pass() -> Self {
        Self {
            passed: true,
            issues: Vec::new(),
        }
    }

    pub fn fail(issues: Vec<BeautyValidationIssueV20>) -> Self {
        Self {
            passed: false,
            issues,
        }
    }
}

pub fn validate_beauty_scene_v20(scene: &BeautySceneV20) -> BeautyValidationReportV20 {
    let mut issues = Vec::new();

    if !scene.quarantine.accepts(scene.artifact_counters) {
        issues.push(issue(
            "artifact_leak",
            format!(
                "strict Beauty Mode rejected {} artifacts",
                scene.artifact_counters.total()
            ),
        ));
    }

    if !scene.environment.has_natural_readability() {
        issues.push(issue(
            "weak_natural_light",
            "scene must have readable sun or moon lighting",
        ));
    }

    if !scene.environment.bloom_is_restrained() {
        issues.push(issue(
            "fake_glare_risk",
            "bloom/glare settings are too aggressive",
        ));
    }

    if !scene.environment.neon_is_accent() {
        issues.push(issue(
            "neon_not_accent",
            "neon multiplier must remain an accent in V20 Beauty",
        ));
    }

    for biome in [
        WorldBiomeV20::City,
        WorldBiomeV20::NatureReserve,
        WorldBiomeV20::Landfill,
    ] {
        if !scene.has_biome(biome) {
            issues.push(issue(
                "missing_biome",
                format!("missing required biome: {biome:?}"),
            ));
        }
    }

    if scene.visible_content_count() < 40 {
        issues.push(issue(
            "too_little_content",
            "V20 golden scene must contain enough visible world content",
        ));
    }

    if scene.human_count() < 3 {
        issues.push(issue(
            "too_few_humans",
            "V20 golden scene must contain at least 3 world-anchored humans",
        ));
    }

    if scene.vehicle_count() < 2 {
        issues.push(issue(
            "too_few_vehicles",
            "V20 golden scene must contain at least 2 proportionate vehicles/machines",
        ));
    }

    if scene.water_film_count() == 0 {
        issues.push(issue(
            "missing_grounded_water",
            "V20 golden scene should contain grounded puddles/water films",
        ));
    }

    for cell in &scene.cells {
        if !cell.has_grounded_water_only() {
            issues.push(issue(
                "floating_water",
                format!("cell {} contains ungrounded water film", cell.cell_id),
            ));
        }
        if !cell.has_valid_humans() {
            issues.push(issue(
                "invalid_human",
                format!(
                    "cell {} contains rod/camera-relative/incomplete human",
                    cell.cell_id
                ),
            ));
        }
        if !cell.has_valid_vehicles() {
            issues.push(issue(
                "invalid_vehicle",
                format!("cell {} contains box/improper vehicle", cell.cell_id),
            ));
        }
    }

    for recipe in &scene.material_recipes {
        validate_material_recipe(recipe, &mut issues);
    }

    if issues.is_empty() {
        BeautyValidationReportV20::pass()
    } else {
        BeautyValidationReportV20::fail(issues)
    }
}

fn validate_material_recipe(
    recipe: &SurfaceTextureRecipeV20,
    issues: &mut Vec<BeautyValidationIssueV20>,
) {
    if !recipe.is_structured_enough() {
        issues.push(issue(
            "material_not_structured",
            format!(
                "material {:?} surface {:?} is not structured enough",
                recipe.material_id, recipe.surface_id
            ),
        ));
    }
    if !recipe.has_channel(TextureChannelV20::BaseColor) {
        issues.push(issue(
            "material_missing_base_color",
            format!("surface {:?} missing base color", recipe.surface_id),
        ));
    }
    if !recipe.has_channel(TextureChannelV20::Normal) {
        issues.push(issue(
            "material_missing_normal",
            format!("surface {:?} missing normal", recipe.surface_id),
        ));
    }
    if !recipe.has_channel(TextureChannelV20::Roughness) {
        issues.push(issue(
            "material_missing_roughness",
            format!("surface {:?} missing roughness", recipe.surface_id),
        ));
    }
    if recipe.procedural_warp_0_to_1 > 0.10 || recipe.smear_risk_0_to_1 > 0.15 {
        issues.push(issue(
            "material_smear_risk",
            format!("surface {:?} risks procedural smear", recipe.surface_id),
        ));
    }
}

fn issue(code: &'static str, message: impl Into<String>) -> BeautyValidationIssueV20 {
    BeautyValidationIssueV20 {
        code,
        message: message.into(),
    }
}

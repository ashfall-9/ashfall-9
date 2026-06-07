//! V21 Beauty validation.

use super::materials::{SurfaceTextureRecipeV21, TextureChannelV21};
use super::scene::BeautySceneV21;
use super::world::WorldBiomeV21;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeautyValidationIssueV21 {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeautyValidationReportV21 {
    pub passed: bool,
    pub issues: Vec<BeautyValidationIssueV21>,
}

impl BeautyValidationReportV21 {
    pub fn pass() -> Self {
        Self {
            passed: true,
            issues: Vec::new(),
        }
    }
    pub fn fail(issues: Vec<BeautyValidationIssueV21>) -> Self {
        Self {
            passed: false,
            issues,
        }
    }
}

pub fn validate_beauty_scene_v21(scene: &BeautySceneV21) -> BeautyValidationReportV21 {
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
    if !scene.previous_versions_quarantined {
        issues.push(issue(
            "old_beauty_versions_active",
            "pure Beauty Mode must not mix V15-V20 placeholder paths with V21",
        ));
    }
    if !scene.pure_beauty_debug_overlays_disabled {
        issues.push(issue(
            "debug_overlays_enabled",
            "pure Beauty Mode must disable debug overlays",
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
            "bloom/glare settings are too aggressive or fake glare sprites are allowed",
        ));
    }
    if !scene.environment.neon_is_accent() {
        issues.push(issue(
            "neon_not_accent",
            "neon multiplier must remain an accent in V21 Beauty",
        ));
    }

    for biome in [
        WorldBiomeV21::City,
        WorldBiomeV21::NatureReserve,
        WorldBiomeV21::Landfill,
    ] {
        if !scene.has_biome(biome) {
            issues.push(issue(
                "missing_biome",
                format!("missing required biome: {biome:?}"),
            ));
        }
    }

    if scene.visible_content_count() < 75 {
        issues.push(issue(
            "too_little_content",
            "V21 golden scene must contain enough visible world content",
        ));
    }
    if scene.human_count() < 3 {
        issues.push(issue(
            "too_few_humans",
            "V21 golden scene must contain at least 3 world-anchored non-cartoon humans",
        ));
    }
    if scene.vehicle_count() < 2 {
        issues.push(issue(
            "too_few_vehicles",
            "V21 golden scene must contain at least 2 proportionate vehicles/machines",
        ));
    }
    if scene.water_film_count() == 0 {
        issues.push(issue(
            "missing_grounded_water",
            "V21 golden scene should contain grounded puddles/water films",
        ));
    }
    if scene.plant_count() < 20 {
        issues.push(issue(
            "too_few_plants",
            "V21 nature area should contain plant life",
        ));
    }
    if scene.stone_count() < 10 {
        issues.push(issue(
            "too_few_stones",
            "V21 nature/landfill area should contain stones",
        ));
    }
    if scene.landfill_prop_count() < 10 {
        issues.push(issue(
            "too_few_landfill_props",
            "V21 landfill area should contain visible debris",
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
                    "cell {} contains rod/cartoon/camera-relative/incomplete human",
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
        if !cell.has_valid_geometry() {
            issues.push(issue(
                "invalid_geometry",
                format!(
                    "cell {} contains blocky/invalid Beauty geometry",
                    cell.cell_id
                ),
            ));
        }
    }

    for recipe in &scene.material_recipes {
        validate_material_recipe(recipe, &mut issues);
    }

    if issues.is_empty() {
        BeautyValidationReportV21::pass()
    } else {
        BeautyValidationReportV21::fail(issues)
    }
}

fn validate_material_recipe(
    recipe: &SurfaceTextureRecipeV21,
    issues: &mut Vec<BeautyValidationIssueV21>,
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
    for channel in [
        TextureChannelV21::BaseColor,
        TextureChannelV21::Normal,
        TextureChannelV21::Roughness,
    ] {
        if !recipe.has_channel(channel) {
            issues.push(issue(
                "material_missing_required_channel",
                format!("surface {:?} missing {channel:?}", recipe.surface_id),
            ));
        }
    }
    if recipe.procedural_warp_0_to_1 > 0.065 || recipe.smear_risk_0_to_1 > 0.075 {
        issues.push(issue(
            "material_smear_risk",
            format!("surface {:?} risks procedural smear", recipe.surface_id),
        ));
    }
}

fn issue(code: &'static str, message: impl Into<String>) -> BeautyValidationIssueV21 {
    BeautyValidationIssueV21 {
        code,
        message: message.into(),
    }
}

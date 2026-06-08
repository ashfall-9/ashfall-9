//! V22 Beauty validation.

use super::materials::{SurfaceTextureRecipeV22, TextureChannelV22};
use super::scene::BeautySceneV22;
use super::world::WorldBiomeV22;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeautyValidationIssueV22 {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeautyValidationReportV22 {
    pub passed: bool,
    pub issues: Vec<BeautyValidationIssueV22>,
}

impl BeautyValidationReportV22 {
    pub fn pass() -> Self {
        Self {
            passed: true,
            issues: Vec::new(),
        }
    }

    pub fn fail(issues: Vec<BeautyValidationIssueV22>) -> Self {
        Self {
            passed: false,
            issues,
        }
    }
}

pub fn validate_beauty_scene_v22(scene: &BeautySceneV22) -> BeautyValidationReportV22 {
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

    if scene.legacy_path_flags.any_active() {
        issues.push(issue(
            "legacy_beauty_path_active",
            "pure Beauty Mode must not invoke V15-V21 player-facing paths",
        ));
    }

    if !scene.previous_versions_quarantined || !scene.pure_beauty_uses_only_v22 {
        issues.push(issue(
            "old_beauty_versions_active",
            "pure Beauty Mode must use only V22 for player-facing draw",
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
            "neon multiplier must remain an accent in V22 Beauty",
        ));
    }

    if !scene.environment.no_camera_attached_environment_effects() {
        issues.push(issue(
            "camera_attached_environment_effects",
            "sky/weather must not depend on camera-wave backgrounds or screen stripes",
        ));
    }

    for biome in [
        WorldBiomeV22::City,
        WorldBiomeV22::NatureReserve,
        WorldBiomeV22::Landfill,
    ] {
        if !scene.has_biome(biome) {
            issues.push(issue(
                "missing_biome",
                format!("missing required biome: {biome:?}"),
            ));
        }
    }

    if scene.visible_content_count() < 90 {
        issues.push(issue(
            "too_little_content",
            "V22 golden scene must contain enough visible world content",
        ));
    }

    if scene.human_count() < 3 {
        issues.push(issue(
            "too_few_humans",
            "V22 golden scene must contain at least 3 coherent, world-anchored humans",
        ));
    }

    if scene.vehicle_count() < 2 {
        issues.push(issue(
            "too_few_vehicles",
            "V22 golden scene must contain at least 2 coherent vehicles/machines",
        ));
    }

    if scene.water_film_count() == 0 {
        issues.push(issue(
            "missing_grounded_water",
            "V22 golden scene should contain grounded water films",
        ));
    }

    if scene.plant_count() < 24 {
        issues.push(issue(
            "too_few_plants",
            "V22 nature area should contain plant life",
        ));
    }

    if scene.stone_count() < 12 {
        issues.push(issue(
            "too_few_stones",
            "V22 nature/landfill area should contain stones",
        ));
    }

    if scene.landfill_prop_count() < 12 {
        issues.push(issue(
            "too_few_landfill_props",
            "V22 landfill area should contain visible debris",
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
                    "cell {} contains fractured/rod/cartoon/camera-relative human",
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
        BeautyValidationReportV22::pass()
    } else {
        BeautyValidationReportV22::fail(issues)
    }
}

fn validate_material_recipe(
    recipe: &SurfaceTextureRecipeV22,
    issues: &mut Vec<BeautyValidationIssueV22>,
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
        TextureChannelV22::BaseColor,
        TextureChannelV22::Normal,
        TextureChannelV22::Roughness,
        TextureChannelV22::AmbientOcclusion,
    ] {
        if !recipe.has_channel(channel) {
            issues.push(issue(
                "material_missing_required_channel",
                format!("surface {:?} missing {channel:?}", recipe.surface_id),
            ));
        }
    }

    if recipe.procedural_warp_0_to_1 > 0.040 || recipe.smear_risk_0_to_1 > 0.045 {
        issues.push(issue(
            "material_smear_risk",
            format!("surface {:?} risks procedural smear", recipe.surface_id),
        ));
    }
}

fn issue(code: &'static str, message: impl Into<String>) -> BeautyValidationIssueV22 {
    BeautyValidationIssueV22 {
        code,
        message: message.into(),
    }
}

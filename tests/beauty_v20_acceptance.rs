use ashfall::beauty_scene_v20_bridge::{BeautySceneBuildContextV20, build_beauty_scene_v20};
use ashfall::beauty_scene_v20_capture::capture_beauty_v20_golden_scenes;
use ashfall_rendering::beauty_v20::*;

#[test]
fn v20_golden_scene_has_city_nature_landfill() {
    let scene = build_beauty_scene_v20(BeautySceneBuildContextV20::default());
    assert!(scene.has_biome(WorldBiomeV20::City));
    assert!(scene.has_biome(WorldBiomeV20::NatureReserve));
    assert!(scene.has_biome(WorldBiomeV20::Landfill));
}

#[test]
fn v20_beauty_scene_rejects_visual_artifacts() {
    let scene = build_beauty_scene_v20(BeautySceneBuildContextV20::default());
    assert!(scene.quarantine.accepts(scene.artifact_counters));
    assert_eq!(scene.artifact_counters.total(), 0);

    let mut bad = scene.artifact_counters;
    bad.horizontal_screen_stripes = 1;
    assert!(!scene.quarantine.accepts(bad));

    let mut bad = scene.artifact_counters;
    bad.camera_relative_circles = 1;
    assert!(!scene.quarantine.accepts(bad));

    let mut bad = scene.artifact_counters;
    bad.procedural_smear_surfaces = 1;
    assert!(!scene.quarantine.accepts(bad));
}

#[test]
fn v20_materials_are_structured_not_smears() {
    let scene = build_beauty_scene_v20(BeautySceneBuildContextV20::default());
    assert!(
        scene.material_recipes.len() >= 16,
        "V20 should cover asphalt, concrete, soil, stone, plants, landfill, humans, vehicles, glass, rubber, and metal"
    );
    for recipe in &scene.material_recipes {
        assert!(
            recipe.is_structured_enough(),
            "material {:?} is not structured",
            recipe.surface_id
        );
        assert!(recipe.has_channel(TextureChannelV20::BaseColor));
        assert!(recipe.has_channel(TextureChannelV20::Normal));
        assert!(recipe.has_channel(TextureChannelV20::Roughness));
        assert!(recipe.procedural_warp_0_to_1 <= 0.10);
        assert!(recipe.smear_risk_0_to_1 <= 0.15);
    }
}

#[test]
fn v20_scene_has_valid_humans_vehicles_and_grounded_water() {
    let scene = build_beauty_scene_v20(BeautySceneBuildContextV20::default());
    assert!(
        scene.visible_content_count() >= 180,
        "V20 should include dense enough city/nature/landfill detail"
    );
    assert!(
        scene.human_count() >= 7,
        "V20 should not ship sparse or repeated mannequin placement"
    );
    assert!(
        scene.vehicle_count() >= 5,
        "V20 should have multiple proportionate vehicles/machines"
    );
    assert!(
        scene.water_film_count() >= 6,
        "V20 should test grounded puddles/water films"
    );

    for cell in &scene.cells {
        assert!(
            cell.has_grounded_water_only(),
            "cell {} has floating water film",
            cell.cell_id
        );
        assert!(
            cell.has_valid_humans(),
            "cell {} has invalid human proxy",
            cell.cell_id
        );
        assert!(
            cell.has_valid_vehicles(),
            "cell {} has invalid vehicle proxy",
            cell.cell_id
        );
    }
}

#[test]
fn v20_scene_passes_validation() {
    let scene = build_beauty_scene_v20(BeautySceneBuildContextV20::default());
    let report = validate_beauty_scene_v20(&scene);
    assert!(report.passed, "V20 validation failed: {:?}", report.issues);
}

#[test]
fn v20_headless_capture_writes_city_nature_and_landfill_images() {
    let output_dir = std::env::temp_dir().join("ashfall_v20_golden_capture_test");
    let _ = std::fs::remove_dir_all(&output_dir);

    let paths = capture_beauty_v20_golden_scenes(&output_dir)
        .expect("V20 golden captures should write BMP files");
    assert_eq!(paths.len(), 3);

    for path in paths {
        let bytes = std::fs::read(&path).expect("capture should be readable");
        assert!(bytes.len() > 54);
        assert_eq!(&bytes[0..2], b"BM");
    }
}

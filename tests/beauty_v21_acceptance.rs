use ashfall::beauty_scene_v21_bridge::{BeautySceneBuildContextV21, build_beauty_scene_v21};
use ashfall::beauty_scene_v21_capture::capture_beauty_v21_golden_scenes;
use ashfall_rendering::beauty_v21::{
    BeautyModeQuarantineV21, TextureChannelV21, WorldBiomeV21, sample_material_v21,
    validate_beauty_scene_v21,
};

#[test]
fn v21_golden_scene_passes_strict_validation() {
    let scene = build_beauty_scene_v21(BeautySceneBuildContextV21::default());
    let report = validate_beauty_scene_v21(&scene);
    assert!(
        report.passed,
        "V21 scene failed validation: {:#?}",
        report.issues
    );
}

#[test]
fn v21_contains_city_nature_and_landfill() {
    let scene = build_beauty_scene_v21(BeautySceneBuildContextV21::default());
    assert!(scene.has_biome(WorldBiomeV21::City));
    assert!(scene.has_biome(WorldBiomeV21::NatureReserve));
    assert!(scene.has_biome(WorldBiomeV21::Landfill));
}

#[test]
fn v21_pure_beauty_has_no_artifact_counters() {
    let scene = build_beauty_scene_v21(BeautySceneBuildContextV21::default());
    assert!(BeautyModeQuarantineV21::strict_beauty().accepts(scene.artifact_counters));
    assert_eq!(scene.artifact_counters.total(), 0);
}

#[test]
fn v21_has_non_cartoon_world_anchored_humans() {
    let scene = build_beauty_scene_v21(BeautySceneBuildContextV21::default());
    assert!(scene.human_count() >= 3);
    for cell in &scene.cells {
        for human in &cell.humans {
            assert!(human.visually_valid(), "invalid human: {human:#?}");
        }
    }
}

#[test]
fn v21_has_structured_materials_not_smears() {
    let scene = build_beauty_scene_v21(BeautySceneBuildContextV21::default());
    assert!(scene.material_recipes.len() >= 8);
    for recipe in &scene.material_recipes {
        assert!(
            recipe.is_structured_enough(),
            "unstructured recipe: {recipe:#?}"
        );
        assert!(recipe.has_channel(TextureChannelV21::BaseColor));
        assert!(recipe.has_channel(TextureChannelV21::Normal));
        assert!(recipe.has_channel(TextureChannelV21::Roughness));
        let sample = sample_material_v21(recipe, [0.23, 0.71], 1234);
        assert!(sample.structured_detail_score_0_to_1 >= 0.25);
    }
}

#[test]
fn v21_budget_has_real_caps_for_smoothness() {
    let scene = build_beauty_scene_v21(BeautySceneBuildContextV21::default());
    assert!(scene.frame_budget.target_frame_ms <= 16.67 + f32::EPSILON);
    assert!(scene.frame_budget.max_visible_cells <= 9);
    assert!(scene.frame_budget.max_material_pages_generated_per_frame <= 8);
    assert!(scene.frame_budget.max_geometry_upload_mb_per_frame <= 8.0);
    assert!(scene.frame_budget.max_texture_upload_mb_per_frame <= 12.0);
}

#[test]
fn v21_headless_capture_writes_city_nature_landfill_and_timings() {
    let output_dir = std::env::temp_dir().join("ashfall_v21_golden_capture_test");
    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir).expect("old V21 capture test dir should be removable");
    }

    let report = capture_beauty_v21_golden_scenes(&output_dir)
        .expect("V21 golden captures should write BMP files and timing evidence");

    assert_eq!(report.image_paths.len(), 3);
    assert_eq!(report.timings.len(), 3);
    assert!(report.scene_build_ms >= 0.0);
    assert!(report.timing_path.exists());
    let timing_report =
        std::fs::read_to_string(&report.timing_path).expect("timing report should be readable");
    assert!(timing_report.contains("target_frame_ms="));
    assert!(timing_report.contains("city render_ms="));
    assert!(timing_report.contains("nature render_ms="));
    assert!(timing_report.contains("landfill render_ms="));

    for path in report.image_paths {
        let bytes = std::fs::read(&path).expect("capture should be readable");
        assert!(bytes.len() > 54, "capture is too small: {path:?}");
        assert_eq!(&bytes[0..2], b"BM", "capture should be a BMP file");
    }
    for timing in report.timings {
        assert!(timing.render_ms >= 0.0);
        assert!(timing.write_ms >= 0.0);
        assert!(timing.visible_instances > 0);
        assert!(timing.material_recipe_count >= 8);
    }
}

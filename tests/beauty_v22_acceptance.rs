use ashfall::beauty_scene_v22_bridge::{BeautySceneBuildContextV22, build_beauty_scene_v22};
use ashfall::beauty_scene_v22_capture::capture_beauty_v22_golden_scenes;
use ashfall_rendering::beauty_v22::{
    BeautyModeQuarantineV22, LegacyBeautyPathFlagsV22, TextureChannelV22, WorldBiomeV22,
    sample_material_v22, validate_beauty_scene_v22,
};

#[test]
fn v22_golden_scene_passes_strict_validation() {
    let scene = build_beauty_scene_v22(BeautySceneBuildContextV22::default());
    let report = validate_beauty_scene_v22(&scene);
    assert!(
        report.passed,
        "V22 scene failed validation: {:#?}",
        report.issues
    );
}

#[test]
fn v22_quarantines_legacy_beauty_paths() {
    let scene = build_beauty_scene_v22(BeautySceneBuildContextV22 {
        legacy_paths_active: LegacyBeautyPathFlagsV22 {
            v21_active: true,
            ..LegacyBeautyPathFlagsV22::default()
        },
        ..BeautySceneBuildContextV22::default()
    });
    let report = validate_beauty_scene_v22(&scene);
    assert!(
        !report.passed,
        "V22 must fail if V21 is active in pure Beauty"
    );
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == "legacy_beauty_path_active")
    );
}

#[test]
fn v22_contains_city_nature_and_landfill() {
    let scene = build_beauty_scene_v22(BeautySceneBuildContextV22::default());
    assert!(scene.has_biome(WorldBiomeV22::City));
    assert!(scene.has_biome(WorldBiomeV22::NatureReserve));
    assert!(scene.has_biome(WorldBiomeV22::Landfill));
}

#[test]
fn v22_pure_beauty_has_no_artifact_counters() {
    let scene = build_beauty_scene_v22(BeautySceneBuildContextV22::default());
    assert!(BeautyModeQuarantineV22::strict_beauty().accepts(scene.artifact_counters));
    assert_eq!(scene.artifact_counters.total(), 0);
}

#[test]
fn v22_humans_are_coherent_and_not_fractured() {
    let scene = build_beauty_scene_v22(BeautySceneBuildContextV22::default());
    assert!(scene.human_count() >= 3);
    for cell in &scene.cells {
        for human in &cell.humans {
            assert!(human.visually_valid(), "invalid human: {human:#?}");
            assert_eq!(human.fracture_piece_count, 0);
            assert_eq!(human.disconnected_part_count, 0);
            assert!(human.mesh_binding.single_coherent_body_mesh);
            assert!(!human.mesh_binding.draw_as_disconnected_parts);
        }
    }
}

#[test]
fn v22_has_structured_materials_not_smears() {
    let scene = build_beauty_scene_v22(BeautySceneBuildContextV22::default());
    assert!(scene.material_recipes.len() >= 8);
    for recipe in &scene.material_recipes {
        assert!(
            recipe.is_structured_enough(),
            "unstructured recipe: {recipe:#?}"
        );
        assert!(recipe.has_channel(TextureChannelV22::BaseColor));
        assert!(recipe.has_channel(TextureChannelV22::Normal));
        assert!(recipe.has_channel(TextureChannelV22::Roughness));
        assert!(recipe.has_channel(TextureChannelV22::AmbientOcclusion));
        assert!(recipe.procedural_warp_0_to_1 <= 0.040);
        assert!(recipe.smear_risk_0_to_1 <= 0.045);

        let sample = sample_material_v22(recipe, [0.23, 0.71], 1234);
        assert!(sample.structured_detail_score_0_to_1 >= 0.30);
    }
}

#[test]
fn v22_budget_is_stricter_than_v21_style_placeholder_growth() {
    let scene = build_beauty_scene_v22(BeautySceneBuildContextV22::default());
    assert!(scene.frame_budget.target_frame_ms <= 16.67 + f32::EPSILON);
    assert!(scene.frame_budget.target_cpu_scene_build_ms <= 1.6 + f32::EPSILON);
    assert!(scene.frame_budget.max_visible_cells <= 7);
    assert!(scene.frame_budget.max_material_pages_generated_per_frame <= 4);
    assert!(scene.frame_budget.max_geometry_upload_mb_per_frame <= 6.0);
    assert!(scene.frame_budget.max_texture_upload_mb_per_frame <= 8.0);
    assert_eq!(scene.frame_budget.max_legacy_beauty_paths_active, 0);
}

#[test]
fn v22_headless_capture_writes_city_nature_landfill_and_timings() {
    let output_dir = std::env::temp_dir().join("ashfall_v22_golden_capture_test");
    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir).expect("old V22 capture test dir should be removable");
    }

    let report = capture_beauty_v22_golden_scenes(&output_dir)
        .expect("V22 golden captures should write BMP files and timing evidence");

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

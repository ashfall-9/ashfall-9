use ashfall::beauty_scene_v19_bridge::{BeautySceneBuildContextV19, build_beauty_scene_v19};
use ashfall::beauty_scene_v19_capture::capture_beauty_v19_golden_scenes;
use ashfall_rendering::beauty_v19::{
    BeautyArtifactCountersV19, BeautyArtifactPolicyV19, MaterialClassV19, WorldBiomeV19,
    validate_scene_v19,
};

#[test]
fn v19_scene_has_city_nature_and_landfill() {
    let scene = build_beauty_scene_v19(BeautySceneBuildContextV19::default());
    assert!(
        scene.biome_count(WorldBiomeV19::City) > 0,
        "city cell missing"
    );
    assert!(
        scene.biome_count(WorldBiomeV19::NatureReserve) > 0,
        "nature cell missing"
    );
    assert!(
        scene.biome_count(WorldBiomeV19::Landfill) > 0,
        "landfill cell missing"
    );
}

#[test]
fn v19_strict_beauty_rejects_stripes_circles_and_debug_leaks() {
    let policy = BeautyArtifactPolicyV19::strict_beauty();
    assert!(policy.accepts(BeautyArtifactCountersV19::default()));
    assert!(!policy.accepts(BeautyArtifactCountersV19 {
        horizontal_screen_stripes: 1,
        ..Default::default()
    }));
    assert!(!policy.accepts(BeautyArtifactCountersV19 {
        camera_relative_circles: 1,
        ..Default::default()
    }));
    assert!(!policy.accepts(BeautyArtifactCountersV19 {
        circular_glare_sprites: 1,
        ..Default::default()
    }));
    assert!(!policy.accepts(BeautyArtifactCountersV19 {
        debug_chunk_boxes: 1,
        ..Default::default()
    }));
    assert!(!policy.accepts(BeautyArtifactCountersV19 {
        procedural_smear_surfaces: 1,
        ..Default::default()
    }));
}

#[test]
fn v19_scene_validates_as_artifact_free_golden_scene_seed() {
    let scene = build_beauty_scene_v19(BeautySceneBuildContextV19::default());
    let report = validate_scene_v19(&scene);
    assert!(
        report.passed,
        "V19 golden scene failed validation: {:?}",
        report.errors
    );
}

#[test]
fn v19_materials_are_structured_not_smears() {
    let scene = build_beauty_scene_v19(BeautySceneBuildContextV19::default());
    assert!(
        scene
            .material_recipes
            .iter()
            .any(|r| r.class == MaterialClassV19::WetAsphalt)
    );
    assert!(
        scene
            .material_recipes
            .iter()
            .any(|r| r.class == MaterialClassV19::SoilMud)
    );
    assert!(
        scene
            .material_recipes
            .iter()
            .any(|r| r.class == MaterialClassV19::StoneRock)
    );
    assert!(
        scene
            .material_recipes
            .iter()
            .any(|r| r.class == MaterialClassV19::PlantLeaf)
    );
    assert!(
        scene
            .material_recipes
            .iter()
            .any(|r| r.class == MaterialClassV19::LandfillPlastic)
    );
    for recipe in &scene.material_recipes {
        assert!(
            recipe.is_structured_enough(),
            "bad material recipe: {:?}",
            recipe
        );
    }
}

#[test]
fn v19_humans_vehicles_and_puddles_are_world_anchored() {
    let scene = build_beauty_scene_v19(BeautySceneBuildContextV19::default());
    assert!(scene.human_count() >= 1, "missing human proxy");
    assert!(scene.vehicle_count() >= 1, "missing vehicle proxy");
    for cell in &scene.cells {
        for human in &cell.humans {
            assert!(human.is_believable_proxy());
            assert!(!human.camera_relative);
        }
        for vehicle in &cell.vehicles {
            assert!(vehicle.is_believable_proxy());
            assert!(!vehicle.box_placeholder);
        }
        for water in &cell.water_films {
            assert!(
                water.is_grounded(),
                "water film is not grounded: {:?}",
                water
            );
        }
    }
}

#[test]
fn v19_headless_capture_writes_city_nature_and_landfill_images() {
    let output_dir = std::env::temp_dir().join("ashfall_v19_golden_capture_test");
    let _ = std::fs::remove_dir_all(&output_dir);

    let paths = capture_beauty_v19_golden_scenes(&output_dir)
        .expect("V19 golden captures should write BMP files");

    assert_eq!(paths.len(), 3);
    for path in paths {
        let bytes = std::fs::read(&path).expect("capture should be readable");
        assert!(bytes.len() > 54, "capture is too small: {path:?}");
        assert_eq!(&bytes[0..2], b"BM", "capture should be a BMP file");
    }
}

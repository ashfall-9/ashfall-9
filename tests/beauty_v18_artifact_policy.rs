use ashfall_rendering::beauty_v18::{
    BeautyArtifactCountersV18, BeautyArtifactPolicyV18, BeautyMaterialIdV18, BeautySurfaceIdV18,
    BeautyV18Failure, BeautyValidationInputV18, NaturalLightingRigV18, SurfaceTextureRecipeV18,
};

#[test]
fn strict_beauty_policy_rejects_view_following_circles_and_fake_glare() {
    let input = BeautyValidationInputV18 {
        policy: BeautyArtifactPolicyV18::strict_beauty(),
        counters: BeautyArtifactCountersV18 {
            large_screen_space_ellipse_count: 3,
            camera_facing_haze_blob_count: 2,
            circular_fake_glare_count: 1,
            floating_puddle_count: 0,
            debug_box_count: 0,
            material_smear_fallback_count: 0,
        },
        lighting: NaturalLightingRigV18::default(),
        material_recipes: vec![SurfaceTextureRecipeV18::wet_asphalt(
            BeautySurfaceIdV18(1),
            BeautyMaterialIdV18(2),
        )],
    };

    let report = input.validate();
    assert!(!report.pass);
    assert!(
        report
            .failures
            .contains(&BeautyV18Failure::ArtifactPolicyViolation)
    );
}

#[test]
fn strict_beauty_policy_accepts_natural_light_and_non_smeared_materials() {
    let input = BeautyValidationInputV18 {
        policy: BeautyArtifactPolicyV18::strict_beauty(),
        counters: BeautyArtifactCountersV18::default(),
        lighting: NaturalLightingRigV18::rainy_day(),
        material_recipes: vec![
            SurfaceTextureRecipeV18::wet_asphalt(BeautySurfaceIdV18(1), BeautyMaterialIdV18(10)),
            SurfaceTextureRecipeV18::dirty_concrete(BeautySurfaceIdV18(2), BeautyMaterialIdV18(11)),
            SurfaceTextureRecipeV18::car_paint(BeautySurfaceIdV18(3), BeautyMaterialIdV18(12)),
        ],
    };

    let report = input.validate();
    assert!(
        report.pass,
        "V18 scene should pass without artifact leaks: {report:?}"
    );
}

#[test]
fn material_recipe_rejects_smear_level_warp() {
    let mut bad_recipe =
        SurfaceTextureRecipeV18::wet_asphalt(BeautySurfaceIdV18(1), BeautyMaterialIdV18(10));
    bad_recipe.procedural_warp_0_to_1 = 0.72;

    let input = BeautyValidationInputV18 {
        policy: BeautyArtifactPolicyV18::strict_beauty(),
        counters: BeautyArtifactCountersV18::default(),
        lighting: NaturalLightingRigV18::rainy_day(),
        material_recipes: vec![bad_recipe],
    };

    let report = input.validate();
    assert!(!report.pass);
    assert!(
        report
            .failures
            .contains(&BeautyV18Failure::MaterialRecipeSmears)
    );
}

use ashfall_rendering::beauty_v16::{
    AnchoredPuddleV16, BeautyDebugLeakFlagsV16, BeautyFailureReasonV16, BeautyObjectIdV16,
    BeautySceneV16, BeautySurfaceIdV16, EnvironmentStateV16,
};

#[test]
fn v16_environment_is_readable_without_neon() {
    let day = EnvironmentStateV16::rainy_alley_day().with_neon_disabled();
    assert!(day.has_visible_sky());
    assert!(day.has_natural_light());
    assert!(day.readable_without_neon());

    let night = EnvironmentStateV16::rainy_alley_night().with_neon_disabled();
    assert!(night.has_visible_sky());
    assert!(night.has_natural_light());
    assert!(night.readable_without_neon());
}

#[test]
fn v16_rejects_debug_leaks_in_beauty_mode() {
    let mut scene = BeautySceneV16::new(0);
    scene.debug_leaks = BeautyDebugLeakFlagsV16 {
        chunk_bounds_visible: true,
        ..BeautyDebugLeakFlagsV16::default()
    };
    let report = scene.validate_for_visual_realism();
    assert!(!report.pass);
    assert!(
        report
            .failure_reasons
            .contains(&BeautyFailureReasonV16::DebugPrimitiveLeak)
    );
}

#[test]
fn v16_rejects_floating_puddles() {
    let puddle = AnchoredPuddleV16 {
        object_id: BeautyObjectIdV16(1),
        receiver_surface_id: BeautySurfaceIdV16(0),
        center_world: [0.0, 0.0, 3.0],
        receiver_normal_world: [0.0, 0.0, 0.0],
        radius_meters: 1.0,
        max_depth_meters: 0.01,
        edge_softness_meters: 0.1,
        material_id: ashfall_rendering::beauty_v16::BeautyMaterialIdV16(1),
    };
    assert!(!puddle.is_ground_anchored());
}

#[test]
fn v16_empty_scene_fails_visual_realism() {
    let scene = BeautySceneV16::new(0);
    let report = scene.validate_for_visual_realism();
    assert!(!report.pass);
    assert!(
        report
            .failure_reasons
            .contains(&BeautyFailureReasonV16::NoValidBeautyCell)
    );
    assert!(
        report
            .failure_reasons
            .contains(&BeautyFailureReasonV16::NoHumanProxy)
    );
    assert!(
        report
            .failure_reasons
            .contains(&BeautyFailureReasonV16::NoVehicleProxy)
    );
    assert!(
        report
            .failure_reasons
            .contains(&BeautyFailureReasonV16::NoGeneratedMaterialPages)
    );
}

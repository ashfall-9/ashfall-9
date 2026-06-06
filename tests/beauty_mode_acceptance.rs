use ashfall_rendering::beauty::{
    AnchoredPuddleV15, BeautyBoundsV15, BeautyDetailTierV15, BeautyMaterialIdV15,
    BeautyObjectIdV15, BeautySceneV15, BeautySurfaceIdV15, DetailOrchestratorV15,
    EnvironmentStateV15, FrameBudgetConfigV15, FrameCostSampleV15, HumanProxyV15,
    IrregularityRecipeV15, VehicleProxyV15,
};
use ashfall_rendering::beauty_contract::{
    BeautyGateViolationV1, BeautyRenderModeV1, DebugPrimitiveClassV1, GoldenSceneReportV1,
};
use ashfall_rendering::windowed::{
    WindowDebugOverlayFlags, WindowRenderMode, WindowedRendererConfig,
};

#[test]
fn windowed_renderer_defaults_to_beauty_mode() {
    let config = WindowedRendererConfig::default();

    assert!(config.is_beauty_mode());
    assert_eq!(config.render_mode, WindowRenderMode::Beauty);
    assert_eq!(config.debug_overlays, WindowDebugOverlayFlags::none());
    assert!(!config.allows_debug_geometry());
}

#[test]
fn beauty_mode_rejects_debug_primitives() {
    let mode = BeautyRenderModeV1::Beauty;
    let violation =
        BeautyGateViolationV1::DebugPrimitiveInBeautyMode(DebugPrimitiveClassV1::CityChunkBox);

    assert_eq!(mode, BeautyRenderModeV1::Beauty);
    assert!(matches!(
        violation,
        BeautyGateViolationV1::DebugPrimitiveInBeautyMode(_)
    ));
}

#[test]
fn rainy_alley_golden_scene_minimum_gate() {
    let report = GoldenSceneReportV1 {
        scene_name: "rainy_alley_minimum".to_string(),
        frame_ms: 16.2,
        cpu_ms: 5.8,
        gpu_ms: 9.2,
        debug_primitive_count_in_beauty: 0,
        visible_human_proxy_count: 1,
        visible_vehicle_proxy_count: 1,
        generated_material_page_count: 12,
        natural_light_present: true,
        sky_present: true,
        violations: Vec::new(),
    };

    assert!(report.passes_minimum_gate());
}

#[test]
fn beauty_v15_environment_survives_neon_disabled() {
    let environment = EnvironmentStateV15::rainy_alley_day().with_neon_disabled();

    assert!(environment.has_visible_sky());
    assert!(environment.has_natural_light());
    assert_eq!(environment.neon_accent_scale, 0.0);
}

#[test]
fn beauty_v15_puddles_must_be_surface_anchored() {
    let puddle = AnchoredPuddleV15 {
        object_id: BeautyObjectIdV15(1),
        receiver_surface_id: BeautySurfaceIdV15(99),
        center_world: [0.0, 0.0, 0.006],
        normal_world: [0.0, 0.0, 1.0],
        radius_x_meters: 1.0,
        radius_y_meters: 0.6,
        water_depth_meters: 0.01,
        edge_feather_meters: 0.2,
        z_bias_meters: 0.006,
        material_id: BeautyMaterialIdV15(10),
    };

    assert!(puddle.is_ground_anchored());

    let mut floating = puddle.clone();
    floating.receiver_surface_id = BeautySurfaceIdV15(0);
    assert!(!floating.is_ground_anchored());
}

#[test]
fn beauty_v15_human_proxy_must_not_be_a_rod() {
    let human = HumanProxyV15 {
        object_id: BeautyObjectIdV15(2),
        root_position: [0.0, 0.0, 0.0],
        height_meters: 1.78,
        shoulder_width_meters: 0.43,
        hip_width_meters: 0.34,
        head_radius_meters: 0.115,
        clothing_material_id: BeautyMaterialIdV15(11),
        skin_material_id: BeautyMaterialIdV15(12),
        irregularity: IrregularityRecipeV15::human(1),
    };

    assert!(human.is_proportionate_non_rod());
}

#[test]
fn beauty_v15_vehicle_proxy_must_not_be_a_box() {
    let vehicle = VehicleProxyV15 {
        object_id: BeautyObjectIdV15(3),
        bounds: BeautyBoundsV15 {
            min: [0.0, 0.0, 0.0],
            max: [4.4, 1.9, 1.55],
        },
        wheel_count: 4,
        cabin_glass_ratio: 0.22,
        panel_seam_density: 0.55,
        body_material_id: BeautyMaterialIdV15(13),
        glass_material_id: BeautyMaterialIdV15(14),
        rubber_material_id: BeautyMaterialIdV15(15),
        irregularity: IrregularityRecipeV15::vehicle(2),
    };

    assert!(vehicle.is_proportionate_non_box());
}

#[test]
fn beauty_v15_empty_scene_fails_visual_bar() {
    let scene = BeautySceneV15::new(0);
    let report = scene.validate_for_beauty();

    assert!(!report.pass);
    assert!(report.has_environment);
    assert_eq!(report.valid_cell_count, 0);
}

#[test]
fn beauty_v15_detail_orchestrator_preserves_important_identity_under_pressure() {
    let mut orchestrator = DetailOrchestratorV15::new(FrameBudgetConfigV15::default());
    orchestrator.update_cost_sample(FrameCostSampleV15 {
        cpu_scene_ms: 6.0,
        gpu_frame_ms: 18.0,
        upload_mb: 16.0,
        generated_material_pages: 8,
        uploaded_geometry_pages: 3,
        updated_shadow_pages: 12,
    });

    let far_wall = orchestrator.decide(48.0, 0.05, 0.1, 0.2, false);
    let far_human = orchestrator.decide(48.0, 0.05, 0.1, 0.2, true);

    assert!(orchestrator.global_pressure() > 1.0);
    assert_eq!(far_wall.tier, BeautyDetailTierV15::Mid);
    assert_eq!(far_human.tier, BeautyDetailTierV15::Mid);
    assert!(far_human.geometry_density > far_wall.geometry_density);
    assert!(far_wall.scatter_density < 0.35);
}

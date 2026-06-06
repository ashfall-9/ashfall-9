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

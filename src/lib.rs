pub mod beauty_scene_builder;
pub mod beauty_scene_v15_bridge;
pub mod scenarios;
pub mod windowed;

pub use ashfall_core::assets;
pub use ashfall_core::budget;
pub use ashfall_core::core;
pub use ashfall_core::core::*;
pub use ashfall_core::gpu;
pub use ashfall_core::module_abi;
pub use ashfall_core::runtime;
pub use ashfall_core::runtime::{
    DynamicEngineModule, DynamicModuleRegistrationError, EngineModule, EngineRuntime,
    FrameInspectionIssue, FrameInspectionReport, FrameInspectionSeverity, FrameReport,
    ModuleInspectionReport, ModuleQualityState, ModuleStateRecord, RenderFrameReport,
    ReplayCapture, ReplayCaptureFilter, RuntimeSaveData,
};
pub use ashfall_core::schema;
pub use ashfall_core::world;

pub mod modules {
    pub use ashfall_ai as ai;
    pub use ashfall_human as human;
    pub use ashfall_materials as materials;
    pub use ashfall_physics as physics;
    pub use ashfall_rendering as rendering;
    pub use ashfall_tools as tools;
    pub use ashfall_voice as voice;
    pub use ashfall_worldgen as worldgen;
}

#[cfg(test)]
mod tests {
    use super::scenarios::{
        build_alley_runtime, capture_alley_debug_report, queue_glass_break_attempt,
        validate_alley_debug_capture,
    };
    use super::world::WorldEventKind;
    use ashfall_tools::{
        AiAcceptanceGateKind, AiPipelineAcceptanceGateKind, AssetPackageAcceptanceGateKind,
        BuildAutomationStageKind, CityWorldAcceptanceGateKind, CityWorldPipelineAcceptanceGateKind,
        EngineCoreAcceptanceCheckKind, ExecutiveSanityCheckKind, FeatureAcceptanceFeatureKind,
        GoldenSceneKind, GpuServicesAcceptanceGateKind, HumanAcceptanceGateKind,
        HumanPipelineAcceptanceGateKind, InterfaceSchemaSectionKind, MaterialAcceptanceGateKind,
        MaterialPipelineAcceptanceGateKind, OpenResearchPackageKind, PerformanceAcceptanceGateKind,
        PhotorealRenderingAcceptanceGateKind, PhysicsAcceptanceGateKind,
        PhysicsPipelineAcceptanceGateKind, ProductQualityCheckKind, ReferenceAnchorKind,
        ReferenceValidationGateKind, RendererAcceptanceGateKind, RoadmapPhaseKind,
        RuntimeDependencyPolicyCheckKind, SafetyProvenanceDomainKind, TeamCompatibilityLevel,
        TeamHandoffComponentKind, ToolsAcceptanceGateKind, VirtualGeometryAcceptanceGateKind,
        VirtualGeometryPipelineAcceptanceGateKind, VoiceAcceptanceGateKind,
        VoicePipelineAcceptanceGateKind,
    };

    #[test]
    fn alley_runtime_load_restores_module_state_without_replaying_one_shots() {
        let mut runtime = build_alley_runtime().expect("alley runtime should build");
        queue_glass_break_attempt(&mut runtime);
        runtime.step().expect("first alley frame should run");

        let save_data = runtime.save_runtime();
        let saved_module_ids = save_data
            .module_states
            .iter()
            .map(|state| state.module_id)
            .collect::<Vec<_>>();
        for expected in [10, 20, 30, 40, 50, 55, 60, 65] {
            assert!(saved_module_ids.contains(&expected));
        }

        let mut restored = build_alley_runtime().expect("fresh alley runtime should build");
        restored.load_runtime(save_data);
        let second_frame = restored
            .step()
            .expect("restored alley frame should continue");

        assert!(second_frame.events.iter().all(|event| {
            !matches!(
                event.kind,
                WorldEventKind::NpcWitnessedCrime { .. }
                    | WorldEventKind::PlayerIdentityExposed
                    | WorldEventKind::AgentMemoryUpdated { .. }
                    | WorldEventKind::AgentIntentProposed { .. }
                    | WorldEventKind::AgentStateChanged { .. }
                    | WorldEventKind::AgentDecisionExplained { .. }
                    | WorldEventKind::NavigationMoveBlocked { .. }
                    | WorldEventKind::SecurityAlertRaised { .. }
                    | WorldEventKind::FactionReputationChanged { .. }
                    | WorldEventKind::SurveillanceIncreased { .. }
                    | WorldEventKind::VoiceLineSpoken { .. }
                    | WorldEventKind::SpeechSynthesized { .. }
                    | WorldEventKind::AudioFrameMixed { .. }
                    | WorldEventKind::FacialAnimationApplied { .. }
                    | WorldEventKind::HumanAppearanceUpdated { .. }
                    | WorldEventKind::AssetStreamingRequested { .. }
            )
        }));
    }

    #[test]
    fn alley_runtime_tools_capture_surfaces_replay_profiler_and_asset_inventory() {
        let mut runtime = build_alley_runtime().expect("alley runtime should build");
        queue_glass_break_attempt(&mut runtime);
        let frame = runtime.step().expect("alley frame should run");

        let capture = capture_alley_debug_report(&runtime, &frame);
        let validation = validate_alley_debug_capture(&capture);

        assert!(validation.passed);
        assert_eq!(capture.replay.len(), 1);
        assert_eq!(capture.replay_debugger.frame_count, 1);
        assert_eq!(capture.replay_debugger.command_count, 1);
        assert!(capture.replay_debugger.event_count >= 41);
        assert!(capture.replay_debugger.unique_event_kind_count >= 25);
        assert!(capture.replay_debugger.source_link_count >= 8);
        assert_eq!(capture.replay_debugger.critical_issue_count(), 0);
        assert!(
            capture
                .replay_debugger
                .mode(ashfall_tools::ReplayDebugMode::FullLocal)
                .is_some_and(|mode| mode.available)
        );
        assert!(
            capture
                .replay_debugger
                .mode(ashfall_tools::ReplayDebugMode::PhysicsOnly)
                .is_some_and(|mode| mode.available)
        );
        assert!(
            capture
                .replay_debugger
                .mode(ashfall_tools::ReplayDebugMode::AiConversation)
                .is_some_and(|mode| mode.available)
        );
        assert!(
            capture
                .replay_debugger
                .mode(ashfall_tools::ReplayDebugMode::RenderingCapture)
                .is_some_and(|mode| mode.available)
        );
        assert_eq!(capture.replay_inspection.total_frame_count, 1);
        assert_eq!(capture.replay_inspection.visible_frame_count, 1);
        assert!(capture.replay_inspection.visible_event_count >= 41);
        assert_eq!(capture.replay_inspection.critical_issue_count(), 0);
        assert_eq!(
            capture
                .replay_inspection
                .selected_frame
                .as_ref()
                .map(|frame| frame.frame),
            Some(1)
        );
        assert_eq!(capture.replay_inspection.exports.len(), 3);
        assert!(
            capture
                .replay_inspection
                .export(ashfall_tools::ReplayInspectionExportFormat::DebugText)
                .is_some_and(|export| export.event_count >= 41
                    && export.frame_count == 1
                    && export.deterministic_hash != 0)
        );
        assert!(
            capture
                .replay_inspection
                .visible_events
                .iter()
                .any(|event| event.category == ashfall_tools::ReplayEventCategory::AssetStreaming)
        );
        assert!(capture.editor_workspace.passed());
        assert_eq!(
            capture.editor_workspace.panel_count,
            ashfall_tools::EDITOR_REQUIRED_PANEL_COUNT
        );
        assert_eq!(
            capture.editor_workspace.available_panel_count,
            capture.editor_workspace.panel_count
        );
        assert!(capture.editor_workspace.engine_renderer_viewport_available);
        assert!(capture.editor_workspace.validation_report_viewer_available);
        assert!(capture.editor_workspace.versioned_package_output_enforced);
        assert!(
            capture
                .editor_workspace
                .panel(ashfall_tools::EditorPanelKind::WorldViewport)
                .is_some_and(|panel| panel.available && panel.renderer_backed)
        );
        assert!(
            capture
                .editor_workspace
                .panel(ashfall_tools::EditorPanelKind::GoldenSceneRunner)
                .is_some_and(|panel| panel.available
                    && panel.renderer_backed
                    && panel.replay_backed
                    && panel.profiler_backed
                    && panel.validation_backed)
        );
        assert!(
            capture
                .editor_workspace
                .panel(ashfall_tools::EditorPanelKind::ValidationReportViewer)
                .is_some_and(|panel| panel.available && panel.validation_backed)
        );
        assert!(!capture.profiler.samples.is_empty());
        assert!(capture.profiler.total_cpu_time_us() > 0);
        assert!(capture.profiler.total_gpu_time_us() > 0);
        assert_eq!(capture.asset_inventory.total_assets, runtime.assets().len());
        assert!(capture.asset_inventory.total_assets >= 9);
        assert!(capture.asset_inventory.resident_assets > 0);
        assert!(capture.asset_inventory.unloaded_assets > 0);
        assert!(capture.asset_inventory.passed());
        assert_eq!(
            capture.asset_browser.runtime_asset_count,
            capture.asset_inventory.total_assets
        );
        assert_eq!(capture.asset_browser.package_count, 0);
        assert!(!capture.asset_browser.is_empty());
        assert!(capture.asset_browser.passed());
        assert_eq!(capture.asset_browser.missing_runtime_dependency_count, 0);
        assert_eq!(capture.asset_browser.critical_issue_count(), 0);
        assert!(capture.asset_browser.by_kind.len() >= 2);
        assert!(
            capture.asset_browser.kind("Mesh").is_some_and(
                |kind| kind.runtime_asset_count >= 5 && kind.resident_runtime_asset_count >= 4
            )
        );
        assert!(
            capture
                .asset_browser
                .kind("MaterialGraph")
                .is_some_and(|kind| kind.runtime_asset_count
                    >= capture.city_generation.material_variant_count
                    && kind.generated_runtime_asset_count
                        >= capture.city_generation.material_variant_count)
        );
        assert!(capture.asset_browser.kind("Texture").is_some_and(|kind| {
            kind.runtime_asset_count >= capture.city_generation.material_cache_dependency_count
                && kind.generated_runtime_asset_count
                    >= capture.city_generation.material_cache_dependency_count
        }));
        assert!(capture.asset_browser.kind("WorldChunk").is_some_and(
            |kind| kind.runtime_asset_count == capture.city_generation.streaming_cell_count
                && kind.generated_runtime_asset_count
                    == capture.city_generation.streaming_cell_count
        ));
        assert!(
            capture
                .asset_browser
                .runtime_assets
                .iter()
                .any(|asset| ashfall_worldgen::is_worldgen_material_cache_asset(asset.asset_id))
        );
        assert!(capture.asset_browser.runtime_assets.iter().any(|asset| {
            ashfall_worldgen::is_worldgen_city_cell_asset(asset.asset_id)
                && asset.dependency_count > 0
        }));
        assert!(
            capture
                .asset_browser
                .runtime_assets
                .iter()
                .any(|asset| asset.generated)
        );
        assert_eq!(
            capture.schema_inspector.registered_schema_count,
            runtime.schemas().len()
        );
        assert_eq!(
            capture.schema_inspector.module_requirement_count,
            frame.schema_compatibility.checked_requirements
        );
        assert_eq!(capture.schema_inspector.package_schema_count, 0);
        assert!(capture.schema_inspector.owner_count >= 8);
        assert!(capture.schema_inspector.passed());
        assert_eq!(capture.schema_inspector.critical_issue_count(), 0);
        assert!(
            capture
                .schema_inspector
                .module_requirements
                .iter()
                .all(|requirement| requirement.satisfied)
        );
        assert_eq!(capture.streaming_debugger.frame_count, 1);
        assert_eq!(
            capture.streaming_debugger.latest_total_registered_assets,
            capture.asset_inventory.total_assets
        );
        assert_eq!(capture.streaming_debugger.latest_pending_assets, 0);
        assert_eq!(capture.streaming_debugger.total_completed_assets, 4);
        assert_eq!(capture.streaming_debugger.resident_event_count, 4);
        assert!(capture.streaming_debugger.request_event_count >= 8);
        assert!(capture.streaming_debugger.total_bytes_streamed > 0);
        assert!(capture.streaming_debugger.requester_count > 0);
        assert!(capture.streaming_debugger.passed());
        assert_eq!(capture.streaming_debugger.critical_issue_count(), 0);
        assert!(
            capture
                .streaming_debugger
                .frames
                .first()
                .is_some_and(|frame| frame.pressure == ashfall_tools::StreamingPressure::Healthy)
        );
        assert_eq!(capture.performance_heatmap.frame_count, 1);
        assert_eq!(
            capture.performance_heatmap.module_count,
            frame.module_reports.len()
        );
        assert!(capture.performance_heatmap.total_cpu_time_us > 0);
        assert!(capture.performance_heatmap.total_module_gpu_time_us > 0);
        assert_eq!(
            capture.performance_heatmap.total_streaming_bytes,
            capture.streaming_debugger.total_bytes_streamed
        );
        assert!(capture.performance_heatmap.total_gpu_resource_bytes > 0);
        assert_eq!(capture.performance_heatmap.total_budget_pressure_count, 0);
        assert_eq!(capture.performance_heatmap.critical_issue_count(), 0);
        assert!(capture.performance_heatmap.passed());
        assert!(!capture.performance_heatmap.hotspots.is_empty());
        assert!(
            capture
                .performance_heatmap
                .frames
                .first()
                .is_some_and(|frame| frame.heat == ashfall_tools::PerformanceHeatLevel::Hot)
        );
        assert!(capture.r_and_d_gates.passed());
        assert_eq!(
            capture.r_and_d_gates.production_ready_count,
            capture.r_and_d_gates.feature_count
        );
        assert_eq!(capture.r_and_d_gates.missing_check_count, 0);
        assert!(capture.r_and_d_gates.feature_count >= frame.module_reports.len());
        assert!(capture.frame_graphs.len() >= 2);
        assert!(capture.frame_graphs[0].passed());
        assert!(capture.frame_graphs[0].pass_count > 20);
        assert_eq!(
            capture.frame_graphs[0].owned_pass_count,
            capture.frame_graphs[0].pass_count
        );
        assert_eq!(
            capture.frame_graphs[0].expected_cost_pass_count,
            capture.frame_graphs[0].pass_count
        );
        assert!(capture.frame_graphs[0].dependency_hint_count > 0);
        assert!(capture.frame_graphs[0].descriptor_pressure.passed());
        assert!(capture.frame_graphs[0].descriptor_pressure.binding_count > 0);
        assert!(capture.frame_graphs[0].residency.passed());
        assert!(capture.frame_graphs[0].residency.streamed_resident_bytes > 0);
        assert!(capture.frame_graphs[0].residency.transient_bytes > 0);
        assert!(capture.frame_graphs[0].access_summary.access_count > 0);
        assert!(!capture.frame_graphs[0].queue_occupancy.is_empty());
        assert!(capture.frame_graphs[0].shader_pipeline.passed());
        assert_eq!(
            capture.frame_graphs[0]
                .shader_pipeline
                .shader_contract_count,
            capture.frame_graphs[0].pipeline_count
        );
        assert!(capture.frame_graphs[0].shader_pipeline.shader_variant_count > 0);
        assert_eq!(
            capture.frame_graphs[0]
                .shader_pipeline
                .validation_error_count,
            0
        );
        assert!(capture.frame_graphs[0].compute_passes > 0);
        assert!(capture.frame_graphs[0].graphics_passes > 0);
        assert!(!capture.frame_graphs[0].top_passes.is_empty());
        assert!(!capture.frame_graphs[0].resource_lifetimes.is_empty());
        assert!(
            capture.frame_graphs[0]
                .resource_lifetimes
                .iter()
                .any(
                    |resource| resource.label == "material runtime program table"
                        && resource.reader_count >= 2
                )
        );
        assert!(
            capture.frame_graphs[0]
                .resource_lifetimes
                .iter()
                .any(
                    |resource| resource.label == "material cache residency table"
                        && resource.reader_count >= 1
                )
        );
        assert!(
            capture.frame_graphs[0]
                .resource_lifetimes
                .iter()
                .any(
                    |resource| resource.label == "material aged metal surface cache output"
                        && resource.writer_count >= 1
                )
        );
        assert!(capture.frame_graphs[0].material_runtime.passed());
        assert!(!capture.frame_graphs[0].material_runtime.is_empty());
        assert!(
            capture.frame_graphs[0]
                .material_runtime
                .runtime_program_table_bindless
        );
        assert!(
            capture.frame_graphs[0]
                .material_runtime
                .resolver_reads_program_table
        );
        assert!(
            capture.frame_graphs[0]
                .material_runtime
                .lighting_reads_cache_table
        );
        assert!(
            capture.frame_graphs[0]
                .material_runtime
                .lighting_pipeline_has_cache_miss_fallback
        );
        assert!(
            capture.frame_graphs[0]
                .material_runtime
                .aged_surface_cache_resource_present
        );
        assert!(
            capture.frame_graphs[0]
                .material_runtime
                .aged_surface_cache_update_pass_present
        );
        assert!(
            capture.frame_graphs[0]
                .material_runtime
                .aged_surface_cache_writes_output
        );
        assert!(
            capture.frame_graphs[0]
                .material_runtime
                .aged_surface_pipeline_has_corrosion_response
        );
        assert!(
            capture.frame_graphs[0]
                .material_runtime
                .aged_surface_pipeline_has_heat_discoloration
        );
        assert!(capture.frame_graphs[0].micro_geometry.passed());
        assert!(!capture.frame_graphs[0].micro_geometry.is_empty());
        assert!(capture.frame_graphs[0].micro_geometry.cluster_table_present);
        assert!(
            capture.frame_graphs[0]
                .micro_geometry
                .cluster_table_bindless
        );
        assert!(
            capture.frame_graphs[0]
                .micro_geometry
                .visible_cluster_buffer_present
        );
        assert!(
            capture.frame_graphs[0]
                .micro_geometry
                .renderer_streaming_feedback_present
        );
        assert!(
            capture.frame_graphs[0]
                .micro_geometry
                .streaming_feedback_pass_present
        );
        assert!(
            capture.frame_graphs[0]
                .micro_geometry
                .worldgen_visibility_pass_present
        );
        assert!(
            capture.frame_graphs[0]
                .micro_geometry
                .culling_pipeline_has_lod_define
        );
        assert!(
            capture.frame_graphs[0]
                .micro_geometry
                .feedback_pipeline_has_cluster_streaming_define
        );
        assert!(
            capture.frame_graphs[0]
                .micro_geometry
                .main_lighting_reads_indirect_draws
        );
        assert!(capture.frame_graphs[0].virtual_shadow_pages.passed());
        assert!(
            capture.frame_graphs[0]
                .virtual_shadow_pages
                .estimated_page_capacity
                > 0
        );
        assert!(
            capture.frame_graphs[0]
                .virtual_shadow_pages
                .shadow_pass_reads_dirty_pages
        );
        assert!(
            capture.frame_graphs[0]
                .virtual_shadow_pages
                .lighting_reads_page_table
        );
        assert!(capture.frame_graphs[0].camera_color.passed());
        assert!(capture.frame_graphs[0].camera_color.pipeline_active());
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .physical_camera_buffer_present
        );
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .hdr_lighting_target_present
        );
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .luminance_histogram_present
        );
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .exposure_adaptation_present
        );
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .motion_blur_target_present
        );
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .depth_of_field_target_present
        );
        assert!(capture.frame_graphs[0].camera_color.bloom_pyramid_present);
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .color_grading_lut_present
        );
        assert!(capture.frame_graphs[0].camera_color.exposure_pass_present);
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .lens_effects_pass_present
        );
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .exposure_reads_hdr_lighting
        );
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .lens_reads_motion_vectors
        );
        assert!(capture.frame_graphs[0].camera_color.lens_reads_depth);
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .post_reads_color_grading_lut
        );
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .post_pipeline_has_linear_hdr_define
        );
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .post_pipeline_has_filmic_tonemap_define
        );
        assert!(
            capture.frame_graphs[0]
                .camera_color
                .post_pipeline_has_color_grading_define
        );
        assert!(capture.frame_graphs[0].temporal_stability.passed());
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .reconstruction_active()
        );
        assert!(capture.frame_graphs[0].temporal_stability.mask_pass_present);
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .reconstruction_pass_present
        );
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .reactive_mask_present
        );
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .disocclusion_mask_present
        );
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .history_input_present
        );
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .history_output_present
        );
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .reconstruction_reads_reactive_mask
        );
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .reconstruction_reads_disocclusion_mask
        );
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .reconstruction_pipeline_has_history_rejection_define
        );
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .reconstruction_pipeline_has_upscaling_define
        );
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .reconstruction_pipeline_has_ghosting_debug_define
        );
        assert!(
            capture.frame_graphs[0]
                .temporal_stability
                .ui_reads_upscaled_target
        );
        assert!(capture.frame_graphs[0].human_rendering.passed());
        assert!(capture.frame_graphs[0].human_rendering.pipeline_active());
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .runtime_bundle_table_present
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .skin_subsurface_profiles_present
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .pore_microdetail_atlas_present
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .facial_wrinkle_maps_present
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .eye_optics_table_present
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .hair_groom_table_present
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .cybernetic_material_table_present
        );
        assert!(capture.frame_graphs[0].human_rendering.skin_pass_present);
        assert!(capture.frame_graphs[0].human_rendering.eye_pass_present);
        assert!(capture.frame_graphs[0].human_rendering.hair_pass_present);
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .clothing_cybernetics_pass_present
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .composite_pass_present
        );
        assert!(capture.frame_graphs[0].human_rendering.composite_reads_skin);
        assert!(capture.frame_graphs[0].human_rendering.composite_reads_eyes);
        assert!(capture.frame_graphs[0].human_rendering.composite_reads_hair);
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .composite_reads_clothing
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .lighting_reads_composite
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .skin_pipeline_has_subsurface_define
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .skin_pipeline_has_pore_define
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .eye_pipeline_has_tearline_define
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .hair_pipeline_has_hybrid_lod_define
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .clothing_pipeline_has_cybernetic_define
        );
        assert!(
            capture.frame_graphs[0]
                .human_rendering
                .composite_pipeline_has_closeup_define
        );
        assert!(capture.frame_graphs[0].reference_render.passed());
        assert!(capture.frame_graphs[0].reference_render.is_empty());
        assert!(capture.reference_comparison.passed());
        assert_eq!(capture.reference_comparison.gameplay_frame_count, 1);
        assert_eq!(capture.reference_comparison.reference_frame_count, 1);
        assert_eq!(capture.reference_comparison.comparison_pass_count, 1);
        assert!(capture.reference_comparison.total_radiance_target_bytes > 0);
        assert!(capture.reference_comparison.total_metrics_buffer_bytes > 0);
        assert!(capture.golden_scenes.passed());
        assert_eq!(capture.golden_scenes.scene_count, 7);
        assert_eq!(capture.golden_scenes.passed_scene_count, 7);
        assert_eq!(
            capture.golden_scenes.ci_ready_scene_count,
            capture.golden_scenes.scene_count
        );
        assert_eq!(capture.golden_scenes.signal_count, 21);
        assert_eq!(capture.golden_scenes.passed_signal_count, 21);
        assert!(capture.golden_scenes.artifact_count >= capture.golden_scenes.scene_count * 9);
        assert!(capture.golden_scenes.screenshot_count >= capture.golden_scenes.scene_count);
        assert!(capture.golden_scenes.video_capture_count >= capture.golden_scenes.scene_count);
        assert!(capture.golden_scenes.frame_capture_count >= capture.golden_scenes.scene_count);
        assert!(capture.golden_scenes.total_cpu_time_us > 0);
        assert!(capture.golden_scenes.total_gpu_time_us > 0);
        assert!(capture.golden_scenes.peak_vram_bytes > 0);
        assert!(capture.golden_scenes.total_streaming_bytes > 0);
        assert_ne!(capture.golden_scenes.replay_hash, 0);
        assert!(
            capture
                .golden_scenes
                .scene(GoldenSceneKind::RainyNeonAlley)
                .is_some_and(|scene| scene.passed)
        );
        assert!(
            capture
                .golden_scenes
                .scene(GoldenSceneKind::MaterialLab)
                .is_some_and(|scene| scene.passed)
        );
        assert!(
            capture
                .golden_scenes
                .scene(GoldenSceneKind::CrowdedMarketAiCharacters)
                .is_some_and(|scene| {
                    scene.passed && scene.signal_count == 3 && scene.artifacts.complete_for_ci()
                })
        );
        assert!(capture.runtime_policy.passed());
        assert_eq!(capture.runtime_policy.check_count, 10);
        assert_eq!(capture.runtime_policy.passed_check_count, 10);
        assert_eq!(
            capture.runtime_policy.hidden_gpu_backend_dependency_count,
            0
        );
        assert_eq!(capture.runtime_policy.ml_gpu_backend_dependency_count, 0);
        assert_eq!(capture.runtime_policy.forbidden_ui_dependency_count, 0);
        assert!(capture.runtime_policy.shader_pipeline_report_count > 0);
        assert!(capture.runtime_policy.shader_contract_count > 0);
        assert_eq!(
            capture.runtime_policy.validated_shader_contract_count,
            capture.runtime_policy.shader_contract_count
        );
        assert!(
            capture
                .runtime_policy
                .spirv_shader_artifact_count
                .saturating_add(capture.runtime_policy.rust_gpu_shader_artifact_count)
                .saturating_add(capture.runtime_policy.engine_shader_source_artifact_count)
                > 0
        );
        assert_eq!(capture.runtime_policy.unknown_shader_artifact_count, 0);
        assert_eq!(capture.runtime_policy.shader_validation_error_count, 0);
        assert!(
            capture
                .runtime_policy
                .check(RuntimeDependencyPolicyCheckKind::HiddenGpuBackendBoundary)
                .is_some_and(|check| check.passed)
        );
        assert!(
            capture
                .runtime_policy
                .check(RuntimeDependencyPolicyCheckKind::RustOwnedShaderPipeline)
                .is_some_and(|check| check.passed)
        );
        assert!(capture.reference_anchors.passed());
        assert_eq!(capture.reference_anchors.anchor_count, 10);
        assert_eq!(capture.reference_anchors.passed_anchor_count, 10);
        assert_eq!(capture.reference_anchors.unreal_class_anchor_count, 6);
        assert_eq!(capture.reference_anchors.vulkan_rust_anchor_count, 1);
        assert_eq!(capture.reference_anchors.material_scene_anchor_count, 1);
        assert_eq!(capture.reference_anchors.physics_research_anchor_count, 1);
        assert_eq!(capture.reference_anchors.unique_direction_signal_count, 2);
        assert!(
            capture
                .reference_anchors
                .anchor(ReferenceAnchorKind::NaniteVirtualizedGeometry)
                .is_some_and(|anchor| anchor.passed)
        );
        assert!(
            capture
                .reference_anchors
                .anchor(ReferenceAnchorKind::UniqueAshfallDirection)
                .is_some_and(|anchor| anchor.passed)
        );
        assert!(capture.interface_schema_coverage.passed());
        assert_eq!(capture.interface_schema_coverage.section_count, 14);
        assert_eq!(capture.interface_schema_coverage.passed_section_count, 14);
        assert_eq!(capture.interface_schema_coverage.signal_count, 29);
        assert_eq!(capture.interface_schema_coverage.passed_signal_count, 29);
        assert!(capture.interface_schema_coverage.core_id_alias_count >= 11);
        assert!(capture.interface_schema_coverage.source_file_count >= 10);
        assert!(capture.interface_schema_coverage.gpu_contract_count >= 6);
        assert!(capture.interface_schema_coverage.rendering_contract_count >= 14);
        assert!(capture.interface_schema_coverage.simulation_contract_count >= 8);
        assert!(capture.interface_schema_coverage.ai_voice_contract_count >= 7);
        assert!(
            capture
                .interface_schema_coverage
                .section(InterfaceSchemaSectionKind::GpuGraphContracts)
                .is_some_and(|section| section.passed)
        );
        assert!(
            capture
                .interface_schema_coverage
                .section(InterfaceSchemaSectionKind::SchemaBackbone)
                .is_some_and(|section| section.passed)
        );
        assert!(capture.executive_sanity.passed());
        assert_eq!(capture.executive_sanity.check_count, 8);
        assert_eq!(capture.executive_sanity.passed_check_count, 8);
        assert_eq!(capture.executive_sanity.signal_count, 16);
        assert_eq!(capture.executive_sanity.passed_signal_count, 16);
        assert_eq!(capture.executive_sanity.runtime_platform_signal_count, 2);
        assert_eq!(capture.executive_sanity.photoreal_chain_signal_count, 2);
        assert_eq!(capture.executive_sanity.physical_loop_signal_count, 6);
        assert_eq!(
            capture.executive_sanity.production_readiness_signal_count,
            6
        );
        assert!(capture.executive_sanity.measured_artifact_count > 0);
        assert!(
            capture
                .executive_sanity
                .check(ExecutiveSanityCheckKind::PhotorealChain)
                .is_some_and(|check| check.passed)
        );
        assert!(
            capture
                .executive_sanity
                .check(ExecutiveSanityCheckKind::TruthCacheBoundary)
                .is_some_and(|check| check.passed)
        );
        assert!(
            capture
                .executive_sanity
                .check(ExecutiveSanityCheckKind::PhysicalProceduralLoop)
                .is_some_and(|check| check.passed)
        );
        assert!(capture.roadmap_acceptance.passed());
        assert_eq!(capture.roadmap_acceptance.phase_count, 8);
        assert_eq!(capture.roadmap_acceptance.ready_phase_count, 8);
        assert_eq!(capture.roadmap_acceptance.blocked_phase_count, 0);
        assert_eq!(capture.roadmap_acceptance.signal_count, 48);
        assert_eq!(capture.roadmap_acceptance.passed_signal_count, 48);
        assert_eq!(capture.roadmap_acceptance.schema_test_signal_count, 8);
        assert_eq!(
            capture.roadmap_acceptance.performance_budget_signal_count,
            8
        );
        assert_eq!(capture.roadmap_acceptance.debug_view_signal_count, 8);
        assert_eq!(capture.roadmap_acceptance.golden_review_signal_count, 8);
        assert_eq!(capture.roadmap_acceptance.failure_behavior_signal_count, 8);
        assert_eq!(
            capture.roadmap_acceptance.integration_evidence_signal_count,
            8
        );
        assert!(
            capture
                .roadmap_acceptance
                .phase(RoadmapPhaseKind::ArchitectureSkeleton)
                .is_some_and(|phase| phase.ready_to_advance)
        );
        assert!(
            capture
                .roadmap_acceptance
                .phase(RoadmapPhaseKind::RainyNeonAlley)
                .is_some_and(|phase| phase.ready_to_advance)
        );
        assert!(
            capture
                .roadmap_acceptance
                .phase(RoadmapPhaseKind::ProductionHardening)
                .is_some_and(|phase| phase.ready_to_advance)
        );
        assert!(capture.product_quality_bar.passed());
        assert_eq!(capture.product_quality_bar.check_count, 7);
        assert_eq!(capture.product_quality_bar.passed_check_count, 7);
        assert_eq!(capture.product_quality_bar.blocked_check_count, 0);
        assert_eq!(capture.product_quality_bar.signal_count, 29);
        assert_eq!(capture.product_quality_bar.passed_signal_count, 29);
        assert_eq!(capture.product_quality_bar.final_target_signal_count, 4);
        assert_eq!(capture.product_quality_bar.golden_scene_signal_count, 3);
        assert_eq!(capture.product_quality_bar.visual_quality_signal_count, 4);
        assert_eq!(
            capture.product_quality_bar.performance_quality_signal_count,
            5
        );
        assert_eq!(capture.product_quality_bar.failure_guard_signal_count, 5);
        assert_eq!(capture.product_quality_bar.acceptance_rule_signal_count, 5);
        assert_eq!(capture.product_quality_bar.practical_target_signal_count, 3);
        assert!(
            capture
                .product_quality_bar
                .check(ProductQualityCheckKind::FinalProductTarget)
                .is_some_and(|check| check.passed)
        );
        assert!(
            capture
                .product_quality_bar
                .check(ProductQualityCheckKind::FailureSignsGuard)
                .is_some_and(|check| check.passed)
        );
        assert!(
            capture
                .product_quality_bar
                .check(ProductQualityCheckKind::PracticalTargetPhilosophy)
                .is_some_and(|check| check.passed)
        );
        assert!(capture.engine_core_acceptance.passed());
        assert_eq!(capture.engine_core_acceptance.check_count, 9);
        assert_eq!(capture.engine_core_acceptance.passed_check_count, 9);
        assert_eq!(capture.engine_core_acceptance.blocked_check_count, 0);
        assert_eq!(capture.engine_core_acceptance.signal_count, 32);
        assert_eq!(capture.engine_core_acceptance.passed_signal_count, 32);
        assert_eq!(capture.engine_core_acceptance.snapshot_signal_count, 3);
        assert_eq!(
            capture
                .engine_core_acceptance
                .world_truth_contract_ready_count,
            capture.engine_core_acceptance.world_truth_contract_count
        );
        assert_eq!(
            capture.engine_core_acceptance.world_truth_contract_count,
            17
        );
        assert_eq!(
            capture
                .engine_core_acceptance
                .module_lifecycle_contract_ready_count,
            capture
                .engine_core_acceptance
                .module_lifecycle_contract_count
        );
        assert_eq!(
            capture
                .engine_core_acceptance
                .module_lifecycle_contract_count,
            10
        );
        assert_eq!(
            capture.engine_core_acceptance.validation_guard_ready_count,
            capture.engine_core_acceptance.validation_guard_count
        );
        assert_eq!(capture.engine_core_acceptance.validation_guard_count, 8);
        assert_eq!(
            capture.engine_core_acceptance.debug_surface_ready_count,
            capture.engine_core_acceptance.debug_surface_count
        );
        assert_eq!(capture.engine_core_acceptance.debug_surface_count, 8);
        assert_eq!(
            capture
                .engine_core_acceptance
                .command_validation_signal_count,
            3
        );
        assert_eq!(capture.engine_core_acceptance.event_ledger_signal_count, 4);
        assert_eq!(
            capture.engine_core_acceptance.module_contract_signal_count,
            4
        );
        assert_eq!(capture.engine_core_acceptance.save_truth_signal_count, 4);
        assert_eq!(capture.engine_core_acceptance.replay_signal_count, 4);
        assert_eq!(capture.engine_core_acceptance.gpu_boundary_signal_count, 3);
        assert!(
            capture
                .engine_core_acceptance
                .check(EngineCoreAcceptanceCheckKind::WorldTruthDataContracts)
                .is_some_and(|check| check.passed)
        );
        assert!(
            capture
                .engine_core_acceptance
                .check(EngineCoreAcceptanceCheckKind::CommandValidationTransactions)
                .is_some_and(|check| check.passed)
        );
        assert!(
            capture
                .engine_core_acceptance
                .check(EngineCoreAcceptanceCheckKind::AuthoritativeEventLedger)
                .is_some_and(|check| check.passed)
        );
        assert!(
            capture
                .engine_core_acceptance
                .check(EngineCoreAcceptanceCheckKind::ValidationDebugObservability)
                .is_some_and(|check| check.passed)
        );
        assert!(
            capture
                .engine_core_acceptance
                .check(EngineCoreAcceptanceCheckKind::GpuBoundaryOwnership)
                .is_some_and(|check| check.passed)
        );
        assert!(capture.gpu_services_acceptance.passed());
        assert_eq!(capture.gpu_services_acceptance.gate_count, 12);
        assert_eq!(capture.gpu_services_acceptance.passed_gate_count, 12);
        assert_eq!(capture.gpu_services_acceptance.blocked_gate_count, 0);
        assert_eq!(capture.gpu_services_acceptance.signal_count, 47);
        assert_eq!(capture.gpu_services_acceptance.passed_signal_count, 47);
        assert_eq!(capture.gpu_services_acceptance.ownership_signal_count, 7);
        assert_eq!(capture.gpu_services_acceptance.render_graph_signal_count, 4);
        assert_eq!(capture.gpu_services_acceptance.validation_signal_count, 6);
        assert_eq!(capture.gpu_services_acceptance.timing_signal_count, 5);
        assert_eq!(capture.gpu_services_acceptance.memory_signal_count, 7);
        assert_eq!(capture.gpu_services_acceptance.queue_signal_count, 4);
        assert_eq!(capture.gpu_services_acceptance.streaming_signal_count, 4);
        assert_eq!(capture.gpu_services_acceptance.fallback_signal_count, 6);
        assert_eq!(capture.gpu_services_acceptance.debug_signal_count, 4);
        assert_eq!(
            capture
                .gpu_services_acceptance
                .public_interface_handle_count,
            9
        );
        assert_eq!(
            capture
                .gpu_services_acceptance
                .public_interface_handle_ready_count,
            capture
                .gpu_services_acceptance
                .public_interface_handle_count
        );
        assert_eq!(
            capture.gpu_services_acceptance.vulkan_feature_policy_count,
            9
        );
        assert_eq!(
            capture
                .gpu_services_acceptance
                .vulkan_feature_policy_ready_count,
            capture.gpu_services_acceptance.vulkan_feature_policy_count
        );
        assert_eq!(
            capture
                .gpu_services_acceptance
                .graph_declaration_field_count,
            9
        );
        assert_eq!(
            capture
                .gpu_services_acceptance
                .graph_declaration_field_ready_count,
            capture
                .gpu_services_acceptance
                .graph_declaration_field_count
        );
        assert_eq!(
            capture.gpu_services_acceptance.memory_policy_category_count,
            12
        );
        assert_eq!(
            capture
                .gpu_services_acceptance
                .memory_policy_category_ready_count,
            capture.gpu_services_acceptance.memory_policy_category_count
        );
        assert_eq!(capture.gpu_services_acceptance.descriptor_policy_count, 8);
        assert_eq!(
            capture
                .gpu_services_acceptance
                .descriptor_policy_ready_count,
            capture.gpu_services_acceptance.descriptor_policy_count
        );
        assert_eq!(
            capture.gpu_services_acceptance.buffer_address_policy_count,
            4
        );
        assert_eq!(
            capture
                .gpu_services_acceptance
                .buffer_address_policy_ready_count,
            capture.gpu_services_acceptance.buffer_address_policy_count
        );
        assert_eq!(
            capture
                .gpu_services_acceptance
                .optional_feature_policy_count,
            5
        );
        assert_eq!(
            capture
                .gpu_services_acceptance
                .optional_feature_policy_ready_count,
            capture
                .gpu_services_acceptance
                .optional_feature_policy_count
        );
        assert_eq!(
            capture.gpu_services_acceptance.synchronization_policy_count,
            6
        );
        assert_eq!(
            capture
                .gpu_services_acceptance
                .synchronization_policy_ready_count,
            capture.gpu_services_acceptance.synchronization_policy_count
        );
        assert_eq!(
            capture.gpu_services_acceptance.profiling_requirement_count,
            11
        );
        assert_eq!(
            capture
                .gpu_services_acceptance
                .profiling_requirement_ready_count,
            capture.gpu_services_acceptance.profiling_requirement_count
        );
        assert_eq!(capture.gpu_services_acceptance.debug_requirement_count, 9);
        assert_eq!(
            capture
                .gpu_services_acceptance
                .debug_requirement_ready_count,
            capture.gpu_services_acceptance.debug_requirement_count
        );
        assert!(
            capture
                .gpu_services_acceptance
                .gate(GpuServicesAcceptanceGateKind::VulkanOwnershipBoundary)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .gpu_services_acceptance
                .gate(GpuServicesAcceptanceGateKind::EngineHandlePublicInterface)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .gpu_services_acceptance
                .gate(GpuServicesAcceptanceGateKind::VulkanFeaturePlanning)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .gpu_services_acceptance
                .gate(GpuServicesAcceptanceGateKind::RenderGraphDependencyContract)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .gpu_services_acceptance
                .gate(GpuServicesAcceptanceGateKind::DescriptorAndAddressPolicy)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .gpu_services_acceptance
                .gate(GpuServicesAcceptanceGateKind::ProfilingDebugSurfaces)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .gpu_services_acceptance
                .gate(GpuServicesAcceptanceGateKind::MemoryResidencyAliasing)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.photoreal_rendering_acceptance.passed());
        assert_eq!(capture.photoreal_rendering_acceptance.gate_count, 8);
        assert_eq!(capture.photoreal_rendering_acceptance.passed_gate_count, 8);
        assert_eq!(capture.photoreal_rendering_acceptance.blocked_gate_count, 0);
        assert_eq!(capture.photoreal_rendering_acceptance.signal_count, 40);
        assert_eq!(
            capture.photoreal_rendering_acceptance.passed_signal_count,
            40
        );
        assert_eq!(capture.photoreal_rendering_acceptance.scene_signal_count, 6);
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .camera_color_signal_count,
            5
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .lighting_shadow_signal_count,
            5
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .material_volume_signal_count,
            5
        );
        assert_eq!(capture.photoreal_rendering_acceptance.human_signal_count, 5);
        assert_eq!(
            capture.photoreal_rendering_acceptance.temporal_signal_count,
            4
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .reference_signal_count,
            4
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .debug_performance_signal_count,
            6
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .golden_scene_matrix_count,
            5
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .golden_scene_matrix_ready_count,
            capture
                .photoreal_rendering_acceptance
                .golden_scene_matrix_count
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .renderer_input_packet_field_count,
            11
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .renderer_input_packet_field_ready_count,
            capture
                .photoreal_rendering_acceptance
                .renderer_input_packet_field_count
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .renderer_output_artifact_count,
            8
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .renderer_output_artifact_ready_count,
            capture
                .photoreal_rendering_acceptance
                .renderer_output_artifact_count
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .lighting_strategy_requirement_count,
            8
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .lighting_strategy_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .lighting_strategy_requirement_count
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .camera_color_pipeline_requirement_count,
            8
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .camera_color_pipeline_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .camera_color_pipeline_requirement_count
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .material_volume_requirement_count,
            10
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .material_volume_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .material_volume_requirement_count
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .human_rendering_requirement_count,
            10
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .human_rendering_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .human_rendering_requirement_count
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .temporal_stability_requirement_count,
            9
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .temporal_stability_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .temporal_stability_requirement_count
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .reference_path_requirement_count,
            6
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .reference_path_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .reference_path_requirement_count
        );
        assert_eq!(
            capture.photoreal_rendering_acceptance.rejection_guard_count,
            8
        );
        assert_eq!(
            capture
                .photoreal_rendering_acceptance
                .rejection_guard_ready_count,
            capture.photoreal_rendering_acceptance.rejection_guard_count
        );
        assert!(
            capture
                .photoreal_rendering_acceptance
                .gate(PhotorealRenderingAcceptanceGateKind::RainyNeonAlleyScene)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .photoreal_rendering_acceptance
                .gate(PhotorealRenderingAcceptanceGateKind::CameraColorPostProcess)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .photoreal_rendering_acceptance
                .gate(PhotorealRenderingAcceptanceGateKind::HeroHumanRendering)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .photoreal_rendering_acceptance
                .gate(PhotorealRenderingAcceptanceGateKind::ReferenceComparisonPath)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.conformance.passed);
        assert_eq!(capture.conformance.checked_output_count, 45);
        assert_eq!(capture.conformance.missing_output_count, 0);
        assert!(capture.build_automation.passed());
        assert_eq!(capture.build_automation.stage_count, 11);
        assert_eq!(capture.build_automation.passed_stage_count, 11);
        assert_eq!(capture.build_automation.signal_count, 22);
        assert_eq!(capture.build_automation.passed_signal_count, 22);
        assert!(
            capture
                .build_automation
                .stage(BuildAutomationStageKind::ShaderSpirvValidation)
                .is_some_and(|stage| stage.passed)
        );
        assert!(
            capture
                .build_automation
                .stage(BuildAutomationStageKind::GoldenSceneHeadlessRenders)
                .is_some_and(|stage| stage.passed)
        );
        assert!(capture.safety_provenance.passed());
        assert_eq!(capture.safety_provenance.domain_count, 6);
        assert_eq!(capture.safety_provenance.passed_domain_count, 6);
        assert_eq!(capture.safety_provenance.signal_count, 18);
        assert_eq!(capture.safety_provenance.passed_signal_count, 18);
        assert!(
            capture
                .safety_provenance
                .generated_provenance_evidence_count
                > 0
        );
        assert!(capture.safety_provenance.rights_evidence_count > 0);
        assert!(capture.safety_provenance.consent_evidence_count > 0);
        assert!(
            capture
                .safety_provenance
                .domain(SafetyProvenanceDomainKind::VoiceRightsConsent)
                .is_some_and(|domain| domain.passed)
        );
        assert!(
            capture
                .safety_provenance
                .domain(SafetyProvenanceDomainKind::AiContentGrounding)
                .is_some_and(|domain| domain.passed)
        );
        assert!(capture.asset_package_acceptance.passed());
        assert_eq!(capture.asset_package_acceptance.gate_count, 13);
        assert_eq!(capture.asset_package_acceptance.passed_gate_count, 13);
        assert_eq!(capture.asset_package_acceptance.blocked_gate_count, 0);
        assert_eq!(
            capture
                .asset_package_acceptance
                .versioned_package_output_count,
            capture.asset_package_acceptance.package_output_count
        );
        assert_eq!(
            capture.asset_package_acceptance.schema_declaration_count,
            capture.asset_package_acceptance.package_output_count
        );
        assert!(capture.asset_package_acceptance.package_output_count > 0);
        assert!(capture.asset_package_acceptance.package_schema_count > 0);
        assert!(
            capture.asset_package_acceptance.validation_report_count
                >= capture.asset_package_acceptance.package_output_count
        );
        assert!(capture.asset_package_acceptance.truth_lineage_count > 0);
        assert_eq!(
            capture.asset_package_acceptance.passed_truth_lineage_count,
            capture.asset_package_acceptance.truth_lineage_count
        );
        assert!(
            capture
                .asset_package_acceptance
                .generated_cache_signal_count
                > 0
        );
        assert!(capture.asset_package_acceptance.cache_key_test_signal_count > 0);
        assert!(
            capture
                .asset_package_acceptance
                .schema_validation_signal_count
                > 0
        );
        assert!(capture.asset_package_acceptance.provenance_evidence_count > 0);
        assert!(capture.asset_package_acceptance.rights_evidence_count > 0);
        assert!(capture.asset_package_acceptance.consent_evidence_count > 0);
        assert_eq!(
            capture.asset_package_acceptance.invalid_asset_failure_count,
            0
        );
        assert!(
            capture
                .asset_package_acceptance
                .invalid_asset_diagnostic_count
                > 0
        );
        assert_eq!(
            capture
                .asset_package_acceptance
                .clean_cache_rebuild_scene_count,
            capture.golden_scenes.scene_count
        );
        assert!(
            capture
                .asset_package_acceptance
                .clean_cache_rebuild_artifact_count
                > 0
        );
        assert!(capture.asset_package_acceptance.migration_step_count > 0);
        assert_eq!(capture.asset_package_acceptance.package_contract_count, 14);
        assert_eq!(
            capture
                .asset_package_acceptance
                .package_contract_ready_count,
            capture.asset_package_acceptance.package_contract_count
        );
        assert_eq!(capture.asset_package_acceptance.provenance_field_count, 9);
        assert_eq!(
            capture
                .asset_package_acceptance
                .provenance_field_ready_count,
            capture.asset_package_acceptance.provenance_field_count
        );
        assert_eq!(
            capture
                .asset_package_acceptance
                .cache_rebuild_contract_count,
            6
        );
        assert_eq!(
            capture
                .asset_package_acceptance
                .cache_rebuild_contract_ready_count,
            capture
                .asset_package_acceptance
                .cache_rebuild_contract_count
        );
        assert_eq!(
            capture
                .asset_package_acceptance
                .textureless_policy_signal_count,
            6
        );
        assert_eq!(
            capture
                .asset_package_acceptance
                .textureless_policy_ready_count,
            capture
                .asset_package_acceptance
                .textureless_policy_signal_count
        );
        assert_eq!(
            capture.asset_package_acceptance.runtime_loading_rule_count,
            10
        );
        assert_eq!(
            capture
                .asset_package_acceptance
                .runtime_loading_rule_ready_count,
            capture.asset_package_acceptance.runtime_loading_rule_count
        );
        assert_eq!(
            capture
                .asset_package_acceptance
                .quality_performance_tag_count,
            8
        );
        assert_eq!(
            capture
                .asset_package_acceptance
                .quality_performance_tag_ready_count,
            capture
                .asset_package_acceptance
                .quality_performance_tag_count
        );
        assert_eq!(
            capture
                .asset_package_acceptance
                .validation_rejection_rule_count,
            9
        );
        assert_eq!(
            capture
                .asset_package_acceptance
                .validation_rejection_rule_ready_count,
            capture
                .asset_package_acceptance
                .validation_rejection_rule_count
        );
        assert!(capture.asset_package_acceptance.streaming_bytes > 0);
        assert!(
            capture
                .asset_package_acceptance
                .gate(AssetPackageAcceptanceGateKind::PackageMetadataContract)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .asset_package_acceptance
                .gate(AssetPackageAcceptanceGateKind::ProvenanceRightsMetadata)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .asset_package_acceptance
                .gate(AssetPackageAcceptanceGateKind::TexturelessGeneratedMaterialStrategy)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .asset_package_acceptance
                .gate(AssetPackageAcceptanceGateKind::RuntimeLoadingPredictability)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .asset_package_acceptance
                .gate(AssetPackageAcceptanceGateKind::QualityPerformanceTags)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .asset_package_acceptance
                .gate(AssetPackageAcceptanceGateKind::ValidationRejectionRules)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .asset_package_acceptance
                .gate(AssetPackageAcceptanceGateKind::MigrationTests)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .asset_package_acceptance
                .gate(AssetPackageAcceptanceGateKind::CleanCacheGoldenRebuild)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.reference_validation_acceptance.passed());
        assert_eq!(capture.reference_validation_acceptance.gate_count, 5);
        assert_eq!(capture.reference_validation_acceptance.passed_gate_count, 5);
        assert_eq!(
            capture.reference_validation_acceptance.blocked_gate_count,
            0
        );
        assert!(
            capture
                .reference_validation_acceptance
                .reference_frame_count
                > 0
        );
        assert!(
            capture
                .reference_validation_acceptance
                .comparison_pass_count
                > 0
        );
        assert!(
            capture
                .reference_validation_acceptance
                .material_reference_evidence_count
                > 0
        );
        assert!(
            capture
                .reference_validation_acceptance
                .human_review_image_count
                > 0
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .covered_reference_use_case_count,
            capture
                .reference_validation_acceptance
                .reference_use_case_count
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .reference_use_case_count,
            7
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .covered_reference_stage_count,
            capture
                .reference_validation_acceptance
                .reference_stage_count
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .reference_stage_count,
            4
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .same_truth_domain_ready_count,
            capture
                .reference_validation_acceptance
                .same_truth_domain_count
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .same_truth_domain_count,
            8
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .comparison_output_ready_count,
            capture
                .reference_validation_acceptance
                .comparison_output_count
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .comparison_output_count,
            10
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .realtime_approximation_ready_count,
            capture
                .reference_validation_acceptance
                .realtime_approximation_count
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .realtime_approximation_count,
            10
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .preserved_quality_ready_count,
            capture
                .reference_validation_acceptance
                .preserved_quality_count
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .preserved_quality_count,
            8
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .material_validation_check_ready_count,
            capture
                .reference_validation_acceptance
                .material_validation_check_count
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .material_validation_check_count,
            8
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .human_validation_check_ready_count,
            capture
                .reference_validation_acceptance
                .human_validation_check_count
        );
        assert_eq!(
            capture
                .reference_validation_acceptance
                .human_validation_check_count,
            7
        );
        assert!(
            capture
                .reference_validation_acceptance
                .gate(ReferenceValidationGateKind::RainyAlleyReferenceComparison)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .reference_validation_acceptance
                .gate(ReferenceValidationGateKind::ChannelDifferenceInspection)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.renderer_acceptance.passed());
        assert_eq!(capture.renderer_acceptance.gate_count, 7);
        assert_eq!(capture.renderer_acceptance.passed_gate_count, 7);
        assert_eq!(capture.renderer_acceptance.blocked_gate_count, 0);
        assert!(capture.renderer_acceptance.rainy_alley_signal_count > 0);
        assert!(capture.renderer_acceptance.hero_human_signal_count > 0);
        assert!(capture.renderer_acceptance.reference_comparison_count > 0);
        assert!(capture.renderer_acceptance.performance_tier_signal_count >= 2);
        assert!(capture.renderer_acceptance.debug_view_signal_count > 0);
        assert!(capture.renderer_acceptance.streaming_stability_signal_count > 0);
        assert!(capture.renderer_acceptance.gpu_services_signal_count > 0);
        assert!(
            capture
                .renderer_acceptance
                .gate(RendererAcceptanceGateKind::RainyNeonAlley)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .renderer_acceptance
                .gate(RendererAcceptanceGateKind::GpuServicesOnly)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.virtual_geometry_acceptance.passed());
        assert_eq!(capture.virtual_geometry_acceptance.gate_count, 7);
        assert_eq!(capture.virtual_geometry_acceptance.passed_gate_count, 7);
        assert_eq!(capture.virtual_geometry_acceptance.blocked_gate_count, 0);
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .dense_surface_signal_count,
            6
        );
        assert_eq!(
            capture.virtual_geometry_acceptance.streaming_signal_count,
            6
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .fractured_geometry_signal_count,
            4
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .procedural_microgeometry_signal_count,
            4
        );
        assert_eq!(capture.virtual_geometry_acceptance.budget_signal_count, 5);
        assert_eq!(
            capture.virtual_geometry_acceptance.debug_view_signal_count,
            4
        );
        assert_eq!(capture.virtual_geometry_acceptance.fallback_signal_count, 4);
        assert!(
            capture
                .virtual_geometry_acceptance
                .microgeometry_frame_count
                > 0
        );
        assert!(
            capture
                .virtual_geometry_acceptance
                .peak_visible_cluster_capacity
                > 0
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .cluster_record_field_count,
            8
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .cluster_record_field_ready_count,
            capture
                .virtual_geometry_acceptance
                .cluster_record_field_count
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .visibility_pipeline_step_count,
            7
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .visibility_pipeline_step_ready_count,
            capture
                .virtual_geometry_acceptance
                .visibility_pipeline_step_count
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .microgeometry_source_count,
            6
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .microgeometry_source_ready_count,
            capture
                .virtual_geometry_acceptance
                .microgeometry_source_count
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .displacement_strategy_count,
            6
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .displacement_strategy_ready_count,
            capture
                .virtual_geometry_acceptance
                .displacement_strategy_count
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .fracture_integration_step_count,
            9
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .fracture_integration_step_ready_count,
            capture
                .virtual_geometry_acceptance
                .fracture_integration_step_count
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .world_streaming_package_field_count,
            7
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .world_streaming_package_field_ready_count,
            capture
                .virtual_geometry_acceptance
                .world_streaming_package_field_count
        );
        assert_eq!(
            capture.virtual_geometry_acceptance.performance_rule_count,
            6
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .performance_rule_ready_count,
            capture.virtual_geometry_acceptance.performance_rule_count
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .hardware_tier_policy_count,
            3
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .hardware_tier_policy_ready_count,
            capture
                .virtual_geometry_acceptance
                .hardware_tier_policy_count
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .debug_view_requirement_count,
            8
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .debug_view_requirement_ready_count,
            capture
                .virtual_geometry_acceptance
                .debug_view_requirement_count
        );
        assert_eq!(
            capture.virtual_geometry_acceptance.acceptance_scene_count,
            6
        );
        assert_eq!(
            capture
                .virtual_geometry_acceptance
                .acceptance_scene_ready_count,
            capture.virtual_geometry_acceptance.acceptance_scene_count
        );
        assert!(
            capture
                .virtual_geometry_acceptance
                .gate(VirtualGeometryAcceptanceGateKind::DenseRainyAlleySurfaces)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .virtual_geometry_acceptance
                .gate(VirtualGeometryAcceptanceGateKind::GeneratedProceduralMicrogeometry)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.virtual_geometry_pipeline_acceptance.passed());
        assert_eq!(capture.virtual_geometry_pipeline_acceptance.gate_count, 8);
        assert_eq!(
            capture
                .virtual_geometry_pipeline_acceptance
                .passed_gate_count,
            8
        );
        assert_eq!(
            capture
                .virtual_geometry_pipeline_acceptance
                .blocked_gate_count,
            0
        );
        assert_eq!(
            capture.virtual_geometry_pipeline_acceptance.signal_count,
            32
        );
        assert_eq!(
            capture
                .virtual_geometry_pipeline_acceptance
                .passed_signal_count,
            32
        );
        assert_eq!(
            capture
                .virtual_geometry_pipeline_acceptance
                .truth_cache_signal_count,
            4
        );
        assert_eq!(
            capture
                .virtual_geometry_pipeline_acceptance
                .cluster_data_signal_count,
            4
        );
        assert_eq!(
            capture
                .virtual_geometry_pipeline_acceptance
                .visibility_signal_count,
            4
        );
        assert_eq!(
            capture
                .virtual_geometry_pipeline_acceptance
                .streaming_lod_signal_count,
            4
        );
        assert_eq!(
            capture
                .virtual_geometry_pipeline_acceptance
                .microgeometry_signal_count,
            4
        );
        assert_eq!(
            capture
                .virtual_geometry_pipeline_acceptance
                .fracture_signal_count,
            4
        );
        assert_eq!(
            capture
                .virtual_geometry_pipeline_acceptance
                .budget_debug_signal_count,
            4
        );
        assert_eq!(
            capture
                .virtual_geometry_pipeline_acceptance
                .fallback_signal_count,
            4
        );
        assert!(
            capture
                .virtual_geometry_pipeline_acceptance
                .gate(VirtualGeometryPipelineAcceptanceGateKind::ClusterMeshletData)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .virtual_geometry_pipeline_acceptance
                .gate(VirtualGeometryPipelineAcceptanceGateKind::GpuVisibilityPipeline)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .virtual_geometry_pipeline_acceptance
                .gate(VirtualGeometryPipelineAcceptanceGateKind::FractureDamageGeometryUpdate)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .virtual_geometry_pipeline_acceptance
                .gate(VirtualGeometryPipelineAcceptanceGateKind::HardwareTierFallbacks)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.material_acceptance.passed());
        assert_eq!(capture.material_acceptance.gate_count, 7);
        assert_eq!(capture.material_acceptance.passed_gate_count, 7);
        assert_eq!(capture.material_acceptance.blocked_gate_count, 0);
        assert!(capture.material_acceptance.renderable_cache_signal_count > 0);
        assert!(capture.material_acceptance.material_lab_signal_count > 0);
        assert!(
            capture
                .material_acceptance
                .wetness_cross_system_signal_count
                > 0
        );
        assert!(capture.material_acceptance.fracture_interior_signal_count > 0);
        assert!(capture.material_acceptance.cache_rebuild_signal_count > 0);
        assert!(capture.material_acceptance.provenance_signal_count > 0);
        assert!(capture.material_acceptance.state_channel_signal_count > 0);
        assert!(capture.material_acceptance.material_runtime_frame_count > 0);
        assert_eq!(capture.material_acceptance.active_state_channel_count, 2);
        assert_eq!(
            capture.material_acceptance.fully_linked_state_channel_count,
            capture.material_acceptance.active_state_channel_count
        );
        assert_eq!(capture.material_acceptance.descriptor_domain_count, 6);
        assert_eq!(
            capture.material_acceptance.descriptor_domain_ready_count,
            capture.material_acceptance.descriptor_domain_count
        );
        assert_eq!(
            capture.material_acceptance.visual_descriptor_field_count,
            10
        );
        assert_eq!(
            capture
                .material_acceptance
                .visual_descriptor_field_ready_count,
            capture.material_acceptance.visual_descriptor_field_count
        );
        assert_eq!(
            capture.material_acceptance.physical_descriptor_field_count,
            11
        );
        assert_eq!(
            capture
                .material_acceptance
                .physical_descriptor_field_ready_count,
            capture.material_acceptance.physical_descriptor_field_count
        );
        assert_eq!(
            capture.material_acceptance.state_channel_requirement_count,
            14
        );
        assert_eq!(
            capture
                .material_acceptance
                .state_channel_requirement_ready_count,
            capture.material_acceptance.state_channel_requirement_count
        );
        assert_eq!(capture.material_acceptance.generated_data_type_count, 11);
        assert_eq!(
            capture.material_acceptance.generated_data_type_ready_count,
            capture.material_acceptance.generated_data_type_count
        );
        assert_eq!(
            capture
                .material_acceptance
                .capture_evidence_requirement_count,
            8
        );
        assert_eq!(
            capture
                .material_acceptance
                .capture_evidence_requirement_ready_count,
            capture
                .material_acceptance
                .capture_evidence_requirement_count
        );
        assert_eq!(
            capture.material_acceptance.graph_output_requirement_count,
            8
        );
        assert_eq!(
            capture
                .material_acceptance
                .graph_output_requirement_ready_count,
            capture.material_acceptance.graph_output_requirement_count
        );
        assert_eq!(
            capture
                .material_acceptance
                .rendering_integration_requirement_count,
            9
        );
        assert_eq!(
            capture
                .material_acceptance
                .rendering_integration_requirement_ready_count,
            capture
                .material_acceptance
                .rendering_integration_requirement_count
        );
        assert_eq!(
            capture
                .material_acceptance
                .physics_integration_requirement_count,
            9
        );
        assert_eq!(
            capture
                .material_acceptance
                .physics_integration_requirement_ready_count,
            capture
                .material_acceptance
                .physics_integration_requirement_count
        );
        assert_eq!(
            capture
                .material_acceptance
                .audio_integration_requirement_count,
            6
        );
        assert_eq!(
            capture
                .material_acceptance
                .audio_integration_requirement_ready_count,
            2
        );
        assert_eq!(
            capture
                .material_acceptance
                .ai_gameplay_integration_requirement_count,
            6
        );
        assert_eq!(
            capture
                .material_acceptance
                .ai_gameplay_integration_requirement_ready_count,
            capture
                .material_acceptance
                .ai_gameplay_integration_requirement_count
        );
        assert_eq!(capture.material_acceptance.performance_rule_count, 8);
        assert_eq!(
            capture.material_acceptance.performance_rule_ready_count,
            capture.material_acceptance.performance_rule_count
        );
        assert_eq!(capture.material_acceptance.debug_view_requirement_count, 9);
        assert_eq!(
            capture
                .material_acceptance
                .debug_view_requirement_ready_count,
            capture.material_acceptance.debug_view_requirement_count
        );
        assert_eq!(capture.material_acceptance.acceptance_scene_count, 6);
        assert_eq!(
            capture.material_acceptance.acceptance_scene_ready_count,
            capture.material_acceptance.acceptance_scene_count
        );
        assert!(
            capture
                .material_acceptance
                .gate(MaterialAcceptanceGateKind::RenderableCacheData)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .material_acceptance
                .gate(MaterialAcceptanceGateKind::StateChannelReadersWriters)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.material_pipeline_acceptance.passed());
        assert_eq!(capture.material_pipeline_acceptance.gate_count, 8);
        assert_eq!(capture.material_pipeline_acceptance.passed_gate_count, 8);
        assert_eq!(capture.material_pipeline_acceptance.blocked_gate_count, 0);
        assert_eq!(capture.material_pipeline_acceptance.signal_count, 32);
        assert_eq!(capture.material_pipeline_acceptance.passed_signal_count, 32);
        assert_eq!(
            capture
                .material_pipeline_acceptance
                .descriptor_truth_signal_count,
            4
        );
        assert_eq!(
            capture
                .material_pipeline_acceptance
                .procedural_graph_signal_count,
            4
        );
        assert_eq!(
            capture
                .material_pipeline_acceptance
                .generated_cache_signal_count,
            4
        );
        assert_eq!(
            capture
                .material_pipeline_acceptance
                .state_channel_signal_count,
            4
        );
        assert_eq!(
            capture
                .material_pipeline_acceptance
                .cross_system_signal_count,
            4
        );
        assert_eq!(
            capture
                .material_pipeline_acceptance
                .fracture_interior_signal_count,
            4
        );
        assert_eq!(
            capture.material_pipeline_acceptance.provenance_signal_count,
            4
        );
        assert_eq!(
            capture
                .material_pipeline_acceptance
                .rebuild_fallback_signal_count,
            4
        );
        assert_eq!(
            capture
                .material_pipeline_acceptance
                .active_state_channel_count,
            2
        );
        assert_eq!(
            capture
                .material_pipeline_acceptance
                .fully_linked_state_channel_count,
            capture
                .material_pipeline_acceptance
                .active_state_channel_count
        );
        assert!(
            capture
                .material_pipeline_acceptance
                .gate(MaterialPipelineAcceptanceGateKind::DescriptorTruthContracts)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .material_pipeline_acceptance
                .gate(MaterialPipelineAcceptanceGateKind::GeneratedCacheRuntime)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .material_pipeline_acceptance
                .gate(MaterialPipelineAcceptanceGateKind::WetnessCrossSystemBehavior)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .material_pipeline_acceptance
                .gate(MaterialPipelineAcceptanceGateKind::FractureInteriorSurfaces)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .material_pipeline_acceptance
                .gate(MaterialPipelineAcceptanceGateKind::CacheRebuildAndFallbacks)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.physics_acceptance.passed());
        assert_eq!(capture.physics_acceptance.gate_count, 7);
        assert_eq!(capture.physics_acceptance.passed_gate_count, 7);
        assert_eq!(capture.physics_acceptance.blocked_gate_count, 0);
        assert!(
            capture
                .physics_acceptance
                .shopfront_consequence_signal_count
                > 0
        );
        assert!(capture.physics_acceptance.wetness_behavior_signal_count > 0);
        assert!(
            capture
                .physics_acceptance
                .gas_visibility_lighting_signal_count
                > 0
        );
        assert!(capture.physics_acceptance.quality_island_signal_count > 0);
        assert!(
            capture
                .physics_acceptance
                .material_schema_delta_signal_count
                > 0
        );
        assert!(capture.physics_acceptance.replay_debug_signal_count > 0);
        assert!(capture.physics_acceptance.fallback_signal_count > 0);
        assert!(capture.physics_acceptance.physics_runtime_frame_count > 0);
        assert!(capture.physics_acceptance.fully_linked_fracture_count > 0);
        assert!(capture.physics_acceptance.consequential_volume_count >= 2);
        assert!(capture.physics_acceptance.quality_island_frame_count > 0);
        assert_eq!(capture.physics_acceptance.required_domain_count, 10);
        assert_eq!(
            capture.physics_acceptance.required_domain_ready_count,
            capture.physics_acceptance.required_domain_count
        );
        assert_eq!(capture.physics_acceptance.fidelity_tier_count, 6);
        assert_eq!(
            capture.physics_acceptance.fidelity_tier_ready_count,
            capture.physics_acceptance.fidelity_tier_count
        );
        assert_eq!(capture.physics_acceptance.solver_family_count, 6);
        assert_eq!(
            capture.physics_acceptance.solver_family_ready_count,
            capture.physics_acceptance.solver_family_count
        );
        assert_eq!(capture.physics_acceptance.material_coupling_count, 8);
        assert_eq!(
            capture.physics_acceptance.material_coupling_ready_count,
            capture.physics_acceptance.material_coupling_count
        );
        assert_eq!(capture.physics_acceptance.fracture_output_count, 10);
        assert_eq!(
            capture.physics_acceptance.fracture_output_ready_count,
            capture.physics_acceptance.fracture_output_count
        );
        assert_eq!(capture.physics_acceptance.liquid_requirement_count, 9);
        assert_eq!(
            capture.physics_acceptance.liquid_requirement_ready_count,
            capture.physics_acceptance.liquid_requirement_count
        );
        assert_eq!(capture.physics_acceptance.gas_requirement_count, 10);
        assert_eq!(
            capture.physics_acceptance.gas_requirement_ready_count,
            capture.physics_acceptance.gas_requirement_count
        );
        assert_eq!(
            capture.physics_acceptance.human_physics_requirement_count,
            7
        );
        assert_eq!(
            capture
                .physics_acceptance
                .human_physics_requirement_ready_count,
            capture.physics_acceptance.human_physics_requirement_count
        );
        assert_eq!(capture.physics_acceptance.ai_story_event_count, 8);
        assert_eq!(
            capture.physics_acceptance.ai_story_event_ready_count,
            capture.physics_acceptance.ai_story_event_count
        );
        assert_eq!(
            capture.physics_acceptance.performance_promotion_rule_count,
            8
        );
        assert_eq!(
            capture
                .physics_acceptance
                .performance_promotion_rule_ready_count,
            capture.physics_acceptance.performance_promotion_rule_count
        );
        assert_eq!(
            capture
                .physics_acceptance
                .determinism_replay_requirement_count,
            5
        );
        assert_eq!(
            capture
                .physics_acceptance
                .determinism_replay_requirement_ready_count,
            capture
                .physics_acceptance
                .determinism_replay_requirement_count
        );
        assert_eq!(capture.physics_acceptance.debug_view_requirement_count, 10);
        assert_eq!(
            capture
                .physics_acceptance
                .debug_view_requirement_ready_count,
            capture.physics_acceptance.debug_view_requirement_count
        );
        assert_eq!(capture.physics_acceptance.acceptance_scene_count, 6);
        assert_eq!(
            capture.physics_acceptance.acceptance_scene_ready_count,
            capture.physics_acceptance.acceptance_scene_count
        );
        assert!(
            capture
                .physics_acceptance
                .gate(PhysicsAcceptanceGateKind::DestructibleShopfrontConsequence)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .physics_acceptance
                .gate(PhysicsAcceptanceGateKind::FallbackTiersPreserveConsequence)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.physics_pipeline_acceptance.passed());
        assert_eq!(capture.physics_pipeline_acceptance.gate_count, 8);
        assert_eq!(capture.physics_pipeline_acceptance.passed_gate_count, 8);
        assert_eq!(capture.physics_pipeline_acceptance.blocked_gate_count, 0);
        assert_eq!(capture.physics_pipeline_acceptance.signal_count, 32);
        assert_eq!(capture.physics_pipeline_acceptance.passed_signal_count, 32);
        assert_eq!(capture.physics_pipeline_acceptance.solver_signal_count, 4);
        assert_eq!(
            capture
                .physics_pipeline_acceptance
                .quality_island_signal_count,
            4
        );
        assert_eq!(capture.physics_pipeline_acceptance.fracture_signal_count, 4);
        assert_eq!(capture.physics_pipeline_acceptance.liquid_signal_count, 4);
        assert_eq!(capture.physics_pipeline_acceptance.gas_signal_count, 4);
        assert_eq!(
            capture
                .physics_pipeline_acceptance
                .material_state_signal_count,
            4
        );
        assert_eq!(
            capture
                .physics_pipeline_acceptance
                .replay_debug_signal_count,
            4
        );
        assert_eq!(capture.physics_pipeline_acceptance.fallback_signal_count, 4);
        assert!(
            capture
                .physics_pipeline_acceptance
                .physics_runtime_frame_count
                > 0
        );
        assert!(
            capture
                .physics_pipeline_acceptance
                .fully_linked_fracture_count
                > 0
        );
        assert!(
            capture
                .physics_pipeline_acceptance
                .consequential_volume_count
                >= 2
        );
        assert!(
            capture
                .physics_pipeline_acceptance
                .quality_island_frame_count
                > 0
        );
        assert!(
            capture
                .physics_pipeline_acceptance
                .gate(PhysicsPipelineAcceptanceGateKind::SolverFamilyCoverage)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .physics_pipeline_acceptance
                .gate(PhysicsPipelineAcceptanceGateKind::FractureDestructionConsequences)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .physics_pipeline_acceptance
                .gate(PhysicsPipelineAcceptanceGateKind::LiquidWetnessConsequences)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .physics_pipeline_acceptance
                .gate(PhysicsPipelineAcceptanceGateKind::GasPressureVisibilityConsequences)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .physics_pipeline_acceptance
                .gate(PhysicsPipelineAcceptanceGateKind::FallbackTierConsequencePreservation)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.human_acceptance.passed());
        assert_eq!(capture.human_acceptance.gate_count, 9);
        assert_eq!(capture.human_acceptance.passed_gate_count, 9);
        assert_eq!(capture.human_acceptance.blocked_gate_count, 0);
        assert!(capture.human_acceptance.neutral_face_signal_count > 0);
        assert!(capture.human_acceptance.eye_lighting_signal_count > 0);
        assert!(capture.human_acceptance.speech_sync_signal_count > 0);
        assert!(capture.human_acceptance.hair_stability_signal_count > 0);
        assert!(capture.human_acceptance.skin_lighting_signal_count > 0);
        assert!(capture.human_acceptance.body_motion_signal_count > 0);
        assert!(capture.human_acceptance.lod_stability_signal_count > 0);
        assert!(capture.human_acceptance.provenance_signal_count > 0);
        assert!(capture.human_acceptance.golden_scene_signal_count > 0);
        assert!(capture.human_acceptance.human_rendering_frame_count > 0);
        assert!(capture.human_acceptance.synchronized_speech_count > 0);
        assert!(capture.human_acceptance.hero_bundle_request_count > 0);
        assert!(capture.human_acceptance.appearance_state_count > 0);
        assert_eq!(capture.human_acceptance.required_layer_count, 13);
        assert_eq!(
            capture.human_acceptance.required_layer_ready_count,
            capture.human_acceptance.required_layer_count
        );
        assert_eq!(capture.human_acceptance.quality_tier_count, 5);
        assert_eq!(
            capture.human_acceptance.quality_tier_ready_count,
            capture.human_acceptance.quality_tier_count
        );
        assert_eq!(capture.human_acceptance.skin_requirement_count, 12);
        assert_eq!(
            capture.human_acceptance.skin_requirement_ready_count,
            capture.human_acceptance.skin_requirement_count
        );
        assert_eq!(capture.human_acceptance.eye_requirement_count, 11);
        assert_eq!(
            capture.human_acceptance.eye_requirement_ready_count,
            capture.human_acceptance.eye_requirement_count
        );
        assert_eq!(capture.human_acceptance.mouth_requirement_count, 9);
        assert_eq!(
            capture.human_acceptance.mouth_requirement_ready_count,
            capture.human_acceptance.mouth_requirement_count
        );
        assert_eq!(capture.human_acceptance.hair_requirement_count, 8);
        assert_eq!(
            capture.human_acceptance.hair_requirement_ready_count,
            capture.human_acceptance.hair_requirement_count
        );
        assert_eq!(capture.human_acceptance.body_motion_requirement_count, 11);
        assert_eq!(
            capture.human_acceptance.body_motion_requirement_ready_count,
            capture.human_acceptance.body_motion_requirement_count
        );
        assert_eq!(
            capture
                .human_acceptance
                .clothing_cybernetics_requirement_count,
            9
        );
        assert_eq!(
            capture
                .human_acceptance
                .clothing_cybernetics_requirement_ready_count,
            capture
                .human_acceptance
                .clothing_cybernetics_requirement_count
        );
        assert_eq!(capture.human_acceptance.ai_voice_integration_count, 8);
        assert_eq!(
            capture.human_acceptance.ai_voice_integration_ready_count,
            capture.human_acceptance.ai_voice_integration_count
        );
        assert_eq!(
            capture.human_acceptance.procedural_generation_policy_count,
            6
        );
        assert_eq!(
            capture
                .human_acceptance
                .procedural_generation_policy_ready_count,
            capture.human_acceptance.procedural_generation_policy_count
        );
        assert_eq!(capture.human_acceptance.renderer_interface_count, 9);
        assert_eq!(
            capture.human_acceptance.renderer_interface_ready_count,
            capture.human_acceptance.renderer_interface_count
        );
        assert_eq!(capture.human_acceptance.debug_view_requirement_count, 9);
        assert_eq!(
            capture.human_acceptance.debug_view_requirement_ready_count,
            capture.human_acceptance.debug_view_requirement_count
        );
        assert_eq!(capture.human_acceptance.acceptance_scene_count, 7);
        assert_eq!(
            capture.human_acceptance.acceptance_scene_ready_count,
            capture.human_acceptance.acceptance_scene_count
        );
        assert!(
            capture
                .human_acceptance
                .gate(HumanAcceptanceGateKind::SpeechLipFacialSync)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .human_acceptance
                .gate(HumanAcceptanceGateKind::HeroHumanGoldenCloseup)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.human_pipeline_acceptance.passed());
        assert_eq!(capture.human_pipeline_acceptance.gate_count, 9);
        assert_eq!(capture.human_pipeline_acceptance.passed_gate_count, 9);
        assert_eq!(capture.human_pipeline_acceptance.blocked_gate_count, 0);
        assert_eq!(capture.human_pipeline_acceptance.signal_count, 36);
        assert_eq!(capture.human_pipeline_acceptance.passed_signal_count, 36);
        assert_eq!(
            capture.human_pipeline_acceptance.truth_cache_signal_count,
            4
        );
        assert_eq!(capture.human_pipeline_acceptance.face_mouth_signal_count, 4);
        assert_eq!(capture.human_pipeline_acceptance.skin_signal_count, 4);
        assert_eq!(capture.human_pipeline_acceptance.eye_signal_count, 4);
        assert_eq!(capture.human_pipeline_acceptance.hair_signal_count, 4);
        assert_eq!(
            capture.human_pipeline_acceptance.body_motion_signal_count,
            4
        );
        assert_eq!(capture.human_pipeline_acceptance.lod_signal_count, 4);
        assert_eq!(capture.human_pipeline_acceptance.ai_voice_signal_count, 4);
        assert_eq!(capture.human_pipeline_acceptance.provenance_signal_count, 4);
        assert!(
            capture
                .human_pipeline_acceptance
                .human_rendering_frame_count
                > 0
        );
        assert!(
            capture
                .human_pipeline_acceptance
                .runtime_bundle_request_count
                > 0
        );
        assert!(
            capture
                .human_pipeline_acceptance
                .renderer_detail_request_count
                > 0
        );
        assert!(
            capture
                .human_pipeline_acceptance
                .synchronized_performance_count
                > 0
        );
        assert!(
            capture
                .human_pipeline_acceptance
                .responsive_appearance_count
                > 0
        );
        assert!(
            capture
                .human_pipeline_acceptance
                .gate(HumanPipelineAcceptanceGateKind::TruthCacheContracts)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .human_pipeline_acceptance
                .gate(HumanPipelineAcceptanceGateKind::FaceMouthRigging)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .human_pipeline_acceptance
                .gate(HumanPipelineAcceptanceGateKind::LayeredSkinResponse)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .human_pipeline_acceptance
                .gate(HumanPipelineAcceptanceGateKind::AiVoicePerformanceIntegration)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .human_pipeline_acceptance
                .gate(HumanPipelineAcceptanceGateKind::ProvenanceGoldenCloseup)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.ai_acceptance.passed());
        assert_eq!(capture.ai_acceptance.gate_count, 8);
        assert_eq!(capture.ai_acceptance.passed_gate_count, 8);
        assert_eq!(capture.ai_acceptance.blocked_gate_count, 0);
        assert!(capture.ai_acceptance.shopfront_reaction_signal_count > 0);
        assert!(capture.ai_acceptance.limited_perception_signal_count > 0);
        assert!(capture.ai_acceptance.dialogue_grounding_signal_count > 0);
        assert!(capture.ai_acceptance.action_validation_signal_count > 0);
        assert!(capture.ai_acceptance.story_pressure_signal_count > 0);
        assert!(capture.ai_acceptance.decision_tool_signal_count > 0);
        assert!(capture.ai_acceptance.memory_audit_signal_count > 0);
        assert!(capture.ai_acceptance.voice_handoff_signal_count > 0);
        assert!(capture.ai_acceptance.agent_decision_count >= 2);
        assert!(capture.ai_acceptance.memory_update_count >= 2);
        assert!(capture.ai_acceptance.accepted_intent_count >= 4);
        assert!(capture.ai_acceptance.rejected_intent_count >= 2);
        assert!(capture.ai_acceptance.grounded_dialogue_count >= 1);
        assert!(capture.ai_acceptance.story_event_count >= 1);
        assert!(capture.ai_acceptance.faction_event_count >= 3);
        assert!(
            capture
                .ai_acceptance
                .gate(AiAcceptanceGateKind::InvalidActionRejection)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .ai_acceptance
                .gate(AiAcceptanceGateKind::VoiceLipSyncHandoff)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.ai_pipeline_acceptance.passed());
        assert_eq!(capture.ai_pipeline_acceptance.gate_count, 8);
        assert_eq!(capture.ai_pipeline_acceptance.passed_gate_count, 8);
        assert_eq!(capture.ai_pipeline_acceptance.blocked_gate_count, 0);
        assert_eq!(capture.ai_pipeline_acceptance.signal_count, 32);
        assert_eq!(capture.ai_pipeline_acceptance.passed_signal_count, 32);
        assert_eq!(
            capture.ai_pipeline_acceptance.world_authority_signal_count,
            4
        );
        assert_eq!(capture.ai_pipeline_acceptance.perception_signal_count, 4);
        assert_eq!(capture.ai_pipeline_acceptance.agent_model_signal_count, 4);
        assert_eq!(
            capture
                .ai_pipeline_acceptance
                .intent_validation_signal_count,
            4
        );
        assert_eq!(
            capture.ai_pipeline_acceptance.dialogue_safety_signal_count,
            4
        );
        assert_eq!(
            capture.ai_pipeline_acceptance.story_director_signal_count,
            4
        );
        assert_eq!(capture.ai_pipeline_acceptance.replay_audit_signal_count, 4);
        assert_eq!(capture.ai_pipeline_acceptance.voice_handoff_signal_count, 4);
        assert!(capture.ai_pipeline_acceptance.agent_decision_count >= 2);
        assert!(capture.ai_pipeline_acceptance.sourced_memory_count >= 2);
        assert!(capture.ai_pipeline_acceptance.sourced_decision_count >= 2);
        assert!(capture.ai_pipeline_acceptance.grounded_dialogue_count >= 1);
        assert!(capture.ai_pipeline_acceptance.accepted_intent_count >= 4);
        assert!(capture.ai_pipeline_acceptance.rejected_intent_count >= 2);
        assert!(capture.ai_pipeline_acceptance.story_pressure_event_count >= 4);
        assert!(
            capture
                .ai_pipeline_acceptance
                .gate(AiPipelineAcceptanceGateKind::WorldAuthorityBoundary)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .ai_pipeline_acceptance
                .gate(AiPipelineAcceptanceGateKind::PerceptionKnowledgeLimits)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .ai_pipeline_acceptance
                .gate(AiPipelineAcceptanceGateKind::GroundedDialogueSafety)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .ai_pipeline_acceptance
                .gate(AiPipelineAcceptanceGateKind::ReplayDebugAudit)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .ai_pipeline_acceptance
                .gate(AiPipelineAcceptanceGateKind::VoiceLipSyncHandoff)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.voice_acceptance.passed());
        assert_eq!(capture.voice_acceptance.gate_count, 7);
        assert_eq!(capture.voice_acceptance.passed_gate_count, 7);
        assert_eq!(capture.voice_acceptance.blocked_gate_count, 0);
        assert!(capture.voice_acceptance.hero_speech_timing_signal_count > 0);
        assert!(capture.voice_acceptance.fracture_audio_ai_signal_count > 0);
        assert!(capture.voice_acceptance.material_state_audio_signal_count > 0);
        assert!(capture.voice_acceptance.nonblocking_generation_signal_count > 0);
        assert!(capture.voice_acceptance.persona_consistency_signal_count > 0);
        assert!(capture.voice_acceptance.spatial_audio_signal_count > 0);
        assert!(capture.voice_acceptance.provenance_signal_count > 0);
        assert!(capture.voice_acceptance.speech_event_count >= 1);
        assert!(capture.voice_acceptance.synced_speech_count >= 1);
        assert!(capture.voice_acceptance.physical_sound_count >= 2);
        assert!(capture.voice_acceptance.material_aware_sound_count >= 2);
        assert!(capture.voice_acceptance.voice_persona_count >= 1);
        assert_eq!(
            capture.voice_acceptance.performance_capture_count,
            capture.voice_acceptance.validated_performance_capture_count
        );
        assert!(
            capture
                .voice_acceptance
                .gate(VoiceAcceptanceGateKind::HeroSpeechTiming)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .voice_acceptance
                .gate(VoiceAcceptanceGateKind::ProvenanceMetadata)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.voice_pipeline_acceptance.passed());
        assert_eq!(capture.voice_pipeline_acceptance.gate_count, 7);
        assert_eq!(capture.voice_pipeline_acceptance.passed_gate_count, 7);
        assert_eq!(capture.voice_pipeline_acceptance.blocked_gate_count, 0);
        assert_eq!(capture.voice_pipeline_acceptance.signal_count, 28);
        assert_eq!(capture.voice_pipeline_acceptance.passed_signal_count, 28);
        assert_eq!(
            capture.voice_pipeline_acceptance.speech_timing_signal_count,
            4
        );
        assert_eq!(
            capture
                .voice_pipeline_acceptance
                .physical_audio_signal_count,
            4
        );
        assert_eq!(
            capture
                .voice_pipeline_acceptance
                .material_foley_signal_count,
            4
        );
        assert_eq!(capture.voice_pipeline_acceptance.latency_signal_count, 4);
        assert_eq!(capture.voice_pipeline_acceptance.persona_signal_count, 4);
        assert_eq!(
            capture
                .voice_pipeline_acceptance
                .spatial_acoustic_signal_count,
            4
        );
        assert_eq!(
            capture
                .voice_pipeline_acceptance
                .provenance_debug_signal_count,
            4
        );
        assert!(capture.voice_pipeline_acceptance.speech_event_count >= 1);
        assert!(capture.voice_pipeline_acceptance.synced_speech_count >= 1);
        assert!(capture.voice_pipeline_acceptance.phoneme_sample_count > 0);
        assert!(capture.voice_pipeline_acceptance.viseme_sample_count > 0);
        assert_eq!(
            capture.voice_pipeline_acceptance.performance_capture_count,
            capture
                .voice_pipeline_acceptance
                .validated_performance_capture_count
        );
        assert!(capture.voice_pipeline_acceptance.performance_capture_count > 0);
        assert!(capture.voice_pipeline_acceptance.physical_sound_count >= 2);
        assert!(capture.voice_pipeline_acceptance.material_aware_sound_count >= 2);
        assert!(capture.voice_pipeline_acceptance.spatial_sound_count >= 1);
        assert!(capture.voice_pipeline_acceptance.ai_heard_event_count >= 1);
        assert!(capture.voice_pipeline_acceptance.soundscape_layer_count >= 3);
        assert!(
            capture
                .voice_pipeline_acceptance
                .gate(VoicePipelineAcceptanceGateKind::HeroSpeechTimingAndPerformance)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .voice_pipeline_acceptance
                .gate(VoicePipelineAcceptanceGateKind::PhysicalAudioAiHearing)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .voice_pipeline_acceptance
                .gate(VoicePipelineAcceptanceGateKind::ProvenanceAndDebugCoverage)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.city_world_acceptance.passed());
        assert_eq!(capture.city_world_acceptance.gate_count, 7);
        assert_eq!(capture.city_world_acceptance.passed_gate_count, 7);
        assert_eq!(capture.city_world_acceptance.blocked_gate_count, 0);
        assert!(
            capture
                .city_world_acceptance
                .rainy_alley_streaming_signal_count
                > 0
        );
        assert!(
            capture
                .city_world_acceptance
                .infrastructure_consequence_signal_count
                > 0
        );
        assert!(capture.city_world_acceptance.district_identity_signal_count > 0);
        assert!(capture.city_world_acceptance.persistence_signal_count > 0);
        assert!(
            capture
                .city_world_acceptance
                .population_identity_signal_count
                > 0
        );
        assert!(capture.city_world_acceptance.streaming_budget_signal_count > 0);
        assert!(
            capture
                .city_world_acceptance
                .grounded_story_hook_signal_count
                > 0
        );
        assert!(capture.city_world_acceptance.district_count >= 3);
        assert!(capture.city_world_acceptance.streaming_cell_count >= 3);
        assert_eq!(
            capture.city_world_acceptance.loaded_cell_count,
            capture.city_world_acceptance.streaming_cell_count
        );
        assert!(
            capture
                .city_world_acceptance
                .unique_streaming_dependency_count
                > 0
        );
        assert!(capture.city_world_acceptance.requested_streaming_bytes <= 256 * 1024 * 1024);
        assert!(capture.city_world_acceptance.persistent_event_count > 0);
        assert!(
            capture.city_world_acceptance.population_summary_count
                == capture.city_world_acceptance.streaming_cell_count
        );
        assert!(capture.city_world_acceptance.story_hook_count > 0);
        assert!(
            capture
                .city_world_acceptance
                .gate(CityWorldAcceptanceGateKind::RainyAlleyStreaming)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .city_world_acceptance
                .gate(CityWorldAcceptanceGateKind::GroundedStoryHooks)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.city_world_pipeline_acceptance.passed());
        assert_eq!(capture.city_world_pipeline_acceptance.gate_count, 7);
        assert_eq!(capture.city_world_pipeline_acceptance.passed_gate_count, 7);
        assert_eq!(capture.city_world_pipeline_acceptance.blocked_gate_count, 0);
        assert_eq!(capture.city_world_pipeline_acceptance.signal_count, 28);
        assert_eq!(
            capture.city_world_pipeline_acceptance.passed_signal_count,
            28
        );
        assert_eq!(
            capture
                .city_world_pipeline_acceptance
                .truth_stream_signal_count,
            4
        );
        assert_eq!(
            capture
                .city_world_pipeline_acceptance
                .district_identity_signal_count,
            4
        );
        assert_eq!(
            capture
                .city_world_pipeline_acceptance
                .infrastructure_signal_count,
            4
        );
        assert_eq!(
            capture
                .city_world_pipeline_acceptance
                .streaming_cell_signal_count,
            4
        );
        assert_eq!(
            capture
                .city_world_pipeline_acceptance
                .procedural_content_signal_count,
            4
        );
        assert_eq!(
            capture
                .city_world_pipeline_acceptance
                .persistence_navigation_signal_count,
            4
        );
        assert_eq!(
            capture
                .city_world_pipeline_acceptance
                .story_budget_signal_count,
            4
        );
        assert!(capture.city_world_pipeline_acceptance.district_count >= 3);
        assert!(capture.city_world_pipeline_acceptance.streaming_cell_count >= 3);
        assert_eq!(
            capture.city_world_pipeline_acceptance.loaded_cell_count,
            capture.city_world_pipeline_acceptance.streaming_cell_count
        );
        assert!(
            capture
                .city_world_pipeline_acceptance
                .infrastructure_system_count
                >= 6
        );
        assert!(capture.city_world_pipeline_acceptance.npc_seed_count > 0);
        assert!(
            capture
                .city_world_pipeline_acceptance
                .persistent_event_count
                > 0
        );
        assert!(
            capture
                .city_world_pipeline_acceptance
                .navigation_route_update_count
                > 0
        );
        assert!(capture.city_world_pipeline_acceptance.story_hook_count > 0);
        assert!(
            capture
                .city_world_pipeline_acceptance
                .requested_streaming_bytes
                <= 256 * 1024 * 1024
        );
        assert!(
            capture
                .city_world_pipeline_acceptance
                .estimated_streaming_memory_bytes
                <= 256 * 1024 * 1024
        );
        assert!(
            capture
                .city_world_pipeline_acceptance
                .gate(CityWorldPipelineAcceptanceGateKind::WorldTruthStreamBoundary)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .city_world_pipeline_acceptance
                .gate(CityWorldPipelineAcceptanceGateKind::InfrastructureNetworkConsequences)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .city_world_pipeline_acceptance
                .gate(CityWorldPipelineAcceptanceGateKind::BudgetedGroundedStoryHooks)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.tools_acceptance.passed());
        assert_eq!(capture.tools_acceptance.gate_count, 7);
        assert_eq!(capture.tools_acceptance.passed_gate_count, 7);
        assert_eq!(capture.tools_acceptance.blocked_gate_count, 0);
        assert!(capture.tools_acceptance.debug_view_signal_count > 0);
        assert!(capture.tools_acceptance.profiler_signal_count > 0);
        assert!(capture.tools_acceptance.golden_runner_signal_count > 0);
        assert!(capture.tools_acceptance.asset_validation_signal_count > 0);
        assert!(capture.tools_acceptance.replay_reproduction_signal_count > 0);
        assert!(capture.tools_acceptance.ai_inspection_signal_count > 0);
        assert!(capture.tools_acceptance.reference_review_signal_count > 0);
        assert_eq!(capture.tools_acceptance.required_tool_count, 17);
        assert_eq!(
            capture.tools_acceptance.available_required_tool_count,
            capture.tools_acceptance.required_tool_count
        );
        assert_eq!(capture.tools_acceptance.missing_required_tool_count, 0);
        assert_eq!(capture.tools_acceptance.component_debug_domain_count, 8);
        assert_eq!(
            capture.tools_acceptance.component_debug_domain_ready_count,
            capture.tools_acceptance.component_debug_domain_count
        );
        assert_eq!(capture.tools_acceptance.profiler_dimension_count, 11);
        assert_eq!(
            capture.tools_acceptance.profiler_dimension_ready_count,
            capture.tools_acceptance.profiler_dimension_count
        );
        assert_eq!(capture.tools_acceptance.validation_check_count, 11);
        assert_eq!(
            capture.tools_acceptance.validation_check_ready_count,
            capture.tools_acceptance.validation_check_count
        );
        assert!(capture.tools_acceptance.failure_report_evidence_count > 0);
        assert!(capture.tools_acceptance.reviewer_artifact_count > 0);
        assert!(capture.tools_acceptance.available_panel_count >= 11);
        assert!(capture.tools_acceptance.profiler_sample_count > 0);
        assert!(capture.tools_acceptance.performance_hotspot_count > 0);
        assert!(capture.tools_acceptance.golden_scene_count >= 7);
        assert!(capture.tools_acceptance.replay_frame_count > 0);
        assert!(capture.tools_acceptance.ai_decision_count >= 2);
        assert!(capture.tools_acceptance.reference_comparison_count > 0);
        assert!(
            capture
                .tools_acceptance
                .gate(ToolsAcceptanceGateKind::InspectableAiDecisions)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .tools_acceptance
                .gate(ToolsAcceptanceGateKind::ReferenceGameplayReview)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.performance_acceptance.passed());
        assert_eq!(capture.performance_acceptance.gate_count, 6);
        assert_eq!(capture.performance_acceptance.passed_gate_count, 6);
        assert_eq!(capture.performance_acceptance.blocked_gate_count, 0);
        assert!(capture.performance_acceptance.budget_signal_count > 0);
        assert!(
            capture
                .performance_acceptance
                .profiler_ownership_signal_count
                > 0
        );
        assert!(capture.performance_acceptance.quality_tier_signal_count > 0);
        assert!(capture.performance_acceptance.degradation_signal_count > 0);
        assert!(capture.performance_acceptance.gpu_graph_signal_count > 0);
        assert!(capture.performance_acceptance.memory_pressure_signal_count > 0);
        assert_eq!(
            capture.performance_acceptance.complete_budget_count,
            capture.performance_acceptance.component_budget_count
        );
        assert!(capture.performance_acceptance.profiler_sample_count > 0);
        assert!(capture.performance_acceptance.heatmap_frame_count > 0);
        assert!(capture.performance_acceptance.heatmap_module_count > 0);
        assert!(capture.performance_acceptance.supported_quality_tier_count >= 2);
        assert!(capture.performance_acceptance.golden_scene_count >= 7);
        assert!(capture.performance_acceptance.golden_timing_sample_count > 0);
        assert!(capture.performance_acceptance.declared_failure_policy_count > 0);
        assert!(capture.performance_acceptance.owned_gpu_pass_count > 0);
        assert!(capture.performance_acceptance.gpu_resource_count > 0);
        assert!(capture.performance_acceptance.visible_memory_bytes > 0);
        assert!(capture.performance_acceptance.visible_streaming_bytes > 0);
        assert_eq!(
            capture.performance_acceptance.covered_budget_category_count,
            capture
                .performance_acceptance
                .required_budget_category_count
        );
        assert_eq!(
            capture
                .performance_acceptance
                .required_budget_category_count,
            10
        );
        assert_eq!(
            capture.performance_acceptance.covered_hardware_tier_count,
            capture.performance_acceptance.hardware_tier_count
        );
        assert_eq!(capture.performance_acceptance.hardware_tier_count, 4);
        assert_eq!(
            capture
                .performance_acceptance
                .covered_scalability_ladder_step_count,
            capture.performance_acceptance.scalability_ladder_step_count
        );
        assert_eq!(
            capture.performance_acceptance.scalability_ladder_step_count,
            10
        );
        assert_eq!(
            capture
                .performance_acceptance
                .protected_truth_domain_safe_count,
            capture.performance_acceptance.protected_truth_domain_count
        );
        assert_eq!(
            capture.performance_acceptance.protected_truth_domain_count,
            6
        );
        assert_eq!(
            capture
                .performance_acceptance
                .handled_memory_pressure_action_count,
            capture.performance_acceptance.memory_pressure_action_count
        );
        assert_eq!(
            capture.performance_acceptance.memory_pressure_action_count,
            7
        );
        assert_eq!(
            capture
                .performance_acceptance
                .visible_streaming_counter_count,
            capture.performance_acceptance.streaming_counter_count
        );
        assert_eq!(capture.performance_acceptance.streaming_counter_count, 7);
        assert!(capture.performance_acceptance.shader_variant_count > 0);
        assert_eq!(capture.performance_acceptance.cache_build_signal_count, 5);
        assert!(
            capture
                .performance_acceptance
                .gate(PerformanceAcceptanceGateKind::ProfilerBudgetOwnership)
                .is_some_and(|gate| gate.passed)
        );
        assert!(
            capture
                .performance_acceptance
                .gate(PerformanceAcceptanceGateKind::MemoryPressureVisibleHandled)
                .is_some_and(|gate| gate.passed)
        );
        assert!(capture.open_research.passed());
        assert_eq!(capture.open_research.package_count, 9);
        assert_eq!(capture.open_research.ready_package_count, 9);
        assert_eq!(capture.open_research.blocked_package_count, 0);
        assert_eq!(capture.open_research.signal_count, 54);
        assert_eq!(capture.open_research.passed_signal_count, 54);
        assert_eq!(capture.open_research.measured_prototype_count, 9);
        assert_eq!(capture.open_research.schema_proposal_count, 9);
        assert_eq!(capture.open_research.performance_data_count, 9);
        assert_eq!(capture.open_research.debug_view_count, 9);
        assert_eq!(capture.open_research.fallback_behavior_count, 9);
        assert_eq!(capture.open_research.golden_integration_plan_count, 9);
        assert!(
            capture
                .open_research
                .package(OpenResearchPackageKind::RustShaderPipeline)
                .is_some_and(|package| package.ready_for_production_work)
        );
        assert!(
            capture
                .open_research
                .package(OpenResearchPackageKind::GroundedAiStory)
                .is_some_and(|package| package.ready_for_production_work)
        );
        assert!(
            capture
                .open_research
                .package(OpenResearchPackageKind::RustNativeMlInference)
                .is_some_and(|package| package.ready_for_production_work)
        );
        assert!(capture.feature_acceptance.passed());
        assert_eq!(capture.feature_acceptance.feature_count, 10);
        assert_eq!(capture.feature_acceptance.accepted_feature_count, 10);
        assert_eq!(capture.feature_acceptance.requirement_count, 60);
        assert_eq!(capture.feature_acceptance.passed_requirement_count, 60);
        assert_eq!(
            capture.feature_acceptance.golden_scene_requirement_count,
            10
        );
        assert_eq!(capture.feature_acceptance.schema_requirement_count, 10);
        assert_eq!(capture.feature_acceptance.performance_requirement_count, 10);
        assert_eq!(capture.feature_acceptance.validation_requirement_count, 10);
        assert_eq!(capture.feature_acceptance.provenance_requirement_count, 10);
        assert_eq!(capture.feature_acceptance.tooling_requirement_count, 10);
        assert!(
            capture
                .feature_acceptance
                .feature(FeatureAcceptanceFeatureKind::DigitalHumanCloseup)
                .is_some_and(|feature| feature.accepted)
        );
        assert!(
            capture
                .feature_acceptance
                .feature(FeatureAcceptanceFeatureKind::AiStory)
                .is_some_and(|feature| feature.accepted)
        );
        assert!(
            capture
                .feature_acceptance
                .feature(FeatureAcceptanceFeatureKind::ToolsValidation)
                .is_some_and(|feature| feature.accepted)
        );
        assert!(capture.team_handoff_matrix.passed());
        assert_eq!(capture.team_handoff_matrix.component_count, 10);
        assert_eq!(capture.team_handoff_matrix.ready_component_count, 10);
        assert_eq!(capture.team_handoff_matrix.blocked_component_count, 0);
        assert_eq!(capture.team_handoff_matrix.research_only_component_count, 0);
        assert_eq!(
            capture
                .team_handoff_matrix
                .schema_compatible_component_count,
            capture.team_handoff_matrix.component_count
        );
        assert_eq!(
            capture
                .team_handoff_matrix
                .runtime_integrated_component_count,
            capture.team_handoff_matrix.component_count
        );
        assert_eq!(
            capture
                .team_handoff_matrix
                .golden_scene_participant_component_count,
            capture.team_handoff_matrix.component_count
        );
        assert_eq!(
            capture
                .team_handoff_matrix
                .performance_approved_component_count,
            capture.team_handoff_matrix.component_count
        );
        assert_eq!(
            capture
                .team_handoff_matrix
                .production_candidate_component_count,
            capture.team_handoff_matrix.component_count
        );
        assert!(
            capture
                .team_handoff_matrix
                .component(TeamHandoffComponentKind::Renderer)
                .is_some_and(|component| {
                    component.compatibility_level == TeamCompatibilityLevel::ProductionCandidate
                })
        );
        assert!(
            capture
                .team_handoff_matrix
                .component(TeamHandoffComponentKind::ToolsValidation)
                .is_some_and(|component| {
                    component.compatibility_level == TeamCompatibilityLevel::ProductionCandidate
                        && component.ready
                })
        );
        let reference_frame_graph = capture
            .frame_graphs
            .iter()
            .find(|frame_graph| !frame_graph.reference_render.is_empty())
            .expect("capture should include a reference validation frame graph");
        assert!(reference_frame_graph.passed());
        assert!(reference_frame_graph.reference_render.passed());
        assert!(
            reference_frame_graph
                .reference_render
                .path_tracing_pass_present
        );
        assert!(
            reference_frame_graph
                .reference_render
                .comparison_pass_present
        );
        assert!(
            reference_frame_graph
                .reference_render
                .radiance_target_present
        );
        assert!(
            reference_frame_graph
                .reference_render
                .metrics_buffer_present
        );
        assert!(capture.ai_story.agent_decisions.len() >= 2);
        assert!(capture.ai_story.accepted_intent_count >= 4);
        assert!(capture.ai_story.rejected_intent_count >= 2);
        assert!(capture.ai_story.agent_memories.len() >= 2);
        assert!(!capture.ai_story.dialogues.is_empty());
        assert!(capture.ai_story.dialogue_grounding.len() >= capture.ai_story.dialogues.len());
        assert!(
            capture
                .ai_story
                .dialogue_grounding
                .iter()
                .all(|grounding| grounding.grounded
                    && grounding.unsupported_claim_count == 0
                    && !grounding.source_events.is_empty())
        );
        assert!(capture.ai_story.conversation_replay.replay_available);
        assert!(capture.ai_story.conversation_replay.passed);
        assert!(capture.ai_story.conversation_replay.grounded_line_count >= 1);
        assert!(
            capture
                .ai_story
                .conversation_replay
                .linked_source_event_count
                >= 1
        );
        assert!(capture.ai_story.conversation_replay.fully_voiced_line_count >= 1);
        assert!(!capture.ai_story.story_events.is_empty());
        assert!(capture.ai_story.faction_events.len() >= 3);
        assert_eq!(capture.ai_story.witness_reports.len(), 1);
        assert_eq!(capture.ai_story.sound_reports.len(), 1);
        assert!(capture.ai_story.rumor_network.viewer_available);
        assert!(capture.ai_story.rumor_network.passed());
        assert!(capture.ai_story.rumor_network.propagation_pass_count >= 1);
        assert!(
            capture
                .ai_story
                .rumor_network
                .propagation_buffer_resource_count
                >= 1
        );
        assert!(capture.ai_story.opportunity_network.viewer_available);
        assert!(capture.ai_story.opportunity_network.passed());
        assert!(capture.ai_story.opportunity_network.index_pass_count >= 1);
        assert!(
            capture
                .ai_story
                .opportunity_network
                .opportunity_table_resource_count
                >= 1
        );
        assert!(capture.ai_story.agent_summaries.iter().any(|summary| {
            summary.agent == 3 && summary.decision_count >= 2 && summary.memory_count >= 2
        }));
        assert!(capture.voice_audio.speech_count >= 1);
        assert_eq!(
            capture.voice_audio.synced_speech_count,
            capture.voice_audio.speech_count
        );
        assert_eq!(capture.voice_audio.unsynced_speech_count, 0);
        assert!(capture.voice_audio.dialogue_count >= 1);
        assert!(capture.voice_audio.voice_sound_count >= 1);
        assert!(capture.voice_audio.physics_sound_count >= 2);
        assert!(capture.voice_audio.material_aware_sound_count >= 2);
        assert_eq!(capture.voice_audio.critical_issue_count(), 0);
        assert!(capture.voice_audio.comfort.passed);
        assert!(capture.voice_audio.comfort.projected_ten_minute_score >= 0.75);
        assert_eq!(
            capture.voice_audio.comfort.line_count,
            capture.voice_audio.speech_count
        );
        assert!(capture.voice_audio.soundscape.layer_count >= 6);
        assert!(capture.voice_audio.soundscape.event_driven_layer_count >= 2);
        assert!(capture.voice_audio.soundscape.validation_passed);
        assert!(capture.voice_audio.soundscape.emergency_layer_present);
        assert!(capture.voice_audio.soundscape.ventilation_layer_present);
        assert!(capture.voice_audio.soundscape.police_scanner_present);
        assert!(
            capture
                .voice_audio
                .face_sync
                .iter()
                .any(|sync| sync.speaker == 3 && sync.synchronized)
        );
        assert!(capture.human_lab.dialogue_count >= 1);
        assert!(capture.human_lab.voice_line_count >= 1);
        assert!(capture.human_lab.speech_performance_count >= 1);
        assert_eq!(
            capture.human_lab.synchronized_face_count,
            capture.human_lab.speech_performance_count
        );
        assert_eq!(capture.human_lab.unsynchronized_face_count, 0);
        assert!(capture.human_lab.bundle_request_count >= 2);
        assert_eq!(capture.human_lab.critical_issue_count(), 0);
        assert!(capture.human_lab.speech_performances.iter().any(|speech| {
            speech.speaker == 3 && speech.synchronized && speech.has_bundle_request
        }));
        assert!(capture.human_lab.dialogues.iter().any(|dialogue| {
            dialogue.speaker == 3
                && dialogue.has_voice_line
                && dialogue.has_speech
                && dialogue.has_facial_animation
                && dialogue.has_bundle_request
        }));
        assert!(capture.cinematic_timeline.beat_count >= 30);
        assert!(capture.cinematic_timeline.track_count >= 8);
        assert_eq!(capture.cinematic_timeline.critical_issue_count(), 0);
        assert!(
            capture
                .cinematic_timeline
                .track(ashfall_tools::CinematicTrackKind::PhysicsPlayback)
                .is_some()
        );
        assert!(
            capture
                .cinematic_timeline
                .track(ashfall_tools::CinematicTrackKind::MaterialState)
                .is_some()
        );
        assert!(
            capture
                .cinematic_timeline
                .track(ashfall_tools::CinematicTrackKind::Voice)
                .is_some()
        );
        assert!(
            capture
                .cinematic_timeline
                .track(ashfall_tools::CinematicTrackKind::FacialPerformance)
                .is_some()
        );
        assert!(
            capture
                .cinematic_timeline
                .track(ashfall_tools::CinematicTrackKind::AiStory)
                .is_some()
        );
        assert!(
            capture
                .cinematic_timeline
                .track(ashfall_tools::CinematicTrackKind::AssetStreaming)
                .is_some()
        );
        assert_eq!(capture.physics_lab.fracture_count, 1);
        assert_eq!(capture.physics_lab.fluid_event_count, 1);
        assert!(capture.physics_lab.gas_event_count >= 1);
        assert!(capture.physics_lab.volume_events.iter().any(|volume| {
            volume.kind == ashfall_tools::PhysicsVolumeKind::Gas
                && volume.medium.as_deref() == Some("steam")
                && volume.density == Some(0.45)
                && volume.visibility_blocking == Some(0.45)
                && volume.hazard_level == Some(0.28)
                && volume.density_cell_count == Some(343)
        }));
        assert_eq!(capture.physics_lab.material_update_count, 2);
        assert_eq!(capture.physics_lab.mesh_replacement_count, 1);
        assert!(capture.physics_lab.physics_sound_count >= 2);
        assert_eq!(capture.physics_lab.critical_issue_count(), 0);
        assert!(capture.physics_lab.simulation_lod.viewer_available);
        assert!(capture.physics_lab.simulation_lod.lod_pass_count >= 1);
        assert!(
            capture
                .physics_lab
                .simulation_lod
                .lod_decision_resource_count
                >= 1
        );
        assert!(capture.physics_lab.fractures.iter().any(|fracture| {
            fracture.entity == 2
                && fracture.material_updated
                && fracture.mesh_replaced
                && fracture.audio_emitted
                && fracture.ai_or_story_reacted
                && fracture.asset_streaming_requested
        }));
        assert_eq!(capture.material_lab.material_update_count, 2);
        assert!(capture.material_lab.cache_request_count >= 4);
        assert_eq!(capture.material_lab.damage_update_count, 1);
        assert_eq!(capture.material_lab.wetness_update_count, 1);
        assert_eq!(capture.material_lab.critical_issue_count(), 0);
        assert!(capture.material_lab.material_updates.iter().any(|update| {
            update.entity == 2
                && update.has_cache_request
                && update.has_render_link
                && update.has_audio_link
                && update.has_physics_cause
        }));
        assert!(capture.material_lab.material_updates.iter().any(|update| {
            update.entity == 6
                && update.has_cache_request
                && update.has_render_link
                && update.has_audio_link
                && update.has_physics_cause
        }));
        assert!(capture.city_generation.validation_passed);
        assert_eq!(capture.city_generation.district_count, 3);
        assert_eq!(capture.city_generation.streaming_cell_count, 3);
        assert_eq!(
            capture.city_generation.city_cell_package_count,
            capture.city_generation.streaming_cell_count
        );
        assert_eq!(
            capture.city_generation.city_cell_manifest_count,
            capture.city_generation.city_cell_package_count
        );
        assert_eq!(
            capture
                .city_generation
                .city_cell_manifest_validation_error_count,
            0
        );
        assert_eq!(
            capture.city_generation.packaged_material_package_count,
            capture.city_generation.material_cache_dependency_count
        );
        assert_eq!(capture.city_generation.infrastructure.len(), 6);
        assert!(capture.city_generation.story_seed_count >= 3);
        assert!(capture.city_generation.unique_streaming_dependency_count >= 6);
        assert!(capture.city_generation.highest_pressure_cell().is_some_and(
            |cell| cell.entity_count > 0
                && cell.estimated_streaming_bytes > 0
                && cell.package_manifest_present
                && cell.package_manifest_validation_passed
        ));
        assert_eq!(capture.city_streaming.cell_count, 3);
        assert!(capture.city_streaming.loaded_cell_count >= 3);
        assert_eq!(capture.city_streaming.hero_loaded_count, 1);
        assert!(capture.city_streaming.render_high_detail_count >= 1);
        assert!(capture.city_streaming.gameplay_loaded_count >= 1);
        assert!(
            capture.city_streaming.unique_requested_asset_count
                >= 6 + capture.city_generation.city_cell_package_count
        );
        assert!(capture.city_streaming.requested_streaming_bytes > 64 * 1024 * 1024);
        assert_eq!(capture.city_streaming.critical_issue_count(), 0);
        assert!(
            capture
                .city_streaming
                .highest_priority_cell()
                .is_some_and(|cell| {
                    cell.state == ashfall_worldgen::CityCellStreamingState::HeroLoaded
                        && cell.package_requested
                        && ashfall_worldgen::is_worldgen_city_cell_asset(cell.package_asset)
                        && cell.story_focus
                        && cell.active_event
                })
        );
        assert!(capture.city_infrastructure.passed());
        assert_eq!(
            capture.city_infrastructure.system,
            Some(ashfall_worldgen::InfrastructureSystem::Power)
        );
        assert!(capture.city_infrastructure.affected_cell_count >= 2);
        assert!(capture.city_infrastructure.affected_node_count >= 2);
        assert!(capture.city_infrastructure.affected_light_count > 0);
        assert!(capture.city_infrastructure.affected_camera_count > 0);
        assert!(capture.city_infrastructure.affected_npc_count > 0);
        assert!(capture.city_infrastructure.affected_audio_zone_count > 0);
        assert!(
            capture
                .city_infrastructure
                .affected_navigation_location_count
                > 0
        );
        assert!(capture.city_infrastructure.story_seed_count > 0);
        assert_eq!(capture.city_infrastructure.critical_issue_count(), 0);
        assert!(
            capture
                .city_infrastructure
                .highest_impact_cell()
                .is_some_and(|cell| {
                    cell.light_count > 0
                        && cell.camera_count > 0
                        && cell.npc_count > 0
                        && cell.navigation_location_count > 0
                })
        );
        assert_eq!(capture.city_persistence.cell_count, 1);
        assert!(capture.city_persistence.save_required);
        assert!(capture.city_persistence.persistent_event_count >= 15);
        assert_eq!(capture.city_persistence.material_override_count, 2);
        assert!(capture.city_persistence.damaged_entity_count >= 1);
        assert_eq!(capture.city_persistence.mesh_replacement_count, 1);
        assert!(capture.city_persistence.infrastructure_delta_count >= 1);
        assert!(capture.city_persistence.faction_delta_count >= 2);
        assert!(capture.city_persistence.story_thread_ref_count >= 1);
        assert!(capture.city_persistence.ai_memory_ref_count >= 1);
        assert_eq!(capture.city_persistence.critical_issue_count(), 0);
        assert!(
            capture
                .city_persistence
                .most_changed_cell()
                .is_some_and(|cell| {
                    cell.save_priority >= ashfall_tools::CityPersistenceSavePriority::High
                        && cell.change_count >= 8
                })
        );
        assert_eq!(capture.city_navigation.cell_count, 3);
        assert_eq!(capture.city_navigation.changed_cell_count, 1);
        assert!(capture.city_navigation.total_node_count >= 12);
        assert!(capture.city_navigation.total_edge_count >= 9);
        assert!(capture.city_navigation.route_update_count > 0);
        assert_eq!(capture.city_navigation.blocked_route_count, 0);
        assert!(capture.city_navigation.dangerous_route_count > 0);
        assert!(capture.city_navigation.restricted_route_count > 0);
        assert_eq!(capture.city_navigation.partially_unreachable_cell_count, 0);
        assert_eq!(capture.city_navigation.critical_issue_count(), 0);
        assert!(
            capture
                .city_navigation
                .most_changed_cell()
                .is_some_and(|cell| {
                    cell.route_update_count > 0
                        && cell.danger_field_count > 0
                        && cell.reachable_important_location_count == cell.important_location_count
                })
        );
        assert!(
            capture
                .notes
                .iter()
                .any(|note| note.contains("registered asset"))
        );
    }
}

use ashfall_ai::{
    AiCharactersConfig, AiCharactersModule, StoryDirectorConfig, StoryDirectorModule,
};
use ashfall_core::core::{QualityTier, Vec3};
use ashfall_core::runtime::{EngineRuntime, FrameReport, ReplayCaptureFilter};
use ashfall_core::world::CommandError;
use ashfall_human::HumanGeneratorModule;
use ashfall_materials::ProceduralMaterialsModule;
use ashfall_physics::ConsequencePhysicsModule;
use ashfall_rendering::RenderingModule;
use ashfall_tools::{
    AutomatedValidationGate, ReplayDebugCapture, ToolValidationReport,
    ai_acceptance_report_from_capture, ai_pipeline_acceptance_report_from_capture,
    architecture_contract_report_from_capture, asset_package_acceptance_report_from_capture,
    build_automation_report_from_capture, build_runtime_debug_capture,
    city_generation_inspector_report_from_template, city_infrastructure_inspector_report_from_map,
    city_navigation_inspector_report_from_events, city_persistence_inspector_report_from_events,
    city_streaming_planner_report_from_plan, city_world_acceptance_report_from_capture,
    city_world_pipeline_acceptance_report_from_capture, conformance_report_from_capture,
    editor_workspace_report_from_capture, engine_core_acceptance_report_from_capture,
    executive_sanity_audit_report_from_capture, feature_acceptance_report_from_capture,
    frame_graph_debug_report_from_frame, golden_scene_report_from_capture,
    gpu_services_acceptance_report_from_capture, human_acceptance_report_from_capture,
    human_pipeline_acceptance_report_from_capture, interface_schema_coverage_report_from_capture,
    material_acceptance_report_from_capture, material_pipeline_acceptance_report_from_capture,
    open_research_report_from_capture, performance_acceptance_report_from_capture,
    photoreal_rendering_acceptance_report_from_capture, physics_acceptance_report_from_capture,
    physics_pipeline_acceptance_report_from_capture, product_quality_bar_report_from_capture,
    production_milestone_report_from_capture, production_risk_register_report_from_capture,
    reference_anchor_sanity_report_from_capture, reference_comparison_report_from_frame_graphs,
    reference_validation_acceptance_report_from_capture, renderer_acceptance_report_from_capture,
    roadmap_acceptance_report_from_capture, runtime_dependency_policy_report_from_capture,
    safety_provenance_report_from_capture, stress_scene_report_from_capture,
    team_handoff_matrix_report_from_capture, tools_acceptance_report_from_capture,
    truth_cache_boundary_report_from_capture, virtual_geometry_acceptance_report_from_capture,
    virtual_geometry_pipeline_acceptance_report_from_capture, voice_acceptance_report_from_capture,
    voice_pipeline_acceptance_report_from_capture,
};
use ashfall_voice::{AudioMixerModule, VoiceAudioModule};
use ashfall_worldgen::{
    CityRendererStreamingFeedback, CityStreamingRequest, CyberpunkWorldGenerationModule,
    InfrastructureDamageRequest, PLAYER_ID, WorldGenerationRequest, WorldTemplate,
    alley_scripted_glass_break_command, build_alley_scene_seed, build_world_template_asset_records,
    generate_world_template, infrastructure_consequence_map_for_damage, plan_city_cell_streaming,
};

pub fn build_alley_runtime() -> Result<EngineRuntime, CommandError> {
    let seed = build_alley_scene_seed();
    let world = seed.build_world_state()?;
    let ai_seed = seed.ai;
    let mut runtime = EngineRuntime::new(world);
    seed.register_assets(runtime.assets_mut());
    let city_template = generate_world_template(&alley_city_generation_request());
    for asset in build_world_template_asset_records(&city_template) {
        runtime.assets_mut().register_known(asset);
    }
    runtime.register_module(CyberpunkWorldGenerationModule::default());
    runtime.register_module(ConsequencePhysicsModule::default());
    runtime.register_module(ProceduralMaterialsModule::default());
    runtime.register_module(AiCharactersModule::new(AiCharactersConfig {
        default_authority_faction: ai_seed.security_faction,
        fallback_navigation_location: ai_seed.story_location,
    }));
    runtime.register_module(StoryDirectorModule::new(StoryDirectorConfig {
        security_faction: ai_seed.security_faction,
        opposition_faction: ai_seed.opposition_faction,
        story_location: ai_seed.story_location,
    }));
    runtime.register_module(VoiceAudioModule::default());
    runtime.register_module(AudioMixerModule::default());
    runtime.register_module(HumanGeneratorModule::default());
    runtime.register_module(RenderingModule::default());
    runtime.init_modules();

    Ok(runtime)
}

pub fn queue_glass_break_attempt(runtime: &mut EngineRuntime) {
    runtime.queue_command(alley_scripted_glass_break_command());
}

fn capture_alley_reference_frame() -> FrameReport {
    let mut runtime = build_alley_runtime().expect("reference alley runtime should build");
    let previous_quality = runtime.set_module_quality(10, QualityTier::ReferenceOfflineValidation);
    debug_assert_eq!(previous_quality, Some(QualityTier::NormalRuntime));
    queue_glass_break_attempt(&mut runtime);
    runtime.step().expect("reference alley frame should run")
}

pub fn capture_alley_debug_report(
    runtime: &EngineRuntime,
    frame: &FrameReport,
) -> ReplayDebugCapture {
    let mut capture = build_runtime_debug_capture(
        runtime.replay_log(),
        &ReplayCaptureFilter::default().with_min_event_count(1),
        std::slice::from_ref(frame),
        runtime.assets(),
        runtime.schemas(),
        &runtime.module_registry_report(),
        &[],
    );
    let city_template = generate_world_template(&alley_city_generation_request());
    capture.city_generation = city_generation_inspector_report_from_template(&city_template);
    let city_streaming_plan = plan_city_cell_streaming(
        &city_template,
        &alley_city_streaming_request(&city_template, frame),
    );
    capture.city_streaming = city_streaming_planner_report_from_plan(&city_streaming_plan);
    let city_infrastructure_map = infrastructure_consequence_map_for_damage(
        &city_template,
        &alley_city_infrastructure_damage_request(&city_template),
    );
    capture.city_infrastructure =
        city_infrastructure_inspector_report_from_map(&city_infrastructure_map);
    capture.city_persistence =
        city_persistence_inspector_report_from_events(&city_template, &frame.events);
    capture.city_navigation =
        city_navigation_inspector_report_from_events(&city_template, &frame.events);
    let reference_frame = capture_alley_reference_frame();
    capture
        .frame_graphs
        .push(frame_graph_debug_report_from_frame(&reference_frame));
    capture.reference_comparison =
        reference_comparison_report_from_frame_graphs(&capture.frame_graphs);
    capture.notes.push(format!(
        "inspected {} generated city district(s)",
        capture.city_generation.district_count
    ));
    capture.notes.push(format!(
        "planned {} city streaming cell(s)",
        capture.city_streaming.cell_count
    ));
    capture.notes.push(format!(
        "inspected {} infrastructure impacted city cell(s)",
        capture.city_infrastructure.affected_cell_count
    ));
    capture.notes.push(format!(
        "inspected {} persistent city cell(s)",
        capture.city_persistence.cell_count
    ));
    capture.notes.push(format!(
        "inspected {} generated navigation cell(s)",
        capture.city_navigation.cell_count
    ));
    capture.notes.push(format!(
        "captured {} reference comparison frame(s)",
        capture.reference_comparison.reference_frame_count
    ));
    capture.stress_scenes = stress_scene_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} required stress scene(s)",
        capture.stress_scenes.scene_count
    ));
    capture.golden_scenes = golden_scene_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} v6 golden scene(s)",
        capture.golden_scenes.scene_count
    ));
    capture.editor_workspace = editor_workspace_report_from_capture(&capture);
    capture.notes.push(format!(
        "assembled editor workspace with {} of {} panel(s) available",
        capture.editor_workspace.available_panel_count, capture.editor_workspace.panel_count
    ));
    capture.production_milestones = production_milestone_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} production milestone stage(s)",
        capture.production_milestones.stage_count
    ));
    capture.production_risks = production_risk_register_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} production risk(s)",
        capture.production_risks.risk_count
    ));
    capture.architecture_contract = architecture_contract_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} architecture contract rule(s)",
        capture.architecture_contract.rule_count
    ));
    capture.runtime_policy = runtime_dependency_policy_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} runtime dependency policy check(s)",
        capture.runtime_policy.check_count
    ));
    capture.reference_anchors = reference_anchor_sanity_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} reference-anchor sanity check(s)",
        capture.reference_anchors.anchor_count
    ));
    capture.interface_schema_coverage = interface_schema_coverage_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} interface schema section(s)",
        capture.interface_schema_coverage.section_count
    ));
    capture.executive_sanity = executive_sanity_audit_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} executive sanity audit check(s)",
        capture.executive_sanity.check_count
    ));
    capture.truth_cache_boundary = truth_cache_boundary_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} truth/cache lineage(s)",
        capture.truth_cache_boundary.lineage_count
    ));
    capture.build_automation = build_automation_report_from_capture(&capture);
    capture.safety_provenance = safety_provenance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} safety/provenance domain(s)",
        capture.safety_provenance.domain_count
    ));
    capture.asset_package_acceptance = asset_package_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} asset package acceptance gate(s)",
        capture.asset_package_acceptance.gate_count
    ));
    capture.reference_validation_acceptance =
        reference_validation_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} reference validation acceptance gate(s)",
        capture.reference_validation_acceptance.gate_count
    ));
    capture.renderer_acceptance = renderer_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} renderer acceptance gate(s)",
        capture.renderer_acceptance.gate_count
    ));
    capture.virtual_geometry_acceptance = virtual_geometry_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} virtual geometry acceptance gate(s)",
        capture.virtual_geometry_acceptance.gate_count
    ));
    capture.material_acceptance = material_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} material acceptance gate(s)",
        capture.material_acceptance.gate_count
    ));
    capture.physics_acceptance = physics_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} physical consequence acceptance gate(s)",
        capture.physics_acceptance.gate_count
    ));
    capture.human_acceptance = human_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} digital human acceptance gate(s)",
        capture.human_acceptance.gate_count
    ));
    capture.ai_acceptance = ai_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} AI/story acceptance gate(s)",
        capture.ai_acceptance.gate_count
    ));
    capture.voice_acceptance = voice_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} voice/audio acceptance gate(s)",
        capture.voice_acceptance.gate_count
    ));
    capture.city_world_acceptance = city_world_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} city/world acceptance gate(s)",
        capture.city_world_acceptance.gate_count
    ));
    capture.tools_acceptance = tools_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} editor/tools acceptance gate(s)",
        capture.tools_acceptance.gate_count
    ));
    capture.performance_acceptance = performance_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} performance acceptance gate(s)",
        capture.performance_acceptance.gate_count
    ));
    capture.open_research = open_research_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} open research work package(s)",
        capture.open_research.package_count
    ));
    capture.roadmap_acceptance = roadmap_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} roadmap phase(s)",
        capture.roadmap_acceptance.phase_count
    ));
    capture.product_quality_bar = product_quality_bar_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} product quality bar check(s)",
        capture.product_quality_bar.check_count
    ));
    capture.engine_core_acceptance = engine_core_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} engine core acceptance check(s)",
        capture.engine_core_acceptance.check_count
    ));
    capture.gpu_services_acceptance = gpu_services_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} GPU Services acceptance gate(s)",
        capture.gpu_services_acceptance.gate_count
    ));
    capture.virtual_geometry_pipeline_acceptance =
        virtual_geometry_pipeline_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} virtual geometry pipeline acceptance gate(s)",
        capture.virtual_geometry_pipeline_acceptance.gate_count
    ));
    capture.material_pipeline_acceptance =
        material_pipeline_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} material pipeline acceptance gate(s)",
        capture.material_pipeline_acceptance.gate_count
    ));
    capture.physics_pipeline_acceptance = physics_pipeline_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} physics pipeline acceptance gate(s)",
        capture.physics_pipeline_acceptance.gate_count
    ));
    capture.human_pipeline_acceptance = human_pipeline_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} human pipeline acceptance gate(s)",
        capture.human_pipeline_acceptance.gate_count
    ));
    capture.ai_pipeline_acceptance = ai_pipeline_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} AI pipeline acceptance gate(s)",
        capture.ai_pipeline_acceptance.gate_count
    ));
    capture.voice_pipeline_acceptance = voice_pipeline_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} voice/audio pipeline acceptance gate(s)",
        capture.voice_pipeline_acceptance.gate_count
    ));
    capture.city_world_pipeline_acceptance =
        city_world_pipeline_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} city/world pipeline acceptance gate(s)",
        capture.city_world_pipeline_acceptance.gate_count
    ));
    capture.photoreal_rendering_acceptance =
        photoreal_rendering_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} photoreal rendering acceptance gate(s)",
        capture.photoreal_rendering_acceptance.gate_count
    ));
    capture.conformance = conformance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated conformance fixture {} with {} issue(s)",
        capture.conformance.fixture_id, capture.conformance.issue_count
    ));
    capture.build_automation = build_automation_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} build automation stage(s)",
        capture.build_automation.stage_count
    ));
    capture.feature_acceptance = feature_acceptance_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} v8 feature acceptance check(s)",
        capture.feature_acceptance.feature_count
    ));
    capture.team_handoff_matrix = team_handoff_matrix_report_from_capture(&capture);
    capture.notes.push(format!(
        "evaluated {} v7 team handoff component(s)",
        capture.team_handoff_matrix.component_count
    ));
    capture
}

pub fn validate_alley_debug_capture(capture: &ReplayDebugCapture) -> ToolValidationReport {
    AutomatedValidationGate {
        require_asset_inventory: true,
        require_asset_validation: true,
        require_asset_browser: true,
        require_schema_inspector: true,
        require_streaming_debugger: true,
        require_performance_heatmap: true,
        require_component_budgets: true,
        require_failure_modes: true,
        require_editor_workspace: true,
        require_frame_graph: true,
        require_camera_color_pipeline: true,
        require_temporal_stability: true,
        require_human_rendering: true,
        require_reference_comparison: true,
        require_reference_validation_acceptance: true,
        require_renderer_acceptance: true,
        require_virtual_geometry_acceptance: true,
        require_virtual_geometry_pipeline_acceptance: true,
        require_material_acceptance: true,
        require_material_pipeline_acceptance: true,
        require_physics_acceptance: true,
        require_physics_pipeline_acceptance: true,
        require_human_acceptance: true,
        require_human_pipeline_acceptance: true,
        require_ai_acceptance: true,
        require_ai_pipeline_acceptance: true,
        require_voice_acceptance: true,
        require_voice_pipeline_acceptance: true,
        require_city_world_acceptance: true,
        require_city_world_pipeline_acceptance: true,
        require_tools_acceptance: true,
        require_performance_acceptance: true,
        require_open_research: true,
        require_roadmap_acceptance: true,
        require_product_quality_bar: true,
        require_engine_core_acceptance: true,
        require_gpu_services_acceptance: true,
        require_photoreal_rendering_acceptance: true,
        require_r_and_d_gates: true,
        require_production_milestones: true,
        require_production_risks: true,
        require_architecture_contract: true,
        require_stress_scenes: true,
        require_golden_scenes: true,
        require_runtime_policy: true,
        require_reference_anchors: true,
        require_interface_schema_coverage: true,
        require_executive_sanity: true,
        require_conformance: true,
        require_truth_cache_boundary: true,
        require_build_automation: true,
        require_safety_provenance: true,
        require_asset_package_acceptance: true,
        require_feature_acceptance: true,
        require_ai_story: true,
        require_voice_audio: true,
        require_human_lab: true,
        require_cinematic_timeline: true,
        require_physics_lab: true,
        require_material_lab: true,
        require_city_generation: true,
        require_city_streaming: true,
        require_city_infrastructure: true,
        require_city_persistence: true,
        require_city_navigation: true,
        require_replay_frames: true,
        require_replay_debugger: true,
        require_replay_inspection: true,
        ..AutomatedValidationGate::default()
    }
    .evaluate(capture)
}

pub(crate) fn alley_city_generation_request() -> WorldGenerationRequest {
    WorldGenerationRequest {
        request_id: 7,
        seed: 12_345,
        city_profile: "humid corporate-fringe arcology".to_string(),
        district_requests: vec![
            "rain alley slum".to_string(),
            "black market".to_string(),
            "clinic district".to_string(),
        ],
        target_platform_budget: 64,
        story_requirements: vec![
            "physical hooks".to_string(),
            "faction pressure".to_string(),
            "consequence-aware city streaming".to_string(),
        ],
        material_palette: 900,
        faction_templates: vec![
            "security group".to_string(),
            "hacker collective".to_string(),
            "clinic network".to_string(),
        ],
        validation_level: QualityTier::NormalRuntime,
    }
}

fn alley_city_infrastructure_damage_request(
    template: &WorldTemplate,
) -> InfrastructureDamageRequest {
    InfrastructureDamageRequest {
        damaged_node: template
            .infrastructure
            .power
            .nodes
            .first()
            .map(|node| node.id)
            .unwrap_or_default(),
        severity: 0.92,
        source_entity: Some(PLAYER_ID),
    }
}

fn alley_city_streaming_request(
    template: &WorldTemplate,
    frame: &FrameReport,
) -> CityStreamingRequest {
    let story_focus_locations = template
        .chunks
        .first()
        .map(|chunk| chunk.local_navigation.important_locations.clone())
        .unwrap_or_default();
    let predicted_route = template
        .chunks
        .last()
        .map(|chunk| chunk.local_navigation.important_locations.clone())
        .unwrap_or_default();
    let renderer_visible_chunks = template
        .chunks
        .iter()
        .take(2)
        .map(|chunk| chunk.chunk_id)
        .collect::<Vec<_>>();
    let renderer_feedback = template
        .chunks
        .iter()
        .take(2)
        .enumerate()
        .map(|(index, chunk)| {
            CityRendererStreamingFeedback::new(chunk.chunk_id)
                .with_visibility(0.62 - index as f32 * 0.18, 160 - index * 48)
                .with_streaming_pressure(
                    (chunk.streaming_dependencies.len() as f32 / 8.0).clamp(0.0, 1.0),
                    0.72 - index as f32 * 0.14,
                )
        })
        .collect::<Vec<_>>();
    let active_event_locations = frame
        .events
        .iter()
        .filter(|event| {
            event.kind.physical_type().is_some()
                || matches!(
                    event.kind,
                    ashfall_core::world::WorldEventKind::SecurityAlertRaised { .. }
                        | ashfall_core::world::WorldEventKind::StoryEventEmitted { .. }
                )
        })
        .map(|event| event.location_meters)
        .collect::<Vec<_>>();

    CityStreamingRequest {
        player_position: Vec3::ZERO,
        camera_forward: Vec3::new(1.0, 0.0, 0.0),
        player_velocity: Vec3::new(4.0, 0.0, 0.0),
        story_focus_locations,
        active_event_locations,
        renderer_visible_chunks,
        renderer_feedback,
        predicted_route,
        ..CityStreamingRequest::default()
    }
}

use ashfall::scenarios::{
    build_alley_runtime, capture_alley_debug_report, queue_glass_break_attempt,
    validate_alley_debug_capture,
};
use ashfall::world::WorldEventKind;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|arg| arg == "--capture-v19-golden" || arg == "--capture-golden-v19")
    {
        let output_dir = output_dir_arg(&args)
            .unwrap_or_else(|| std::path::PathBuf::from("target/ashfall_v19_golden"));
        let paths =
            ashfall::beauty_scene_v19_capture::capture_beauty_v19_golden_scenes(&output_dir)?;
        println!(
            "Captured {} V19 golden scene image(s) to {}",
            paths.len(),
            output_dir.display()
        );
        for path in paths {
            println!("{}", path.display());
        }
        return Ok(());
    }

    if args
        .iter()
        .any(|arg| arg == "--headless" || arg == "--demo-log")
    {
        return run_headless_demo(args.iter().any(|arg| {
            arg == "--tools-capture" || arg == "--tool-report" || arg == "--debug-capture"
        }));
    }

    ashfall::windowed::run_windowed_game()
}

fn output_dir_arg(args: &[String]) -> Option<std::path::PathBuf> {
    args.iter()
        .position(|arg| arg == "--output-dir")
        .and_then(|index| args.get(index + 1))
        .map(std::path::PathBuf::from)
        .or_else(|| {
            args.iter().find_map(|arg| {
                arg.strip_prefix("--output-dir=")
                    .map(std::path::PathBuf::from)
            })
        })
}

fn run_headless_demo(print_tools_capture: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = build_alley_runtime()?;
    queue_glass_break_attempt(&mut runtime);
    let report = runtime.step()?;
    let render_frame = runtime.render_frame(1.0 / 144.0);
    let module_registry = runtime.module_registry_report();

    println!("Ashfall Rust runtime booted.");
    println!("Frame {} at tick {}", report.frame_id, report.sim_time.tick);
    println!(
        "Render frame {} interpolating sim frame {} at alpha {:.2}",
        render_frame.render_frame_id, render_frame.sim_frame_id, render_frame.interpolation_alpha
    );
    println!("GPU backend: {:?}", report.gpu_backend);
    println!("GPU passes scheduled: {}", report.gpu_passes.len());
    println!("GPU barriers inserted: {}", report.gpu_barriers.len());
    println!(
        "GPU graph validation passed: {} ({} diagnostics)",
        report.gpu_validation.passed,
        report.gpu_validation.issues.len()
    );
    println!(
        "GPU transient memory plan: {} bytes across {} alias groups ({} bytes saved)",
        report.gpu_memory_plan.total_transient_bytes,
        report.gpu_memory_plan.alias_group_count,
        report.gpu_memory_plan.aliased_bytes_saved
    );
    println!(
        "GPU declared resources: {} ({} owned usages)",
        report.gpu_declared_resources.len(),
        report.gpu_registered_resource_usage.len()
    );
    println!(
        "GPU descriptors: {} bindings, {} bindless, {} descriptor sets",
        report.gpu_descriptor_report.bindings.len(),
        report.gpu_descriptor_report.bindless_binding_count,
        report.gpu_descriptor_report.descriptor_set_count
    );
    println!(
        "GPU pipelines: {} known, {} cache hits, {} unknown",
        report.gpu_pipeline_report.pipelines.len(),
        report.gpu_pipeline_report.cache_hit_count,
        report.gpu_pipeline_report.unknown_pipeline_count
    );
    println!(
        "GPU estimated frame time: {:.2} ms",
        report.gpu_total_milliseconds
    );
    println!(
        "GPU execution schedule: {:.2} ms wall, {:.2} ms overlapped, {} cross-queue waits",
        report.gpu_execution_plan.estimated_wall_milliseconds,
        report.gpu_execution_plan.overlapped_work_milliseconds,
        report.gpu_execution_plan.cross_queue_wait_count
    );
    println!(
        "Profiler totals: {:.2} ms CPU, {:.2} ms module GPU, {} bytes module memory",
        report.profiler.total_module_cpu_milliseconds,
        report.profiler.total_module_gpu_milliseconds,
        report.profiler.total_module_memory_bytes
    );
    println!(
        "Budget pressure: {} signals, {} critical, {} quality directives",
        report.budget_plan.total_pressure_count,
        report.budget_plan.critical_pressure_count,
        report.budget_plan.demotions().count()
    );
    println!("Assets registered: {}", runtime.assets().len());
    println!(
        "Assets streamed this frame: {} ({} bytes, {} pending)",
        report.asset_streaming.completions.len(),
        report.asset_streaming.bytes_streamed_this_frame,
        report.asset_streaming.pending_assets
    );
    println!("Schemas registered: {}", runtime.schemas().len());
    println!(
        "Schema compatibility passed: {} ({} requirements)",
        report.schema_compatibility.passed, report.schema_compatibility.checked_requirements
    );
    println!(
        "Modules registered: {} static modules (dynamic ABI v{})",
        module_registry.static_module_count, module_registry.dynamic_module_api_version
    );
    let replay_summaries = runtime.replay_log().summaries();
    println!("Replay frames captured: {}", replay_summaries.len());
    if let Some(summary) = replay_summaries.first() {
        println!(
            "Replay frame {} captured {} queued commands and {} events",
            summary.frame_id, summary.queued_command_count, summary.event_count
        );
    }
    println!("Module reports: {}", report.module_reports.len());
    let inspection = report.inspect();
    println!(
        "Inspection passed: {} ({} diagnostics, {} GPU-owned bytes, {} pipelines, {:.2} ms GPU wall)",
        inspection.passed,
        inspection.issues.len(),
        inspection.total_gpu_resource_bytes,
        inspection.total_gpu_pipeline_count,
        inspection.gpu_scheduled_wall_milliseconds
    );

    if print_tools_capture {
        let capture = capture_alley_debug_report(&runtime, &report);
        let tools_validation = validate_alley_debug_capture(&capture);
        println!(
            "Tools capture passed: {} ({} diagnostics)",
            tools_validation.passed,
            tools_validation.issues.len()
        );
        println!(
            "Tools capture: {} replay frame(s), {} profiler sample(s), {} registered asset(s), {} frame graph(s)",
            capture.replay.len(),
            capture.profiler.samples.len(),
            capture.asset_inventory.total_assets,
            capture.frame_graphs.len()
        );
        println!(
            "Tools replay/debug: {} frame(s), {} command(s), {} event(s), {} kind(s), {} source link(s), {} mode(s), {} issue(s)",
            capture.replay_debugger.frame_count,
            capture.replay_debugger.command_count,
            capture.replay_debugger.event_count,
            capture.replay_debugger.unique_event_kind_count,
            capture.replay_debugger.source_link_count,
            capture
                .replay_debugger
                .modes
                .iter()
                .filter(|mode| mode.available)
                .count(),
            capture.replay_debugger.issues.len()
        );
        if let Some(kind) = capture.replay_debugger.event_kinds.first() {
            println!(
                "Tools replay top event: {} x{} across {} frame(s)",
                kind.kind_label, kind.count, kind.frame_count
            );
        }
        if let Some(frame) = &capture.replay_inspection.selected_frame {
            println!(
                "Tools replay inspection: frame {} tick {} shows {} of {} event(s), {} export(s), {} issue(s)",
                frame.frame,
                frame.tick,
                frame.visible_event_count,
                frame.event_count,
                capture.replay_inspection.exports.len(),
                capture.replay_inspection.issues.len()
            );
        }
        if let Some(event) = capture.replay_inspection.visible_events.first() {
            println!(
                "Tools replay selected event: {} {:?} on frame {}",
                event.kind_label, event.category, event.frame
            );
        }
        println!(
            "Tools asset inventory: {} resident, {} unloaded, {} failed, {} missing dependency edges",
            capture.asset_inventory.resident_assets,
            capture.asset_inventory.unloaded_assets,
            capture.asset_inventory.failed_assets,
            capture.asset_inventory.missing_dependency_edges.len()
        );
        println!(
            "Tools asset browser: {} package(s), {} runtime asset(s), {} kind(s), {} dependency edge(s), {} schema(s), {} issue(s)",
            capture.asset_browser.package_count,
            capture.asset_browser.runtime_asset_count,
            capture.asset_browser.by_kind.len(),
            capture.asset_browser.dependency_edge_count,
            capture.asset_browser.schema_count,
            capture.asset_browser.issues.len()
        );
        if let Some(kind) = capture
            .asset_browser
            .by_kind
            .iter()
            .max_by_key(|kind| kind.known_bytes)
        {
            println!(
                "Tools asset browser top kind: {} has {} package(s), {} runtime asset(s), {} known bytes",
                kind.kind_label, kind.package_count, kind.runtime_asset_count, kind.known_bytes
            );
        }
        println!(
            "Tools schema inspector: {} registered, {} requirement(s), {} package schema(s), {} migration(s), {} issue(s), passed {}",
            capture.schema_inspector.registered_schema_count,
            capture.schema_inspector.module_requirement_count,
            capture.schema_inspector.package_schema_count,
            capture.schema_inspector.migration_step_count,
            capture.schema_inspector.issues.len(),
            capture.schema_inspector.passed()
        );
        if let Some(schema) = capture.schema_inspector.registered_schemas.first() {
            println!(
                "Tools schema first: {}@v{} owner {} required by {} module(s)",
                schema.schema_name, schema.version, schema.owner, schema.required_by_count
            );
        }
        println!(
            "Tools streaming: {} frame(s), {} requester(s), {} completed asset(s), {} request event(s), {} resident event(s), {} bytes streamed, {} issue(s)",
            capture.streaming_debugger.frame_count,
            capture.streaming_debugger.requester_count,
            capture.streaming_debugger.total_completed_assets,
            capture.streaming_debugger.request_event_count,
            capture.streaming_debugger.resident_event_count,
            capture.streaming_debugger.total_bytes_streamed,
            capture.streaming_debugger.issues.len()
        );
        if let Some(frame) = capture.streaming_debugger.frames.first() {
            println!(
                "Tools streaming frame {}: {:?}, {} resident, {} pending, {} bytes streamed",
                frame.frame,
                frame.pressure,
                frame.resident_assets,
                frame.pending_assets,
                frame.bytes_streamed_this_frame
            );
        }
        println!(
            "Tools performance heatmap: {} frame(s), {} module(s), {} CPU us, {} GPU us, {} streaming bytes, {} pressure signal(s), {} issue(s)",
            capture.performance_heatmap.frame_count,
            capture.performance_heatmap.module_count,
            capture.performance_heatmap.total_cpu_time_us,
            capture.performance_heatmap.total_module_gpu_time_us,
            capture.performance_heatmap.total_streaming_bytes,
            capture.performance_heatmap.total_budget_pressure_count,
            capture.performance_heatmap.issues.len()
        );
        if let Some(hotspot) = capture.performance_heatmap.hotspots.first() {
            println!(
                "Tools performance hotspot: {:?} {:?} {} score {:.2}",
                hotspot.heat, hotspot.dimension, hotspot.label, hotspot.score
            );
        }
        println!(
            "Tools component budgets: {} component(s), {} complete, {} quality tier link(s), {} failure behavior(s), {} debug counter(s), {} over budget, {} issue(s)",
            capture.component_budgets.component_count,
            capture.component_budgets.complete_count,
            capture.component_budgets.quality_tier_count,
            capture.component_budgets.failure_behavior_count,
            capture.component_budgets.debug_counter_count,
            capture.component_budgets.over_budget_component_count,
            capture.component_budgets.issue_count
        );
        println!(
            "Tools failure modes: {} entry(s), {} declared, {} active, {} budget response(s), {} streaming response(s), {} validation response(s), {} GPU fallback(s), {} warning(s), {} error(s)",
            capture.failure_modes.entry_count,
            capture.failure_modes.declared_policy_count,
            capture.failure_modes.active_trigger_count,
            capture.failure_modes.budget_response_count,
            capture.failure_modes.streaming_response_count,
            capture.failure_modes.validation_response_count,
            capture.failure_modes.gpu_fallback_count,
            capture.failure_modes.warning_count,
            capture.failure_modes.error_count
        );
        println!(
            "Tools performance acceptance: {}/{} gate(s), budgets {}, profiler {}, tiers {}, degradation {}, GPU {}, memory {}, components {}/{}, samples {}, heatmap {}/{}, quality tiers {}, golden {}, policies {}, active responses {}, GPU passes {}, resources {}, memory {:.2} MiB, streaming {:.2} MiB, categories {}/{}, hardware {}/{}, ladder {}/{}, truth {}/{}, memory actions {}/{}, streaming counters {}/{}, shaders {}, cache {}, {} issue(s)",
            capture.performance_acceptance.passed_gate_count,
            capture.performance_acceptance.gate_count,
            capture.performance_acceptance.budget_signal_count,
            capture
                .performance_acceptance
                .profiler_ownership_signal_count,
            capture.performance_acceptance.quality_tier_signal_count,
            capture.performance_acceptance.degradation_signal_count,
            capture.performance_acceptance.gpu_graph_signal_count,
            capture.performance_acceptance.memory_pressure_signal_count,
            capture.performance_acceptance.complete_budget_count,
            capture.performance_acceptance.component_budget_count,
            capture.performance_acceptance.profiler_sample_count,
            capture.performance_acceptance.heatmap_frame_count,
            capture.performance_acceptance.heatmap_module_count,
            capture.performance_acceptance.supported_quality_tier_count,
            capture.performance_acceptance.golden_scene_count,
            capture.performance_acceptance.declared_failure_policy_count,
            capture
                .performance_acceptance
                .active_degradation_response_count,
            capture.performance_acceptance.owned_gpu_pass_count,
            capture.performance_acceptance.gpu_resource_count,
            capture.performance_acceptance.visible_memory_bytes as f64 / (1024.0 * 1024.0),
            capture.performance_acceptance.visible_streaming_bytes as f64 / (1024.0 * 1024.0),
            capture.performance_acceptance.covered_budget_category_count,
            capture
                .performance_acceptance
                .required_budget_category_count,
            capture.performance_acceptance.covered_hardware_tier_count,
            capture.performance_acceptance.hardware_tier_count,
            capture
                .performance_acceptance
                .covered_scalability_ladder_step_count,
            capture.performance_acceptance.scalability_ladder_step_count,
            capture
                .performance_acceptance
                .protected_truth_domain_safe_count,
            capture.performance_acceptance.protected_truth_domain_count,
            capture
                .performance_acceptance
                .handled_memory_pressure_action_count,
            capture.performance_acceptance.memory_pressure_action_count,
            capture
                .performance_acceptance
                .visible_streaming_counter_count,
            capture.performance_acceptance.streaming_counter_count,
            capture.performance_acceptance.shader_variant_count,
            capture.performance_acceptance.cache_build_signal_count,
            capture.performance_acceptance.issue_count
        );
        if let Some(issue) = capture.performance_acceptance.issues.first() {
            println!(
                "Tools performance acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools open research: {}/{} package(s) ready, {}/{} signal(s), measured {}, schema {}, performance {}, debug {}, fallback {}, golden {}, {} issue(s)",
            capture.open_research.ready_package_count,
            capture.open_research.package_count,
            capture.open_research.passed_signal_count,
            capture.open_research.signal_count,
            capture.open_research.measured_prototype_count,
            capture.open_research.schema_proposal_count,
            capture.open_research.performance_data_count,
            capture.open_research.debug_view_count,
            capture.open_research.fallback_behavior_count,
            capture.open_research.golden_integration_plan_count,
            capture.open_research.issue_count
        );
        if let Some(issue) = capture.open_research.issues.first() {
            println!(
                "Tools open research issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools roadmap: {}/{} phase(s) ready, {}/{} signal(s), schema {}, budget {}, debug {}, golden {}, failure {}, integration {}, {} issue(s)",
            capture.roadmap_acceptance.ready_phase_count,
            capture.roadmap_acceptance.phase_count,
            capture.roadmap_acceptance.passed_signal_count,
            capture.roadmap_acceptance.signal_count,
            capture.roadmap_acceptance.schema_test_signal_count,
            capture.roadmap_acceptance.performance_budget_signal_count,
            capture.roadmap_acceptance.debug_view_signal_count,
            capture.roadmap_acceptance.golden_review_signal_count,
            capture.roadmap_acceptance.failure_behavior_signal_count,
            capture.roadmap_acceptance.integration_evidence_signal_count,
            capture.roadmap_acceptance.issue_count
        );
        if let Some(issue) = capture.roadmap_acceptance.issues.first() {
            println!("Tools roadmap issue: {} - {}", issue.subject, issue.message);
        }
        println!(
            "Tools product quality bar: {}/{} check(s), {}/{} signal(s), final {}, golden {}, visual {}, performance {}, failure {}, acceptance {}, practical {}, {} issue(s)",
            capture.product_quality_bar.passed_check_count,
            capture.product_quality_bar.check_count,
            capture.product_quality_bar.passed_signal_count,
            capture.product_quality_bar.signal_count,
            capture.product_quality_bar.final_target_signal_count,
            capture.product_quality_bar.golden_scene_signal_count,
            capture.product_quality_bar.visual_quality_signal_count,
            capture.product_quality_bar.performance_quality_signal_count,
            capture.product_quality_bar.failure_guard_signal_count,
            capture.product_quality_bar.acceptance_rule_signal_count,
            capture.product_quality_bar.practical_target_signal_count,
            capture.product_quality_bar.issue_count
        );
        if let Some(issue) = capture.product_quality_bar.issues.first() {
            println!(
                "Tools product quality issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools engine core: {}/{} check(s), {}/{} signal(s), snapshot {}, command {}, ledger {}, module {}, save {}, replay {}, GPU {}, truth {}/{}, lifecycle {}/{}, validation {}/{}, debug {}/{}, {} issue(s)",
            capture.engine_core_acceptance.passed_check_count,
            capture.engine_core_acceptance.check_count,
            capture.engine_core_acceptance.passed_signal_count,
            capture.engine_core_acceptance.signal_count,
            capture.engine_core_acceptance.snapshot_signal_count,
            capture
                .engine_core_acceptance
                .command_validation_signal_count,
            capture.engine_core_acceptance.event_ledger_signal_count,
            capture.engine_core_acceptance.module_contract_signal_count,
            capture.engine_core_acceptance.save_truth_signal_count,
            capture.engine_core_acceptance.replay_signal_count,
            capture.engine_core_acceptance.gpu_boundary_signal_count,
            capture
                .engine_core_acceptance
                .world_truth_contract_ready_count,
            capture.engine_core_acceptance.world_truth_contract_count,
            capture
                .engine_core_acceptance
                .module_lifecycle_contract_ready_count,
            capture
                .engine_core_acceptance
                .module_lifecycle_contract_count,
            capture.engine_core_acceptance.validation_guard_ready_count,
            capture.engine_core_acceptance.validation_guard_count,
            capture.engine_core_acceptance.debug_surface_ready_count,
            capture.engine_core_acceptance.debug_surface_count,
            capture.engine_core_acceptance.issue_count
        );
        if let Some(issue) = capture.engine_core_acceptance.issues.first() {
            println!(
                "Tools engine core issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools GPU Services: {}/{} gate(s), {}/{} signal(s), ownership {}, graph {}, validation {}, timing {}, memory {}, queue {}, streaming {}, fallback {}, debug {}, handles {}/{}, features {}/{}, pass fields {}/{}, memory policy {}/{}, descriptors {}/{}, BDA {}/{}, optional {}/{}, sync {}/{}, profiling {}/{}, debug req {}/{}, {} issue(s)",
            capture.gpu_services_acceptance.passed_gate_count,
            capture.gpu_services_acceptance.gate_count,
            capture.gpu_services_acceptance.passed_signal_count,
            capture.gpu_services_acceptance.signal_count,
            capture.gpu_services_acceptance.ownership_signal_count,
            capture.gpu_services_acceptance.render_graph_signal_count,
            capture.gpu_services_acceptance.validation_signal_count,
            capture.gpu_services_acceptance.timing_signal_count,
            capture.gpu_services_acceptance.memory_signal_count,
            capture.gpu_services_acceptance.queue_signal_count,
            capture.gpu_services_acceptance.streaming_signal_count,
            capture.gpu_services_acceptance.fallback_signal_count,
            capture.gpu_services_acceptance.debug_signal_count,
            capture
                .gpu_services_acceptance
                .public_interface_handle_ready_count,
            capture
                .gpu_services_acceptance
                .public_interface_handle_count,
            capture
                .gpu_services_acceptance
                .vulkan_feature_policy_ready_count,
            capture.gpu_services_acceptance.vulkan_feature_policy_count,
            capture
                .gpu_services_acceptance
                .graph_declaration_field_ready_count,
            capture
                .gpu_services_acceptance
                .graph_declaration_field_count,
            capture
                .gpu_services_acceptance
                .memory_policy_category_ready_count,
            capture.gpu_services_acceptance.memory_policy_category_count,
            capture
                .gpu_services_acceptance
                .descriptor_policy_ready_count,
            capture.gpu_services_acceptance.descriptor_policy_count,
            capture
                .gpu_services_acceptance
                .buffer_address_policy_ready_count,
            capture.gpu_services_acceptance.buffer_address_policy_count,
            capture
                .gpu_services_acceptance
                .optional_feature_policy_ready_count,
            capture
                .gpu_services_acceptance
                .optional_feature_policy_count,
            capture
                .gpu_services_acceptance
                .synchronization_policy_ready_count,
            capture.gpu_services_acceptance.synchronization_policy_count,
            capture
                .gpu_services_acceptance
                .profiling_requirement_ready_count,
            capture.gpu_services_acceptance.profiling_requirement_count,
            capture
                .gpu_services_acceptance
                .debug_requirement_ready_count,
            capture.gpu_services_acceptance.debug_requirement_count,
            capture.gpu_services_acceptance.issue_count
        );
        if let Some(issue) = capture.gpu_services_acceptance.issues.first() {
            println!(
                "Tools GPU Services issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools photoreal rendering: {}/{} gate(s), {}/{} signal(s), scene {}, camera/color {}, lighting/shadow {}, material/volume {}, human {}, temporal {}, reference {}, debug/perf {}, scenes {}/{}, input {}/{}, output {}/{}, light {}/{}, camera {}/{}, material/volume {}/{}, human req {}/{}, temporal req {}/{}, reference req {}/{}, reject {}/{}, {} issue(s)",
            capture.photoreal_rendering_acceptance.passed_gate_count,
            capture.photoreal_rendering_acceptance.gate_count,
            capture.photoreal_rendering_acceptance.passed_signal_count,
            capture.photoreal_rendering_acceptance.signal_count,
            capture.photoreal_rendering_acceptance.scene_signal_count,
            capture
                .photoreal_rendering_acceptance
                .camera_color_signal_count,
            capture
                .photoreal_rendering_acceptance
                .lighting_shadow_signal_count,
            capture
                .photoreal_rendering_acceptance
                .material_volume_signal_count,
            capture.photoreal_rendering_acceptance.human_signal_count,
            capture.photoreal_rendering_acceptance.temporal_signal_count,
            capture
                .photoreal_rendering_acceptance
                .reference_signal_count,
            capture
                .photoreal_rendering_acceptance
                .debug_performance_signal_count,
            capture
                .photoreal_rendering_acceptance
                .golden_scene_matrix_ready_count,
            capture
                .photoreal_rendering_acceptance
                .golden_scene_matrix_count,
            capture
                .photoreal_rendering_acceptance
                .renderer_input_packet_field_ready_count,
            capture
                .photoreal_rendering_acceptance
                .renderer_input_packet_field_count,
            capture
                .photoreal_rendering_acceptance
                .renderer_output_artifact_ready_count,
            capture
                .photoreal_rendering_acceptance
                .renderer_output_artifact_count,
            capture
                .photoreal_rendering_acceptance
                .lighting_strategy_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .lighting_strategy_requirement_count,
            capture
                .photoreal_rendering_acceptance
                .camera_color_pipeline_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .camera_color_pipeline_requirement_count,
            capture
                .photoreal_rendering_acceptance
                .material_volume_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .material_volume_requirement_count,
            capture
                .photoreal_rendering_acceptance
                .human_rendering_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .human_rendering_requirement_count,
            capture
                .photoreal_rendering_acceptance
                .temporal_stability_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .temporal_stability_requirement_count,
            capture
                .photoreal_rendering_acceptance
                .reference_path_requirement_ready_count,
            capture
                .photoreal_rendering_acceptance
                .reference_path_requirement_count,
            capture
                .photoreal_rendering_acceptance
                .rejection_guard_ready_count,
            capture.photoreal_rendering_acceptance.rejection_guard_count,
            capture.photoreal_rendering_acceptance.issue_count
        );
        if let Some(issue) = capture.photoreal_rendering_acceptance.issues.first() {
            println!(
                "Tools photoreal rendering issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools milestones: {}/{} stage(s), {}/{} acceptance check(s), {} blocked, {} issue(s)",
            capture.production_milestones.passed_stage_count,
            capture.production_milestones.stage_count,
            capture.production_milestones.passed_acceptance_check_count,
            capture.production_milestones.acceptance_check_count,
            capture.production_milestones.blocked_stage_count,
            capture.production_milestones.issue_count
        );
        if let Some(issue) = capture.production_milestones.issues.first() {
            println!(
                "Tools milestone issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools risks: {}/{} mitigated, {}/{} acceptance signal(s), {} blocked, {} issue(s)",
            capture.production_risks.mitigated_risk_count,
            capture.production_risks.risk_count,
            capture.production_risks.passed_acceptance_signal_count,
            capture.production_risks.acceptance_signal_count,
            capture.production_risks.blocked_risk_count,
            capture.production_risks.issue_count
        );
        if let Some(issue) = capture.production_risks.issues.first() {
            println!("Tools risk issue: {} - {}", issue.subject, issue.message);
        }
        println!(
            "Tools architecture contract: {}/{} rule(s), {}/{} evidence item(s), {} blocked, {} issue(s)",
            capture.architecture_contract.passed_rule_count,
            capture.architecture_contract.rule_count,
            capture.architecture_contract.passed_evidence_count,
            capture.architecture_contract.evidence_count,
            capture.architecture_contract.blocked_rule_count,
            capture.architecture_contract.issue_count
        );
        if let Some(issue) = capture.architecture_contract.issues.first() {
            println!(
                "Tools architecture issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools stress scenes: {}/{} scene(s), {}/{} signal(s), {} blocked, {} issue(s)",
            capture.stress_scenes.passed_scene_count,
            capture.stress_scenes.scene_count,
            capture.stress_scenes.passed_signal_count,
            capture.stress_scenes.signal_count,
            capture.stress_scenes.blocked_scene_count,
            capture.stress_scenes.issue_count
        );
        if let Some(issue) = capture.stress_scenes.issues.first() {
            println!(
                "Tools stress scene issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools golden scenes: {}/{} scene(s), {} CI-ready, {}/{} signal(s), {} artifact(s), screenshots {}, videos {}, frame captures {}, overlays {}, {} blocked, {} issue(s), replay hash {}, CPU {} us, GPU {} us, VRAM {:.2} MiB, streaming {:.2} MiB",
            capture.golden_scenes.passed_scene_count,
            capture.golden_scenes.scene_count,
            capture.golden_scenes.ci_ready_scene_count,
            capture.golden_scenes.passed_signal_count,
            capture.golden_scenes.signal_count,
            capture.golden_scenes.artifact_count,
            capture.golden_scenes.screenshot_count,
            capture.golden_scenes.video_capture_count,
            capture.golden_scenes.frame_capture_count,
            capture.golden_scenes.debug_overlay_count,
            capture.golden_scenes.blocked_scene_count,
            capture.golden_scenes.issue_count,
            capture.golden_scenes.replay_hash,
            capture.golden_scenes.total_cpu_time_us,
            capture.golden_scenes.total_gpu_time_us,
            capture.golden_scenes.peak_vram_bytes as f64 / (1024.0 * 1024.0),
            capture.golden_scenes.total_streaming_bytes as f64 / (1024.0 * 1024.0)
        );
        if let Some(issue) = capture.golden_scenes.issues.first() {
            println!(
                "Tools golden scene issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools runtime policy: {}/{} check(s), {}/{} signal(s), {} crate(s), {} external dep(s), {} Rust-first, {} platform boundary, {} leaked GPU, {} leaked window, hidden GPU {}, ML GPU {}, UI {}, shaders {}/{} validated, artifacts SPIR-V {} rust-gpu {} engine {}, unknown {}, errors {}, {} forbidden, {} ABI export(s), {} issue(s)",
            capture.runtime_policy.passed_check_count,
            capture.runtime_policy.check_count,
            capture.runtime_policy.passed_signal_count,
            capture.runtime_policy.signal_count,
            capture.runtime_policy.runtime_crate_count,
            capture.runtime_policy.external_dependency_count,
            capture.runtime_policy.rust_first_dependency_count,
            capture.runtime_policy.platform_boundary_dependency_count,
            capture.runtime_policy.leaked_gpu_dependency_count,
            capture.runtime_policy.leaked_window_dependency_count,
            capture.runtime_policy.hidden_gpu_backend_dependency_count,
            capture.runtime_policy.ml_gpu_backend_dependency_count,
            capture.runtime_policy.forbidden_ui_dependency_count,
            capture.runtime_policy.validated_shader_contract_count,
            capture.runtime_policy.shader_contract_count,
            capture.runtime_policy.spirv_shader_artifact_count,
            capture.runtime_policy.rust_gpu_shader_artifact_count,
            capture.runtime_policy.engine_shader_source_artifact_count,
            capture.runtime_policy.unknown_shader_artifact_count,
            capture.runtime_policy.shader_validation_error_count,
            capture.runtime_policy.forbidden_dependency_count,
            capture.runtime_policy.dynamic_abi_export_count,
            capture.runtime_policy.issue_count
        );
        if let Some(issue) = capture.runtime_policy.issues.first() {
            println!(
                "Tools runtime policy issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools reference anchors: {}/{} anchor(s), {}/{} signal(s), {} Unreal-class, {} Vulkan/Rust, {} material/scene, {} physics, {} unique-direction signal(s), {} issue(s)",
            capture.reference_anchors.passed_anchor_count,
            capture.reference_anchors.anchor_count,
            capture.reference_anchors.passed_signal_count,
            capture.reference_anchors.signal_count,
            capture.reference_anchors.unreal_class_anchor_count,
            capture.reference_anchors.vulkan_rust_anchor_count,
            capture.reference_anchors.material_scene_anchor_count,
            capture.reference_anchors.physics_research_anchor_count,
            capture.reference_anchors.unique_direction_signal_count,
            capture.reference_anchors.issue_count
        );
        if let Some(issue) = capture.reference_anchors.issues.first() {
            println!(
                "Tools reference anchor issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools interface schemas: {}/{} section(s), {}/{} signal(s), {} ID alias(es), {} source file(s), {} package, {} world, {} material, {} GPU, {} render, {} physics, {} AI/voice, {} city, {} conformance, {} runtime schema(s), {} issue(s)",
            capture.interface_schema_coverage.passed_section_count,
            capture.interface_schema_coverage.section_count,
            capture.interface_schema_coverage.passed_signal_count,
            capture.interface_schema_coverage.signal_count,
            capture.interface_schema_coverage.core_id_alias_count,
            capture.interface_schema_coverage.source_file_count,
            capture.interface_schema_coverage.package_contract_count,
            capture.interface_schema_coverage.world_contract_count,
            capture.interface_schema_coverage.material_contract_count,
            capture.interface_schema_coverage.gpu_contract_count,
            capture.interface_schema_coverage.rendering_contract_count,
            capture.interface_schema_coverage.simulation_contract_count,
            capture.interface_schema_coverage.ai_voice_contract_count,
            capture.interface_schema_coverage.city_contract_count,
            capture.interface_schema_coverage.conformance_contract_count,
            capture.interface_schema_coverage.runtime_schema_count,
            capture.interface_schema_coverage.issue_count
        );
        if let Some(issue) = capture.interface_schema_coverage.issues.first() {
            println!(
                "Tools interface schema issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools executive sanity: {}/{} check(s), {}/{} signal(s), {} runtime/platform, {} photoreal, {} physical-loop, {} production-readiness, {} measured artifact(s), {} issue(s)",
            capture.executive_sanity.passed_check_count,
            capture.executive_sanity.check_count,
            capture.executive_sanity.passed_signal_count,
            capture.executive_sanity.signal_count,
            capture.executive_sanity.runtime_platform_signal_count,
            capture.executive_sanity.photoreal_chain_signal_count,
            capture.executive_sanity.physical_loop_signal_count,
            capture.executive_sanity.production_readiness_signal_count,
            capture.executive_sanity.measured_artifact_count,
            capture.executive_sanity.issue_count
        );
        if let Some(issue) = capture.executive_sanity.issues.first() {
            println!(
                "Tools executive sanity issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools truth/cache: {}/{} lineage(s), {}/{} signal(s), truth {}, cache {}, stream {}, evidence {}, {} issue(s)",
            capture.truth_cache_boundary.passed_lineage_count,
            capture.truth_cache_boundary.lineage_count,
            capture.truth_cache_boundary.passed_signal_count,
            capture.truth_cache_boundary.signal_count,
            capture.truth_cache_boundary.truth_signal_count,
            capture.truth_cache_boundary.generated_cache_signal_count,
            capture.truth_cache_boundary.runtime_stream_signal_count,
            capture.truth_cache_boundary.debug_evidence_signal_count,
            capture.truth_cache_boundary.issue_count
        );
        if let Some(issue) = capture.truth_cache_boundary.issues.first() {
            println!(
                "Tools truth/cache issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools build automation: {}/{} stage(s), {}/{} signal(s), {} CI-required, rust {}, shader {}, validation {}, scene {}, performance {}, {} issue(s)",
            capture.build_automation.passed_stage_count,
            capture.build_automation.stage_count,
            capture.build_automation.passed_signal_count,
            capture.build_automation.signal_count,
            capture.build_automation.ci_required_stage_count,
            capture.build_automation.rust_stage_count,
            capture.build_automation.shader_stage_count,
            capture.build_automation.validation_stage_count,
            capture.build_automation.scene_stage_count,
            capture.build_automation.performance_stage_count,
            capture.build_automation.issue_count
        );
        if let Some(issue) = capture.build_automation.issues.first() {
            println!(
                "Tools build automation issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools safety/provenance: {}/{} domain(s), {}/{} signal(s), asset {}, human/voice {}, AI safety {}, production {}, provenance {}, rights {}, consent {}, {} issue(s)",
            capture.safety_provenance.passed_domain_count,
            capture.safety_provenance.domain_count,
            capture.safety_provenance.passed_signal_count,
            capture.safety_provenance.signal_count,
            capture.safety_provenance.asset_signal_count,
            capture.safety_provenance.human_voice_signal_count,
            capture.safety_provenance.ai_safety_signal_count,
            capture.safety_provenance.production_signal_count,
            capture
                .safety_provenance
                .generated_provenance_evidence_count,
            capture.safety_provenance.rights_evidence_count,
            capture.safety_provenance.consent_evidence_count,
            capture.safety_provenance.issue_count
        );
        if let Some(issue) = capture.safety_provenance.issues.first() {
            println!(
                "Tools safety/provenance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools asset package acceptance: {}/{} gate(s), {} package output(s), {}/{} versioned, schema decl {}, package schemas {}, validations {}, truth/cache {}/{}, generated cache {}, cache CI {}, schema CI {}, provenance/rights/consent {}/{}/{}, v9 package {}/{}, provenance {}/{}, cache {}/{}, textureless {}/{}, runtime {}/{}, tags {}/{}, reject {}/{}, invalid failures {}, diagnostics {}, clean-cache scenes {}, artifacts {}, {} migration(s), {:.2} MiB package, {:.2} MiB runtime, {:.2} MiB streaming, {} issue(s)",
            capture.asset_package_acceptance.passed_gate_count,
            capture.asset_package_acceptance.gate_count,
            capture.asset_package_acceptance.package_output_count,
            capture
                .asset_package_acceptance
                .versioned_package_output_count,
            capture.asset_package_acceptance.package_output_count,
            capture.asset_package_acceptance.schema_declaration_count,
            capture.asset_package_acceptance.package_schema_count,
            capture.asset_package_acceptance.validation_report_count,
            capture.asset_package_acceptance.passed_truth_lineage_count,
            capture.asset_package_acceptance.truth_lineage_count,
            capture
                .asset_package_acceptance
                .generated_cache_signal_count,
            capture.asset_package_acceptance.cache_key_test_signal_count,
            capture
                .asset_package_acceptance
                .schema_validation_signal_count,
            capture.asset_package_acceptance.provenance_evidence_count,
            capture.asset_package_acceptance.rights_evidence_count,
            capture.asset_package_acceptance.consent_evidence_count,
            capture
                .asset_package_acceptance
                .package_contract_ready_count,
            capture.asset_package_acceptance.package_contract_count,
            capture
                .asset_package_acceptance
                .provenance_field_ready_count,
            capture.asset_package_acceptance.provenance_field_count,
            capture
                .asset_package_acceptance
                .cache_rebuild_contract_ready_count,
            capture
                .asset_package_acceptance
                .cache_rebuild_contract_count,
            capture
                .asset_package_acceptance
                .textureless_policy_ready_count,
            capture
                .asset_package_acceptance
                .textureless_policy_signal_count,
            capture
                .asset_package_acceptance
                .runtime_loading_rule_ready_count,
            capture.asset_package_acceptance.runtime_loading_rule_count,
            capture
                .asset_package_acceptance
                .quality_performance_tag_ready_count,
            capture
                .asset_package_acceptance
                .quality_performance_tag_count,
            capture
                .asset_package_acceptance
                .validation_rejection_rule_ready_count,
            capture
                .asset_package_acceptance
                .validation_rejection_rule_count,
            capture.asset_package_acceptance.invalid_asset_failure_count,
            capture
                .asset_package_acceptance
                .invalid_asset_diagnostic_count,
            capture
                .asset_package_acceptance
                .clean_cache_rebuild_scene_count,
            capture
                .asset_package_acceptance
                .clean_cache_rebuild_artifact_count,
            capture.asset_package_acceptance.migration_step_count,
            capture.asset_package_acceptance.package_memory_bytes as f64 / (1024.0 * 1024.0),
            capture.asset_package_acceptance.runtime_known_bytes as f64 / (1024.0 * 1024.0),
            capture.asset_package_acceptance.streaming_bytes as f64 / (1024.0 * 1024.0),
            capture.asset_package_acceptance.issue_count
        );
        if let Some(issue) = capture.asset_package_acceptance.issues.first() {
            println!(
                "Tools asset package acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools v8 feature acceptance: {}/{} feature(s), {}/{} requirement(s), golden {}, schema {}, budget {}, validation {}, provenance {}, tooling {}, {} issue(s)",
            capture.feature_acceptance.accepted_feature_count,
            capture.feature_acceptance.feature_count,
            capture.feature_acceptance.passed_requirement_count,
            capture.feature_acceptance.requirement_count,
            capture.feature_acceptance.golden_scene_requirement_count,
            capture.feature_acceptance.schema_requirement_count,
            capture.feature_acceptance.performance_requirement_count,
            capture.feature_acceptance.validation_requirement_count,
            capture.feature_acceptance.provenance_requirement_count,
            capture.feature_acceptance.tooling_requirement_count,
            capture.feature_acceptance.issue_count
        );
        if let Some(issue) = capture.feature_acceptance.issues.first() {
            println!(
                "Tools v8 feature acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools editor: {}/{} panel(s), renderer {}, replay {}, profiler {}, validation {}, package-output {}, {} issue(s)",
            capture.editor_workspace.available_panel_count,
            capture.editor_workspace.panel_count,
            capture.editor_workspace.renderer_backed_panel_count,
            capture.editor_workspace.replay_backed_panel_count,
            capture.editor_workspace.profiler_backed_panel_count,
            capture.editor_workspace.validation_backed_panel_count,
            capture.editor_workspace.package_output_panel_count,
            capture.editor_workspace.issue_count
        );
        println!(
            "Tools editor/tools acceptance: {}/{} gate(s), debug {}, profiler {}, golden {}, asset {}, replay {}, AI {}, reference {}, required tools {}/{}, debug domains {}/{}, profiler dims {}/{}, validation {}/{}, failure evidence {}, reviewer artifacts {}, panels {}, samples {}, hotspots {}, golden scenes {}, replay frames {}, AI decisions {}, comparisons {}, {} issue(s)",
            capture.tools_acceptance.passed_gate_count,
            capture.tools_acceptance.gate_count,
            capture.tools_acceptance.debug_view_signal_count,
            capture.tools_acceptance.profiler_signal_count,
            capture.tools_acceptance.golden_runner_signal_count,
            capture.tools_acceptance.asset_validation_signal_count,
            capture.tools_acceptance.replay_reproduction_signal_count,
            capture.tools_acceptance.ai_inspection_signal_count,
            capture.tools_acceptance.reference_review_signal_count,
            capture.tools_acceptance.available_required_tool_count,
            capture.tools_acceptance.required_tool_count,
            capture.tools_acceptance.component_debug_domain_ready_count,
            capture.tools_acceptance.component_debug_domain_count,
            capture.tools_acceptance.profiler_dimension_ready_count,
            capture.tools_acceptance.profiler_dimension_count,
            capture.tools_acceptance.validation_check_ready_count,
            capture.tools_acceptance.validation_check_count,
            capture.tools_acceptance.failure_report_evidence_count,
            capture.tools_acceptance.reviewer_artifact_count,
            capture.tools_acceptance.available_panel_count,
            capture.tools_acceptance.profiler_sample_count,
            capture.tools_acceptance.performance_hotspot_count,
            capture.tools_acceptance.golden_scene_count,
            capture.tools_acceptance.replay_frame_count,
            capture.tools_acceptance.ai_decision_count,
            capture.tools_acceptance.reference_comparison_count,
            capture.tools_acceptance.issue_count
        );
        if let Some(issue) = capture.tools_acceptance.issues.first() {
            println!(
                "Tools editor/tools acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools R&D gates: {} feature(s), {} prototype, {} integration, {} performance, {} production-ready, {} missing check(s)",
            capture.r_and_d_gates.feature_count,
            capture.r_and_d_gates.prototype_pass_count,
            capture.r_and_d_gates.integration_pass_count,
            capture.r_and_d_gates.performance_pass_count,
            capture.r_and_d_gates.production_ready_count,
            capture.r_and_d_gates.missing_check_count
        );
        println!(
            "Tools conformance: fixture {}, passed {}, {} output rule(s), {} missing output(s), {} issue(s)",
            capture.conformance.fixture_id,
            capture.conformance.passed,
            capture.conformance.checked_output_count,
            capture.conformance.missing_output_count,
            capture.conformance.issue_count
        );
        println!(
            "Tools v7 handoff: {}/{} component(s) ready, L0 {}, L1+ {}, L2+ {}, L3+ {}, L4+ {}, L5 {}, {}/{} universal, {}/{} team, {}/{} handoff item(s), {} issue(s)",
            capture.team_handoff_matrix.ready_component_count,
            capture.team_handoff_matrix.component_count,
            capture.team_handoff_matrix.research_only_component_count,
            capture
                .team_handoff_matrix
                .schema_compatible_component_count,
            capture
                .team_handoff_matrix
                .runtime_integrated_component_count,
            capture
                .team_handoff_matrix
                .golden_scene_participant_component_count,
            capture
                .team_handoff_matrix
                .performance_approved_component_count,
            capture
                .team_handoff_matrix
                .production_candidate_component_count,
            capture.team_handoff_matrix.passed_universal_check_count,
            capture.team_handoff_matrix.universal_check_count,
            capture.team_handoff_matrix.passed_team_check_count,
            capture.team_handoff_matrix.team_check_count,
            capture.team_handoff_matrix.passed_handoff_item_count,
            capture.team_handoff_matrix.handoff_item_count,
            capture.team_handoff_matrix.issue_count
        );
        if let Some(issue) = capture.team_handoff_matrix.issues.first() {
            println!(
                "Tools v7 handoff issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools reference comparison: {} gameplay frame(s), {} reference frame(s), {} comparison pass(es), {:.2} MiB radiance, {:.2} KiB metrics, {} issue(s)",
            capture.reference_comparison.gameplay_frame_count,
            capture.reference_comparison.reference_frame_count,
            capture.reference_comparison.comparison_pass_count,
            capture.reference_comparison.total_radiance_target_bytes as f64 / (1024.0 * 1024.0),
            capture.reference_comparison.total_metrics_buffer_bytes as f64 / 1024.0,
            capture.reference_comparison.issue_count
        );
        println!(
            "Tools reference validation: {}/{} gate(s), material evidence {}, rainy comparison {}, human review {}, channel inspections {}, automated signals {}, use cases {}/{}, stages {}/{}, truth {}/{}, outputs {}/{}, approximation {}/{}, preserved {}/{}, material checks {}/{}, human checks {}/{}, {} issue(s)",
            capture.reference_validation_acceptance.passed_gate_count,
            capture.reference_validation_acceptance.gate_count,
            capture
                .reference_validation_acceptance
                .material_reference_evidence_count,
            capture
                .reference_validation_acceptance
                .rainy_alley_comparison_count,
            capture
                .reference_validation_acceptance
                .human_review_image_count,
            capture
                .reference_validation_acceptance
                .channel_inspection_count,
            capture
                .reference_validation_acceptance
                .automated_review_signal_count,
            capture
                .reference_validation_acceptance
                .covered_reference_use_case_count,
            capture
                .reference_validation_acceptance
                .reference_use_case_count,
            capture
                .reference_validation_acceptance
                .covered_reference_stage_count,
            capture
                .reference_validation_acceptance
                .reference_stage_count,
            capture
                .reference_validation_acceptance
                .same_truth_domain_ready_count,
            capture
                .reference_validation_acceptance
                .same_truth_domain_count,
            capture
                .reference_validation_acceptance
                .comparison_output_ready_count,
            capture
                .reference_validation_acceptance
                .comparison_output_count,
            capture
                .reference_validation_acceptance
                .realtime_approximation_ready_count,
            capture
                .reference_validation_acceptance
                .realtime_approximation_count,
            capture
                .reference_validation_acceptance
                .preserved_quality_ready_count,
            capture
                .reference_validation_acceptance
                .preserved_quality_count,
            capture
                .reference_validation_acceptance
                .material_validation_check_ready_count,
            capture
                .reference_validation_acceptance
                .material_validation_check_count,
            capture
                .reference_validation_acceptance
                .human_validation_check_ready_count,
            capture
                .reference_validation_acceptance
                .human_validation_check_count,
            capture.reference_validation_acceptance.issue_count
        );
        if let Some(issue) = capture.reference_validation_acceptance.issues.first() {
            println!(
                "Tools reference validation issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools renderer acceptance: {}/{} gate(s), rainy {}, human {}, reference {}, tier signals {}, debug views {}, streaming {}, gpu services {}, {} issue(s)",
            capture.renderer_acceptance.passed_gate_count,
            capture.renderer_acceptance.gate_count,
            capture.renderer_acceptance.rainy_alley_signal_count,
            capture.renderer_acceptance.hero_human_signal_count,
            capture.renderer_acceptance.reference_comparison_count,
            capture.renderer_acceptance.performance_tier_signal_count,
            capture.renderer_acceptance.debug_view_signal_count,
            capture.renderer_acceptance.streaming_stability_signal_count,
            capture.renderer_acceptance.gpu_services_signal_count,
            capture.renderer_acceptance.issue_count
        );
        if let Some(issue) = capture.renderer_acceptance.issues.first() {
            println!(
                "Tools renderer acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools virtual geometry acceptance: {}/{} gate(s), dense {}, streaming {}, fracture {}, procedural {}, budget {}, debug {}, fallback {}, peak visible {}, clusters {}/{}, visibility {}/{}, sources {}/{}, displacement {}/{}, fracture steps {}/{}, world package {}/{}, perf rules {}/{}, tiers {}/{}, debug req {}/{}, scenes {}/{}, {} issue(s)",
            capture.virtual_geometry_acceptance.passed_gate_count,
            capture.virtual_geometry_acceptance.gate_count,
            capture
                .virtual_geometry_acceptance
                .dense_surface_signal_count,
            capture.virtual_geometry_acceptance.streaming_signal_count,
            capture
                .virtual_geometry_acceptance
                .fractured_geometry_signal_count,
            capture
                .virtual_geometry_acceptance
                .procedural_microgeometry_signal_count,
            capture.virtual_geometry_acceptance.budget_signal_count,
            capture.virtual_geometry_acceptance.debug_view_signal_count,
            capture.virtual_geometry_acceptance.fallback_signal_count,
            capture
                .virtual_geometry_acceptance
                .peak_visible_cluster_capacity,
            capture
                .virtual_geometry_acceptance
                .cluster_record_field_ready_count,
            capture
                .virtual_geometry_acceptance
                .cluster_record_field_count,
            capture
                .virtual_geometry_acceptance
                .visibility_pipeline_step_ready_count,
            capture
                .virtual_geometry_acceptance
                .visibility_pipeline_step_count,
            capture
                .virtual_geometry_acceptance
                .microgeometry_source_ready_count,
            capture
                .virtual_geometry_acceptance
                .microgeometry_source_count,
            capture
                .virtual_geometry_acceptance
                .displacement_strategy_ready_count,
            capture
                .virtual_geometry_acceptance
                .displacement_strategy_count,
            capture
                .virtual_geometry_acceptance
                .fracture_integration_step_ready_count,
            capture
                .virtual_geometry_acceptance
                .fracture_integration_step_count,
            capture
                .virtual_geometry_acceptance
                .world_streaming_package_field_ready_count,
            capture
                .virtual_geometry_acceptance
                .world_streaming_package_field_count,
            capture
                .virtual_geometry_acceptance
                .performance_rule_ready_count,
            capture.virtual_geometry_acceptance.performance_rule_count,
            capture
                .virtual_geometry_acceptance
                .hardware_tier_policy_ready_count,
            capture
                .virtual_geometry_acceptance
                .hardware_tier_policy_count,
            capture
                .virtual_geometry_acceptance
                .debug_view_requirement_ready_count,
            capture
                .virtual_geometry_acceptance
                .debug_view_requirement_count,
            capture
                .virtual_geometry_acceptance
                .acceptance_scene_ready_count,
            capture.virtual_geometry_acceptance.acceptance_scene_count,
            capture.virtual_geometry_acceptance.issue_count
        );
        if let Some(issue) = capture.virtual_geometry_acceptance.issues.first() {
            println!(
                "Tools virtual geometry acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools virtual geometry pipeline: {}/{} gate(s), {}/{} signal(s), truth/cache {}, cluster {}, visibility {}, streaming/LOD {}, microgeometry {}, fracture {}, budget/debug {}, fallback {}, {} issue(s)",
            capture
                .virtual_geometry_pipeline_acceptance
                .passed_gate_count,
            capture.virtual_geometry_pipeline_acceptance.gate_count,
            capture
                .virtual_geometry_pipeline_acceptance
                .passed_signal_count,
            capture.virtual_geometry_pipeline_acceptance.signal_count,
            capture
                .virtual_geometry_pipeline_acceptance
                .truth_cache_signal_count,
            capture
                .virtual_geometry_pipeline_acceptance
                .cluster_data_signal_count,
            capture
                .virtual_geometry_pipeline_acceptance
                .visibility_signal_count,
            capture
                .virtual_geometry_pipeline_acceptance
                .streaming_lod_signal_count,
            capture
                .virtual_geometry_pipeline_acceptance
                .microgeometry_signal_count,
            capture
                .virtual_geometry_pipeline_acceptance
                .fracture_signal_count,
            capture
                .virtual_geometry_pipeline_acceptance
                .budget_debug_signal_count,
            capture
                .virtual_geometry_pipeline_acceptance
                .fallback_signal_count,
            capture.virtual_geometry_pipeline_acceptance.issue_count
        );
        if let Some(issue) = capture.virtual_geometry_pipeline_acceptance.issues.first() {
            println!(
                "Tools virtual geometry pipeline issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools material acceptance: {}/{} gate(s), cache {}, lab {}, wetness {}, fracture {}, rebuild {}, provenance {}, channels {}/{}, runtime frames {}, descriptor {}/{}, visual {}/{}, physical {}/{}, state req {}/{}, generated {}/{}, capture {}/{}, graph {}/{}, render {}/{}, physics {}/{}, audio {}/{}, AI/gameplay {}/{}, perf {}/{}, debug req {}/{}, scenes {}/{}, {} issue(s)",
            capture.material_acceptance.passed_gate_count,
            capture.material_acceptance.gate_count,
            capture.material_acceptance.renderable_cache_signal_count,
            capture.material_acceptance.material_lab_signal_count,
            capture
                .material_acceptance
                .wetness_cross_system_signal_count,
            capture.material_acceptance.fracture_interior_signal_count,
            capture.material_acceptance.cache_rebuild_signal_count,
            capture.material_acceptance.provenance_signal_count,
            capture.material_acceptance.fully_linked_state_channel_count,
            capture.material_acceptance.active_state_channel_count,
            capture.material_acceptance.material_runtime_frame_count,
            capture.material_acceptance.descriptor_domain_ready_count,
            capture.material_acceptance.descriptor_domain_count,
            capture
                .material_acceptance
                .visual_descriptor_field_ready_count,
            capture.material_acceptance.visual_descriptor_field_count,
            capture
                .material_acceptance
                .physical_descriptor_field_ready_count,
            capture.material_acceptance.physical_descriptor_field_count,
            capture
                .material_acceptance
                .state_channel_requirement_ready_count,
            capture.material_acceptance.state_channel_requirement_count,
            capture.material_acceptance.generated_data_type_ready_count,
            capture.material_acceptance.generated_data_type_count,
            capture
                .material_acceptance
                .capture_evidence_requirement_ready_count,
            capture
                .material_acceptance
                .capture_evidence_requirement_count,
            capture
                .material_acceptance
                .graph_output_requirement_ready_count,
            capture.material_acceptance.graph_output_requirement_count,
            capture
                .material_acceptance
                .rendering_integration_requirement_ready_count,
            capture
                .material_acceptance
                .rendering_integration_requirement_count,
            capture
                .material_acceptance
                .physics_integration_requirement_ready_count,
            capture
                .material_acceptance
                .physics_integration_requirement_count,
            capture
                .material_acceptance
                .audio_integration_requirement_ready_count,
            capture
                .material_acceptance
                .audio_integration_requirement_count,
            capture
                .material_acceptance
                .ai_gameplay_integration_requirement_ready_count,
            capture
                .material_acceptance
                .ai_gameplay_integration_requirement_count,
            capture.material_acceptance.performance_rule_ready_count,
            capture.material_acceptance.performance_rule_count,
            capture
                .material_acceptance
                .debug_view_requirement_ready_count,
            capture.material_acceptance.debug_view_requirement_count,
            capture.material_acceptance.acceptance_scene_ready_count,
            capture.material_acceptance.acceptance_scene_count,
            capture.material_acceptance.issue_count
        );
        if let Some(issue) = capture.material_acceptance.issues.first() {
            println!(
                "Tools material acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools material pipeline: {}/{} gate(s), {}/{} signal(s), descriptor {}, graph {}, cache {}, state {}, cross-system {}, fracture {}, provenance {}, rebuild/fallback {}, channels {}/{}, runtime frames {}, {} issue(s)",
            capture.material_pipeline_acceptance.passed_gate_count,
            capture.material_pipeline_acceptance.gate_count,
            capture.material_pipeline_acceptance.passed_signal_count,
            capture.material_pipeline_acceptance.signal_count,
            capture
                .material_pipeline_acceptance
                .descriptor_truth_signal_count,
            capture
                .material_pipeline_acceptance
                .procedural_graph_signal_count,
            capture
                .material_pipeline_acceptance
                .generated_cache_signal_count,
            capture
                .material_pipeline_acceptance
                .state_channel_signal_count,
            capture
                .material_pipeline_acceptance
                .cross_system_signal_count,
            capture
                .material_pipeline_acceptance
                .fracture_interior_signal_count,
            capture.material_pipeline_acceptance.provenance_signal_count,
            capture
                .material_pipeline_acceptance
                .rebuild_fallback_signal_count,
            capture
                .material_pipeline_acceptance
                .fully_linked_state_channel_count,
            capture
                .material_pipeline_acceptance
                .active_state_channel_count,
            capture
                .material_pipeline_acceptance
                .material_runtime_frame_count,
            capture.material_pipeline_acceptance.issue_count
        );
        if let Some(issue) = capture.material_pipeline_acceptance.issues.first() {
            println!(
                "Tools material pipeline issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools physics acceptance: {}/{} gate(s), shopfront {}, wetness {}, gas {}, quality {}, schema {}, replay {}, fallback {}, linked fracture {}, volumes {}, runtime frames {}, domains {}/{}, tiers {}/{}, solvers {}/{}, material coupling {}/{}, fracture outputs {}/{}, liquid {}/{}, gas req {}/{}, human physics {}/{}, AI/story {}/{}, promotion {}/{}, replay req {}/{}, debug req {}/{}, scenes {}/{}, {} issue(s)",
            capture.physics_acceptance.passed_gate_count,
            capture.physics_acceptance.gate_count,
            capture
                .physics_acceptance
                .shopfront_consequence_signal_count,
            capture.physics_acceptance.wetness_behavior_signal_count,
            capture
                .physics_acceptance
                .gas_visibility_lighting_signal_count,
            capture.physics_acceptance.quality_island_signal_count,
            capture
                .physics_acceptance
                .material_schema_delta_signal_count,
            capture.physics_acceptance.replay_debug_signal_count,
            capture.physics_acceptance.fallback_signal_count,
            capture.physics_acceptance.fully_linked_fracture_count,
            capture.physics_acceptance.consequential_volume_count,
            capture.physics_acceptance.physics_runtime_frame_count,
            capture.physics_acceptance.required_domain_ready_count,
            capture.physics_acceptance.required_domain_count,
            capture.physics_acceptance.fidelity_tier_ready_count,
            capture.physics_acceptance.fidelity_tier_count,
            capture.physics_acceptance.solver_family_ready_count,
            capture.physics_acceptance.solver_family_count,
            capture.physics_acceptance.material_coupling_ready_count,
            capture.physics_acceptance.material_coupling_count,
            capture.physics_acceptance.fracture_output_ready_count,
            capture.physics_acceptance.fracture_output_count,
            capture.physics_acceptance.liquid_requirement_ready_count,
            capture.physics_acceptance.liquid_requirement_count,
            capture.physics_acceptance.gas_requirement_ready_count,
            capture.physics_acceptance.gas_requirement_count,
            capture
                .physics_acceptance
                .human_physics_requirement_ready_count,
            capture.physics_acceptance.human_physics_requirement_count,
            capture.physics_acceptance.ai_story_event_ready_count,
            capture.physics_acceptance.ai_story_event_count,
            capture
                .physics_acceptance
                .performance_promotion_rule_ready_count,
            capture.physics_acceptance.performance_promotion_rule_count,
            capture
                .physics_acceptance
                .determinism_replay_requirement_ready_count,
            capture
                .physics_acceptance
                .determinism_replay_requirement_count,
            capture
                .physics_acceptance
                .debug_view_requirement_ready_count,
            capture.physics_acceptance.debug_view_requirement_count,
            capture.physics_acceptance.acceptance_scene_ready_count,
            capture.physics_acceptance.acceptance_scene_count,
            capture.physics_acceptance.issue_count
        );
        if let Some(issue) = capture.physics_acceptance.issues.first() {
            println!(
                "Tools physics acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools physics pipeline: {}/{} gate(s), {}/{} signal(s), solver {}, quality {}, fracture {}, liquid {}, gas {}, material {}, replay {}, fallback {}, linked fracture {}, volumes {}, runtime frames {}, {} issue(s)",
            capture.physics_pipeline_acceptance.passed_gate_count,
            capture.physics_pipeline_acceptance.gate_count,
            capture.physics_pipeline_acceptance.passed_signal_count,
            capture.physics_pipeline_acceptance.signal_count,
            capture.physics_pipeline_acceptance.solver_signal_count,
            capture
                .physics_pipeline_acceptance
                .quality_island_signal_count,
            capture.physics_pipeline_acceptance.fracture_signal_count,
            capture.physics_pipeline_acceptance.liquid_signal_count,
            capture.physics_pipeline_acceptance.gas_signal_count,
            capture
                .physics_pipeline_acceptance
                .material_state_signal_count,
            capture
                .physics_pipeline_acceptance
                .replay_debug_signal_count,
            capture.physics_pipeline_acceptance.fallback_signal_count,
            capture
                .physics_pipeline_acceptance
                .fully_linked_fracture_count,
            capture
                .physics_pipeline_acceptance
                .consequential_volume_count,
            capture
                .physics_pipeline_acceptance
                .physics_runtime_frame_count,
            capture.physics_pipeline_acceptance.issue_count
        );
        if let Some(issue) = capture.physics_pipeline_acceptance.issues.first() {
            println!(
                "Tools physics pipeline issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools human acceptance: {}/{} gate(s), face {}, eyes {}, speech {}, hair {}, skin {}, body {}, LOD {}, provenance {}, golden {}, render frames {}, synced speech {}, hero bundles {}, appearance {}, {} issue(s)",
            capture.human_acceptance.passed_gate_count,
            capture.human_acceptance.gate_count,
            capture.human_acceptance.neutral_face_signal_count,
            capture.human_acceptance.eye_lighting_signal_count,
            capture.human_acceptance.speech_sync_signal_count,
            capture.human_acceptance.hair_stability_signal_count,
            capture.human_acceptance.skin_lighting_signal_count,
            capture.human_acceptance.body_motion_signal_count,
            capture.human_acceptance.lod_stability_signal_count,
            capture.human_acceptance.provenance_signal_count,
            capture.human_acceptance.golden_scene_signal_count,
            capture.human_acceptance.human_rendering_frame_count,
            capture.human_acceptance.synchronized_speech_count,
            capture.human_acceptance.hero_bundle_request_count,
            capture.human_acceptance.appearance_state_count,
            capture.human_acceptance.issue_count
        );
        println!(
            "Tools human V9: layers {}/{}, tiers {}/{}, skin {}/{}, eyes {}/{}, mouth {}/{}, hair {}/{}, body {}/{}, clothing/cyber {}/{}, AI/voice {}/{}, generation policy {}/{}, renderer iface {}/{}, debug {}/{}, scenes {}/{}",
            capture.human_acceptance.required_layer_ready_count,
            capture.human_acceptance.required_layer_count,
            capture.human_acceptance.quality_tier_ready_count,
            capture.human_acceptance.quality_tier_count,
            capture.human_acceptance.skin_requirement_ready_count,
            capture.human_acceptance.skin_requirement_count,
            capture.human_acceptance.eye_requirement_ready_count,
            capture.human_acceptance.eye_requirement_count,
            capture.human_acceptance.mouth_requirement_ready_count,
            capture.human_acceptance.mouth_requirement_count,
            capture.human_acceptance.hair_requirement_ready_count,
            capture.human_acceptance.hair_requirement_count,
            capture.human_acceptance.body_motion_requirement_ready_count,
            capture.human_acceptance.body_motion_requirement_count,
            capture
                .human_acceptance
                .clothing_cybernetics_requirement_ready_count,
            capture
                .human_acceptance
                .clothing_cybernetics_requirement_count,
            capture.human_acceptance.ai_voice_integration_ready_count,
            capture.human_acceptance.ai_voice_integration_count,
            capture
                .human_acceptance
                .procedural_generation_policy_ready_count,
            capture.human_acceptance.procedural_generation_policy_count,
            capture.human_acceptance.renderer_interface_ready_count,
            capture.human_acceptance.renderer_interface_count,
            capture.human_acceptance.debug_view_requirement_ready_count,
            capture.human_acceptance.debug_view_requirement_count,
            capture.human_acceptance.acceptance_scene_ready_count,
            capture.human_acceptance.acceptance_scene_count
        );
        if let Some(issue) = capture.human_acceptance.issues.first() {
            println!(
                "Tools human acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools human pipeline: {}/{} gate(s), {}/{} signal(s), truth {}, face/mouth {}, skin {}, eyes {}, hair {}, body {}, LOD {}, AI/voice {}, provenance {}, render frames {}, bundles {}/{}, synced performance {}, appearance {}, {} issue(s)",
            capture.human_pipeline_acceptance.passed_gate_count,
            capture.human_pipeline_acceptance.gate_count,
            capture.human_pipeline_acceptance.passed_signal_count,
            capture.human_pipeline_acceptance.signal_count,
            capture.human_pipeline_acceptance.truth_cache_signal_count,
            capture.human_pipeline_acceptance.face_mouth_signal_count,
            capture.human_pipeline_acceptance.skin_signal_count,
            capture.human_pipeline_acceptance.eye_signal_count,
            capture.human_pipeline_acceptance.hair_signal_count,
            capture.human_pipeline_acceptance.body_motion_signal_count,
            capture.human_pipeline_acceptance.lod_signal_count,
            capture.human_pipeline_acceptance.ai_voice_signal_count,
            capture.human_pipeline_acceptance.provenance_signal_count,
            capture
                .human_pipeline_acceptance
                .human_rendering_frame_count,
            capture
                .human_pipeline_acceptance
                .runtime_bundle_request_count,
            capture
                .human_pipeline_acceptance
                .renderer_detail_request_count,
            capture
                .human_pipeline_acceptance
                .synchronized_performance_count,
            capture
                .human_pipeline_acceptance
                .responsive_appearance_count,
            capture.human_pipeline_acceptance.issue_count
        );
        if let Some(issue) = capture.human_pipeline_acceptance.issues.first() {
            println!(
                "Tools human pipeline issue: {} - {}",
                issue.subject, issue.message
            );
        }
        if let Some(frame_graph) = capture.frame_graphs.first() {
            let reference_frame_graph = capture
                .frame_graphs
                .iter()
                .find(|candidate| !candidate.reference_render.is_empty())
                .unwrap_or(frame_graph);
            println!(
                "Tools frame graph: {} passes ({} graphics, {} compute, {} transfer), {} owned, {} costed, {} dependency hint(s), {} barriers, {} resources, {} bindless, {:.2} ms wall",
                frame_graph.pass_count,
                frame_graph.graphics_passes,
                frame_graph.compute_passes,
                frame_graph.transfer_passes,
                frame_graph.owned_pass_count,
                frame_graph.expected_cost_pass_count,
                frame_graph.dependency_hint_count,
                frame_graph.barrier_count,
                frame_graph.registered_resource_count,
                frame_graph.bindless_binding_count,
                frame_graph.estimated_wall_milliseconds
            );
            println!(
                "Tools GPU pressure: {:.2}% descriptors, {:.2}% bindless, {:.2} MiB streamed, {:.2} MiB transient, {} access record(s), {} queue report(s)",
                frame_graph.descriptor_pressure.descriptor_pressure_percent,
                frame_graph.descriptor_pressure.bindless_pressure_percent,
                frame_graph.residency.streamed_resident_bytes as f64 / (1024.0 * 1024.0),
                frame_graph.residency.transient_bytes as f64 / (1024.0 * 1024.0),
                frame_graph.access_summary.access_count,
                frame_graph.queue_occupancy.len()
            );
            if let Some(queue) = frame_graph.queue_occupancy.first() {
                println!(
                    "Tools GPU queue {:?}: {} pass(es), {:.2}% occupancy, {:.2} ms idle, {} wait(s)",
                    queue.queue,
                    queue.pass_count,
                    queue.occupancy_percent,
                    queue.idle_milliseconds,
                    queue.wait_dependency_count
                );
            }
            println!(
                "Tools GPU shaders: {} pipeline(s), {} variant(s), {} contract(s), {} validated, {} warning(s), {} error(s), {} hot-reloadable",
                frame_graph.shader_pipeline.pipeline_count,
                frame_graph.shader_pipeline.shader_variant_count,
                frame_graph.shader_pipeline.shader_contract_count,
                frame_graph.shader_pipeline.validated_contract_count,
                frame_graph.shader_pipeline.validation_warning_count,
                frame_graph.shader_pipeline.validation_error_count,
                frame_graph.shader_pipeline.hot_reload_pipeline_count
            );
            println!(
                "Tools GPU capabilities: {} Vulkan {}, tier {:?}, {} supported feature(s), {} fallback(s), {} missing, {:.2} GiB device memory, {} descriptors",
                frame_graph.capabilities.device_name,
                frame_graph.capabilities.vulkan_version,
                frame_graph.capabilities.selected_tier,
                frame_graph.capabilities.supported_feature_count,
                frame_graph.capabilities.fallback_feature_count,
                frame_graph.capabilities.missing_required_feature_count,
                frame_graph.capabilities.total_device_memory_bytes as f64
                    / (1024.0 * 1024.0 * 1024.0),
                frame_graph.capabilities.max_descriptor_count
            );
            if let Some(top_pass) = frame_graph.top_passes.first() {
                println!(
                    "Tools slowest pass: {} on {:?} at {:.2} ms",
                    top_pass.name, top_pass.queue, top_pass.estimated_gpu_milliseconds
                );
            }
            println!(
                "Tools virtual shadows: table {}, dirty {}, atlas {}, capacity {} page(s), dirty capacity {} page(s), issue(s) {}",
                frame_graph.virtual_shadow_pages.page_table_present,
                frame_graph.virtual_shadow_pages.dirty_page_list_present,
                frame_graph.virtual_shadow_pages.shadow_atlas_present,
                frame_graph.virtual_shadow_pages.estimated_page_capacity,
                frame_graph.virtual_shadow_pages.dirty_page_capacity,
                frame_graph.virtual_shadow_pages.issue_count
            );
            println!(
                "Tools camera/color: physical {}, HDR {}, exposure {}, lens {}, bloom {}, DoF {}, motion blur {}, LUT {}, filmic {}, issue(s) {}",
                frame_graph.camera_color.physical_camera_buffer_present,
                frame_graph.camera_color.hdr_lighting_target_present,
                frame_graph.camera_color.exposure_pass_present
                    && frame_graph.camera_color.exposure_adaptation_present,
                frame_graph.camera_color.lens_effects_pass_present,
                frame_graph.camera_color.bloom_pyramid_present,
                frame_graph.camera_color.depth_of_field_target_present,
                frame_graph.camera_color.motion_blur_target_present,
                frame_graph.camera_color.color_grading_lut_present,
                frame_graph
                    .camera_color
                    .post_pipeline_has_filmic_tonemap_define,
                frame_graph.camera_color.issue_count
            );
            println!(
                "Tools temporal stability: masks {}, reconstruct {}, reactive {}, disocclusion {}, history {}, upscaled {}, ghost debug {}, UI upscale {}, issue(s) {}",
                frame_graph.temporal_stability.mask_pass_present,
                frame_graph.temporal_stability.reconstruction_pass_present,
                frame_graph.temporal_stability.reactive_mask_present,
                frame_graph.temporal_stability.disocclusion_mask_present,
                frame_graph.temporal_stability.history_input_present
                    && frame_graph.temporal_stability.history_output_present,
                frame_graph.temporal_stability.upscaled_target_present,
                frame_graph
                    .temporal_stability
                    .instability_debug_buffer_present,
                frame_graph.temporal_stability.ui_reads_upscaled_target,
                frame_graph.temporal_stability.issue_count
            );
            println!(
                "Tools render outputs: picking {}, readback {}, perception {}, capture {}, screenshots {}, cinematic {}, replay metadata {}, issue(s) {}",
                frame_graph.render_outputs.picking_pass_present
                    && frame_graph.render_outputs.picking_buffer_present,
                frame_graph.render_outputs.picking_readback_present,
                frame_graph.render_outputs.perception_pass_present
                    && frame_graph
                        .render_outputs
                        .perception_entity_id_buffer_present
                    && frame_graph
                        .render_outputs
                        .perception_material_id_buffer_present
                    && frame_graph.render_outputs.perception_depth_pyramid_present
                    && frame_graph.render_outputs.perception_hazard_buffer_present,
                frame_graph.render_outputs.capture_export_pass_present,
                frame_graph.render_outputs.screenshot_target_present
                    && frame_graph.render_outputs.capture_writes_screenshot,
                frame_graph.render_outputs.cinematic_target_present
                    && frame_graph.render_outputs.capture_writes_cinematic,
                frame_graph.render_outputs.frame_capture_manifest_present
                    && frame_graph.render_outputs.capture_writes_manifest,
                frame_graph.render_outputs.issue_count
            );
            println!(
                "Tools human rendering: skin {}, eyes {}, hair {}, clothing {}, cybernetics {}, closeup {}, lighting {}, issue(s) {}",
                frame_graph.human_rendering.skin_pass_present
                    && frame_graph
                        .human_rendering
                        .skin_pipeline_has_subsurface_define,
                frame_graph.human_rendering.eye_pass_present
                    && frame_graph.human_rendering.eye_pipeline_has_cornea_define,
                frame_graph.human_rendering.hair_pass_present
                    && frame_graph
                        .human_rendering
                        .hair_pipeline_has_hybrid_lod_define,
                frame_graph
                    .human_rendering
                    .clothing_cybernetics_pass_present
                    && frame_graph
                        .human_rendering
                        .clothing_pipeline_has_motion_define,
                frame_graph
                    .human_rendering
                    .cybernetic_material_table_present
                    && frame_graph
                        .human_rendering
                        .clothing_pipeline_has_cybernetic_define,
                frame_graph.human_rendering.composite_pass_present
                    && frame_graph
                        .human_rendering
                        .composite_pipeline_has_closeup_define,
                frame_graph.human_rendering.lighting_reads_composite,
                frame_graph.human_rendering.issue_count
            );
            println!(
                "Tools reference render: path {}, compare {}, radiance {}, metrics {}, compute fallback {}, hardware RT {}, issue(s) {}",
                reference_frame_graph
                    .reference_render
                    .path_tracing_pass_present,
                reference_frame_graph
                    .reference_render
                    .comparison_pass_present,
                reference_frame_graph
                    .reference_render
                    .radiance_target_present,
                reference_frame_graph
                    .reference_render
                    .metrics_buffer_present,
                reference_frame_graph.reference_render.compute_fallback_used,
                reference_frame_graph
                    .reference_render
                    .hardware_ray_tracing_used,
                reference_frame_graph.reference_render.issue_count
            );
            println!(
                "Tools material runtime: programs {}, cache {}, virtual pages {}, feedback {}, resolve {}, page pass {}, lighting {}, aged cache {}, aged pass {}, aged writes {}, capacity {} program(s), {} cache binding(s), {} page(s), issue(s) {}",
                frame_graph.material_runtime.runtime_program_table_present,
                frame_graph.material_runtime.cache_residency_table_present,
                frame_graph.material_runtime.virtual_page_table_present,
                frame_graph.material_runtime.virtual_feedback_buffer_present,
                frame_graph.material_runtime.material_resolve_pass_present,
                frame_graph
                    .material_runtime
                    .virtual_page_update_pass_present,
                frame_graph.material_runtime.lighting_pass_present,
                frame_graph
                    .material_runtime
                    .aged_surface_cache_resource_present,
                frame_graph
                    .material_runtime
                    .aged_surface_cache_update_pass_present,
                frame_graph
                    .material_runtime
                    .aged_surface_cache_writes_output,
                frame_graph.material_runtime.program_capacity,
                frame_graph.material_runtime.cache_binding_capacity,
                frame_graph.material_runtime.virtual_page_capacity,
                frame_graph.material_runtime.issue_count
            );
            println!(
                "Tools micro-geometry: clusters {}, visible {}, indirect {}, feedback {}, worldgen {}, capacity {} cluster(s), issue(s) {}",
                frame_graph.micro_geometry.cluster_table_present,
                frame_graph.micro_geometry.visible_cluster_buffer_present,
                frame_graph.micro_geometry.indirect_draw_buffer_present,
                frame_graph
                    .micro_geometry
                    .renderer_streaming_feedback_present,
                frame_graph.micro_geometry.worldgen_visibility_pass_present,
                frame_graph.micro_geometry.cluster_capacity,
                frame_graph.micro_geometry.issue_count
            );
        }
        println!(
            "Tools AI/story: {} decisions, {} memories, {} accepted intents, {} rejected intents, {} dialogue lines, {} story events, {} faction events",
            capture.ai_story.agent_decisions.len(),
            capture.ai_story.agent_memories.len(),
            capture.ai_story.accepted_intent_count,
            capture.ai_story.rejected_intent_count,
            capture.ai_story.dialogues.len(),
            capture.ai_story.story_events.len(),
            capture.ai_story.faction_events.len()
        );
        println!(
            "Tools AI acceptance: {}/{} gate(s), shopfront {}, perception {}, dialogue {}, validation {}, pressure {}, tools {}, memory {}, voice {}, decisions {}, memories {}, intents {}/{}, grounded dialogue {}, story {}, faction {}, {} issue(s)",
            capture.ai_acceptance.passed_gate_count,
            capture.ai_acceptance.gate_count,
            capture.ai_acceptance.shopfront_reaction_signal_count,
            capture.ai_acceptance.limited_perception_signal_count,
            capture.ai_acceptance.dialogue_grounding_signal_count,
            capture.ai_acceptance.action_validation_signal_count,
            capture.ai_acceptance.story_pressure_signal_count,
            capture.ai_acceptance.decision_tool_signal_count,
            capture.ai_acceptance.memory_audit_signal_count,
            capture.ai_acceptance.voice_handoff_signal_count,
            capture.ai_acceptance.agent_decision_count,
            capture.ai_acceptance.memory_update_count,
            capture.ai_acceptance.accepted_intent_count,
            capture.ai_acceptance.rejected_intent_count,
            capture.ai_acceptance.grounded_dialogue_count,
            capture.ai_acceptance.story_event_count,
            capture.ai_acceptance.faction_event_count,
            capture.ai_acceptance.issue_count
        );
        if let Some(issue) = capture.ai_acceptance.issues.first() {
            println!(
                "Tools AI acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools AI pipeline: {}/{} gate(s), {}/{} signal(s), authority {}, perception {}, agent {}, intents {}, dialogue {}, story {}, replay {}, voice {}, decisions {}, sourced memories {}, sourced decisions {}, grounded dialogue {}, intents {}/{}, pressure {}, {} issue(s)",
            capture.ai_pipeline_acceptance.passed_gate_count,
            capture.ai_pipeline_acceptance.gate_count,
            capture.ai_pipeline_acceptance.passed_signal_count,
            capture.ai_pipeline_acceptance.signal_count,
            capture.ai_pipeline_acceptance.world_authority_signal_count,
            capture.ai_pipeline_acceptance.perception_signal_count,
            capture.ai_pipeline_acceptance.agent_model_signal_count,
            capture
                .ai_pipeline_acceptance
                .intent_validation_signal_count,
            capture.ai_pipeline_acceptance.dialogue_safety_signal_count,
            capture.ai_pipeline_acceptance.story_director_signal_count,
            capture.ai_pipeline_acceptance.replay_audit_signal_count,
            capture.ai_pipeline_acceptance.voice_handoff_signal_count,
            capture.ai_pipeline_acceptance.agent_decision_count,
            capture.ai_pipeline_acceptance.sourced_memory_count,
            capture.ai_pipeline_acceptance.sourced_decision_count,
            capture.ai_pipeline_acceptance.grounded_dialogue_count,
            capture.ai_pipeline_acceptance.accepted_intent_count,
            capture.ai_pipeline_acceptance.rejected_intent_count,
            capture.ai_pipeline_acceptance.story_pressure_event_count,
            capture.ai_pipeline_acceptance.issue_count
        );
        if let Some(issue) = capture.ai_pipeline_acceptance.issues.first() {
            println!(
                "Tools AI pipeline issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools AI rumor network: {} pass(es), {} table resource(s), {} propagation resource(s), {} bytes, viewer {}",
            capture.ai_story.rumor_network.propagation_pass_count,
            capture.ai_story.rumor_network.grounded_table_resource_count,
            capture
                .ai_story
                .rumor_network
                .propagation_buffer_resource_count,
            capture.ai_story.rumor_network.estimated_rumor_bytes,
            capture.ai_story.rumor_network.viewer_available
        );
        println!(
            "Tools AI opportunities: {} pass(es), {} table resource(s), {} index resource(s), {} bytes, viewer {}",
            capture.ai_story.opportunity_network.index_pass_count,
            capture
                .ai_story
                .opportunity_network
                .opportunity_table_resource_count,
            capture
                .ai_story
                .opportunity_network
                .opportunity_index_resource_count,
            capture
                .ai_story
                .opportunity_network
                .estimated_opportunity_bytes,
            capture.ai_story.opportunity_network.viewer_available
        );
        println!(
            "Tools AI conversation replay: {} line(s), {} grounded, {} source event(s), {} fully voiced, passed {}",
            capture.ai_story.conversation_replay.line_count,
            capture.ai_story.conversation_replay.grounded_line_count,
            capture
                .ai_story
                .conversation_replay
                .linked_source_event_count,
            capture.ai_story.conversation_replay.fully_voiced_line_count,
            capture.ai_story.conversation_replay.passed
        );
        if let Some(grounding) = capture.ai_story.dialogue_grounding.first() {
            println!(
                "Tools AI dialogue grounding entity {}: {} source event(s), {} memory link(s), unsupported {}",
                grounding.speaker,
                grounding.source_events.len(),
                grounding.linked_memory_count,
                grounding.unsupported_claim_count
            );
        }
        if let Some(summary) = capture.ai_story.agent_summaries.first() {
            println!(
                "Tools AI agent {}: {} decision(s), {} memory update(s), latest goal: {}",
                summary.agent,
                summary.decision_count,
                summary.memory_count,
                summary.latest_goal.as_deref().unwrap_or("none")
            );
        }
        println!(
            "Tools voice/audio: {} speech event(s), {} synced, {} unsynced, {} sound event(s), {} material-aware, active {}, capture {}, capture memory {} bytes, max lip error {:.3}s, peak mix {:.2}, comfort {:.2}, policy {}, {} issue(s)",
            capture.voice_audio.speech_count,
            capture.voice_audio.synced_speech_count,
            capture.voice_audio.unsynced_speech_count,
            capture.voice_audio.sound_events.len(),
            capture.voice_audio.material_aware_sound_count,
            capture.voice_audio.performance.active_sound_voice_count,
            capture.voice_audio.performance.performance_capture_count,
            capture.voice_audio.performance.capture_memory_bytes,
            capture.voice_audio.performance.max_lip_sync_error_seconds,
            capture.voice_audio.peak_mix_intensity,
            capture.voice_audio.comfort.projected_ten_minute_score,
            capture.voice_audio.performance.passed,
            capture.voice_audio.issues.len()
        );
        println!(
            "Tools voice acceptance: {}/{} gate(s), hero {}, fracture {}, material {}, nonblocking {}, personas {}, spatial {}, provenance {}, speech {}, synced {}, physical {}, material-aware {}, personas {}, captures {}/{}, soundscape {}, {} issue(s)",
            capture.voice_acceptance.passed_gate_count,
            capture.voice_acceptance.gate_count,
            capture.voice_acceptance.hero_speech_timing_signal_count,
            capture.voice_acceptance.fracture_audio_ai_signal_count,
            capture.voice_acceptance.material_state_audio_signal_count,
            capture.voice_acceptance.nonblocking_generation_signal_count,
            capture.voice_acceptance.persona_consistency_signal_count,
            capture.voice_acceptance.spatial_audio_signal_count,
            capture.voice_acceptance.provenance_signal_count,
            capture.voice_acceptance.speech_event_count,
            capture.voice_acceptance.synced_speech_count,
            capture.voice_acceptance.physical_sound_count,
            capture.voice_acceptance.material_aware_sound_count,
            capture.voice_acceptance.voice_persona_count,
            capture.voice_acceptance.validated_performance_capture_count,
            capture.voice_acceptance.performance_capture_count,
            capture.voice_acceptance.soundscape_layer_count,
            capture.voice_acceptance.issue_count
        );
        if let Some(issue) = capture.voice_acceptance.issues.first() {
            println!(
                "Tools voice acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools voice pipeline: {}/{} gate(s), {}/{} signal(s), speech {}, physical {}, material {}, latency {}, persona {}, spatial {}, provenance {}, captures {}/{}, phonemes {}, visemes {}, spatial sounds {}, heard {}, soundscape {}, {} issue(s)",
            capture.voice_pipeline_acceptance.passed_gate_count,
            capture.voice_pipeline_acceptance.gate_count,
            capture.voice_pipeline_acceptance.passed_signal_count,
            capture.voice_pipeline_acceptance.signal_count,
            capture.voice_pipeline_acceptance.speech_timing_signal_count,
            capture
                .voice_pipeline_acceptance
                .physical_audio_signal_count,
            capture
                .voice_pipeline_acceptance
                .material_foley_signal_count,
            capture.voice_pipeline_acceptance.latency_signal_count,
            capture.voice_pipeline_acceptance.persona_signal_count,
            capture
                .voice_pipeline_acceptance
                .spatial_acoustic_signal_count,
            capture
                .voice_pipeline_acceptance
                .provenance_debug_signal_count,
            capture
                .voice_pipeline_acceptance
                .validated_performance_capture_count,
            capture.voice_pipeline_acceptance.performance_capture_count,
            capture.voice_pipeline_acceptance.phoneme_sample_count,
            capture.voice_pipeline_acceptance.viseme_sample_count,
            capture.voice_pipeline_acceptance.spatial_sound_count,
            capture.voice_pipeline_acceptance.ai_heard_event_count,
            capture.voice_pipeline_acceptance.soundscape_layer_count,
            capture.voice_pipeline_acceptance.issue_count
        );
        if let Some(issue) = capture.voice_pipeline_acceptance.issues.first() {
            println!(
                "Tools voice pipeline issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools soundscape: {} layer(s), {} event-driven, weather {}, power {:.2}, alert {:.2}, tension {:.2}, duck {:.2}, score {} stem(s), transition {}, radio {}, diegetic {}, score passed {}, passed {}",
            capture.voice_audio.soundscape.layer_count,
            capture.voice_audio.soundscape.event_driven_layer_count,
            capture.voice_audio.soundscape.weather,
            capture.voice_audio.soundscape.power_level,
            capture.voice_audio.soundscape.alertness,
            capture.voice_audio.soundscape.tension_score,
            capture.voice_audio.soundscape.music_ducking,
            capture.voice_audio.soundscape.adaptive_score_stem_count,
            capture.voice_audio.soundscape.score_transition,
            capture.voice_audio.soundscape.radio_station_enabled,
            capture.voice_audio.soundscape.diegetic_music_source_count,
            capture.voice_audio.soundscape.score_validation_passed,
            capture.voice_audio.soundscape.validation_passed
        );
        if let Some(sync) = capture.voice_audio.face_sync.first() {
            println!(
                "Tools voice sync entity {}: {} viseme(s), face delta {:.3}s, synchronized {}",
                sync.speaker,
                sync.speech_viseme_count,
                sync.duration_delta_seconds,
                sync.synchronized
            );
        }
        println!(
            "Tools human: {} dialogue(s), {} speech performance(s), {} synced face(s), {} appearance update(s), {} bundle request(s), {} issue(s)",
            capture.human_lab.dialogue_count,
            capture.human_lab.speech_performance_count,
            capture.human_lab.synchronized_face_count,
            capture.human_lab.appearance_update_count,
            capture.human_lab.bundle_request_count,
            capture.human_lab.issues.len()
        );
        if let Some(performance) = capture.human_lab.speech_performances.first() {
            println!(
                "Tools human speech entity {}: {} viseme(s), face delta {}, bundle {}, synchronized {}",
                performance.speaker,
                performance.speech_viseme_count,
                performance
                    .duration_delta_seconds
                    .map(|delta| format!("{delta:.3}s"))
                    .unwrap_or_else(|| "missing".to_string()),
                performance.has_bundle_request,
                performance.synchronized
            );
        }
        println!(
            "Tools cinematic: {} beat(s), {} track(s), physics {}, material {}, voice {}, facial {}, AI/story {}, streaming {}, {} issue(s)",
            capture.cinematic_timeline.beat_count,
            capture.cinematic_timeline.track_count,
            capture.cinematic_timeline.physics_beat_count,
            capture.cinematic_timeline.material_beat_count,
            capture.cinematic_timeline.voice_beat_count,
            capture.cinematic_timeline.facial_beat_count,
            capture.cinematic_timeline.ai_story_beat_count,
            capture.cinematic_timeline.asset_streaming_beat_count,
            capture.cinematic_timeline.issues.len()
        );
        if let Some(beat) = capture.cinematic_timeline.beats.first() {
            println!(
                "Tools cinematic first beat tick {} {:?}: {}",
                beat.tick, beat.track, beat.label
            );
        }
        println!(
            "Tools physics: {} physical event(s), {} fracture(s), {} material update(s), {} mesh replacement(s), {} physics sound(s), {} fluid, {} gas, {} issue(s)",
            capture.physics_lab.physical_event_count,
            capture.physics_lab.fracture_count,
            capture.physics_lab.material_update_count,
            capture.physics_lab.mesh_replacement_count,
            capture.physics_lab.physics_sound_count,
            capture.physics_lab.fluid_event_count,
            capture.physics_lab.gas_event_count,
            capture.physics_lab.issues.len()
        );
        println!(
            "Tools physics LOD: {} pass(es), {} signal resource(s), {} decision resource(s), {} decision bytes, viewer {}",
            capture.physics_lab.simulation_lod.lod_pass_count,
            capture.physics_lab.simulation_lod.lod_signal_resource_count,
            capture
                .physics_lab
                .simulation_lod
                .lod_decision_resource_count,
            capture.physics_lab.simulation_lod.estimated_decision_bytes,
            capture.physics_lab.simulation_lod.viewer_available
        );
        if let Some(fracture) = capture.physics_lab.fractures.first() {
            println!(
                "Tools fracture entity {}: material {}, mesh {}, audio {}, AI/story {}, streaming {}",
                fracture.entity,
                fracture.material_updated,
                fracture.mesh_replaced,
                fracture.audio_emitted,
                fracture.ai_or_story_reacted,
                fracture.asset_streaming_requested
            );
        }
        println!(
            "Tools material: {} update(s), {} cache request(s), {} damage, {} wetness, {} contamination, {} renderer link(s), {} audio link(s), {} issue(s)",
            capture.material_lab.material_update_count,
            capture.material_lab.cache_request_count,
            capture.material_lab.damage_update_count,
            capture.material_lab.wetness_update_count,
            capture.material_lab.contamination_update_count,
            capture.material_lab.render_link_count,
            capture.material_lab.audio_link_count,
            capture.material_lab.issues.len()
        );
        if let Some(update) = capture.material_lab.material_updates.first() {
            println!(
                "Tools material entity {}: {:?}, cache {}, render {}, audio {}, physics {}",
                update.entity,
                update.category,
                update.has_cache_request,
                update.has_render_link,
                update.has_audio_link,
                update.has_physics_cause
            );
        }
        println!(
            "Tools city: {} district(s), {} material variant(s), {} assigned, {} streaming cell(s), {} package(s), {} manifest(s), {} manifest error(s), {} faction(s), {} story opportunities, {} crowd rule(s), {} traffic rule(s), {} expected crowd, {} transit, {} population summary passed, {} population summary issue(s), {:.2} population activity, {} unique dependencies, {} material cache ({:.2} MiB)",
            capture.city_generation.district_count,
            capture.city_generation.material_variant_count,
            capture.city_generation.assigned_material_variant_count,
            capture.city_generation.streaming_cell_count,
            capture.city_generation.city_cell_package_count,
            capture.city_generation.city_cell_manifest_count,
            capture
                .city_generation
                .city_cell_manifest_validation_error_count,
            capture.city_generation.faction_count,
            capture.city_generation.story_seed_count,
            capture.city_generation.crowd_spawn_rule_count,
            capture.city_generation.traffic_rule_count,
            capture.city_generation.expected_background_crowd_count,
            capture.city_generation.expected_vehicle_or_transit_count,
            capture
                .city_generation
                .population_summary_validation_passed_count,
            capture.city_generation.population_summary_issue_count,
            capture.city_generation.average_population_activity,
            capture.city_generation.unique_streaming_dependency_count,
            capture.city_generation.material_cache_dependency_count,
            capture.city_generation.estimated_streaming_memory_bytes as f64 / (1024.0 * 1024.0)
        );
        println!(
            "Tools city acceptance: {}/{} gate(s), rainy {}, infrastructure {}, identity {}, persistence {}, population {}, budget {}, story {}, districts {}, cells {}/{}, deps {}, requested {:.2} MiB, persistent {}, population summaries {}, story hooks {}, {} issue(s)",
            capture.city_world_acceptance.passed_gate_count,
            capture.city_world_acceptance.gate_count,
            capture
                .city_world_acceptance
                .rainy_alley_streaming_signal_count,
            capture
                .city_world_acceptance
                .infrastructure_consequence_signal_count,
            capture.city_world_acceptance.district_identity_signal_count,
            capture.city_world_acceptance.persistence_signal_count,
            capture
                .city_world_acceptance
                .population_identity_signal_count,
            capture.city_world_acceptance.streaming_budget_signal_count,
            capture
                .city_world_acceptance
                .grounded_story_hook_signal_count,
            capture.city_world_acceptance.district_count,
            capture.city_world_acceptance.loaded_cell_count,
            capture.city_world_acceptance.streaming_cell_count,
            capture
                .city_world_acceptance
                .unique_streaming_dependency_count,
            capture.city_world_acceptance.requested_streaming_bytes as f64 / (1024.0 * 1024.0),
            capture.city_world_acceptance.persistent_event_count,
            capture.city_world_acceptance.population_summary_count,
            capture.city_world_acceptance.story_hook_count,
            capture.city_world_acceptance.issue_count
        );
        if let Some(issue) = capture.city_world_acceptance.issues.first() {
            println!(
                "Tools city acceptance issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools city pipeline: {}/{} gate(s), {}/{} signal(s), truth {}, district {}, infra {}, streaming {}, procedural {}, persistence/nav {}, story/budget {}, districts {}, cells {}/{}, infra {}, NPC seeds {}, persistent {}, route updates {}, story hooks {}, requested {:.2} MiB, estimated {:.2} MiB, {} issue(s)",
            capture.city_world_pipeline_acceptance.passed_gate_count,
            capture.city_world_pipeline_acceptance.gate_count,
            capture.city_world_pipeline_acceptance.passed_signal_count,
            capture.city_world_pipeline_acceptance.signal_count,
            capture
                .city_world_pipeline_acceptance
                .truth_stream_signal_count,
            capture
                .city_world_pipeline_acceptance
                .district_identity_signal_count,
            capture
                .city_world_pipeline_acceptance
                .infrastructure_signal_count,
            capture
                .city_world_pipeline_acceptance
                .streaming_cell_signal_count,
            capture
                .city_world_pipeline_acceptance
                .procedural_content_signal_count,
            capture
                .city_world_pipeline_acceptance
                .persistence_navigation_signal_count,
            capture
                .city_world_pipeline_acceptance
                .story_budget_signal_count,
            capture.city_world_pipeline_acceptance.district_count,
            capture.city_world_pipeline_acceptance.loaded_cell_count,
            capture.city_world_pipeline_acceptance.streaming_cell_count,
            capture
                .city_world_pipeline_acceptance
                .infrastructure_system_count,
            capture.city_world_pipeline_acceptance.npc_seed_count,
            capture
                .city_world_pipeline_acceptance
                .persistent_event_count,
            capture
                .city_world_pipeline_acceptance
                .navigation_route_update_count,
            capture.city_world_pipeline_acceptance.story_hook_count,
            capture
                .city_world_pipeline_acceptance
                .requested_streaming_bytes as f64
                / (1024.0 * 1024.0),
            capture
                .city_world_pipeline_acceptance
                .estimated_streaming_memory_bytes as f64
                / (1024.0 * 1024.0),
            capture.city_world_pipeline_acceptance.issue_count
        );
        if let Some(issue) = capture.city_world_pipeline_acceptance.issues.first() {
            println!(
                "Tools city pipeline issue: {} - {}",
                issue.subject, issue.message
            );
        }
        println!(
            "Tools city material packages: {} descriptor(s), {} validated, {} capture-backed, {} scan set(s), {} state response(s), {:.2} avg confidence",
            capture.city_generation.material_package_descriptor_count,
            capture
                .city_generation
                .material_package_validation_passed_count,
            capture
                .city_generation
                .material_package_capture_evidence_count,
            capture.city_generation.material_package_scan_set_count,
            capture
                .city_generation
                .material_package_state_response_count,
            capture
                .city_generation
                .average_material_package_capture_confidence
        );
        if let Some(cell) = capture.city_generation.highest_pressure_cell() {
            println!(
                "Tools city hottest cell {}: {} entities, {} NPC seed(s), {} crowd, {} transit, {} dependency edges, {} material cache ({:.2} MiB)",
                cell.chunk,
                cell.entity_count,
                cell.npc_seed_count,
                cell.expected_background_crowd_count,
                cell.expected_vehicle_or_transit_count,
                cell.dependency_count,
                cell.material_cache_dependency_count,
                cell.estimated_streaming_bytes as f64 / (1024.0 * 1024.0)
            );
        }
        if let Some(district) = capture.city_generation.districts.first() {
            println!(
                "Tools city district {}: surveillance {:.2}, crime {:.2}, pollution {:.2}, crowd {}, traffic {}",
                district.name,
                district.surveillance_level,
                district.crime_pressure,
                district.pollution,
                district.expected_background_crowd_count,
                district.expected_vehicle_or_transit_count
            );
        }
        println!(
            "Tools city streaming plan: {} cell(s), {} loaded, hero {}, render {}, gameplay {}, summary {}, {} unique requested asset(s), {:.2} MiB requested",
            capture.city_streaming.cell_count,
            capture.city_streaming.loaded_cell_count,
            capture.city_streaming.hero_loaded_count,
            capture.city_streaming.render_high_detail_count,
            capture.city_streaming.gameplay_loaded_count,
            capture.city_streaming.summary_loaded_count,
            capture.city_streaming.unique_requested_asset_count,
            capture.city_streaming.requested_streaming_bytes as f64 / (1024.0 * 1024.0)
        );
        if let Some(cell) = capture.city_streaming.highest_priority_cell() {
            println!(
                "Tools city stream cell {}: {:?}, priority {:.2}, {} requested asset(s), package {}, story {}, visible {}, route {}",
                cell.chunk,
                cell.state,
                cell.priority_score,
                cell.requested_asset_count,
                cell.package_requested,
                cell.story_focus,
                cell.renderer_visible,
                cell.predicted_route_match
            );
        }
        println!(
            "Tools city infrastructure: {:?} severity {:.2}, {} cell(s), {} node(s), {} light(s), {} door(s), {} camera(s), {} NPC(s), {} audio zone(s), {} route location(s), {} story seed(s)",
            capture.city_infrastructure.system,
            capture.city_infrastructure.severity,
            capture.city_infrastructure.affected_cell_count,
            capture.city_infrastructure.affected_node_count,
            capture.city_infrastructure.affected_light_count,
            capture.city_infrastructure.affected_door_count,
            capture.city_infrastructure.affected_camera_count,
            capture.city_infrastructure.affected_npc_count,
            capture.city_infrastructure.affected_audio_zone_count,
            capture
                .city_infrastructure
                .affected_navigation_location_count,
            capture.city_infrastructure.story_seed_count
        );
        if let Some(cell) = capture.city_infrastructure.highest_impact_cell() {
            println!(
                "Tools city infrastructure cell {}: {} entities, {} lights, {} cameras, {} NPCs, {} audio zones, {} route locations, {} deps",
                cell.chunk,
                cell.entity_count,
                cell.light_count,
                cell.camera_count,
                cell.npc_count,
                cell.audio_zone_count,
                cell.navigation_location_count,
                cell.streaming_dependency_count
            );
        }
        println!(
            "Tools city persistence: {} cell(s), {} persistent event(s), {} material override(s), {} damaged, {} infrastructure delta(s), {} faction delta(s), {} story ref(s), save required {}",
            capture.city_persistence.cell_count,
            capture.city_persistence.persistent_event_count,
            capture.city_persistence.material_override_count,
            capture.city_persistence.damaged_entity_count,
            capture.city_persistence.infrastructure_delta_count,
            capture.city_persistence.faction_delta_count,
            capture.city_persistence.story_thread_ref_count,
            capture.city_persistence.save_required
        );
        if let Some(cell) = capture.city_persistence.most_changed_cell() {
            println!(
                "Tools city dirty cell {}: priority {:?}, {} change(s), {} event(s), systems {:?}",
                cell.chunk,
                cell.save_priority,
                cell.change_count,
                cell.event_history_count,
                cell.infrastructure_systems
            );
        }
        println!(
            "Tools city navigation: {} cell(s), {} changed, {} node(s), {} edge(s), {} route update(s), {} blocked, {} dangerous, {} restricted, {} partially unreachable",
            capture.city_navigation.cell_count,
            capture.city_navigation.changed_cell_count,
            capture.city_navigation.total_node_count,
            capture.city_navigation.total_edge_count,
            capture.city_navigation.route_update_count,
            capture.city_navigation.blocked_route_count,
            capture.city_navigation.dangerous_route_count,
            capture.city_navigation.restricted_route_count,
            capture.city_navigation.partially_unreachable_cell_count
        );
        if let Some(cell) = capture.city_navigation.most_changed_cell() {
            println!(
                "Tools navigation cell {}: {} route update(s), {} blocker(s), {} danger field(s), reachable {}/{}",
                cell.chunk,
                cell.route_update_count,
                cell.dynamic_blocker_count,
                cell.danger_field_count,
                cell.reachable_important_location_count,
                cell.important_location_count
            );
        }
    }

    for event in &report.events {
        match &event.kind {
            WorldEventKind::DialogueEmitted { speaker, text, .. } => {
                println!("event: dialogue from entity {speaker}: {text}");
            }
            WorldEventKind::AgentDecisionExplained {
                agent,
                goal,
                accepted_actions,
                rejected_actions,
                ..
            } => {
                println!(
                    "event: AI decision entity {agent}: {goal} accepted [{}], rejected {}",
                    accepted_actions.join(","),
                    rejected_actions.len()
                );
            }
            other => println!("event: {:?}", other),
        }
    }

    Ok(())
}

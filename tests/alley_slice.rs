use ashfall::assets::AssetLoadState;
use ashfall::core::{PerformanceCounters, QualityTier, SchemaVersion, Vec3};
use ashfall::gpu::{GpuBackend, GpuBarrierReason};
use ashfall::modules::ai::{ALLEY_SECURITY_FACTION_ID, inspect_agent_decisions_from_ledger};
use ashfall::modules::materials::{
    ASSET_GLASS_CRACK_DETAIL_CACHE, ASSET_WET_ASPHALT_REFLECTION_CACHE,
};
use ashfall::modules::rendering::{build_render_packet, render_output_from_packet};
use ashfall::modules::worldgen::{
    ALLEY_ASPHALT_ID, ALLEY_GLASS_FRACTURED_MESH_ASSET, ALLEY_MARA_HUMAN_BUNDLE_ASSET,
    GLASS_WALL_ID, NPC_MARA_ID, PLAYER_ID,
};
use ashfall::runtime::{EngineModule, ModuleDescriptor};
use ashfall::scenarios::{build_alley_runtime, queue_glass_break_attempt};
use ashfall::world::{AudioEventKind, EventFilter, WorldEventKind};

struct TestRenderingStub;

impl EngineModule for TestRenderingStub {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            module_id: 10,
            name: "rendering_engine_stub",
            schema: SchemaVersion {
                name: "RenderPacket",
                version: 1,
            },
            default_quality: QualityTier::NormalRuntime,
        }
    }

    fn performance_counters(&self) -> PerformanceCounters {
        PerformanceCounters {
            cpu_milliseconds: 0.01,
            gpu_milliseconds: 0.0,
            memory_bytes: 128 * 1024,
        }
    }
}

#[test]
fn alley_slice_records_cross_component_consequences() {
    let mut runtime = build_alley_runtime().expect("alley scene should build");
    queue_glass_break_attempt(&mut runtime);

    let report = runtime.step().expect("frame should run");
    let render_frame = runtime.render_frame(1.0 / 144.0);

    assert_eq!(report.gpu_backend, GpuBackend::Vulkan);
    assert_eq!(render_frame.render_frame_id, 1);
    assert_eq!(render_frame.sim_frame_id, report.frame_id);
    assert_eq!(render_frame.sim_time, report.sim_time);
    assert!(render_frame.interpolation_alpha > 0.0);
    assert!(render_frame.interpolation_alpha < 1.0);
    assert_eq!(render_frame.snapshot.frame_id, report.frame_id);
    assert_eq!(report.module_reports.len(), 9);
    assert_eq!(report.profiler.module_costs.len(), 9);
    assert!(report.profiler.total_module_cpu_milliseconds > 0.0);
    assert!(report.profiler.total_module_memory_bytes > 0);
    assert_eq!(
        report.profiler.gpu_graph_milliseconds,
        report.gpu_total_milliseconds
    );
    assert_eq!(
        report.profiler.asset_streaming_bytes,
        report.asset_streaming.bytes_streamed_this_frame
    );
    assert!(report.budget_pressure.is_empty());
    assert_eq!(report.profiler.budget_pressure_count, 0);
    assert_eq!(report.budget_plan.total_pressure_count, 0);
    assert!(report.budget_plan.demotions().next().is_none());
    assert!(report.schema_compatibility.passed);
    assert!(report.schema_compatibility.checked_requirements >= 10);
    assert!(
        report
            .schema_compatibility
            .issues
            .iter()
            .all(|issue| issue.severity != ashfall::schema::SchemaCompatibilitySeverity::Error)
    );
    assert!(!runtime.replay_log().is_empty());
    assert_eq!(runtime.replay_log().frames()[0].queued_commands.len(), 1);
    assert!(runtime.assets().len() >= 9);
    assert_eq!(report.asset_streaming.completions.len(), 4);
    assert_eq!(report.asset_streaming.pending_assets, 0);
    assert!(report.asset_streaming.bytes_streamed_this_frame > 0);
    assert!(
        report
            .asset_streaming
            .by_requester
            .iter()
            .any(|cost| { cost.requester == 10 && cost.completed_assets >= 1 })
    );
    assert!(
        report
            .asset_streaming
            .by_requester
            .iter()
            .any(|cost| { cost.requester == 30 && cost.completed_assets >= 1 })
    );
    assert!(
        runtime
            .schemas()
            .require_at_least("PhysicsOutput", 1)
            .is_ok()
    );
    assert!(
        runtime
            .schemas()
            .require_at_least("RenderPacket", 1)
            .is_ok()
    );
    assert!(
        runtime
            .schemas()
            .require_at_least("NarrativeDirectorState", 1)
            .is_ok()
    );
    assert!(
        runtime
            .schemas()
            .require_at_least("AudioFrameOutput", 1)
            .is_ok()
    );
    let module_registry = runtime.module_registry_report();
    assert_eq!(module_registry.static_module_count, 9);
    assert_eq!(module_registry.dynamic_module_api_version, 1);
    assert!(module_registry.schema_compatibility.passed);
    assert!(
        module_registry
            .modules
            .iter()
            .any(|module| module.descriptor.name == "rendering_engine")
    );

    assert!(
        report
            .gpu_passes
            .iter()
            .any(|pass| pass.name == "main_lighting")
    );
    assert_eq!(report.gpu_timing.len(), report.gpu_passes.len());
    assert!(report.gpu_total_milliseconds > 0.0);
    assert_eq!(
        report.gpu_execution_plan.scheduled_passes.len(),
        report.gpu_passes.len()
    );
    assert!(
        (report.gpu_execution_plan.total_work_milliseconds - report.gpu_total_milliseconds).abs()
            <= 0.0001
    );
    assert!(report.gpu_execution_plan.estimated_wall_milliseconds <= report.gpu_total_milliseconds);
    assert!(report.gpu_execution_plan.cross_queue_wait_count > 0);
    let gpu_culling_schedule = report
        .gpu_execution_plan
        .scheduled_passes
        .iter()
        .find(|pass| pass.pass_name == "gpu_culling")
        .expect("gpu_culling should be scheduled");
    let main_lighting_schedule = report
        .gpu_execution_plan
        .scheduled_passes
        .iter()
        .find(|pass| pass.pass_name == "main_lighting")
        .expect("main_lighting should be scheduled");
    let indirect_draw_schedule = report
        .gpu_execution_plan
        .scheduled_passes
        .iter()
        .find(|pass| pass.pass_name == "indirect_draw_compaction")
        .expect("indirect draw compaction should be scheduled");
    let streaming_feedback_schedule = report
        .gpu_execution_plan
        .scheduled_passes
        .iter()
        .find(|pass| pass.pass_name == "render_streaming_feedback")
        .expect("renderer streaming feedback should be scheduled");
    assert!(indirect_draw_schedule.start_milliseconds >= gpu_culling_schedule.end_milliseconds);
    assert!(
        streaming_feedback_schedule.start_milliseconds >= indirect_draw_schedule.end_milliseconds
    );
    assert!(main_lighting_schedule.start_milliseconds >= gpu_culling_schedule.end_milliseconds);
    assert!(main_lighting_schedule.start_milliseconds >= indirect_draw_schedule.end_milliseconds);
    assert!(
        main_lighting_schedule.start_milliseconds >= streaming_feedback_schedule.end_milliseconds
    );
    assert!(
        main_lighting_schedule
            .wait_dependencies
            .iter()
            .any(|dependency| dependency.from_pass == "gpu_culling")
    );
    assert!(report.gpu_barriers.iter().any(|barrier| {
        barrier.reason == GpuBarrierReason::ReadAfterWrite
            && barrier.from_pass == "depth_prepass"
            && barrier.to_pass == "gpu_culling"
    }));
    assert!(report.gpu_barriers.iter().any(|barrier| {
        barrier.reason == GpuBarrierReason::ReadAfterWrite
            && barrier.from_pass == "gpu_culling"
            && barrier.to_pass == "main_lighting"
    }));
    assert!(report.gpu_validation.passed);
    assert!(
        report
            .gpu_passes
            .iter()
            .any(|pass| pass.name == "screen_space_reflections")
    );
    assert!(
        report
            .gpu_passes
            .iter()
            .any(|pass| pass.name == "temporal_reconstruction")
    );
    let post_process_schedule = report
        .gpu_execution_plan
        .scheduled_passes
        .iter()
        .find(|pass| pass.pass_name == "post_process")
        .expect("post process should be scheduled");
    let temporal_schedule = report
        .gpu_execution_plan
        .scheduled_passes
        .iter()
        .find(|pass| pass.pass_name == "temporal_reconstruction")
        .expect("temporal reconstruction should be scheduled");
    let ui_schedule = report
        .gpu_execution_plan
        .scheduled_passes
        .iter()
        .find(|pass| pass.pass_name == "ui_composition")
        .expect("UI composition should be scheduled");
    assert!(temporal_schedule.start_milliseconds >= post_process_schedule.end_milliseconds);
    assert!(ui_schedule.start_milliseconds >= temporal_schedule.end_milliseconds);
    assert!(
        report
            .gpu_resource_usage
            .iter()
            .any(|usage| { !usage.readers.is_empty() && !usage.writers.is_empty() })
    );
    assert!(report.gpu_declared_resources.len() >= 17);
    assert!(
        report
            .gpu_registered_resource_usage
            .iter()
            .any(|usage| usage.owner == Some(10)
                && usage.readers.iter().any(|reader| reader == "main_lighting"))
    );
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.label == "virtual shadow page table"
            && usage.writers.iter().any(|writer| writer == "shadow_maps")
            && usage.readers.iter().any(|reader| reader == "main_lighting")
            && usage.byte_len > 0
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.label == "dirty shadow page list"
            && usage.readers.iter().any(|reader| reader == "shadow_maps")
            && usage.byte_len > 0
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(20)
            && usage
                .writers
                .iter()
                .any(|writer| writer == "physics_sparse_fields")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(20)
            && usage.label == "physics simulation lod decision buffer"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "physics_simulation_lod_selection")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(20)
            && usage.label == "physics liquid sparse field output"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "physics_liquid_sparse_fields")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(20)
            && usage.label == "physics compacted debris output"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "physics_debris_compaction")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(30)
            && usage.label == "material crack detail cache output"
            && usage.bindless_index.is_some()
            && usage
                .writers
                .iter()
                .any(|writer| writer == "material_crack_cache_update")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(30)
            && usage.label == "material wetness reflection cache output"
            && usage.bindless_index.is_some()
            && usage
                .writers
                .iter()
                .any(|writer| writer == "material_wetness_cache_update")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(10)
            && usage.label == "material runtime program table"
            && usage.bindless_index.is_some()
            && usage.readers.iter().any(|reader| reader == "main_lighting")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(10)
            && usage.label == "material cache residency table"
            && usage.readers.iter().any(|reader| reader == "main_lighting")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(65)
            && usage.label == "audio reverb tail buffer"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "audio_reverb_convolution")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(65)
            && usage.label == "audio final mix output buffer"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "audio_final_mix")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(40)
            && usage.label == "human runtime bundle table"
            && usage.bindless_index.is_some()
            && usage
                .readers
                .iter()
                .any(|reader| reader == "human_lod_selection")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(40)
            && usage.label == "human facial morph target buffer"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "human_facial_viseme_morphs")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(40)
            && usage.label == "human skinning palette buffer"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "human_skinning_palette_upload")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(70)
            && usage.label == "worldgen chunk metadata buffer"
            && usage.bindless_index.is_some()
            && usage
                .readers
                .iter()
                .any(|reader| reader == "worldgen_chunk_visibility")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(70)
            && usage.label == "worldgen navigation validation output"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "worldgen_navigation_validation")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(70)
            && usage.label == "worldgen compacted streaming dependencies"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "worldgen_streaming_dependency_compaction")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(50)
            && usage.label == "ai agent state table"
            && usage.bindless_index.is_some()
            && usage
                .readers
                .iter()
                .any(|reader| reader == "ai_perception_query_binning")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(50)
            && usage.label == "ai visible observation buffer"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "ai_observation_visibility_filter")
            && usage
                .readers
                .iter()
                .any(|reader| reader == "ai_memory_relevance_index")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(50)
            && usage.label == "ai intent score buffer"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "ai_intent_candidate_scoring")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(55)
            && usage.label == "story active thread table"
            && usage.bindless_index.is_some()
            && usage
                .readers
                .iter()
                .any(|reader| reader == "story_thread_pressure_update")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(55)
            && usage.label == "story grounded opportunity index"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "story_grounded_opportunity_index")
            && usage
                .readers
                .iter()
                .any(|reader| reader == "story_director_command_validation")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(55)
            && usage.label == "story director opportunity table"
            && usage.bindless_index.is_some()
            && usage
                .readers
                .iter()
                .any(|reader| reader == "story_grounded_opportunity_index")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(55)
            && usage.label == "story grounded rumor table"
            && usage.bindless_index.is_some()
            && usage
                .readers
                .iter()
                .any(|reader| reader == "story_rumor_propagation")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(55)
            && usage.label == "story rumor propagation buffer"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "story_rumor_propagation")
            && usage
                .readers
                .iter()
                .any(|reader| reader == "story_grounded_opportunity_index")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(10)
            && usage.label == "indirect draw command buffer"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "indirect_draw_compaction")
            && usage.readers.iter().any(|reader| reader == "main_lighting")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(10)
            && usage.label == "mesh cluster table"
            && usage.bindless_index.is_some()
            && usage.readers.iter().any(|reader| reader == "gpu_culling")
            && usage
                .readers
                .iter()
                .any(|reader| reader == "indirect_draw_compaction")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(10)
            && usage.label == "visible mesh clusters"
            && usage.writers.iter().any(|writer| writer == "gpu_culling")
            && usage
                .readers
                .iter()
                .any(|reader| reader == "render_streaming_feedback")
            && usage.readers.iter().any(|reader| reader == "main_lighting")
    }));
    assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
        usage.owner == Some(10)
            && usage.label == "renderer streaming feedback buffer"
            && usage
                .writers
                .iter()
                .any(|writer| writer == "render_streaming_feedback")
            && usage.readers.iter().any(|reader| reader == "main_lighting")
    }));
    assert_eq!(
        report.gpu_descriptor_report.bindings.len(),
        report.gpu_registered_resource_usage.len()
    );
    assert!(report.gpu_descriptor_report.bindless_binding_count >= 2);
    assert!(report.gpu_descriptor_report.descriptor_set_count >= 2);
    assert!(report.gpu_descriptor_report.descriptor_table_bytes > 0);
    assert!(report.gpu_descriptor_report.bindings.iter().any(|binding| {
        binding.owner == Some(10)
            && binding.bindless_index.is_some()
            && binding
                .pass_names
                .iter()
                .any(|pass| pass == "main_lighting")
    }));
    assert_eq!(
        report.gpu_pipeline_report.total_pipeline_requests,
        report.gpu_passes.len()
    );
    assert_eq!(report.gpu_pipeline_report.unknown_pipeline_count, 0);
    assert!(report.gpu_pipeline_report.pipelines.len() >= 11);
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "main_lighting"
            && pipeline.shader_key == "render/main_lighting.vert+frag"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "BINDLESS_MATERIALS")
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "SCREEN_SPACE_REFLECTIONS")
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "PROCEDURAL_MATERIAL_PROGRAMS")
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "MATERIAL_CACHE_TEXTURES")
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "FRACTURE_INTERIOR_DISPLACEMENT")
            && pipeline.pass_names == vec!["main_lighting".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "temporal_reconstruction"
            && pipeline.shader_key == "render/temporal_reconstruction.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "TEMPORAL_HISTORY")
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "MOTION_VECTORS")
            && pipeline.pass_names == vec!["temporal_reconstruction".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "indirect_draw_compaction"
            && pipeline.shader_key == "render/indirect_draw_compaction.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "GPU_DRIVEN_BATCHING")
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "INDIRECT_DRAW_COMPACTION")
            && pipeline.pass_names == vec!["indirect_draw_compaction".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "gpu_culling"
            && pipeline.shader_key == "render/gpu_culling.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "CLUSTER_LOD_SELECTION")
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "VISIBILITY_FEEDBACK")
            && pipeline.pass_names == vec!["gpu_culling".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "render_streaming_feedback"
            && pipeline.shader_key == "render/streaming_feedback.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "CLUSTER_STREAMING_FEEDBACK")
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "CITY_STREAMING_FEEDBACK")
            && pipeline.pass_names == vec!["render_streaming_feedback".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "physics_simulation_lod_selection"
            && pipeline.shader_key == "physics/simulation_lod_selection.comp"
            && pipeline.pass_names == vec!["physics_simulation_lod_selection".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "physics_liquid_sparse_fields"
            && pipeline.shader_key == "physics/liquid_sparse_fields.comp"
            && pipeline.pass_names == vec!["physics_liquid_sparse_fields".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "physics_debris_compaction"
            && pipeline.shader_key == "physics/debris_compaction.comp"
            && pipeline.pass_names == vec!["physics_debris_compaction".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "material_crack_cache_update"
            && pipeline.shader_key == "materials/crack_cache_update.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "CRACK_RESPONSE")
            && pipeline.pass_names == vec!["material_crack_cache_update".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "material_wetness_cache_update"
            && pipeline.shader_key == "materials/wetness_cache_update.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "WETNESS_RESPONSE")
            && pipeline.pass_names == vec!["material_wetness_cache_update".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "audio_spatialization"
            && pipeline.shader_key == "audio/spatialization.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "SPATIAL_AUDIO")
            && pipeline.pass_names == vec!["audio_spatialization".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "audio_final_mix"
            && pipeline.shader_key == "audio/final_mix.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "PEAK_LIMITER")
            && pipeline.pass_names == vec!["audio_final_mix".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "human_lod_selection"
            && pipeline.shader_key == "human/lod_selection.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "HUMAN_LOD")
            && pipeline.pass_names == vec!["human_lod_selection".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "human_facial_viseme_morphs"
            && pipeline.shader_key == "human/facial_viseme_morphs.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "VISEME_TRACKS")
            && pipeline.pass_names == vec!["human_facial_viseme_morphs".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "human_skinning_palette_upload"
            && pipeline.shader_key == "human/skinning_palette_upload.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "GPU_SKINNING")
            && pipeline.pass_names == vec!["human_skinning_palette_upload".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "worldgen_chunk_visibility"
            && pipeline.shader_key == "worldgen/chunk_visibility.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "WORLD_STREAMING_CHUNKS")
            && pipeline.pass_names == vec!["worldgen_chunk_visibility".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "worldgen_streaming_dependency_compaction"
            && pipeline.shader_key == "worldgen/streaming_dependency_compaction.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "ASSET_DEDUP")
            && pipeline.pass_names == vec!["worldgen_streaming_dependency_compaction".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "ai_observation_visibility_filter"
            && pipeline.shader_key == "ai/observation_visibility_filter.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "NO_GLOBAL_KNOWLEDGE")
            && pipeline.pass_names == vec!["ai_observation_visibility_filter".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "ai_intent_candidate_scoring"
            && pipeline.shader_key == "ai/intent_candidate_scoring.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "INTENT_VALIDATION")
            && pipeline.pass_names == vec!["ai_intent_candidate_scoring".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "story_thread_pressure_update"
            && pipeline.shader_key == "story/thread_pressure_update.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "GROUNDED_THREADS")
            && pipeline.pass_names == vec!["story_thread_pressure_update".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "story_grounded_opportunity_index"
            && pipeline.shader_key == "story/grounded_opportunity_index.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "DIRECTOR_OPPORTUNITIES")
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "WORLD_VALIDATED_OFFERS")
            && pipeline.pass_names == vec!["story_grounded_opportunity_index".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "story_rumor_propagation"
            && pipeline.shader_key == "story/rumor_propagation.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "NO_GLOBAL_KNOWLEDGE")
            && pipeline.pass_names == vec!["story_rumor_propagation".to_string()]
    }));
    assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
        pipeline.label == "story_director_command_validation"
            && pipeline.shader_key == "story/director_command_validation.comp"
            && pipeline
                .permutation_defines
                .iter()
                .any(|define| define == "NO_FORCED_SCENES")
            && pipeline.pass_names == vec!["story_director_command_validation".to_string()]
    }));
    assert!(report.gpu_queue_workloads.iter().any(|workload| {
        workload.queue == ashfall::gpu::GpuQueueKind::Graphics && workload.pass_count > 0
    }));
    assert!(report.gpu_transient_memory_bytes > 0);
    assert_eq!(
        report.gpu_transient_memory_bytes,
        report.gpu_memory_plan.total_transient_bytes
    );
    assert!(report.gpu_memory_plan.transient_resource_count > 0);
    assert!(
        report.gpu_memory_plan.alias_group_count < report.gpu_memory_plan.transient_resource_count
    );
    assert!(report.gpu_memory_plan.aliased_bytes_saved > 0);
    let inspection = report.inspect();
    assert!(inspection.passed);
    assert_eq!(inspection.modules.len(), report.module_reports.len());
    let rendering_inspection = inspection
        .module(10)
        .expect("rendering module should be inspectable");
    assert!(rendering_inspection.streaming_completed_assets >= 1);
    assert!(rendering_inspection.gpu_resource_count >= 15);
    assert!(rendering_inspection.gpu_descriptor_count >= 15);
    assert!(rendering_inspection.gpu_bindless_descriptor_count >= 2);
    let physics_inspection = inspection
        .module(20)
        .expect("physics module should be inspectable");
    assert!(physics_inspection.gpu_resource_count >= 6);
    assert!(physics_inspection.gpu_descriptor_count >= 6);
    let materials_inspection = inspection
        .module(30)
        .expect("materials module should be inspectable");
    assert!(materials_inspection.gpu_resource_count >= 4);
    assert!(materials_inspection.gpu_descriptor_count >= 4);
    assert!(materials_inspection.gpu_bindless_descriptor_count >= 3);
    let audio_inspection = inspection
        .module(65)
        .expect("audio mixer module should be inspectable");
    assert!(audio_inspection.gpu_resource_count >= 6);
    assert!(audio_inspection.gpu_descriptor_count >= 6);
    let human_inspection = inspection
        .module(40)
        .expect("human module should be inspectable");
    assert!(human_inspection.gpu_resource_count >= 6);
    assert!(human_inspection.gpu_descriptor_count >= 6);
    assert!(human_inspection.gpu_bindless_descriptor_count >= 1);
    let worldgen_inspection = inspection
        .module(70)
        .expect("world generation module should be inspectable");
    assert!(worldgen_inspection.gpu_resource_count >= 10);
    assert!(worldgen_inspection.gpu_descriptor_count >= 10);
    assert!(worldgen_inspection.gpu_bindless_descriptor_count >= 2);
    let ai_inspection = inspection
        .module(50)
        .expect("AI characters module should be inspectable");
    assert!(ai_inspection.gpu_resource_count >= 10);
    assert!(ai_inspection.gpu_descriptor_count >= 10);
    assert!(ai_inspection.gpu_bindless_descriptor_count >= 2);
    let story_inspection = inspection
        .module(55)
        .expect("story director module should be inspectable");
    assert!(story_inspection.gpu_resource_count >= 8);
    assert!(story_inspection.gpu_descriptor_count >= 8);
    assert!(story_inspection.gpu_bindless_descriptor_count >= 2);
    assert_eq!(
        inspection.total_streaming_bytes,
        report.asset_streaming.bytes_streamed_this_frame
    );
    assert!(inspection.total_gpu_resource_bytes > 0);
    assert_eq!(
        inspection.total_gpu_descriptor_bindings,
        report.gpu_descriptor_report.bindings.len()
    );
    assert_eq!(
        inspection.total_gpu_bindless_bindings,
        report.gpu_descriptor_report.bindless_binding_count
    );
    assert_eq!(
        inspection.gpu_descriptor_table_bytes,
        report.gpu_descriptor_report.descriptor_table_bytes
    );
    assert_eq!(
        inspection.total_gpu_pipeline_count,
        report.gpu_pipeline_report.pipelines.len()
    );
    assert_eq!(
        inspection.gpu_pipeline_cache_hit_count,
        report.gpu_pipeline_report.cache_hit_count
    );
    assert_eq!(
        inspection.gpu_unknown_pipeline_count,
        report.gpu_pipeline_report.unknown_pipeline_count
    );
    assert_eq!(
        inspection.total_gpu_transient_bytes,
        report.gpu_memory_plan.total_transient_bytes
    );
    assert_eq!(
        inspection.gpu_memory_saved_bytes,
        report.gpu_memory_plan.aliased_bytes_saved
    );
    assert_eq!(
        inspection.gpu_scheduled_wall_milliseconds,
        report.gpu_execution_plan.estimated_wall_milliseconds
    );
    assert_eq!(
        inspection.gpu_overlapped_work_milliseconds,
        report.gpu_execution_plan.overlapped_work_milliseconds
    );

    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::GlassWallFractured { entity } if entity == GLASS_WALL_ID
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::NpcWitnessedCrime { witness, .. } if witness == NPC_MARA_ID
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::NpcHeardSound {
                listener: NPC_MARA_ID,
                confidence,
                ..
            } if confidence > 0.4
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::AgentIntentProposed {
                agent: NPC_MARA_ID,
                action,
                validation_passed: true,
                ..
            } if action == "ReportCrime"
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::AgentIntentProposed {
                agent: NPC_MARA_ID,
                action,
                validation_passed: true,
                ..
            } if action == "Investigate"
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::AgentDecisionExplained {
                agent: NPC_MARA_ID,
                goal,
                accepted_actions,
                rejected_actions,
                reasons,
                ..
            } if goal.contains("preserve evidence")
                && accepted_actions.iter().any(|action| action == "ReportCrime")
                && rejected_actions
                    .iter()
                    .any(|reason| reason.contains("directly punish"))
                && reasons.iter().any(|reason| reason.contains("direct observation"))
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::AgentDecisionExplained {
                agent: NPC_MARA_ID,
                goal,
                accepted_actions,
                rejected_actions,
                ..
            } if goal.contains("unusual nearby sound")
                && accepted_actions.iter().any(|action| action == "Investigate")
                && rejected_actions
                    .iter()
                    .any(|reason| reason.contains("direct witness"))
        )
    }));
    let mara_decisions =
        inspect_agent_decisions_from_ledger(&runtime.world.event_ledger, NPC_MARA_ID);
    assert!(mara_decisions.decisions.len() >= 2);
    assert!(mara_decisions.memories.len() >= 2);
    assert!(mara_decisions.intents.len() >= 4);
    assert!(mara_decisions.has_accepted_action("ReportCrime"));
    assert!(mara_decisions.has_accepted_action("Investigate"));
    assert!(
        mara_decisions
            .why_lines()
            .iter()
            .any(|line| line.contains("knowledge source is inference"))
    );
    assert!(report.events.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::AgentMemoryUpdated {
                agent: NPC_MARA_ID,
                memory_kind,
                content,
                importance,
                confidence,
                ..
            } if memory_kind == "evidence"
                && content.contains("break the alley glass wall")
                && *importance > 0.8
                && *confidence > 0.8
        )
    }));
    assert!(
        report
            .events
            .iter()
            .any(|event| { matches!(event.kind, WorldEventKind::PlayerIdentityExposed) })
    );
    assert!(report.events.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::StoryEventEmitted { label }
                if label == "player_identity_compromised"
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::SecurityAlertRaised {
                faction: ALLEY_SECURITY_FACTION_ID,
                threat: PLAYER_ID,
                severity,
                ..
            } if severity > 0.7
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::FactionReputationChanged {
                faction: ALLEY_SECURITY_FACTION_ID,
                subject: PLAYER_ID,
                delta,
                reason,
            } if *delta < 0.0 && reason.contains("vandalism")
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::SoundEmitted {
                kind: AudioEventKind::VoiceSpeech,
                ..
            }
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::SpeechSynthesized {
                speaker: NPC_MARA_ID,
                phoneme_count,
                viseme_count,
                ..
            } if phoneme_count > 0 && viseme_count > 0
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::FacialAnimationApplied {
                entity: NPC_MARA_ID,
                viseme_count,
                duration_seconds,
            } if viseme_count > 0 && duration_seconds > 0.0
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(event.kind, WorldEventKind::StreetFlooded)
            && event.actors.contains(&ALLEY_ASPHALT_ID)
    }));
    assert!(report.events.iter().any(|event| {
        matches!(event.kind, WorldEventKind::ToxicGasReleased)
            && event
                .physical_evidence
                .iter()
                .any(|evidence| evidence == "gas_density_field")
            && event
                .physical_evidence
                .iter()
                .any(|evidence| evidence == "gas_kind:steam")
            && event
                .physical_evidence
                .iter()
                .any(|evidence| evidence == "gas_density:0.45")
            && event
                .physical_evidence
                .iter()
                .any(|evidence| evidence == "hazard_level:0.28")
            && event
                .narrative_tags
                .iter()
                .any(|tag| tag == "visibility_blocking")
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::SoundEmitted {
                kind: AudioEventKind::WaterSplash,
                ..
            }
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::AudioFrameMixed {
                active_sound_count,
                material_aware_sound_count,
                peak_intensity,
                ..
            } if active_sound_count >= 3
                && material_aware_sound_count >= 2
                && peak_intensity > 0.9
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::AssetStreamingRequested {
                asset_id,
                requester: 30,
                ..
            } if *asset_id == ASSET_GLASS_CRACK_DETAIL_CACHE
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::AssetStreamingRequested {
                asset_id,
                requester: 30,
                ..
            } if *asset_id == ASSET_WET_ASPHALT_REFLECTION_CACHE
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::AssetStreamingRequested {
                asset_id: ALLEY_MARA_HUMAN_BUNDLE_ASSET,
                requester: 40,
                requested_quality,
                ..
            } if *requested_quality == ashfall::core::QualityTier::HeroHighFidelityRuntime
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::AssetStreamingRequested {
                asset_id: ALLEY_GLASS_FRACTURED_MESH_ASSET,
                requested_quality,
                ..
            } if *requested_quality == ashfall::core::QualityTier::HeroHighFidelityRuntime
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::AssetBecameResident {
                asset_id,
                ..
            } if asset_id == ASSET_GLASS_CRACK_DETAIL_CACHE
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::AssetBecameResident {
                asset_id,
                ..
            } if asset_id == ASSET_WET_ASPHALT_REFLECTION_CACHE
        )
    }));
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::AssetBecameResident {
                asset_id: ALLEY_GLASS_FRACTURED_MESH_ASSET,
                ..
            }
        )
    }));
    assert_eq!(
        runtime
            .assets()
            .load_state(ALLEY_GLASS_FRACTURED_MESH_ASSET),
        Some(AssetLoadState::Resident)
    );
    assert_eq!(
        runtime.assets().load_state(ASSET_GLASS_CRACK_DETAIL_CACHE),
        Some(AssetLoadState::Resident)
    );
    assert_eq!(
        runtime
            .assets()
            .load_state(ASSET_WET_ASPHALT_REFLECTION_CACHE),
        Some(AssetLoadState::Resident)
    );
    assert_eq!(runtime.assets().streaming_queue_len(), 0);
    assert!(runtime.world.event_ledger.contains_kind(|kind| {
        matches!(kind, WorldEventKind::VoiceLineSpoken { speaker } if *speaker == NPC_MARA_ID)
    }));

    let witness_events = runtime.world.event_ledger.query(&EventFilter {
        actor: Some(NPC_MARA_ID),
        narrative_tag: Some("witness".to_string()),
        near: Some((Vec3::new(5.5, 0.0, 0.0), 1.0)),
        ..EventFilter::default()
    });
    assert_eq!(witness_events.len(), 2);

    let evidence_memories = runtime.world.event_ledger.query(&EventFilter {
        actor: Some(NPC_MARA_ID),
        narrative_tag: Some("memory".to_string()),
        physical_evidence: Some("personal_memory".to_string()),
        ..EventFilter::default()
    });
    assert!(evidence_memories.len() >= 2);
    assert!(evidence_memories.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::AgentMemoryUpdated {
                memory_kind,
                content,
                ..
            } if memory_kind == "evidence" && content.contains("break the alley glass wall")
        )
    }));
    assert!(evidence_memories.iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::AgentMemoryUpdated {
                memory_kind,
                content,
                ..
            } if memory_kind == "short_term" && content.contains("Heard glass break")
        )
    }));

    let security_alerts = runtime.world.event_ledger.query(&EventFilter {
        actor: Some(PLAYER_ID),
        narrative_tag: Some("security".to_string()),
        physical_evidence: Some("security_feed".to_string()),
        ..EventFilter::default()
    });
    assert_eq!(security_alerts.len(), 1);

    let snapshot = runtime.world.snapshot(report.frame_id, report.sim_time);
    let asphalt_state = snapshot
        .material_states
        .find(ALLEY_ASPHALT_ID)
        .expect("asphalt surface should have material state");
    assert!(asphalt_state.moisture >= 0.9);

    let packet = build_render_packet(&snapshot);
    assert!(packet.visible_entities.contains(&ALLEY_ASPHALT_ID));
    assert!(packet.wet_material_entities.contains(&ALLEY_ASPHALT_ID));
    assert!(packet.cracked_material_entities.contains(&GLASS_WALL_ID));

    let render_output = render_output_from_packet(&packet);
    assert_eq!(
        render_output.draw_plan.visible_instance_count,
        packet.visible_entities.len()
    );
    assert!(render_output.draw_plan.indirect_draw_count > 0);
    assert_eq!(render_output.draw_plan.cpu_draw_call_count, 1);
    assert!(
        render_output
            .debug_data
            .iter()
            .any(|entry| entry.label == "gpu_draw_batches")
    );
}

#[test]
fn world_save_data_can_restore_event_history() {
    let mut runtime = build_alley_runtime().expect("alley scene should build");
    queue_glass_break_attempt(&mut runtime);
    runtime.step().expect("frame should run");

    let saved = runtime.save_world();
    let mut restored = build_alley_runtime().expect("alley scene should rebuild");
    restored.load_world(saved);

    assert!(restored.world.event_ledger.contains_kind(|kind| {
        matches!(kind, WorldEventKind::GlassWallFractured { entity } if *entity == GLASS_WALL_ID)
    }));
}

#[test]
fn runtime_save_data_can_restore_clock_assets_and_replay() {
    let mut runtime = build_alley_runtime().expect("alley scene should build");
    queue_glass_break_attempt(&mut runtime);
    let report = runtime.step().expect("frame should run");
    runtime.render_frame(1.0 / 144.0);

    let saved = runtime.save_runtime();
    let mut restored = build_alley_runtime().expect("alley scene should rebuild");
    restored.load_runtime(saved);
    let render_frame = restored.render_frame(0.0);

    assert_eq!(render_frame.sim_frame_id, report.frame_id);
    assert_eq!(render_frame.sim_time, report.sim_time);
    assert_eq!(restored.replay_log().len(), 1);
    assert_eq!(
        restored
            .assets()
            .load_state(ALLEY_GLASS_FRACTURED_MESH_ASSET),
        Some(AssetLoadState::Resident)
    );
    assert_eq!(
        restored.assets().load_state(ASSET_GLASS_CRACK_DETAIL_CACHE),
        Some(AssetLoadState::Resident)
    );
    assert!(restored.world.event_ledger.contains_kind(|kind| {
        matches!(kind, WorldEventKind::GlassWallFractured { entity } if *entity == GLASS_WALL_ID)
    }));
    assert!(
        restored
            .schemas()
            .require_at_least("RenderPacket", 1)
            .is_ok()
    );
}

#[test]
fn alley_replay_log_matches_for_identical_scene_inputs() {
    let mut expected = build_alley_runtime().expect("alley scene should build");
    queue_glass_break_attempt(&mut expected);
    expected.step().expect("expected frame should run");

    let mut actual = build_alley_runtime().expect("alley scene should rebuild");
    queue_glass_break_attempt(&mut actual);
    let actual_report = actual.step().expect("actual frame should run");

    let replay_report = expected.replay_log().compare(actual.replay_log());
    assert!(
        replay_report.passed,
        "replay mismatch: {:?}",
        replay_report.mismatches
    );
    assert_eq!(replay_report.compared_frames, 1);

    let summary = actual
        .replay_log()
        .frame(0)
        .expect("replay frame should exist")
        .summary();
    assert_eq!(summary.queued_command_count, 1);
    assert_eq!(summary.event_count, actual_report.events.len());
    assert!(
        summary
            .event_fingerprints
            .iter()
            .any(|fingerprint| { fingerprint.kind_label == "GlassWallFractured" })
    );
}

#[test]
fn alley_runtime_can_replace_renderer_with_test_stub() {
    let mut runtime = build_alley_runtime().expect("alley scene should build");
    let replacement = runtime
        .replace_module(TestRenderingStub)
        .expect("renderer stub should satisfy the same RenderPacket contract");

    assert_eq!(replacement.previous.name, "rendering_engine");
    assert_eq!(replacement.replacement.name, "rendering_engine_stub");
    assert!(replacement.schema_compatibility.passed);
    assert!(
        runtime
            .module_descriptors()
            .iter()
            .any(|descriptor| descriptor.name == "rendering_engine_stub")
    );

    queue_glass_break_attempt(&mut runtime);
    let report = runtime.step().expect("stubbed frame should run");

    assert!(report.schema_compatibility.passed);
    assert!(
        report
            .module_reports
            .iter()
            .any(|module| module.descriptor.name == "rendering_engine_stub")
    );
    assert!(report.events.iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::GlassWallFractured { entity } if entity == GLASS_WALL_ID
        )
    }));
}

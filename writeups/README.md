# Photorealistic Rust/Vulkan Game Engine Writeups — Improved V4 Set

This is the fourth-pass sanity-checked documentation package for the Rust-first, Vulkan-first photorealistic game-engine project.

The ambition remains broad: photoreal rendering, physically meaningful consequences, generated materials, realistic humans, AI characters, and voice personas. The execution model is deliberately narrow and milestone-gated. The project must first prove it can render, capture, evaluate, profile, and improve one object class before adding the next.

The first production target remains:

```bash
cargo run -p tools_cli -- eval sky --profile high --seed-set locked_001
```

That command should eventually produce deterministic sky captures, HDR/debug AOVs, GPU/CPU telemetry, Vulkan validation logs, AI/human visual-evaluation reports, temporal metrics, and a pass/fail decision for the **sun + atmosphere + volumetric clouds only** milestone.

## What V4 improves

V4 keeps the V3 architecture and adds implementation-readiness hardening:

- Fixes stale cross-file references and duplicated renderer backend sections.
- Adds source-verified dependency notes for Vulkano, winit, Burn, Vulkan, descriptor indexing, OpenAI APIs, and the OpenAI Evals deprecation timeline.
- Adds a dedicated shader ABI and GPU data-layout contract.
- Adds a renderer validation/testing pyramid.
- Adds a statistical visual/performance promotion protocol.
- Adds a practical first-90-days execution plan.
- Adds a risk register with dependency pivot and scope-control criteria.
- Strengthens `AGENTS.md` rules for coding-agent work, shader ABI changes, doc integrity, and eval promotion.

## Reading order

1. `00_sanity_check_report.md`
2. `37_revision_4_sanity_report.md`
3. `38_source_verified_dependency_notes.md`
4. `01_project_vision.md`
5. `02_workspace_architecture.md`
6. `03_core_interfaces.md`
7. `04_vulkan_renderer_spec.md`
8. `23_vulkan_winit_surface_lifecycle.md`
9. `24_gpu_resource_lifetime_and_sync.md`
10. `40_shader_abi_and_gpu_data_layout.md`
11. `39_renderer_validation_and_testing_plan.md`
12. `34_runtime_data_flow_and_threading.md`
13. `05_clouds_and_sun_milestone.md`
14. `29_sky_certification_rubric.md`
15. `35_eval_dataset_and_judge_protocol.md`
16. `41_eval_statistics_and_promotion_protocol.md`
17. `30_artifact_and_schema_contracts.md`
18. `42_first_90_days_execution_plan.md`
19. `33_first_epics_and_backlog.md`
20. `27_release_gate_playbook.md`
21. `43_risk_register_and_kill_criteria.md`
22. `17_codex_agent_workflow.md`
23. `AGENTS.md`

The remaining component documents can be read as independent R&D specifications.

## Contents

- `00_sanity_check_report.md` — feasibility review, corrections, risks, revised assumptions, and V4 addendum.
- `01_project_vision.md` — project goal, non-goals, development law, and success definition.
- `02_workspace_architecture.md` — Rust workspace, crate boundaries, dependency layers, and build targets.
- `03_core_interfaces.md` — handles, snapshots, events, asset contracts, telemetry, and interface versioning.
- `04_vulkan_renderer_spec.md` — Vulkan renderer architecture, render graph, feature detection, shading, and backend boundary.
- `05_clouds_and_sun_milestone.md` — first milestone specification and acceptance criteria.
- `06_physics_consequence_engine.md` — physical consequence model, solvers, events, and staged feasibility.
- `07_material_texture_generator.md` — procedural/photo-derived material generation and validation.
- `08_human_generator.md` — staged photoreal human generation, skin, hair, eyes, rigging, and consent.
- `09_ai_characters.md` — agentic characters, perception, memory, action contracts, and runtime isolation.
- `10_voice_personas.md` — voice runtime, TTS/realtime paths, lip sync, caching, and safety rules.
- `11_evaluation_feedback_loop.md` — capture/evaluate/profile/iterate loop and artifact schema.
- `12_performance_development_environment.md` — profiling, validation, build modes, and hardware profiles.
- `13_asset_scene_interchange.md` — runtime scene schema, glTF/MaterialX/OpenUSD strategy, and streaming.
- `14_component_rnd_roadmap.md` — staged R&D packages and milestone dependencies.
- `15_first_implementation_sequence.md` — practical implementation order from empty window to certified sky renderer.
- `16_rust_api_contracts.md` — consolidated Rust-style API sketches.
- `17_codex_agent_workflow.md` — how to use Codex/ChatGPT Pro safely for componentized development.
- `18_references_and_terms.md` — sources, terms, and glossary.
- `19_revision_2_change_log.md` — V2 change summary retained for traceability.
- `20_interface_compatibility_matrix.md` — component interface compatibility and forbidden coupling examples.
- `21_sky_eval_rubric_templates.md` — structured sky visual-evaluation rubric and prompt templates.
- `22_revision_3_sanity_report.md` — V3 sanity report focused on implementation risks.
- `23_vulkan_winit_surface_lifecycle.md` — desktop/headless presentation lifecycle and swapchain policy.
- `24_gpu_resource_lifetime_and_sync.md` — GPU resource lifetime, descriptors, pending deletion, and sync contract.
- `25_dependency_spike_plan.md` — how to evaluate and adopt dependencies without destabilizing milestones.
- `26_runtime_quality_profiles_and_budgets.md` — quality profiles and per-feature budgets.
- `27_release_gate_playbook.md` — gate levels from compile/schema safety to human milestone review.
- `28_dependency_and_capability_matrix.md` — dependency ownership, feature flags, Vulkan tiers, and upgrade checklist.
- `29_sky_certification_rubric.md` — concrete sky certification gate and capture requirements.
- `30_artifact_and_schema_contracts.md` — artifact bundle structure and schema/versioning rules.
- `31_security_safety_and_data_governance.md` — secrets, provenance, rights, consent, voice, and supply-chain governance.
- `32_component_acceptance_checklists.md` — component-specific definitions of done.
- `33_first_epics_and_backlog.md` — first work packages and component-scoped backlog.
- `34_runtime_data_flow_and_threading.md` — runtime lanes, snapshots, job queues, backpressure, and determinism levels.
- `35_eval_dataset_and_judge_protocol.md` — dataset tiers, judge calibration, anti-overfitting rules, and promotion criteria.
- `36_revision_3_change_log.md` — V3 verification notes and change summary.
- `37_revision_4_sanity_report.md` — V4 sanity findings and implementation-readiness corrections.
- `38_source_verified_dependency_notes.md` — source-verified dependency assumptions and refresh checklist.
- `39_renderer_validation_and_testing_plan.md` — renderer test pyramid, validation policy, and merge requirements.
- `40_shader_abi_and_gpu_data_layout.md` — shader-visible data layout and CPU/GPU ABI contract.
- `41_eval_statistics_and_promotion_protocol.md` — statistical promotion protocol for visual and performance gates.
- `42_first_90_days_execution_plan.md` — practical scoped plan for proving the sky render/eval loop.
- `43_risk_register_and_kill_criteria.md` — risk register, dependency pivot criteria, and scope-control rules.
- `44_revision_4_change_log.md` — V4 change summary.
- `AGENTS.md` — coding-agent project rules.
- `manifest.json` — machine-readable file list and package metadata.

## Project rule

Do not move to the next object class until the current object class has a locked test set, telemetry history, image-evaluation history, performance budget, artifact schema, human promotion review, and regression gate.

For the current milestone, the only visual content allowed is:

```text
camera + sun + atmosphere + volumetric clouds
```

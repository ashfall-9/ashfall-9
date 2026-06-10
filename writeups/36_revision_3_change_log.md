# 36 — Revision 3 Change Log

## Summary

V3 is an implementation-hardening pass over V2. It does not expand the engine scope. It makes the first milestone safer and more executable by adding concrete contracts for GPU lifetime, shader ABI, provider isolation, evaluation drift, dependency governance, artifact schemas, runtime data flow, and Codex task boundaries.

## Current-source verification notes

Version-sensitive assumptions were checked against current public documentation on June 8, 2026:

```text
- Vulkano remains a safe/rich Rust wrapper around Vulkan, but advanced paths still need a controlled raw Vulkan escape hatch.
- winit currently uses the 0.30 ApplicationHandler/run_app application model, so app_desktop owns window lifecycle.
- Burn exposes a vulkan feature through the WGPU/SPIR-V backend path, so it remains AI/model tooling rather than renderer interop.
- Vulkan 1.4 is current, but Vulkan 1.3 remains the practical first baseline until the hardware matrix proves otherwise.
- Descriptor indexing, dynamic rendering, ray tracing, and synchronization validation require explicit feature and lifetime handling.
- OpenAI Evals has a 2026 deprecation timeline, so eval_lab must own artifacts and schemas.
- Codex supports repository-level AGENTS.md instructions, so coding-agent constraints belong in the repo.
```

## Files added in V3

```text
22_revision_3_sanity_report.md
23_vulkan_winit_surface_lifecycle.md
24_gpu_resource_lifetime_and_sync.md
25_dependency_spike_plan.md
26_runtime_quality_profiles_and_budgets.md
27_release_gate_playbook.md
28_dependency_and_capability_matrix.md
29_sky_certification_rubric.md
30_artifact_and_schema_contracts.md
31_security_safety_and_data_governance.md
32_component_acceptance_checklists.md
33_first_epics_and_backlog.md
34_runtime_data_flow_and_threading.md
35_eval_dataset_and_judge_protocol.md
```

## Main corrections

```text
- Added explicit Vulkano/raw Vulkan ownership boundaries.
- Added surface/swapchain lifecycle handling for winit desktop apps.
- Added GPU resource lifetime and synchronization contracts.
- Added dependency spike and upgrade policy.
- Added runtime quality profiles and feature budgets.
- Added release-gate playbook for milestone promotion.
- Added sky certification rubric with seed sets and capture requirements.
- Added artifact/schema and provenance requirements.
- Added runtime threading/data-flow model.
- Added evaluator dataset and drift protocol.
```

## Net effect

The package is now an architecture plus execution-control pack. It should be suitable to place into a new repository before Codex or any coding agent starts generating implementation code.

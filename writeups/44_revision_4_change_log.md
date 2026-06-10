# 44 — Revision 4 Change Log

## Summary

V4 is a sanity-check and implementation-hardening pass over V3. It keeps the same architecture and first milestone, but fixes documentation defects and adds the missing controls most likely to matter when coding begins.

## Fixed

```text
- corrected stale reference from 04_vulkan_renderer_spec.md to non-existent 29_gpu_resource_lifetime_and_abi (missing legacy reference)
- corrected stale reference from 33_first_epics_and_backlog.md to non-existent 22_artifact_and_schema_contracts (missing legacy reference)
- collapsed duplicated V3 renderer resource-lifetime/raw-interop sections into one V4 section
- updated README title, summary, reading order, and file list to V4
- regenerated manifest with V4 metadata and checksums
```

## Added

```text
37_revision_4_sanity_report.md
38_source_verified_dependency_notes.md
39_renderer_validation_and_testing_plan.md
40_shader_abi_and_gpu_data_layout.md
41_eval_statistics_and_promotion_protocol.md
42_first_90_days_execution_plan.md
43_risk_register_and_kill_criteria.md
44_revision_4_change_log.md
```

## Strengthened

```text
- source-verified assumptions for Vulkano, winit, Burn, Vulkan 1.4, descriptor indexing, OpenAI APIs, and Evals deprecation
- shader ABI policy with fixed-layout GPU structs and reflection tests
- renderer validation pyramid from compile checks to hardware certification
- statistical promotion protocol for visual and performance evaluation
- first-90-days execution plan that keeps scope limited to sky rendering
- risk register with dependency and scope pivot criteria
- AGENTS.md rules for documentation integrity, shader ABI, and eval statistics
```

## Scope unchanged

The current milestone remains:

```text
sun + atmosphere + volumetric clouds only
```

Still forbidden before sky certification:

```text
- terrain
- props
- general material objects
- humans
- AI NPC behavior
- voice runtime implementation
- fracture/liquids/gases outside cloud volume implementation
```

## V4 sanity-check result

The package is suitable as a planning and architecture document for beginning implementation, assuming dependency versions are refreshed before the first Cargo scaffold is created.

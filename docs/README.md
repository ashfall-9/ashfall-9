# Ashfall 2 Docs

The writeups in `writeups/` are the source plan. This `docs/` tree records implementation decisions made while turning those writeups into code.

Current milestone: sky only. The implemented workspace is intentionally narrow until the sky path can create reproducible artifacts with telemetry.

## Starting Points

- `writeups/02_workspace_architecture.md`: crate ownership and dependency layers
- `writeups/15_first_implementation_sequence.md`: phase order
- `writeups/23_vulkan_winit_surface_lifecycle.md`: desktop and presentation boundaries
- `writeups/28_dependency_and_capability_matrix.md`: dependency ownership and version policy
- `writeups/30_artifact_and_schema_contracts.md`: eval artifact bundle shape

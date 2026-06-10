# Dependency Decisions

## Initial Rust Workspace

The first implementation pass pins the same basic window/GPU family used by `ashfall-9`:

| Dependency | Version | Owner | Reason |
|---|---:|---|---|
| `winit` | `0.30.13` | `app_desktop` | Current application handler / `run_app` desktop lifecycle. |
| `vulkano` | `0.35.2` | `gfx_vk` | Safe Vulkan bootstrap path. |
| `vulkano-util` | `0.35.0` | `gfx_vk` | Minimal context/device bootstrap matching the existing reference project. |
| `serde` / `serde_json` | `1.x` | `telemetry`, contract crates | Reproducible JSON artifacts and schema structs. |
| `anyhow` | `1.x` | app/tool crates and contracts | Simple error propagation while interfaces are still forming. |
| `image` | `0.25.10` | `renderer_realtime` | Deterministic PNG capture export for headless sky/cloud smoke runs. |
| `burn` | `0.21.0` | `eval_lab` | CPU ndarray neural-network scorer for cloud photorealism gates. |

Boundary note: dependency use is crate-local. Public cross-crate contracts use project-owned data types.

Burn is configured with `default-features = false` and `features = ["std", "ndarray"]` so the initial judge does not pull GPU/model-training backends into renderer timing.

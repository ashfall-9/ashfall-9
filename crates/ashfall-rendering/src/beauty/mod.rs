//! Beauty Mode contracts for player-facing, non-debug rendering.
//!
//! This module is intentionally engine-owned. It does not expose raw Vulkan,
//! Vulkano, winit, or debug-window primitives. The renderer may translate these
//! records into Vulkan resources internally.

pub mod contract;
pub mod detail_budget;
pub mod environment;
pub mod geometry_primitives;
pub mod irregularity;
pub mod material_pages;

pub use contract::*;
pub use detail_budget::*;
pub use environment::*;
pub use geometry_primitives::*;
pub use irregularity::*;
pub use material_pages::*;

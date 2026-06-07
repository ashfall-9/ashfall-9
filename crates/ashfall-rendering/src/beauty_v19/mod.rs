//! V19 Beauty Mode contracts for artifact-free photoreal direction.
//!
//! This module is data-only. It must not expose Vulkan, Vulkano, winit,
//! command buffers, descriptor sets, swapchains, or synchronization objects.
//! Rendering code translates these records into renderer-owned GPU work.

pub mod artifact_policy;
pub mod draw_contract;
pub mod environment;
pub mod frame_budget;
pub mod geometry;
pub mod materials;
pub mod validation;

pub use artifact_policy::*;
pub use draw_contract::*;
pub use environment::*;
pub use frame_budget::*;
pub use geometry::*;
pub use materials::*;
pub use validation::*;

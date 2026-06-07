//! V18 Beauty Mode artifact policy and photoreal texture/light contracts.
//!
//! This module is data-only. It must not expose Vulkan, Vulkano, winit, command buffers,
//! descriptor sets, swapchains, or synchronization objects.

pub mod artifact_policy;
pub mod lighting;
pub mod materials;
pub mod validation;

pub use artifact_policy::*;
pub use lighting::*;
pub use materials::*;
pub use validation::*;

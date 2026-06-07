//! V20 Beauty Mode contracts.
//!
//! This module is intentionally data-first. It must not expose Vulkan, Vulkano,
//! winit, command buffers, descriptor sets, swapchains, or synchronization objects.
//! The renderer translates these records into renderer-owned GPU resources.

pub mod artifact_quarantine;
pub mod environment;
pub mod frame_budget;
pub mod geometry;
pub mod human;
pub mod material_pages;
pub mod scene;
pub mod validation;
pub mod vehicle;
pub mod world;

pub use artifact_quarantine::*;
pub use environment::*;
pub use frame_budget::*;
pub use geometry::*;
pub use human::*;
pub use material_pages::*;
pub use scene::*;
pub use validation::*;
pub use vehicle::*;
pub use world::*;

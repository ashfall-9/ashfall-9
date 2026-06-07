//! V21 strict Beauty contract.
//!
//! This module is intentionally plain Rust data. It does not expose Vulkan,
//! Vulkano, winit, command buffers, descriptor sets, or swapchain state.
//! The renderer owns GPU translation internally.

pub mod artifacts;
pub mod environment;
pub mod frame_budget;
pub mod geometry;
pub mod human;
pub mod materials;
pub mod scene;
pub mod validation;
pub mod vehicle;
pub mod world;

pub use artifacts::*;
pub use environment::*;
pub use frame_budget::*;
pub use geometry::*;
pub use human::*;
pub use materials::*;
pub use scene::*;
pub use validation::*;
pub use vehicle::*;
pub use world::*;

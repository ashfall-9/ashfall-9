//! V16 Beauty Mode contracts for photorealistic, high-performance rendering.
//!
//! This module is intentionally data-only. It must not expose raw Vulkan,
//! Vulkano, winit, command buffers, descriptors, or GPU synchronization objects.
//! GPU Services may translate these records into Vulkan resources internally.

pub mod contract;
pub mod detail_budget;
pub mod environment;
pub mod geometry;
pub mod irregularity;
pub mod material_pages;
pub mod validation;

pub use contract::*;
pub use detail_budget::*;
pub use environment::*;
pub use geometry::*;
pub use irregularity::*;
pub use material_pages::*;
pub use validation::*;

//! V17 Beauty Mode contracts for Ashfall.
//!
//! Purpose:
//! - Make Beauty Mode render real player-facing scene content, not debug boxes.
//! - Keep all contracts data-only so GPU Services can translate them to Vulkan.
//! - Preserve Rust/Vulkan ownership boundaries: no raw Vulkan, Vulkano, winit,
//!   command buffer, descriptor, swapchain, or synchronization types may appear here.

pub mod contract;
pub mod detail_budget;
pub mod draw_list;
pub mod environment;
pub mod geometry;
pub mod irregularity;
pub mod material_pages;
pub mod validation;

pub use contract::*;
pub use detail_budget::*;
pub use draw_list::*;
pub use environment::*;
pub use geometry::*;
pub use irregularity::*;
pub use material_pages::*;
pub use validation::*;

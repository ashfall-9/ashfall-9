//! V22 strict Beauty contract.
//!
//! V22 is a hard reset of the player-facing Beauty path.
//! It is plain Rust data and exposes no Vulkan, Vulkano, winit,
//! command buffer, descriptor, swapchain, or window state.

pub mod artifacts;
pub mod environment;
pub mod frame_budget;
pub mod geometry;
pub mod human;
pub mod materials;
pub mod render_contract;
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
pub use render_contract::*;
pub use scene::*;
pub use validation::*;
pub use vehicle::*;
pub use world::*;

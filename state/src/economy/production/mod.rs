//! Production subdirectory: production cycles, fixed assets, maintenance, geology, weather, disasters.
pub mod disasters;
pub mod fixed_assets;
pub mod geology;
pub mod maintenance;
pub mod production;
pub mod weather;

// Re-export contents of production.rs at the production/ module level.
pub use production::*;

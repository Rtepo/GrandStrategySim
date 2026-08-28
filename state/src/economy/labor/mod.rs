//! Labor subdirectory: demographics, labor market, migration, assimilation.
pub mod assimilation;
pub mod labor;
pub mod labor_config;
pub mod labor_market;
pub mod migration;

// Re-export contents of labor.rs at the labor/ module level.
pub use labor::*;

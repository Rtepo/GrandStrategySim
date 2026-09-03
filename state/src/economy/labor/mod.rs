//! Labor subdirectory: demographics, labor market, migration, assimilation.
pub mod assimilation;
pub mod disability_config;
pub mod education_progression;
pub mod labor;
pub mod labor_config;
pub mod labor_market;
pub mod migration;

// Re-export contents of labor.rs at the labor/ module level.
pub use education_progression::process_education_progression_turn;
pub use education_progression::EducationProgressionResult;
pub use labor::*;

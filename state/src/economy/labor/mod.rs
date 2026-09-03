//! Labor subdirectory: demographics, labor market, migration, assimilation.
pub mod assimilation;
pub mod class_transitions;
pub mod disability_config;
pub mod education_progression;
pub mod labor;
pub mod labor_config;
pub mod labor_market;
pub mod migration;

// Re-export contents of labor.rs at the labor/ module level.
pub use class_transitions::ClassTransitionResult;
pub use class_transitions::process_rural_urban_class_transitions;
pub use education_progression::compute_child_labor_fte;
pub use education_progression::compute_per_tier_education_needs;
pub use education_progression::process_education_progression_turn;
pub use education_progression::translate_school_seat_types;
pub use education_progression::EducationProgressionResult;
pub use labor::*;

//! Utilities system for water, sewage, heating, electricity, and waste management

pub mod config;
pub mod consumption;
pub mod demand;
pub mod grid;
pub mod resolution;
pub mod waste;
pub mod waste_collection;

pub use config::{UtilityConfig, UtilityPricingConfig};
pub use consumption::{process_utility_consumption, UtilityConsumptionResult};
pub use demand::UtilityDemand;
pub use grid::{distribute_utilities, UtilityDistributionResult};
pub use resolution::StrategicResolution;
pub use waste::{Landfill, LandfillData, LandfillUpgrade, WasteProcessingResult};
pub use waste_collection::{process_waste_turn, WasteTurnResult};

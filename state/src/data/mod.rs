//! Static data registries for compile-time safe game data
//!
//! This module contains compile-time initialized registries that replace
//! JSON-based data loading for improved type safety and performance.

pub mod consumption_registry;
pub mod crop_registry;
pub mod perishability_registry;

pub use consumption_registry::{consumption_registry, substitution_matrix, subsistence_config, ConsumptionBasket, NeedTier};
pub use perishability_registry::perishability_registry;

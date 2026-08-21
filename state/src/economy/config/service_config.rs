//! B2C service pricing configuration.
//!
//! This module defines configuration parameters for B2C service pricing
//! in education, healthcare, and other public service sectors.

use serde::{Deserialize, Serialize};

/// Configuration for B2C service pricing.
///
/// Controls the price per unit for education slots, health capacity,
/// and other public services traded in Phase 6.5.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServicePricingConfig {
    /// Price per education slot.
    /// Default 50.0.
    #[serde(default = "default_education_price")]
    pub education_price_per_slot: f64,

    /// Price per health capacity unit.
    /// Default 75.0.
    #[serde(default = "default_health_price")]
    pub health_price_per_capacity: f64,

    /// Default service price for other sectors.
    /// Default 40.0.
    #[serde(default = "default_service_price")]
    pub default_service_price: f64,

    /// Price per unit of information (media B2C service).
    /// Default 30.0.
    #[serde(default = "default_information_price")]
    pub information_price_per_unit: f64,
}

fn default_education_price() -> f64 {
    50.0
}

fn default_health_price() -> f64 {
    75.0
}

fn default_service_price() -> f64 {
    40.0
}

fn default_information_price() -> f64 {
    30.0
}

impl Default for ServicePricingConfig {
    fn default() -> Self {
        Self {
            education_price_per_slot: default_education_price(),
            health_price_per_capacity: default_health_price(),
            default_service_price: default_service_price(),
            information_price_per_unit: default_information_price(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = ServicePricingConfig::default();
        assert_eq!(config.education_price_per_slot, 50.0);
        assert_eq!(config.health_price_per_capacity, 75.0);
        assert_eq!(config.default_service_price, 40.0);
        assert_eq!(config.information_price_per_unit, 30.0);
    }
}

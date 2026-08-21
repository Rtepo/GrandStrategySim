//! Infrastructure funding and production configuration.
//!
//! This module defines configuration parameters for infrastructure building
//! funding allocation, procurement, and production in Phase 7.2.

use serde::{Deserialize, Serialize};

/// Configuration for infrastructure funding and production.
///
/// Controls the cost per worker for different infrastructure building types,
/// used in funding allocation and production execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InfrastructureConfig {
    /// Cost per worker for education buildings.
    /// Default 100.0.
    #[serde(default = "default_education_cost")]
    pub education_cost_per_worker: f64,

    /// Cost per worker for healthcare buildings.
    /// Default 150.0.
    #[serde(default = "default_healthcare_cost")]
    pub healthcare_cost_per_worker: f64,

    /// Cost per worker for municipal/public buildings.
    /// Default 80.0.
    #[serde(default = "default_municipal_cost")]
    pub municipal_cost_per_worker: f64,

    /// Default cost per worker for other sectors.
    /// Default 50.0.
    #[serde(default = "default_cost")]
    pub default_cost_per_worker: f64,
}

fn default_education_cost() -> f64 {
    100.0
}

fn default_healthcare_cost() -> f64 {
    150.0
}

fn default_municipal_cost() -> f64 {
    80.0
}

fn default_cost() -> f64 {
    50.0
}

impl Default for InfrastructureConfig {
    fn default() -> Self {
        Self {
            education_cost_per_worker: default_education_cost(),
            healthcare_cost_per_worker: default_healthcare_cost(),
            municipal_cost_per_worker: default_municipal_cost(),
            default_cost_per_worker: default_cost(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = InfrastructureConfig::default();
        assert_eq!(config.education_cost_per_worker, 100.0);
        assert_eq!(config.healthcare_cost_per_worker, 150.0);
        assert_eq!(config.municipal_cost_per_worker, 80.0);
        assert_eq!(config.default_cost_per_worker, 50.0);
    }
}

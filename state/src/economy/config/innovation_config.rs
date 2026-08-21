//! Innovation trading and royalty configuration.
//!
//! This module defines configuration parameters for Innovation Points B2B
//! trading and default royalty rates in Phase 7.

use serde::{Deserialize, Serialize};

/// Configuration for innovation trading and royalty defaults.
///
/// Controls the market price for Innovation Points traded between
/// universities and the State, and default royalty ratios.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InnovationConfig {
    /// Base market price per Innovation Point (B2B purchase from private universities).
    /// Default 100.0.
    #[serde(default = "default_innovation_point_price")]
    pub innovation_point_price: f64,

    /// Default royalty VWAP ratio when a patent doesn't specify one.
    /// Default 0.05 (5% of output commodity VWAP).
    #[serde(default = "default_royalty_vwap_ratio")]
    pub default_royalty_vwap_ratio: f64,
}

fn default_innovation_point_price() -> f64 {
    100.0
}

fn default_royalty_vwap_ratio() -> f64 {
    0.05
}

impl Default for InnovationConfig {
    fn default() -> Self {
        Self {
            innovation_point_price: default_innovation_point_price(),
            default_royalty_vwap_ratio: default_royalty_vwap_ratio(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = InnovationConfig::default();
        assert_eq!(config.innovation_point_price, 100.0);
        assert_eq!(config.default_royalty_vwap_ratio, 0.05);
    }
}

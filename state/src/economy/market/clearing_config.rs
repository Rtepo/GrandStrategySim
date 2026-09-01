//! Market clearing configuration (Phase 86.5A).
//!
//! Extracts CRITICAL magic numbers from `economy/market/clearing.rs` into a
//! serializable config struct.

use serde::{Deserialize, Serialize};

/// Configuration for market clearing price bounds and fallbacks.
///
/// Replaces hardcoded constants in `clearing.rs` with configurable values.
/// Price floor and cap are multipliers of the immutable base price to
/// prevent f64 hyperinflation overflows while allowing dynamic market prices.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketClearingConfig {
    /// Price floor as a fraction of the immutable base price.
    /// Prevents prices from collapsing to zero during deflation.
    #[serde(default = "default_price_floor")]
    pub price_floor: f64,

    /// Price cap as a multiple of the immutable base price.
    /// Prevents f64 overflow during hyperinflation.
    #[serde(default = "default_price_cap")]
    pub price_cap: f64,

    /// Fallback base price when global market has no data for a commodity.
    /// This is a nominal value used only when no market history exists.
    #[serde(default = "default_fallback_base_price")]
    pub fallback_base_price: f64,

    /// Shortage premium cap: maximum price above import price during shortage.
    /// E.g., 2.0 means price can at most double the import price.
    #[serde(default = "default_shortage_cap_multiplier")]
    pub shortage_cap_multiplier: f64,

    /// Surplus floor: minimum price below export price during surplus.
    /// E.g., 0.5 means price can at most halve the export price.
    #[serde(default = "default_surplus_floor_multiplier")]
    pub surplus_floor_multiplier: f64,

    /// Coverage smoothing factor for price interpolation.
    /// 0.0 = no smoothing, 1.0 = full smoothing.
    #[serde(default = "default_coverage_smoothing")]
    pub coverage_smoothing: f64,
}

fn default_price_floor() -> f64 {
    0.2
}
fn default_price_cap() -> f64 {
    5.0
}
fn default_fallback_base_price() -> f64 {
    100.0
}
fn default_shortage_cap_multiplier() -> f64 {
    2.0
}
fn default_surplus_floor_multiplier() -> f64 {
    0.5
}
fn default_coverage_smoothing() -> f64 {
    1.0
}

impl Default for MarketClearingConfig {
    fn default() -> Self {
        MarketClearingConfig {
            price_floor: default_price_floor(),
            price_cap: default_price_cap(),
            fallback_base_price: default_fallback_base_price(),
            shortage_cap_multiplier: default_shortage_cap_multiplier(),
            surplus_floor_multiplier: default_surplus_floor_multiplier(),
            coverage_smoothing: default_coverage_smoothing(),
        }
    }
}

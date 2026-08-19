//! B2B order submission configuration.
//!
//! This module defines configuration parameters for automated B2B order
//! submission, dynamic pricing, cash encumbrance, and inventory overflow
//! handling in Phase 6.3 and 6.4b.

use serde::{Deserialize, Serialize};

/// Configuration for B2B order submission and dynamic pricing.
///
/// Controls how companies calculate Buy Bids and Sell Asks based on
/// production method BOMs, inventory utilization, and available cash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct B2bOrderConfig {
    /// Maximum fraction of available_cash to encumber for input purchases.
    /// Default 0.8 (80% of available cash).
    #[serde(rename = "maks_obciążenie_gotówki", default = "default_max_cash_encumbrance")]
    pub max_cash_encumbrance_ratio: f64,

    /// Minimum markup when inventory is at maximum capacity (fire sale).
    /// Default 0.0 (sell at cost).
    #[serde(rename = "minimalna_marża", default = "default_min_markup")]
    pub min_markup_ratio: f64,

    /// Maximum markup when inventory is empty (scarcity premium).
    /// Default 2.0 (3x cost).
    #[serde(rename = "maksymalna_marża", default = "default_max_markup")]
    pub max_markup_ratio: f64,

    /// Inventory utilization threshold above which fire-sale pricing kicks in.
    /// Default 0.8 (80% full).
    #[serde(rename = "próg_wyprzedaży", default = "default_fire_sale_threshold")]
    pub fire_sale_threshold: f64,

    /// Inventory utilization threshold below which scarcity pricing kicks in.
    /// Default 0.2 (20% full).
    #[serde(rename = "próg_niedoboru", default = "default_scarcity_threshold")]
    pub scarcity_threshold: f64,

    /// Maximum inventory capacity per building (tons units).
    /// Default 10000.0.
    #[serde(rename = "pojemność_magazynu_budynku", default = "default_max_building_inventory")]
    pub max_building_inventory: f64,

    /// Storage fee per ton per turn for warehouse overflow.
    /// Default 1.0.
    #[serde(rename = "opłata_magazynowa_za_tonę", default = "default_warehouse_storage_fee")]
    pub warehouse_storage_fee_per_ton: f64,

    /// Small premium added to reference price when submitting Buy Bids.
    /// Default 0.05 (5% above reference price).
    #[serde(rename = "premia_za_zakup", default = "default_buy_premium")]
    pub buy_premium_ratio: f64,

    /// Phase 25: Fraction of the commodity encumbrance reserved for freight costs.
    /// When a company encumbers cash for a B2B buy bid, it also reserves this
    /// fraction extra to cover freight procurement. Default 0.30 (30% extra).
    #[serde(rename = "rezerwa_koszty_frachtu", default = "default_freight_reserve")]
    pub freight_cost_reserve_ratio: f64,
}

fn default_max_cash_encumbrance() -> f64 {
    0.8
}

fn default_min_markup() -> f64 {
    0.0
}

fn default_max_markup() -> f64 {
    2.0
}

fn default_fire_sale_threshold() -> f64 {
    0.8
}

fn default_scarcity_threshold() -> f64 {
    0.2
}

fn default_max_building_inventory() -> f64 {
    10000.0
}

fn default_warehouse_storage_fee() -> f64 {
    1.0
}

fn default_buy_premium() -> f64 {
    0.05
}

fn default_freight_reserve() -> f64 {
    // Phase 31: Reduced from 0.30 to 0.15 after fixing the freight cost
    // dimensional bug. With the bug fixed, fuel cost is no longer multiplied
    // by base_rate, so freight costs are ~7× lower. The reserve ratio of 0.15
    // is sufficient to cover the corrected freight costs.
    0.15
}

impl Default for B2bOrderConfig {
    fn default() -> Self {
        Self {
            max_cash_encumbrance_ratio: default_max_cash_encumbrance(),
            min_markup_ratio: default_min_markup(),
            max_markup_ratio: default_max_markup(),
            fire_sale_threshold: default_fire_sale_threshold(),
            scarcity_threshold: default_scarcity_threshold(),
            max_building_inventory: default_max_building_inventory(),
            warehouse_storage_fee_per_ton: default_warehouse_storage_fee(),
            buy_premium_ratio: default_buy_premium(),
            freight_cost_reserve_ratio: default_freight_reserve(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = B2bOrderConfig::default();
        assert_eq!(config.max_cash_encumbrance_ratio, 0.8);
        assert_eq!(config.min_markup_ratio, 0.0);
        assert_eq!(config.max_markup_ratio, 2.0);
        assert_eq!(config.fire_sale_threshold, 0.8);
        assert_eq!(config.scarcity_threshold, 0.2);
        assert_eq!(config.max_building_inventory, 10000.0);
        assert_eq!(config.warehouse_storage_fee_per_ton, 1.0);
        assert_eq!(config.buy_premium_ratio, 0.05);
    }

    #[test]
    fn custom_config() {
        let config = B2bOrderConfig {
            max_cash_encumbrance_ratio: 0.5,
            min_markup_ratio: 0.1,
            max_markup_ratio: 3.0,
            fire_sale_threshold: 0.7,
            scarcity_threshold: 0.3,
            max_building_inventory: 5000.0,
            warehouse_storage_fee_per_ton: 2.0,
            buy_premium_ratio: 0.1,
            freight_cost_reserve_ratio: 0.3,
        };
        assert_eq!(config.max_cash_encumbrance_ratio, 0.5);
        assert_eq!(config.max_markup_ratio, 3.0);
    }
}

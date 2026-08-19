//! Historical price registry for fallback prices and VWAP calculation.
//!
//! This module provides a deterministic price fallback chain to avoid magic numbers:
//! VWAP (previous turn) → Last trade price → Global base price from market.json.

use crate::registries::enums::Commodity;
use crate::economy::order_book::Trade;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Historical price data for fallback reference.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MarketHistory {
    /// Volume-Weighted Average Price per commodity (previous turn).
    pub vwap_per_commodity: HashMap<Commodity, f64>,
    /// Last known trade price per commodity.
    pub last_trade_price: HashMap<Commodity, f64>,
    /// Global base prices from market.json.
    pub global_base_prices: HashMap<Commodity, f64>,
    /// Phase 25: B2C retail VWAP per commodity (consumer prices paid at retail stores).
    /// Updated after B2C clearing each turn. Used by CPI to track actual consumer prices.
    #[serde(default)]
    pub retail_vwap_per_commodity: HashMap<Commodity, f64>,
    /// Phase 33: Previous-turn net surplus per commodity.
    /// Updated at the end of each turn from market.net_surplus.
    /// Used by the UI to compute turn-over-turn % change in surplus.
    #[serde(default)]
    pub prev_net_surplus: HashMap<Commodity, f64>,
}

/// Get reference price using fallback chain.
///
/// # Arguments
/// * `commodity` - Commodity to get price for.
/// * `history` - Market history with fallback data.
///
/// # Returns
/// * `Some(f64)` - Reference price if available.
/// * `None` - No reference price available (no trade occurs).
///
/// # Rules
/// * Fallback chain: VWAP → Last trade → Global base.
/// * If no reference price exists, order submission fails or uses zero.
pub fn get_reference_price(commodity: &Commodity, history: &MarketHistory) -> Option<f64> {
    // 1. Previous turn VWAP
    if let Some(vwap) = history.vwap_per_commodity.get(commodity) {
        return Some(*vwap);
    }

    // 2. Last known trade price
    if let Some(last) = history.last_trade_price.get(commodity) {
        return Some(*last);
    }

    // 3. Global base price from market.json
    history.global_base_prices.get(commodity).copied()
}

/// Update VWAP after matching for next turn's reference.
///
/// # Arguments
/// * `history` - Mutable reference to market history.
/// * `trades` - Executed trades this turn.
///
/// # Rules
/// * Calculates VWAP as total value / total volume per commodity.
/// * Updates both vwap_per_commodity and last_trade_price.
pub fn update_vwap(history: &mut MarketHistory, trades: &[Trade]) {
    let mut volume_per_commodity: HashMap<Commodity, f64> = HashMap::new();
    let mut value_per_commodity: HashMap<Commodity, f64> = HashMap::new();

    for trade in trades {
        *volume_per_commodity
            .entry(trade.commodity.clone())
            .or_insert(0.0) += trade.quantity;
        *value_per_commodity
            .entry(trade.commodity.clone())
            .or_insert(0.0) += trade.quantity * trade.execution_price;
    }

    for (commodity, volume) in volume_per_commodity {
        if volume > 0.0 {
            let value = value_per_commodity.get(&commodity).unwrap();
            let vwap = value / volume;
            history.vwap_per_commodity.insert(commodity, vwap);
            history.last_trade_price.insert(commodity, vwap);
        }
    }

    // Phase 45: REMOVED VWAP base-price seeding.
    // Previously, commodities with no trades had their VWAP set to the global
    // base price (100.00), which anchored prices artificially and prevented
    // organic price discovery. Now, VWAP is only updated from actual executed
    // trades. The fallback chain in get_reference_price still works:
    //   1. Previous VWAP (from last actual trade)
    //   2. Last trade price
    //   3. Global base price (last resort only)
    // This allows prices to shift organically from supply/demand.
}

/// Phase 25: Update retail VWAP from B2C clearing results.
///
/// Computes a volume-weighted average of retail prices per commodity from
/// the B2C market. This is used by the CPI to track actual consumer prices,
/// not B2B wholesale prices.
///
/// # Arguments
/// * `history` - Mutable reference to market history.
/// * `retail_prices` - Slice of (commodity, quantity_sold, price_per_unit) tuples.
pub fn update_retail_vwap(
    history: &mut MarketHistory,
    retail_prices: &[(Commodity, f64, f64)],
) {
    let mut volume_per_commodity: HashMap<Commodity, f64> = HashMap::new();
    let mut value_per_commodity: HashMap<Commodity, f64> = HashMap::new();

    for (commodity, qty, price) in retail_prices {
        if *qty > 0.0 && *price > 0.0 {
            *volume_per_commodity
                .entry(commodity.clone())
                .or_insert(0.0) += qty;
            *value_per_commodity
                .entry(commodity.clone())
                .or_insert(0.0) += qty * price;
        }
    }

    for (commodity, volume) in volume_per_commodity {
        if volume > 0.0 {
            let value = value_per_commodity.get(&commodity).unwrap();
            let vwap = value / volume;
            history.retail_vwap_per_commodity.insert(commodity, vwap);
        }
    }
}

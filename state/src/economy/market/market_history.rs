//! Historical price registry for fallback prices and VWAP calculation.
//!
//! This module provides a deterministic price fallback chain to avoid magic numbers:
//! VWAP (previous turn) → Last trade price → Global base price from market.json.

use crate::registries::enums::Commodity;
use crate::economy::order_book::Trade;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Hot-path hash map alias for market history internals.
pub type HashMap<K, V> = FxHashMap<K, V>;

/// Phase 79: Window size for rolling VWAP history (24 turns = 2 game-years).
/// Provides a stable baseline for SRA shock-responsive price triggers.
pub const VWAP_HISTORY_WINDOW: usize = 24;

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
    /// Phase 79: Rolling VWAP window (last N turns) per commodity.
    /// Used by the Strategic Reserve Agency for shock-responsive price triggers.
    /// Updated after `update_vwap()` each turn via `update_vwap_history()`.
    #[serde(default)]
    pub vwap_history: HashMap<Commodity, VecDeque<f64>>,
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
    let mut volume_per_commodity: HashMap<Commodity, f64> = HashMap::default();
    let mut value_per_commodity: HashMap<Commodity, f64> = HashMap::default();

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
    let mut volume_per_commodity: HashMap<Commodity, f64> = HashMap::default();
    let mut value_per_commodity: HashMap<Commodity, f64> = HashMap::default();

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

/// Phase 79: Update rolling VWAP history after `update_vwap()`.
///
/// Pushes the current turn's VWAP for each commodity into a `VecDeque` and
/// trims to `VWAP_HISTORY_WINDOW` entries. This provides a multi-turn moving
/// average baseline for the Strategic Reserve Agency's shock-responsive
/// price triggers.
///
/// # Arguments
/// * `history` - Mutable reference to market history.
/// * `max_window` - Maximum number of turns to retain (default: `VWAP_HISTORY_WINDOW`).
pub fn update_vwap_history(history: &mut MarketHistory, max_window: usize) {
    for (&commodity, &vwap) in &history.vwap_per_commodity {
        let deque = history.vwap_history.entry(commodity).or_default();
        deque.push_back(vwap);
        while deque.len() > max_window {
            deque.pop_front();
        }
    }
}

/// Phase 79: Compute the moving-average VWAP for a commodity.
///
/// Returns the arithmetic mean of the stored VWAP values, or `None` if
/// insufficient data exists (empty history). Callers should fall back to
/// `global_market.base_price()` when this returns `None`.
///
/// # Arguments
/// * `history` - Market history with rolling VWAP data.
/// * `commodity` - Commodity to query.
///
/// # Returns
/// * `Some(f64)` - Average VWAP over the stored window.
/// * `None` - No VWAP history for this commodity.
pub fn moving_average_vwap(history: &MarketHistory, commodity: &Commodity) -> Option<f64> {
    let deque = history.vwap_history.get(commodity)?;
    if deque.is_empty() {
        return None;
    }
    let sum: f64 = deque.iter().sum();
    Some(sum / deque.len() as f64)
}

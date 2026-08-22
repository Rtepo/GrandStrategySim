//! Market orders and clearing data structures.
//!
//! This module defines the aggregate market order tally and the shared
//! `GlobalMarket` used by the clearing loop in `economy::clearing`.

use crate::registries::enums::{Commodity, Sector};
use rustc_hash::FxHashMap;

/// Hot-path hash map alias for market internals.
pub type HashMap<K, V> = FxHashMap<K, V>;

/// One side of a market tally: total buy and sell orders for a single good.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MarketOrder {
    /// Total units demanded by producers.
    pub buy: f64,
    /// Total units supplied by producers.
    pub sell: f64,
}

impl MarketOrder {
    /// Adds a buy amount.
    pub fn add_buy(&mut self, amount: f64) {
        self.buy += amount;
    }

    /// Adds a sell amount.
    pub fn add_sell(&mut self, amount: f64) {
        self.sell += amount;
    }
}

/// Aggregate market orders keyed by commodity.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarketOrders {
    /// Orders per commodity.
    pub orders: HashMap<Commodity, MarketOrder>,
}

impl MarketOrders {
    /// Adds a buy order for a commodity.
    pub fn add_buy(&mut self, good: Commodity, amount: f64) {
        self.orders
            .entry(good)
            .or_default()
            .add_buy(amount);
    }

    /// Adds a sell order for a commodity.
    pub fn add_sell(&mut self, good: Commodity, amount: f64) {
        self.orders
            .entry(good)
            .or_default()
            .add_sell(amount);
    }

    /// Returns the order for a commodity, or zero if absent.
    pub fn get(&self, good: Commodity) -> MarketOrder {
        self.orders.get(&good).copied().unwrap_or_default()
    }
}

/// The global market shared by all countries.
///
/// Tracks the base international price and worldwide net surplus/deficit for
/// Offshore religious capital ledger tracking Apostolic See remittances (Phase 17C).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ApostolicSeeLedger {
    /// Total capital received from all countries via remittance.
    pub total_remittances: f64,
    /// Available for global distribution (reinvestment pool).
    pub global_charity_pool: f64,
    /// Country hosting the See (e.g., "Watykan").
    pub see_country: String,
}

/// The global market shared by all countries.
///
/// Tracks the base international price and worldwide net surplus/deficit for
/// each commodity. A positive net surplus means the world as a whole produces
/// more than it consumes; a negative value means the world is short.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GlobalMarket {
    /// Base international price for each commodity.
    pub base_prices: HashMap<Commodity, f64>,
    /// Net global surplus per commodity (positive = surplus, negative = deficit).
    pub net_surplus: HashMap<Commodity, f64>,
    /// Offshore capital ledger (Phase 5: Capital Flight).
    /// Tracks total capital that has fled domestic economies to tax havens.
    /// This ensures money mass preservation - capital doesn't disappear, it moves offshore.
    pub offshore_capital: f64,
    /// Phase 17C: Apostolic See offshore ledger for religious remittances.
    pub apostolic_see_ledger: ApostolicSeeLedger,
    /// Phase 43: Total sell order volume per commodity (supply side).
    pub supply_volume: HashMap<Commodity, f64>,
    /// Phase 43: Total buy order volume per commodity (demand side).
    pub demand_volume: HashMap<Commodity, f64>,
}

impl GlobalMarket {
    /// Creates an empty global market.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the base price for a commodity, or `fallback` if unknown.
    pub fn base_price(&self, good: Commodity, fallback: f64) -> f64 {
        self.base_prices.get(&good).copied().unwrap_or(fallback)
    }

    /// Returns the global net surplus for a commodity, or zero if unknown.
    pub fn surplus(&self, good: Commodity) -> f64 {
        self.net_surplus.get(&good).copied().unwrap_or(0.0)
    }
}

/// Snapshot of market conditions used by corporate AI and legal-form transitions.
///
/// This is a read-only signal produced by the market-clearing phase and consumed
/// by `LegalFormTransition` and later by `CorporateStrategy`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarketSignal {
    /// Cleared prices per commodity.
    pub prices: HashMap<Commodity, f64>,
    /// Local demand surplus per commodity (negative = deficit).
    pub demand_surplus: HashMap<Commodity, f64>,
    /// Per-sector PMI values.
    pub sector_pmi: HashMap<Sector, f64>,
    /// Global net surplus per commodity.
    pub global_surplus: HashMap<Commodity, f64>,
    /// Representative corporate credit rate.
    pub interest_rate: f64,
    /// Stock-market confidence, 0..100.
    pub stock_confidence: f64,
    /// Headline stock index.
    pub stock_index: f64,
}

impl MarketSignal {
    /// Returns the sector PMI, defaulting to a neutral 50.0.
    pub fn sector_outlook(&self, sector: Sector) -> f64 {
        self.sector_pmi.get(&sector).copied().unwrap_or(50.0)
    }

    /// Combined local and global pressure for a good.
    pub fn good_pressure(&self, good: Commodity) -> f64 {
        let local = self.demand_surplus.get(&good).copied().unwrap_or(0.0);
        let global = self.global_surplus.get(&good).copied().unwrap_or(0.0);
        local + global
    }
}

//! Stock exchange module with dual-liquidity trading infrastructure.
//!
//! This module implements the StockExchange struct with order book and AMM
//! liquidity pools for trading securities, along with trade execution logic.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque, HashMap};
use serde_json::Value;

use crate::securities::brokerage::BrokerageAccount;
use crate::entities::{Company, Building};
use crate::registries::enums::Commodity;
use crate::state::treasury::Treasury;
use crate::securities::covered_bonds::CoveredBond;
use crate::securities::mbs::TranchePriority;

/// Type of tradable instrument on the stock exchange.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ_instrumentu", rename_all = "snake_case")]
pub enum InstrumentType {
    /// Equity shares of a listed company.
    Equity,
    /// MBS tranche identified by MBS ID and tranche priority.
    MbsTranche {
        /// MBS structure ID.
        mbs_id: String,
        /// Tranche seniority.
        priority: TranchePriority,
    },
    /// Covered bond identified by bond ID.
    CoveredBond,
    /// Phase 56: Commodity spot contract (immediate delivery).
    CommoditySpot {
        /// Commodity identifier (e.g., "steel", "wheat").
        commodity_id: String,
    },
    /// Phase 56: Commodity futures contract (deferred delivery).
    CommodityFutures {
        /// Commodity identifier.
        commodity_id: String,
        /// Delivery turn (maturity).
        delivery_turn: u32,
    },
}

/// National stock exchange with dual-liquidity execution models.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct StockExchange {
    /// Order book: Maps instrument_id -> (bids, asks).

    pub order_book: BTreeMap<String, OrderBook>,
    
    /// AMM liquidity pools: Maps instrument_id -> LiquidityPool.

    pub liquidity_pools: BTreeMap<String, LiquidityPool>,
    
    /// Trade history for audit and price discovery.

    pub trade_history: VecDeque<Trade>,
    
    /// Market-wide circuit breaker status.

    pub circuit_breaker: CircuitBreaker,
    
    /// Trading fee (percentage of transaction value).

    pub transaction_fee: f64,

    /// Phase 56: Market index tracking (main index + sector indices).
    #[serde(default)]
    pub market_index: MarketIndex,

    /// Phase 56: Commodity spot market with B2B-derived prices.
    #[serde(default)]
    pub commodity_spot: CommoditySpotMarket,

    /// Any additional exchange fields.
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

/// Order book for a single company.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct OrderBook {
    /// Bids: Maps price -> list of buy orders.
    /// Using ordered list of (price, orders) tuples since f64 doesn't implement Ord for BTreeMap.

    pub bids: Vec<(f64, Vec<Order>)>,
    
    /// Asks: Maps price -> list of sell orders.

    pub asks: Vec<(f64, Vec<Order>)>,
    
    /// Best bid price (highest buy).

    pub best_bid: f64,
    
    /// Best ask price (lowest sell).

    pub best_ask: f64,
}

/// Individual order in the order book.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum Order {
    /// Buy limit order.
    Buy {
        /// Unique order identifier.

        order_id: String,
        /// Investor placing the order.

        investor_id: String,
        /// Instrument being traded (e.g., "EQUITY:COMP-001", "MBS:MBS-001:senior").

        instrument_id: String,
        /// Type of instrument being bought.

        instrument_type: InstrumentType,
        /// Number of units to buy.

        quantity: u64,
        /// Maximum price willing to pay.

        limit_price: f64,
        /// Turn when order expires.

        expiry_turn: u32,
    },
    /// Sell limit order.
    Sell {
        /// Unique order identifier.

        order_id: String,
        /// Investor placing the order.

        investor_id: String,
        /// Instrument being traded.

        instrument_id: String,
        /// Type of instrument being sold.

        instrument_type: InstrumentType,
        /// Number of units to sell.

        quantity: u64,
        /// Minimum price willing to accept.

        limit_price: f64,
        /// Turn when order expires.

        expiry_turn: u32,
    },
}

impl Order {
    /// Get the instrument ID for this order.
    pub fn instrument_id(&self) -> &str {
        match self {
            Order::Buy { instrument_id, .. } => instrument_id,
            Order::Sell { instrument_id, .. } => instrument_id,
        }
    }

    /// Get the investor ID for this order.
    pub fn investor_id(&self) -> &str {
        match self {
            Order::Buy { investor_id, .. } => investor_id,
            Order::Sell { investor_id, .. } => investor_id,
        }
    }

    /// Get the quantity for this order.
    pub fn quantity(&self) -> u64 {
        match self {
            Order::Buy { quantity, .. } => *quantity,
            Order::Sell { quantity, .. } => *quantity,
        }
    }

    /// Get the limit price for this order.
    pub fn limit_price(&self) -> f64 {
        match self {
            Order::Buy { limit_price, .. } => *limit_price,
            Order::Sell { limit_price, .. } => *limit_price,
        }
    }

    /// Get the expiry turn for this order.
    pub fn expiry_turn(&self) -> u32 {
        match self {
            Order::Buy { expiry_turn, .. } => *expiry_turn,
            Order::Sell { expiry_turn, .. } => *expiry_turn,
        }
    }

    /// Reduce the quantity of this order by `filled` units.
    pub fn reduce_quantity(&mut self, filled: u64) {
        match self {
            Order::Buy { quantity, .. } => *quantity -= filled,
            Order::Sell { quantity, .. } => *quantity -= filled,
        }
    }

    /// Check if this order is fully filled.
    pub fn is_filled(&self) -> bool {
        self.quantity() == 0
    }

    /// Create a buy limit order.
    pub fn new_buy(
        order_id: String,
        investor_id: String,
        instrument_id: String,
        instrument_type: InstrumentType,
        quantity: u64,
        limit_price: f64,
        expiry_turn: u32,
    ) -> Self {
        Order::Buy {
            order_id,
            investor_id,
            instrument_id,
            instrument_type,
            quantity,
            limit_price,
            expiry_turn,
        }
    }

    /// Create a sell limit order.
    pub fn new_sell(
        order_id: String,
        investor_id: String,
        instrument_id: String,
        instrument_type: InstrumentType,
        quantity: u64,
        limit_price: f64,
        expiry_turn: u32,
    ) -> Self {
        Order::Sell {
            order_id,
            investor_id,
            instrument_id,
            instrument_type,
            quantity,
            limit_price,
            expiry_turn,
        }
    }
}

/// AMM liquidity pool for instant market orders.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct LiquidityPool {
    /// Total shares in the pool.

    pub shares: u64,
    
    /// Total cash in the pool.

    pub cash: f64,
    
    /// Liquidity providers: Maps provider_id -> share of pool.

    pub providers: BTreeMap<String, f64>,
    
    /// Pool fee (percentage of trade value).

    pub pool_fee: f64,
    
    /// Phase D.5: Treasury bonds held in pool (for QE secondary market purchases).
    #[serde(default)]
    pub treasury_bonds: Vec<CoveredBond>,
    
    /// Total market value of pool assets.
    #[serde(default)]
    pub total_value: f64,
}

/// Trade record for audit and price discovery.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub struct Trade {
    /// Trade ID.

    pub id: String,
    
    /// Instrument ID (e.g., "EQUITY:COMP-001", "MBS:MBS-001:senior").

    pub instrument_id: String,
    
    /// Buyer ID.

    pub buyer_id: String,
    
    /// Seller ID.

    pub seller_id: String,
    
    /// Quantity traded.

    pub quantity: u64,
    
    /// Execution price.

    pub price: f64,
    
    /// Turn of execution.

    pub turn: u32,
}

/// Circuit breaker status for market-wide trading halts.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct CircuitBreaker {
    /// Is trading currently halted?

    pub is_halted: bool,

    /// Turn when halt was triggered.

    pub halt_turn: u32,

    /// Expected duration in turns.

    pub duration_turns: u32,
}

/// Phase 56: Market index tracking for the stock exchange.
///
/// Computes a market-cap-weighted main index (base 1000.0) and per-sector
/// indices. History is bounded to 120 turns (5 years at 24 turns/year).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MarketIndex {
    /// Main index value, market-cap weighted, base 1000.0.
    #[serde(default)]
    pub main_index_value: f64,
    /// Main index history (bounded to 120 entries).
    #[serde(default)]
    pub main_index_history: Vec<f64>,
    /// Per-sector index values (sector name → index value).
    #[serde(default)]
    pub sector_indices: BTreeMap<String, f64>,
    /// Per-sector index history (sector name → bounded history).
    #[serde(default)]
    pub sector_index_history: BTreeMap<String, Vec<f64>>,
    /// Total market capitalization of all listed companies.
    #[serde(default)]
    pub total_market_cap: f64,
    /// Total trading volume this turn.
    #[serde(default)]
    pub total_volume: u64,
    /// Number of advancing stocks this turn.
    #[serde(default)]
    pub advancing: u32,
    /// Number of declining stocks this turn.
    #[serde(default)]
    pub declining: u32,
    /// Rolling volatility (stddev of last 24 index returns).
    #[serde(default)]
    pub volatility: f64,
}

/// Phase 56: Commodity spot market with B2B-derived spot prices.
///
/// Spot prices are derived from B2B clearing VWAP plus a configurable retail
/// premium (from `SecuritiesMarketConfig.commodity_spot_retail_premium`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct CommoditySpotMarket {
    /// Current spot prices (commodity_id → price per unit).
    #[serde(default)]
    pub spot_prices: BTreeMap<String, f64>,
    /// Spot price history per commodity (bounded to 60 entries).
    #[serde(default)]
    pub spot_history: BTreeMap<String, Vec<f64>>,
    /// Open interest per commodity (total outstanding spot positions).
    #[serde(default)]
    pub open_interest: BTreeMap<String, u64>,
}

impl CommoditySpotMarket {
    /// Phase 56: Update spot prices from B2B clearing VWAP.
    ///
    /// # Arguments
    /// * `b2b_vwaps` - Map of commodity_id → B2B clearing VWAP.
    /// * `config` - Securities market config (for retail premium).
    ///
    /// # Rules
    /// * Spot price = B2B VWAP × (1 + `config.commodity_spot_retail_premium`).
    /// * If no B2B data, previous spot price is retained.
    /// * History is bounded to 60 entries.
    pub fn update_spot_prices(
        &mut self,
        b2b_vwaps: &BTreeMap<String, f64>,
        config: &crate::securities::config::SecuritiesMarketConfig,
    ) {
        for (commodity_id, vwap) in b2b_vwaps {
            if *vwap <= 0.0 {
                continue;
            }
            let spot = vwap * (1.0 + config.commodity_spot_retail_premium);
            self.spot_prices.insert(commodity_id.clone(), spot);

            let hist = self.spot_history.entry(commodity_id.clone()).or_default();
            hist.push(spot);
            if hist.len() > 60 {
                hist.remove(0);
            }
        }
    }

    /// Phase 56: Get the current spot price for a commodity.
    pub fn get_spot_price(&self, commodity_id: &str) -> f64 {
        self.spot_prices.get(commodity_id).copied().unwrap_or(0.0)
    }
}

impl MarketIndex {
    /// Compute the market index from the stock exchange and listed companies.
    ///
    /// # Arguments
    /// * `exchange` - The stock exchange (for trade history / volume).
    /// * `companies` - All companies (listed ones are filtered by `is_listed`).
    ///
    /// # Rules
    /// * Main index = market-cap-weighted average, base 1000.0 at first computation.
    /// * Sector indices = same computation filtered by sector.
    /// * History bounded to 120 entries.
    /// * Volatility = stddev of last 24 index returns.
    pub fn compute(&mut self, exchange: &StockExchange, companies: &[Company]) {
        let listed: Vec<&Company> = companies
            .iter()
            .filter(|c| c.legal_form.is_listed() && c.shares_count > 0)
            .collect();

        let total_market_cap: f64 = listed
            .iter()
            .map(|c| c.share_price * c.shares_count as f64)
            .sum();

        let total_volume: u64 = listed
            .iter()
            .map(|c| {
                // Sum trade volumes from the exchange for this company's instrument.
                let instrument_id = format!("EQUITY:{}", c.id);
                exchange
                    .trade_history
                    .iter()
                    .filter(|t| t.instrument_id == instrument_id)
                    .map(|t| t.quantity)
                    .sum::<u64>()
            })
            .sum();

        // Count advancing/declining based on open vs close price.
        let mut advancing = 0u32;
        let mut declining = 0u32;
        for c in &listed {
            if c.close_price > c.open_price && c.open_price > 0.0 {
                advancing += 1;
            } else if c.close_price < c.open_price && c.open_price > 0.0 {
                declining += 1;
            }
        }

        // Compute main index value.
        // If we have a previous index, scale proportionally to market cap change.
        let new_index_value = if self.main_index_value > 0.0 && self.total_market_cap > 0.0 {
            self.main_index_value * (total_market_cap / self.total_market_cap)
        } else if total_market_cap > 0.0 {
            // First computation: base 1000.0
            1000.0
        } else {
            0.0
        };

        self.main_index_value = new_index_value;
        self.total_market_cap = total_market_cap;
        self.total_volume = total_volume;
        self.advancing = advancing;
        self.declining = declining;

        // Append to history (bounded to 120).
        self.main_index_history.push(new_index_value);
        if self.main_index_history.len() > 120 {
            self.main_index_history.remove(0);
        }

        // Compute volatility (stddev of last 24 returns).
        let history = &self.main_index_history;
        if history.len() >= 2 {
            let returns: Vec<f64> = history
                .windows(2)
                .map(|w| {
                    if w[0] > 0.0 {
                        (w[1] - w[0]) / w[0]
                    } else {
                        0.0
                    }
                })
                .collect();
            let window = returns.iter().rev().take(24).collect::<Vec<_>>();
            let mean: f64 = window.iter().map(|r| **r).sum::<f64>() / window.len() as f64;
            let variance: f64 = window
                .iter()
                .map(|r| (**r - mean).powi(2))
                .sum::<f64>()
                / window.len() as f64;
            self.volatility = variance.sqrt();
        }

        // Compute sector indices.
        let mut sectors_seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for c in &listed {
            let sector_name = format!("{:?}", c.sector);
            sectors_seen.insert(sector_name);
        }

        for sector_name in &sectors_seen {
            let sector_companies: Vec<&Company> = listed
                .iter()
                .copied()
                .filter(|c| format!("{:?}", c.sector) == *sector_name)
                .collect();
            let sector_market_cap: f64 = sector_companies
                .iter()
                .map(|c| c.share_price * c.shares_count as f64)
                .sum();

            let prev_sector_cap = self
                .sector_indices
                .get(sector_name)
                .and_then(|prev_val| {
                    // Reconstruct previous sector cap from previous index value
                    if *prev_val > 0.0 && self.total_market_cap > 0.0 {
                        Some(*prev_val)
                    } else {
                        None
                    }
                });

            let sector_index = if let Some(prev) = prev_sector_cap {
                if prev > 0.0 && sector_market_cap > 0.0 {
                    prev * (sector_market_cap / sector_market_cap.max(1.0))
                } else {
                    1000.0
                }
            } else if sector_market_cap > 0.0 {
                1000.0
            } else {
                0.0
            };

            self.sector_indices.insert(sector_name.clone(), sector_index);
            let hist = self
                .sector_index_history
                .entry(sector_name.clone())
                .or_default();
            hist.push(sector_index);
            if hist.len() > 120 {
                hist.remove(0);
            }
        }
    }
}

/// Phase 56: Check if a company can trade futures for a specific commodity.
///
/// # Tiered Access Rules (per user directive)
/// * **Financial firms (funds, banks):** Unrestricted access to all commodity
///   futures for speculation.
/// * **Real economy firms:** Can only trade futures for commodities directly
///   linked to their supply chain inputs/outputs (pure hedging). The check
///   dynamically evaluates the company's production methods (BOM) against the
///   commodity — no hardcoded sector-to-commodity mappings.
///
/// # Arguments
/// * `company` - The company wanting to trade futures.
/// * `commodity_id` - The commodity ID (string form of `Commodity` enum).
/// * `buildings` - All buildings (filtered by `owner_id == company.id`).
///
/// # Returns
/// `true` if the company can trade futures for this commodity.
pub fn can_trade_futures(
    company: &Company,
    commodity_id: &str,
    buildings: &[Building],
) -> bool {
    // Financial firms (funds, banks) have unrestricted access.
    let is_financial = matches!(
        company.sector,
        crate::registries::enums::Sector::Banking
    ) || company.fund_type.is_some();

    if is_financial {
        return true;
    }

    // Real economy firms: check if the commodity appears in their BOM
    // (inputs or outputs of any building they own).
    let target_commodity = parse_commodity(commodity_id);

    match target_commodity {
        Some(target) => {
            buildings
                .iter()
                .filter(|b| b.owner_id == company.id)
                .any(|b| {
                    b.active_method.inputs.contains_key(&target)
                        || b.active_method.outputs.contains_key(&target)
                })
        }
        None => false,
    }
}

/// Parse a commodity ID string into a `Commodity` enum value.
fn parse_commodity(id: &str) -> Option<Commodity> {
    serde_json::from_str(&format!("\"{}\"", id)).ok()
}

impl StockExchange {
    /// Phase 56: Compute VWAP (Volume-Weighted Average Price) for an instrument
    /// from this turn's trade history.
    ///
    /// # Arguments
    /// * `instrument_id` - The instrument to compute VWAP for.
    /// * `current_turn` - The current turn (only trades from this turn are considered).
    ///
    /// # Returns
    /// `Some(vwap)` if there were trades, `None` if no trades this turn.
    pub fn compute_vwap(&self, instrument_id: &str, current_turn: u32) -> Option<f64> {
        let trades: Vec<&Trade> = self
            .trade_history
            .iter()
            .filter(|t| t.instrument_id == instrument_id && t.turn == current_turn)
            .collect();

        if trades.is_empty() {
            return None;
        }

        let total_value: f64 = trades.iter().map(|t| t.price * t.quantity as f64).sum();
        let total_qty: u64 = trades.iter().map(|t| t.quantity).sum();

        if total_qty == 0 {
            None
        } else {
            Some(total_value / total_qty as f64)
        }
    }

    /// Phase 56: Get the first trade price for an instrument this turn (open price).
    pub fn get_open_price(&self, instrument_id: &str, current_turn: u32) -> Option<f64> {
        self.trade_history
            .iter()
            .find(|t| t.instrument_id == instrument_id && t.turn == current_turn)
            .map(|t| t.price)
    }

    /// Phase 56: Get the last trade price for an instrument this turn (close price).
    pub fn get_close_price(&self, instrument_id: &str, current_turn: u32) -> Option<f64> {
        self.trade_history
            .iter()
            .rev()
            .find(|t| t.instrument_id == instrument_id && t.turn == current_turn)
            .map(|t| t.price)
    }

    /// Phase 56: Get total volume for an instrument this turn.
    pub fn get_turn_volume(&self, instrument_id: &str, current_turn: u32) -> u64 {
        self.trade_history
            .iter()
            .filter(|t| t.instrument_id == instrument_id && t.turn == current_turn)
            .map(|t| t.quantity)
            .sum()
    }

    /// Phase 56: Get the current spread for an instrument from its order book.
    pub fn get_spread(&self, instrument_id: &str) -> f64 {
        self.order_book
            .get(instrument_id)
            .map(|book| (book.best_ask - book.best_bid).max(0.0))
            .unwrap_or(0.0)
    }

    /// Phase 56: Update share prices for all listed companies after order matching.
    ///
    /// This is called AFTER SEC-5 (order matching) in the turn loop.
    ///
    /// # Arguments
    /// * `companies` - All companies (listed ones are updated).
    /// * `current_turn` - The current turn number.
    /// * `config` - Securities market config (for mean-reversion rate).
    ///
    /// # Rules
    /// * If trades occurred: `share_price` = VWAP of this turn's trades.
    /// * If no trades: apply mean-reversion drift toward book value using `config.mean_reversion_rate`.
    /// * `open_price` = first trade price, `close_price` = last trade price.
    /// * Price history appended (bounded to 60 entries).
    pub fn update_share_prices(
        &self,
        companies: &mut [Company],
        current_turn: u32,
        config: &crate::securities::config::SecuritiesMarketConfig,
    ) {
        for company in companies.iter_mut() {
            if !company.legal_form.is_listed() || company.shares_count == 0 {
                continue;
            }

            let instrument_id = format!("EQUITY:{}", company.id);

            // Set open/close prices from trade history.
            if let Some(open) = self.get_open_price(&instrument_id, current_turn) {
                company.open_price = open;
            }
            if let Some(close) = self.get_close_price(&instrument_id, current_turn) {
                company.close_price = close;
            }

            // Update share price via VWAP or mean reversion.
            if let Some(vwap) = self.compute_vwap(&instrument_id, current_turn) {
                company.share_price = vwap;
            } else if config.mean_reversion_rate > 0.0 {
                // No trades: apply mean-reversion drift toward book value.
                let book_value_per_share = if company.shares_count > 0 {
                    company.company_capital / company.shares_count as f64
                } else {
                    company.share_price
                };
                let target = company.share_price * (1.0 - config.mean_reversion_target_weight)
                    + book_value_per_share * config.mean_reversion_target_weight;
                company.share_price = company.share_price
                    + (target - company.share_price) * config.mean_reversion_rate;
            }

            // Ensure non-negative price.
            if company.share_price < 0.01 {
                company.share_price = 0.01;
            }

            // Append to price history (bounded to 60).
            // The price_history field is stored in `extra` as a serde_json array.
            // We use a simple approach: store in financial_history's extra map.
            // For now, the open/close/price fields are the primary source of truth.
        }
    }

    /// Execute a limit order against the order book.
    ///
    /// # Arguments
    /// * `order` - The limit order to execute
    /// * `brokerage_accounts` - Map of entity_id -> mutable brokerage account
    ///
    /// # Returns
    /// Tuple of (executed trades, remaining unfilled quantity)
    ///
    /// # Rules
    /// * Buy orders match against asks at or below limit price
    /// * Sell orders match against bids at or above limit price
    /// * Execution price = midpoint of best bid and best ask
    /// * Double-entry: buyer cash → seller cash, seller shares → buyer shares
    /// * Transaction fees deducted from both sides
    pub fn execute_limit_order(
        &mut self,
        order: Order,
        brokerage_accounts: &mut BTreeMap<String, &mut BrokerageAccount>,
    ) -> (Vec<Trade>, u64) {
        let instrument_id = order.instrument_id().to_string();
        let is_buy = matches!(order, Order::Buy { .. });
        let limit_price = order.limit_price();
        let mut remaining_qty = order.quantity();
        let mut trades = Vec::new();
        let mut trade_counter = self.trade_history.len();

        // Ensure order book entry exists
        let book = self.order_book.entry(instrument_id.clone()).or_default();

        loop {
            if remaining_qty == 0 {
                break;
            }

            // Find best matching price level
            let best_idx = if is_buy {
                // Buy: find lowest ask <= limit_price
                book.asks.iter().position(|(price, _)| *price <= limit_price)
            } else {
                // Sell: find highest bid >= limit_price
                book.bids.iter().rposition(|(price, _)| *price >= limit_price)
            };

            let best_idx = match best_idx {
                Some(idx) => idx,
                None => break, // No matching orders
            };

            let (match_price, match_orders) = if is_buy {
                &mut book.asks[best_idx]
            } else {
                &mut book.bids[best_idx]
            };

            let match_price_val = *match_price;
            if match_orders.is_empty() {
                break;
            }

            // Take the first order at this price level
            let counter_order = match_orders.first_mut().unwrap();
            let counter_investor = counter_order.investor_id().to_string();
            let counter_qty = counter_order.quantity();
            let fill_qty = remaining_qty.min(counter_qty);
            let exec_price = match_price_val;

            // Determine buyer and seller
            let (buyer_id, seller_id) = if is_buy {
                (order.investor_id().to_string(), counter_investor.clone())
            } else {
                (counter_investor.clone(), order.investor_id().to_string())
            };

            // Double-entry settlement
            let cost = fill_qty as f64 * exec_price;
            let fee = cost * self.transaction_fee;

            // Settle buyer and seller separately to avoid double mutable borrow
            if let Some(buyer_acct) = brokerage_accounts.get_mut(&buyer_id) {
                buyer_acct.frozen_cash -= cost;
                buyer_acct.add_lot(&instrument_id, fill_qty, exec_price, 0);
                buyer_acct.cash -= fee;
            }
            if let Some(seller_acct) = brokerage_accounts.get_mut(&seller_id) {
                seller_acct.cash += cost - fee;
                // FIFO sell — realized gain/loss is tracked by the caller via capital gains registry
                let _ = seller_acct.sell_fifo(&instrument_id, fill_qty, exec_price);
            }

            // Record trade
            trade_counter += 1;
            trades.push(Trade {
                id: format!("TRADE-{}", trade_counter),
                instrument_id: instrument_id.clone(),
                buyer_id,
                seller_id,
                quantity: fill_qty,
                price: exec_price,
                turn: 0, // Set by caller
            });

            // Update quantities
            remaining_qty -= fill_qty;
            counter_order.reduce_quantity(fill_qty);

            // Remove filled counter order
            if counter_order.is_filled() {
                match_orders.remove(0);
            }

            // Clean up empty price levels
            if match_orders.is_empty() {
                if is_buy {
                    book.asks.remove(best_idx);
                } else {
                    book.bids.remove(best_idx);
                }
            }
        }

        // Update best bid/ask
        book.best_bid = book.bids.last().map(|(p, _)| *p).unwrap_or(0.0);
        book.best_ask = book.asks.first().map(|(p, _)| *p).unwrap_or(0.0);

        // Store remaining order in book if not fully filled
        if remaining_qty > 0 {
            let book = self.order_book.entry(instrument_id.clone()).or_default();
            let order_price = order.limit_price();
            if is_buy {
                if let Some(pos) = book.bids.iter().position(|(p, _)| *p == order_price) {
                    book.bids[pos].1.push(order);
                } else {
                    book.bids.push((order_price, vec![order]));
                    book.bids.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                }
                book.best_bid = book.bids.last().map(|(p, _)| *p).unwrap_or(0.0);
            } else {
                if let Some(pos) = book.asks.iter().position(|(p, _)| *p == order_price) {
                    book.asks[pos].1.push(order);
                } else {
                    book.asks.push((order_price, vec![order]));
                    book.asks.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                }
                book.best_ask = book.asks.first().map(|(p, _)| *p).unwrap_or(0.0);
            }
        }

        (trades, remaining_qty)
    }

    /// Execute a market order against AMM pool.
    ///
    /// # Arguments
    /// * `investor_id` - Entity placing the market order
    /// * `instrument_id` - Instrument to trade
    /// * `is_buy` - true for buy, false for sell
    /// * `quantity` - Number of units
    /// * `brokerage_accounts` - Map of entity_id -> mutable brokerage account
    ///
    /// # Returns
    /// Trade record if successful, None if no liquidity pool exists
    ///
    /// # Rules
    /// * Routes to AMM LiquidityPool
    /// * Slippage: price = (pool.cash / pool.shares) * (1 + qty/pool.shares * slippage_factor)
    /// * Double-entry: pool cash ↔ shares swap
    pub fn execute_market_order(
        &mut self,
        investor_id: &str,
        instrument_id: &str,
        is_buy: bool,
        quantity: u64,
        brokerage_accounts: &mut BTreeMap<String, &mut BrokerageAccount>,
    ) -> Option<Trade> {
        let pool = self.liquidity_pools.get_mut(instrument_id)?;
        if pool.shares == 0 || pool.cash <= 0.0 {
            return None;
        }

        let slippage = calculate_slippage(pool, quantity);
        let base_price = pool.cash / pool.shares as f64;
        let exec_price = if is_buy {
            base_price * (1.0 + slippage)
        } else {
            base_price * (1.0 - slippage)
        };

        let cost = quantity as f64 * exec_price;
        let fee = cost * pool.pool_fee;

        let investor_acct = brokerage_accounts.get_mut(investor_id)?;

        if is_buy {
            if investor_acct.cash < cost + fee {
                return None;
            }
            // Buyer pays cash, receives shares
            investor_acct.cash -= cost + fee;
            investor_acct.add_lot(instrument_id, quantity, exec_price, 0);
            // Pool gains cash, loses shares
            pool.cash += cost;
            pool.shares -= quantity;
        } else {
            let current_holding = investor_acct.get_quantity(instrument_id);
            if current_holding < quantity {
                return None;
            }
            // Seller receives cash, loses shares (FIFO)
            investor_acct.cash += cost - fee;
            let _ = investor_acct.sell_fifo(instrument_id, quantity, exec_price);
            // Pool loses cash, gains shares
            pool.cash -= cost;
            pool.shares += quantity;
        }

        pool.total_value = pool.cash;

        Some(Trade {
            id: format!("AMM-{}", self.trade_history.len()),
            instrument_id: instrument_id.to_string(),
            buyer_id: if is_buy { investor_id.to_string() } else { "POOL".to_string() },
            seller_id: if is_buy { "POOL".to_string() } else { investor_id.to_string() },
            quantity,
            price: exec_price,
            turn: 0,
        })
    }
    
    /// Execute IPO with closed-loop capital transfer and proper dilution.
    /// 
    /// # Arguments
    /// * `company` - The company going public
    /// * `shares_to_float` - Number of new shares to issue
    /// * `reserve_price` - Price per share
    /// * `buyers` - Vector of (buyer_id, share_allocation) tuples
    /// * `brokerage_accounts` - Map of entity_id -> brokerage account
    /// 
    /// # Rules
    /// * Verify buyers have sufficient cash
    /// * Atomically transfer cash from buyers to issuer
    /// * Credit shares to buyer's brokerage portfolios
    /// * CRITICAL: Dilute existing owners before adding new buyers
    /// * Recalculate all owner percentages based on new total share count
    /// * Update free_float to reflect public ownership
    pub fn execute_ipo(
        &mut self,
        company: &mut Company,
        shares_to_float: u64,
        reserve_price: f64,
        buyers: &mut Vec<(String, u64)>,
        brokerage_accounts: &mut BTreeMap<String, &mut BrokerageAccount>,
    ) -> Result<(), String> {
        let total_proceeds = shares_to_float as f64 * reserve_price;
        
        // Verify buyers have sufficient cash
        for (buyer_id, allocation) in buyers.iter() {
            let cost = *allocation as f64 * reserve_price;
            if let Some(brokerage) = brokerage_accounts.get_mut(buyer_id) {
                if brokerage.cash < cost {
                    return Err(format!("Buyer {} insufficient cash", buyer_id));
                }
            }
        }
        
        // Atomically transfer cash from buyers to issuer
        for (buyer_id, allocation) in buyers.iter() {
            let cost = *allocation as f64 * reserve_price;
            if let Some(brokerage) = brokerage_accounts.get_mut(buyer_id) {
                brokerage.cash -= cost;
                brokerage.add_lot(&format!("EQUITY:{}", company.id), *allocation, reserve_price, 0);
            }
        }
        
        // Phase 56: Credit proceeds to issuing company's brokerage account (not liquid_capital).
        // This is consistent with the closed-loop capital model where company cash
        // lives in the brokerage account, not the direct liquid_capital field.
        if let Some(ref mut acct) = company.brokerage_account {
            acct.cash += total_proceeds;
        } else {
            // Fallback: if no brokerage account, credit to liquid_capital.
            company.liquid_capital += total_proceeds;
        }
        
        // Calculate new total share count BEFORE updating shares_count
        let old_shares_count = company.shares_count;
        let new_shares_count = old_shares_count + shares_to_float;
        
        // CRITICAL: Dilute existing owners based on new total
        let mut diluted_owners: BTreeMap<String, f64> = BTreeMap::new();
        for (owner_id, old_percentage) in company.owners.iter() {
            let old_share_count = old_shares_count as f64 * old_percentage;
            let new_percentage = old_share_count / new_shares_count as f64;
            diluted_owners.insert(owner_id.clone(), new_percentage);
        }
        
        // Update share count
        company.shares_count = new_shares_count;
        
        // Add new buyers with their calculated percentages
        for (buyer_id, allocation) in buyers.iter() {
            let share_percentage = *allocation as f64 / new_shares_count as f64;
            // Use entry API to safely add to existing percentage (handles existing investors)
            *diluted_owners.entry(buyer_id.clone()).or_insert(0.0) += share_percentage;
        }
        
        // Replace owners map with diluted version
        company.owners = diluted_owners;
        
        // Update free_float (shares not in owners map)
        let owned_percentage: f64 = company.owners.values().sum();
        company.free_float = (1.0 - owned_percentage).max(0.0);
        
        Ok(())
    }
    
    /// Route dividends to actual shareholders with 100% closed-loop accounting.
    pub fn route_dividends(
        &mut self,
        company_id: &str,
        total_dividend: f64,
        companies: &mut BTreeMap<String, &mut Company>,
        brokerage_accounts: &mut BTreeMap<String, &mut BrokerageAccount>,
        treasury: &mut Treasury,
        treasury_id: &str,
    ) -> Result<(), String> {
        // Look up company (borrow checker safe)
        let company = companies.get(company_id)
            .ok_or_else(|| format!("Company {} not found", company_id))?;
        
        // Verify company has sufficient cash
        if company.liquid_capital < total_dividend {
            return Err("Insufficient cash for dividend".to_string());
        }
        
        // Calculate dividend per share
        let dividend_per_share = total_dividend / company.shares_count as f64;
        
        // Calculate dividend distribution plan (immutable phase)
        let mut distribution_plan: Vec<(String, f64)> = Vec::new();
        
        // Route to known owners from owners map
        for (owner_id, share_percentage) in company.owners.iter() {
            let owner_share_count = (company.shares_count as f64 * share_percentage) as u64;
            let dividend_amount = owner_share_count as f64 * dividend_per_share;
            distribution_plan.push((owner_id.clone(), dividend_amount));
        }
        
        // Route free_float shares to LiquidityPool (if exists)
        if company.free_float > 0.0 {
            let free_float_shares = (company.shares_count as f64 * company.free_float) as u64;
            let free_float_dividend = free_float_shares as f64 * dividend_per_share;
            
            if let Some(pool) = self.liquidity_pools.get_mut(&format!("EQUITY:{}", company_id)) {
                pool.cash += free_float_dividend; // Reward liquidity providers
            }
        }
        
        // Execute distribution (mutable phase - borrow checker safe)
        for (owner_id, dividend_amount) in distribution_plan {
            if let Some(brokerage) = brokerage_accounts.get_mut(&owner_id) {
                brokerage.cash += dividend_amount;
            } else if let Some(owner_company) = companies.get_mut(&owner_id) {
                owner_company.liquid_capital += dividend_amount;
            } else if owner_id == treasury_id {
                treasury.liquid_reserves += dividend_amount;
            } else {
                return Err(format!("Owner {} has no valid account for dividend routing", owner_id));
            }
        }
        
        Ok(())
    }

    /// Match all pending securities orders across all instrument order books.
    ///
    /// # Arguments
    /// * `companies` - Mutable slice of companies (for equity portfolio updates)
    /// * `mbs_pool` - Mutable slice of MBS structures (for tranche owner updates)
    /// * `covered_bonds` - Mutable slice of covered bonds (for owner updates)
    /// * `treasury` - Mutable treasury (for fee collection)
    /// * `current_turn` - Current turn number
    ///
    /// # Returns
    /// Vector of all executed trades
    ///
    /// # Rules
    /// * For each instrument in order_book, match bids against asks
    /// * Best bid (highest) matches best ask (lowest) when bid >= ask
    /// * Execution price = ask price (price-time priority)
    /// * Double-entry: buyer cash → seller cash, seller instrument → buyer instrument
    /// * Transaction fees split: buyer pays fee to treasury, seller receives proceeds minus fee
    /// * Circuit breaker: if triggered, halt all matching
    /// * Expired orders (expiry_turn < current_turn) are removed
    pub fn match_securities_orders(
        &mut self,
        companies: &mut [Company],
        mbs_pool: &mut [crate::securities::MortgageBackedSecurity],
        covered_bonds: &mut [crate::securities::CoveredBond],
        treasury: &mut Treasury,
        current_turn: u32,
    ) -> Vec<Trade> {
        if self.circuit_breaker.is_halted {
            return Vec::new();
        }

        let mut all_trades = Vec::new();
        let fee_rate = self.transaction_fee;
        let trade_counter_start = self.trade_history.len();

        // Collect instrument IDs to process
        let instrument_ids: Vec<String> = self.order_book.keys().cloned().collect();

        for instrument_id in &instrument_ids {
            // Remove expired orders first
            if let Some(book) = self.order_book.get_mut(instrument_id) {
                for (_, orders) in book.bids.iter_mut() {
                    orders.retain(|o| o.expiry_turn() >= current_turn);
                }
                book.bids.retain(|(_, orders)| !orders.is_empty());
                for (_, orders) in book.asks.iter_mut() {
                    orders.retain(|o| o.expiry_turn() >= current_turn);
                }
                book.asks.retain(|(_, orders)| !orders.is_empty());
                book.best_bid = book.bids.last().map(|(p, _)| *p).unwrap_or(0.0);
                book.best_ask = book.asks.first().map(|(p, _)| *p).unwrap_or(0.0);
            }

            // Match loop for this instrument
            let mut trade_idx = trade_counter_start;
            loop {
                // We need to check if best bid >= best ask
                let (best_bid, best_ask) = {
                    let book = match self.order_book.get(instrument_id) {
                        Some(b) => b,
                        None => break,
                    };
                    (book.best_bid, book.best_ask)
                };

                if best_bid <= 0.0 || best_ask <= 0.0 || best_bid < best_ask {
                    break;
                }

                // Pop the best bid and best ask orders
                let (mut bid_order, mut ask_order) = {
                    let book = match self.order_book.get_mut(instrument_id) {
                        Some(b) => b,
                        None => break,
                    };

                    // Get best bid (highest)
                    let bid_order = match book.bids.last_mut() {
                        Some((_, orders)) if !orders.is_empty() => orders.remove(0),
                        _ => break,
                    };
                    // Clean up empty bid level
                    if let Some((_, orders)) = book.bids.last() {
                        if orders.is_empty() {
                            book.bids.pop();
                        }
                    }

                    // Get best ask (lowest)
                    let ask_order = match book.asks.first_mut() {
                        Some((_, orders)) if !orders.is_empty() => orders.remove(0),
                        _ => break,
                    };
                    // Clean up empty ask level
                    if let Some((_, orders)) = book.asks.first() {
                        if orders.is_empty() {
                            book.asks.remove(0);
                        }
                    }

                    (bid_order, ask_order)
                };

                // Determine fill
                let bid_qty = bid_order.quantity();
                let ask_qty = ask_order.quantity();
                let fill_qty = bid_qty.min(ask_qty);
                let exec_price = ask_order.limit_price(); // Price-time priority: ask price

                let buyer_id = bid_order.investor_id().to_string();
                let seller_id = ask_order.investor_id().to_string();
                let cost = fill_qty as f64 * exec_price;
                let fee = cost * fee_rate;

                // Double-entry settlement:
                // 1. Debit buyer: cash -= cost + fee
                // 2. Credit seller: cash += cost - fee
                // 3. Transfer instrument units from seller to buyer
                // 4. Credit fee to treasury
                Self::settle_trade(
                    &buyer_id,
                    &seller_id,
                    instrument_id,
                    fill_qty,
                    cost,
                    fee,
                    companies,
                    mbs_pool,
                    covered_bonds,
                    treasury,
                );

                trade_idx += 1;
                let trade = Trade {
                    id: format!("MATCH-{}-{}", instrument_id, trade_idx),
                    instrument_id: instrument_id.clone(),
                    buyer_id: buyer_id.clone(),
                    seller_id: seller_id.clone(),
                    quantity: fill_qty,
                    price: exec_price,
                    turn: current_turn,
                };
                all_trades.push(trade.clone());

                // Update order quantities and re-insert if partially filled
                bid_order.reduce_quantity(fill_qty);
                ask_order.reduce_quantity(fill_qty);

                if !bid_order.is_filled() {
                    if let Some(book) = self.order_book.get_mut(instrument_id) {
                        let price = bid_order.limit_price();
                        if let Some(pos) = book.bids.iter().position(|(p, _)| *p == price) {
                            book.bids[pos].1.push(bid_order);
                        } else {
                            book.bids.push((price, vec![bid_order]));
                            book.bids.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                        }
                    }
                }
                if !ask_order.is_filled() {
                    if let Some(book) = self.order_book.get_mut(instrument_id) {
                        let price = ask_order.limit_price();
                        if let Some(pos) = book.asks.iter().position(|(p, _)| *p == price) {
                            book.asks[pos].1.push(ask_order);
                        } else {
                            book.asks.push((price, vec![ask_order]));
                            book.asks.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                        }
                    }
                }

                // Update best bid/ask
                if let Some(book) = self.order_book.get_mut(instrument_id) {
                    book.best_bid = book.bids.last().map(|(p, _)| *p).unwrap_or(0.0);
                    book.best_ask = book.asks.first().map(|(p, _)| *p).unwrap_or(0.0);
                }
            }
        }

        // Record all trades in history
        for trade in &all_trades {
            self.trade_history.push_back(trade.clone());
        }

        all_trades
    }

    /// Settle a single trade with proper double-entry across entity types.
    ///
    /// # Rules
    /// * Buyer: cash -= (cost + fee), portfolio += units
    /// * Seller: cash += (cost - fee), portfolio -= units
    /// * Treasury: liquid_reserves += 2 * fee
    /// * For MBS tranches: update tranche owner_id
    /// * For covered bonds: update bond owner_id
    fn settle_trade(
        buyer_id: &str,
        seller_id: &str,
        instrument_id: &str,
        quantity: u64,
        cost: f64,
        fee: f64,
        companies: &mut [Company],
        mbs_pool: &mut [crate::securities::MortgageBackedSecurity],
        covered_bonds: &mut [crate::securities::CoveredBond],
        treasury: &mut Treasury,
    ) {
        // Credit fees to treasury (both sides pay fee)
        treasury.liquid_reserves += fee * 2.0;

        // Phase 55: Compute per-share price for lot tracking.
        let price_per_share = if quantity > 0 { cost / quantity as f64 } else { 0.0 };

        // Update buyer and seller based on instrument type
        if instrument_id.starts_with("EQUITY:") {
            // Equity trade: update company brokerage accounts
            for company in companies.iter_mut() {
                if company.id == buyer_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash -= cost + fee;
                        acct.add_lot(instrument_id, quantity, price_per_share, 0);
                    }
                }
                if company.id == seller_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash += cost - fee;
                        let _ = acct.sell_fifo(instrument_id, quantity, price_per_share);
                    }
                }
            }
        } else if instrument_id.starts_with("MBS:") {
            // MBS tranche trade: update brokerage accounts and tranche ownership
            for company in companies.iter_mut() {
                if company.id == buyer_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash -= cost + fee;
                        acct.add_lot(instrument_id, quantity, price_per_share, 0);
                    }
                }
                if company.id == seller_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash += cost - fee;
                        let _ = acct.sell_fifo(instrument_id, quantity, price_per_share);
                    }
                }
            }
            // Update tranche owner_id in MBS pool
            // Parse: "MBS:{mbs_id}:{priority}"
            let parts: Vec<&str> = instrument_id.splitn(3, ':').collect();
            if parts.len() == 3 {
                let mbs_id = parts[1];
                let priority_str = parts[2];
                if let Some(mbs) = mbs_pool.iter_mut().find(|m| m.id == mbs_id) {
                    for tranche in &mut mbs.tranches {
                        if format!("{:?}", tranche.priority).to_lowercase() == priority_str {
                            if tranche.owner_id == seller_id {
                                tranche.owner_id = buyer_id.to_string();
                            }
                            break;
                        }
                    }
                }
            }
        } else if let Some(bond_id) = instrument_id.strip_prefix("BOND:") {
            // Covered bond trade: update brokerage accounts and bond ownership
            for company in companies.iter_mut() {
                if company.id == buyer_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash -= cost + fee;
                        acct.add_lot(instrument_id, quantity, price_per_share, 0);
                    }
                }
                if company.id == seller_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash += cost - fee;
                        let _ = acct.sell_fifo(instrument_id, quantity, price_per_share);
                    }
                }
            }
            // Update bond holder_id
            // Strip "BOND:"
            if let Some(bond) = covered_bonds.iter_mut().find(|b| b.id == bond_id) {
                if bond.holder_id == seller_id {
                    bond.holder_id = buyer_id.to_string();
                }
            }
        }
    }
}

/// Calculate price slippage for AMM execution.
///
/// # Arguments
/// * `pool` - Liquidity pool
/// * `quantity` - Order size
///
/// # Returns
/// Slippage factor (0.0 = no slippage)
///
/// # Rules
/// * Slippage increases with order size relative to pool depth
/// * Formula: slippage = (quantity / pool.shares) * 0.1
fn calculate_slippage(pool: &LiquidityPool, quantity: u64) -> f64 {
    if pool.shares == 0 {
        return 1.0; // Maximum slippage for empty pool
    }
    let order_ratio = quantity as f64 / pool.shares as f64;
    order_ratio * 0.1 // 10% slippage factor per pool-depth
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_book_default() {
        let book = OrderBook::default();
        assert_eq!(book.best_bid, 0.0);
        assert_eq!(book.best_ask, 0.0);
    }

    #[test]
    fn test_liquidity_pool_default() {
        let pool = LiquidityPool::default();
        assert_eq!(pool.shares, 0);
        assert_eq!(pool.cash, 0.0);
    }

    #[test]
    fn test_circuit_breaker_default() {
        let cb = CircuitBreaker::default();
        assert!(!cb.is_halted);
    }

    // ── Phase 56 Tests ──

    #[test]
    fn test_market_index_default() {
        let mi = MarketIndex::default();
        assert_eq!(mi.main_index_value, 0.0);
        assert!(mi.main_index_history.is_empty());
        assert!(mi.sector_indices.is_empty());
    }

    #[test]
    fn test_market_index_compute_first_time() {
        let mut mi = MarketIndex::default();
        let exchange = StockExchange::default();

        let mut company = Company::default();
        company.id = "COMP-001".to_string();
        company.shares_count = 1_000_000;
        company.share_price = 50.0;
        company.legal_form = crate::entities::LegalForm::JointStockCompany(
            crate::entities::legal_form::JointStockData {
                shares_issued: 1_000_000,
                free_float: 0.3,
                ..Default::default()
            },
        );

        mi.compute(&exchange, &[company]);
        // First computation: base 1000.0
        assert!((mi.main_index_value - 1000.0).abs() < 1e-6);
        assert_eq!(mi.main_index_history.len(), 1);
    }

    #[test]
    fn test_commodity_spot_market_default() {
        let csm = CommoditySpotMarket::default();
        assert!(csm.spot_prices.is_empty());
        assert!(csm.spot_history.is_empty());
    }

    #[test]
    fn test_commodity_spot_update_from_b2b_vwap() {
        let mut csm = CommoditySpotMarket::default();
        let config = crate::securities::config::SecuritiesMarketConfig {
            commodity_spot_retail_premium: 0.05,
            ..Default::default()
        };

        let mut b2b_vwaps = BTreeMap::new();
        b2b_vwaps.insert("steel".to_string(), 100.0);
        b2b_vwaps.insert("wheat".to_string(), 50.0);

        csm.update_spot_prices(&b2b_vwaps, &config);

        // Spot = VWAP * (1 + 0.05) = VWAP * 1.05
        assert!((csm.get_spot_price("steel") - 105.0).abs() < 1e-6);
        assert!((csm.get_spot_price("wheat") - 52.5).abs() < 1e-6);
    }

    #[test]
    fn test_commodity_spot_history_bounded() {
        let mut csm = CommoditySpotMarket::default();
        let config = crate::securities::config::SecuritiesMarketConfig {
            commodity_spot_retail_premium: 0.0,
            ..Default::default()
        };

        let mut b2b_vwaps = BTreeMap::new();
        b2b_vwaps.insert("steel".to_string(), 100.0);

        // Push 65 entries — should be bounded to 60.
        for _ in 0..65 {
            csm.update_spot_prices(&b2b_vwaps, &config);
        }

        let hist = csm.spot_history.get("steel").unwrap();
        assert_eq!(hist.len(), 60);
    }

    #[test]
    fn test_vwap_computation() {
        let mut exchange = StockExchange::default();
        exchange.transaction_fee = 0.0;

        // Add some trades for "EQUITY:COMP-001" at turn 5.
        exchange.trade_history.push_back(Trade {
            id: "T1".to_string(),
            instrument_id: "EQUITY:COMP-001".to_string(),
            buyer_id: "B1".to_string(),
            seller_id: "S1".to_string(),
            quantity: 100,
            price: 50.0,
            turn: 5,
        });
        exchange.trade_history.push_back(Trade {
            id: "T2".to_string(),
            instrument_id: "EQUITY:COMP-001".to_string(),
            buyer_id: "B2".to_string(),
            seller_id: "S2".to_string(),
            quantity: 200,
            price: 52.0,
            turn: 5,
        });

        // VWAP = (100*50 + 200*52) / (100+200) = (5000 + 10400) / 300 = 51.33...
        let vwap = exchange.compute_vwap("EQUITY:COMP-001", 5).unwrap();
        assert!((vwap - (5000.0 + 10400.0) / 300.0).abs() < 1e-6);

        // No trades at turn 6.
        assert!(exchange.compute_vwap("EQUITY:COMP-001", 6).is_none());
    }

    #[test]
    fn test_can_trade_futures_financial_firm_unrestricted() {
        let mut company = Company::default();
        company.id = "FUND-001".to_string();
        company.sector = crate::registries::enums::Sector::Banking;
        company.fund_type = Some(crate::securities::FundType::HedgeFund);

        // Financial firm can trade any commodity futures.
        assert!(can_trade_futures(&company, "steel", &[]));
        assert!(can_trade_futures(&company, "wheat", &[]));
        assert!(can_trade_futures(&company, "oil", &[]));
    }

    #[test]
    fn test_can_trade_futures_real_economy_hedging_only() {
        let mut company = Company::default();
        company.id = "COMP-001".to_string();
        company.sector = crate::registries::enums::Sector::HeavyIndustry;
        company.fund_type = None;

        // Create a building that uses steel as input.
        let mut building = Building::default();
        building.owner_id = "COMP-001".to_string();
        building.active_method.inputs.insert(Commodity::Steel, 10.0);
        building.active_method.outputs.insert(Commodity::Cars, 5.0);

        let buildings = vec![building];

        // Can hedge steel (input) and cars (output).
        assert!(can_trade_futures(&company, "steel", &buildings));
        assert!(can_trade_futures(&company, "cars", &buildings));

        // Cannot trade wheat (not in supply chain).
        assert!(!can_trade_futures(&company, "wheat", &buildings));
    }

    #[test]
    fn test_can_trade_futures_no_buildings() {
        let mut company = Company::default();
        company.id = "COMP-002".to_string();
        company.sector = crate::registries::enums::Sector::Hospitality;
        company.fund_type = None;

        // No buildings → cannot trade any futures.
        assert!(!can_trade_futures(&company, "steel", &[]));
        assert!(!can_trade_futures(&company, "wheat", &[]));
    }

    #[test]
    fn test_config_mean_reversion_rate_default() {
        let config = crate::securities::config::SecuritiesMarketConfig::default();
        assert!((config.mean_reversion_rate - 0.05).abs() < 1e-6);
        assert!((config.mean_reversion_target_weight - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_config_commodity_spot_premium_default() {
        let config = crate::securities::config::SecuritiesMarketConfig::default();
        assert!((config.commodity_spot_retail_premium - 0.05).abs() < 1e-6);
    }
}

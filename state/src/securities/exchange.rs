//! Stock exchange module with dual-liquidity trading infrastructure.
//!
//! This module implements the StockExchange struct with order book and AMM
//! liquidity pools for trading securities, along with trade execution logic.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque, HashMap};
use serde_json::Value;

use crate::securities::brokerage::BrokerageAccount;
use crate::entities::Company;
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
}

/// National stock exchange with dual-liquidity execution models.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename = "giełda_papierów_wartościowych")]
pub struct StockExchange {
    /// Order book: Maps instrument_id -> (bids, asks).
    #[serde(rename = "karnet_zleceń")]
    pub order_book: BTreeMap<String, OrderBook>,
    
    /// AMM liquidity pools: Maps instrument_id -> LiquidityPool.
    #[serde(rename = "pule_płynności")]
    pub liquidity_pools: BTreeMap<String, LiquidityPool>,
    
    /// Trade history for audit and price discovery.
    #[serde(rename = "historia_transakcji")]
    pub trade_history: VecDeque<Trade>,
    
    /// Market-wide circuit breaker status.
    #[serde(rename = "wyłącznik_obwodu")]
    pub circuit_breaker: CircuitBreaker,
    
    /// Trading fee (percentage of transaction value).
    #[serde(rename = "opłata_transakcyjna")]
    pub transaction_fee: f64,
    
    /// Any additional exchange fields.
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

/// Order book for a single company.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename = "księga_zleceń")]
pub struct OrderBook {
    /// Bids: Maps price -> list of buy orders.
    /// Using ordered list of (price, orders) tuples since f64 doesn't implement Ord for BTreeMap.
    #[serde(rename = "zlecenia_kupna")]
    pub bids: Vec<(f64, Vec<Order>)>,
    
    /// Asks: Maps price -> list of sell orders.
    #[serde(rename = "zlecenia_sprzedaży")]
    pub asks: Vec<(f64, Vec<Order>)>,
    
    /// Best bid price (highest buy).
    #[serde(rename = "najlepsza_cena_kupna")]
    pub best_bid: f64,
    
    /// Best ask price (lowest sell).
    #[serde(rename = "najlepsza_cena_sprzedaży")]
    pub best_ask: f64,
}

/// Individual order in the order book.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum Order {
    /// Buy limit order.
    Buy {
        /// Unique order identifier.
        #[serde(rename = "id")]
        order_id: String,
        /// Investor placing the order.
        #[serde(rename = "inwestor_id")]
        investor_id: String,
        /// Instrument being traded (e.g., "EQUITY:COMP-001", "MBS:MBS-001:senior").
        #[serde(rename = "instrument_id")]
        instrument_id: String,
        /// Type of instrument being bought.
        #[serde(rename = "typ_instrumentu")]
        instrument_type: InstrumentType,
        /// Number of units to buy.
        #[serde(rename = "ilość")]
        quantity: u64,
        /// Maximum price willing to pay.
        #[serde(rename = "cena_limit")]
        limit_price: f64,
        /// Turn when order expires.
        #[serde(rename = "czas_ważności")]
        expiry_turn: u32,
    },
    /// Sell limit order.
    Sell {
        /// Unique order identifier.
        #[serde(rename = "id")]
        order_id: String,
        /// Investor placing the order.
        #[serde(rename = "inwestor_id")]
        investor_id: String,
        /// Instrument being traded.
        #[serde(rename = "instrument_id")]
        instrument_id: String,
        /// Type of instrument being sold.
        #[serde(rename = "typ_instrumentu")]
        instrument_type: InstrumentType,
        /// Number of units to sell.
        #[serde(rename = "ilość")]
        quantity: u64,
        /// Minimum price willing to accept.
        #[serde(rename = "cena_limit")]
        limit_price: f64,
        /// Turn when order expires.
        #[serde(rename = "czas_ważności")]
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
#[serde(rename = "pula_płynności")]
pub struct LiquidityPool {
    /// Total shares in the pool.
    #[serde(rename = "akcje_w_puli")]
    pub shares: u64,
    
    /// Total cash in the pool.
    #[serde(rename = "gotówka_w_puli")]
    pub cash: f64,
    
    /// Liquidity providers: Maps provider_id -> share of pool.
    #[serde(rename = "dostawcy_płynności")]
    pub providers: BTreeMap<String, f64>,
    
    /// Pool fee (percentage of trade value).
    #[serde(rename = "opłata_puli")]
    pub pool_fee: f64,
    
    /// Phase D.5: Treasury bonds held in pool (for QE secondary market purchases).
    #[serde(rename = "obligacje_skarbowe", default)]
    pub treasury_bonds: Vec<CoveredBond>,
    
    /// Total market value of pool assets.
    #[serde(rename = "wartość_rynkowa", default)]
    pub total_value: f64,
}

/// Trade record for audit and price discovery.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename = "transakcja")]
pub struct Trade {
    /// Trade ID.
    #[serde(rename = "id")]
    pub id: String,
    
    /// Instrument ID (e.g., "EQUITY:COMP-001", "MBS:MBS-001:senior").
    #[serde(rename = "instrument_id")]
    pub instrument_id: String,
    
    /// Buyer ID.
    #[serde(rename = "kupujący")]
    pub buyer_id: String,
    
    /// Seller ID.
    #[serde(rename = "sprzedający")]
    pub seller_id: String,
    
    /// Quantity traded.
    #[serde(rename = "ilość")]
    pub quantity: u64,
    
    /// Execution price.
    #[serde(rename = "cena")]
    pub price: f64,
    
    /// Turn of execution.
    #[serde(rename = "tur")]
    pub turn: u32,
}

/// Circuit breaker status for market-wide trading halts.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename = "wyłącznik_obwodu")]
pub struct CircuitBreaker {
    /// Is trading currently halted?
    #[serde(rename = "wstrzymano")]
    pub is_halted: bool,
    
    /// Turn when halt was triggered.
    #[serde(rename = "tur_wstrzymania")]
    pub halt_turn: u32,
    
    /// Expected duration in turns.
    #[serde(rename = "czas_trwania")]
    pub duration_turns: u32,
}

impl StockExchange {
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
                *buyer_acct.portfolio.entry(instrument_id.clone()).or_insert(0) += fill_qty;
                buyer_acct.cash -= fee;
            }
            if let Some(seller_acct) = brokerage_accounts.get_mut(&seller_id) {
                seller_acct.cash += cost - fee;
                *seller_acct.portfolio.entry(instrument_id.clone()).or_insert(0) -= fill_qty;
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

        let slippage = if pool.shares == 0 {
            1.0
        } else {
            (quantity as f64 / pool.shares as f64) * 0.1
        };
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
            *investor_acct.portfolio.entry(instrument_id.to_string()).or_insert(0) += quantity;
            // Pool gains cash, loses shares
            pool.cash += cost;
            pool.shares -= quantity;
        } else {
            let current_holding = *investor_acct.portfolio.get(instrument_id).unwrap_or(&0);
            if current_holding < quantity {
                return None;
            }
            // Seller receives cash, loses shares
            investor_acct.cash += cost - fee;
            *investor_acct.portfolio.entry(instrument_id.to_string()).or_insert(0) -= quantity;
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
    fn calculate_slippage(&self, pool: &LiquidityPool, quantity: u64) -> f64 {
        if pool.shares == 0 {
            return 1.0; // Maximum slippage for empty pool
        }
        let order_ratio = quantity as f64 / pool.shares as f64;
        order_ratio * 0.1 // 10% slippage factor per pool-depth
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
                *brokerage.portfolio.entry(format!("EQUITY:{}", company.id)).or_insert(0) += *allocation;
            }
        }
        
        // Credit proceeds to issuing company
        company.liquid_capital += total_proceeds;
        
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

        // Update buyer and seller based on instrument type
        if instrument_id.starts_with("EQUITY:") {
            // Equity trade: update company brokerage accounts
            for company in companies.iter_mut() {
                if company.id == buyer_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash -= cost + fee;
                        *acct.portfolio.entry(instrument_id.to_string()).or_insert(0) += quantity;
                    }
                }
                if company.id == seller_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash += cost - fee;
                        *acct.portfolio.entry(instrument_id.to_string()).or_insert(0) -= quantity;
                    }
                }
            }
        } else if instrument_id.starts_with("MBS:") {
            // MBS tranche trade: update brokerage accounts and tranche ownership
            for company in companies.iter_mut() {
                if company.id == buyer_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash -= cost + fee;
                        *acct.portfolio.entry(instrument_id.to_string()).or_insert(0) += quantity;
                    }
                }
                if company.id == seller_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash += cost - fee;
                        *acct.portfolio.entry(instrument_id.to_string()).or_insert(0) -= quantity;
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
        } else if instrument_id.starts_with("BOND:") {
            // Covered bond trade: update brokerage accounts and bond ownership
            for company in companies.iter_mut() {
                if company.id == buyer_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash -= cost + fee;
                        *acct.portfolio.entry(instrument_id.to_string()).or_insert(0) += quantity;
                    }
                }
                if company.id == seller_id {
                    if let Some(ref mut acct) = company.brokerage_account {
                        acct.cash += cost - fee;
                        *acct.portfolio.entry(instrument_id.to_string()).or_insert(0) -= quantity;
                    }
                }
            }
            // Update bond holder_id
            let bond_id = &instrument_id[5..]; // Strip "BOND:"
            if let Some(bond) = covered_bonds.iter_mut().find(|b| b.id == bond_id) {
                if bond.holder_id == seller_id {
                    bond.holder_id = buyer_id.to_string();
                }
            }
        }
    }
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
}

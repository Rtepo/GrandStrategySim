//! Forex market module for global currency trading.
//!
//! This module implements the ForexMarket as a global singleton with AMM liquidity pools
//! for cross-border fiat trading using the IEU (International Exchange Unit) as the absolute reference.

use serde::{Deserialize, Serialize};
use serde_json::Map;
use std::collections::{BTreeMap, HashMap, VecDeque};
use uuid;

/// Forex order type (buy/sell fiat currency).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Copy, Default)]
pub enum ForexOrderType {
    /// Buy target currency with source currency.
    #[default]
    Buy,
    /// Sell target currency for source currency.
    Sell,
}

/// Forex order for currency trading (AMM input/output routing).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ForexOrder {
    /// Order ID.
    #[serde(default)]
    pub id: String,
    
    /// Entity placing the order (HedgeFund, Bank, Country).
    #[serde(default)]
    pub entity_id: String,
    
    /// Input currency code (e.g., "PLN").
    #[serde(default)]
    pub input_currency: String,
    
    /// Output currency code (e.g., "USD").
    #[serde(default)]
    pub output_currency: String,
    
    /// Amount in input currency.
    #[serde(default)]
    pub input_amount: f64,
    
    /// Limit price (optional, None = market order).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<f64>,
    
    /// Turn when order expires.
    #[serde(default)]
    pub expiry_turn: u32,
    
    /// Any additional order fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

/// AMM liquidity pool for a currency pair (e.g., PLN-USD).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ForexLiquidityPool {
    /// Currency pair (e.g., "PLN-USD").
    #[serde(default)]
    pub currency_pair: String,
    
    /// Reserve of source currency.
    #[serde(default)]
    pub source_reserve: f64,
    
    /// Reserve of target currency.
    #[serde(default)]
    pub target_reserve: f64,
    
    /// Liquidity providers: Maps entity_id -> share of pool.
    #[serde(default)]
    pub providers: BTreeMap<String, f64>,
    
    /// Pool fee (percentage of trade value).
    #[serde(default)]
    pub pool_fee: f64,
    
    /// Current spot price (target / source).
    #[serde(default)]
    pub spot_price: f64,
    
    /// Any additional pool fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

impl ForexLiquidityPool {
    /// Calculate spot price from reserves (x * y = k invariant).
    /// 
    /// # Returns
    /// Current spot price (target_reserve / source_reserve)
    pub fn calculate_spot_price(&self) -> f64 {
        if self.source_reserve > 0.0 {
            self.target_reserve / self.source_reserve
        } else {
            f64::INFINITY
        }
    }
    
    /// Execute a swap through the AMM pool (Uniswap V2 style, fee stays in pool).
    /// 
    /// # Arguments
    /// * `input_amount` - Amount of input currency to swap
    /// * `is_input_source` - true if input is source currency, false if input is target currency
    /// 
    /// # Returns
    /// Output amount in output currency after slippage
    /// 
    /// # Rules
    /// - AMM invariant: source_reserve * target_reserve = k (constant)
    /// - Slippage increases with order size relative to pool depth
    /// - Fee applied to INPUT amount (Uniswap V2 standard), not output
    /// - Fee naturally stays in pool reserves, growing k to reward liquidity providers
    /// - CRITICAL: Add FULL input_amount to physical reserves (not just post-fee)
    /// - Use virtual reserve for calculation to apply fee correctly
    /// - Formula: virtual_reserve = old_reserve + (input_amount * (1 - pool_fee))
    /// - Then: new_output_reserve = k / virtual_reserve
    /// - Physical update: input_reserve += input_amount (FULL amount, fee stays inside)
    pub fn execute_swap(&mut self, input_amount: f64, is_input_source: bool) -> f64 {
        if input_amount <= 0.0 {
            return 0.0;
        }
        
        let k = self.source_reserve * self.target_reserve;
        
        if is_input_source {
            // Input is source: calculate output using virtual reserve (post-fee)
            let virtual_source = self.source_reserve + (input_amount * (1.0 - self.pool_fee));
            let new_target_reserve = k / virtual_source;
            let output_amount = self.target_reserve - new_target_reserve;
            
            // Physical reserve updates: add FULL input_amount to source (fee stays in pool)
            self.source_reserve += input_amount;
            self.target_reserve = new_target_reserve;
            
            output_amount
        } else {
            // Input is target: calculate output using virtual reserve (post-fee)
            let virtual_target = self.target_reserve + (input_amount * (1.0 - self.pool_fee));
            let new_source_reserve = k / virtual_target;
            let output_amount = self.source_reserve - new_source_reserve;
            
            // Physical reserve updates: add FULL input_amount to target (fee stays in pool)
            self.target_reserve += input_amount;
            self.source_reserve = new_source_reserve;
            
            output_amount
        }
    }
}

/// Forex trade record for audit.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ForexTrade {
    /// Trade ID.
    #[serde(default)]
    pub id: String,
    
    /// Buyer entity ID.
    #[serde(default)]
    pub buyer_id: String,
    
    /// Seller entity ID (or "AMM_POOL").
    #[serde(default)]
    pub seller_id: String,
    
    /// Source currency code.
    #[serde(default)]
    pub from_currency: String,
    
    /// Target currency code.
    #[serde(default)]
    pub to_currency: String,
    
    /// Amount traded.
    #[serde(default)]
    pub amount: f64,
    
    /// Execution price.
    #[serde(default)]
    pub price: f64,
    
    /// Turn when trade occurred.
    #[serde(default)]
    pub turn: u32,
    
    /// Any additional trade fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

/// Global Forex Market - supranational currency exchange.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ForexMarket {
    /// AMM liquidity pools: Maps currency_pair -> pool.
    #[serde(default)]
    pub liquidity_pools: BTreeMap<String, ForexLiquidityPool>,
    
    /// Order book: Maps currency_pair -> Vec<ForexOrder>.
    #[serde(default)]
    pub order_book: BTreeMap<String, Vec<ForexOrder>>,
    
    /// Trade history for audit.
    #[serde(default)]
    pub trade_history: VecDeque<ForexTrade>,
    
    /// Locked countries (sovereign default - cannot trade).
    #[serde(default)]
    pub locked_countries: Vec<String>,
    
    /// Any additional market fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

impl ForexMarket {
    /// Check if a country is locked out of the Forex market.
    /// 
    /// # Arguments
    /// * `country_id` - Country identifier
    /// 
    /// # Returns
    /// true if country is locked out (sovereign default)
    pub fn is_country_locked(&self, country_id: &str) -> bool {
        self.locked_countries.contains(&country_id.to_string())
    }
    
    /// Lock a country from the Forex market (sovereign default).
    /// 
    /// # Arguments
    /// * `country_id` - Country identifier
    pub fn lock_country(&mut self, country_id: &str) {
        if !self.locked_countries.contains(&country_id.to_string()) {
            self.locked_countries.push(country_id.to_string());
        }
    }
    
    /// Unlock a country from the Forex market (default resolved).
    /// 
    /// # Arguments
    /// * `country_id` - Country identifier
    pub fn unlock_country(&mut self, country_id: &str) {
        self.locked_countries.retain(|id| id != country_id);
    }
    
    /// Execute a Forex trade (checks lockout status, enforces double-entry, multi-currency wallets).
    /// 
    /// # Arguments
    /// * `order` - Forex order to execute (input/output routing)
    /// * `country_id` - Country of the entity placing the order
    /// * `domestic_currency` - Domestic currency code for the entity's country
    /// * `currencies` - Global currency registry (for IEU rates)
    /// * `brokerage_accounts` - Global brokerage accounts (for debit/credit)
    /// 
    /// # Returns
    /// Result with executed trade or error (lockout, insufficient liquidity, insufficient funds)
    /// 
    /// # Rules
    /// - Reject if country is locked out (sovereign default)
    /// - Execute via AMM pool (checks both A-B and B-A pool formats)
    /// - Double-entry: debit input_currency, credit output_currency
    /// - Multi-currency wallets: use fx_balances for foreign currencies, cash for domestic
    /// - Close-loop transaction (no phantom trades)
    /// - Simple AMM routing: no Buy/Sell complexity, just input/output
    pub fn execute_trade(
        &mut self,
        order: ForexOrder,
        country_id: &str,
        domestic_currency: &str,
        _currencies: &HashMap<String, crate::state::Currency>,
        brokerage_accounts: &mut BTreeMap<String, &mut crate::securities::BrokerageAccount>,
        current_turn: u32,
    ) -> Result<ForexTrade, String> {
        // Check lockout status
        if self.is_country_locked(country_id) {
            return Err(format!("Country {} is locked out of Forex market (sovereign default)", country_id));
        }
        
        // Get buyer's brokerage account
        let buyer_account = brokerage_accounts.get_mut(&order.entity_id)
            .ok_or("Entity has no brokerage account")?;
        
        // Check buyer has sufficient input currency (using multi-currency wallet)
        let buyer_balance = buyer_account.get_currency_balance(&order.input_currency, domestic_currency);
        if buyer_balance < order.input_amount {
            return Err(format!("Insufficient {} balance: have {}, need {}", order.input_currency, buyer_balance, order.input_amount));
        }
        
        // Find liquidity pool (check both A-B and B-A formats)
        let currency_pair_ab = format!("{}-{}", order.input_currency, order.output_currency);
        let currency_pair_ba = format!("{}-{}", order.output_currency, order.input_currency);
        
        let (pool, is_input_source) = if let Some(pool) = self.liquidity_pools.get_mut(&currency_pair_ab) {
            (pool, true) // input is source
        } else if let Some(pool) = self.liquidity_pools.get_mut(&currency_pair_ba) {
            (pool, false) // input is target
        } else {
            return Err(format!("No liquidity pool for currency pair {} or {}", currency_pair_ab, currency_pair_ba));
        };
        
        // Execute swap
        let output_amount = pool.execute_swap(order.input_amount, is_input_source);
        
        // Double-entry: debit input_currency, credit output_currency (simple routing)
        buyer_account.debit_currency(&order.input_currency, order.input_amount, domestic_currency);
        buyer_account.credit_currency(&order.output_currency, output_amount, domestic_currency);
        
        let trade = ForexTrade {
            id: format!("FOREX-{}", uuid::Uuid::new_v4()),
            buyer_id: order.entity_id.clone(),
            seller_id: "AMM_POOL".to_string(),
            from_currency: order.input_currency.clone(),
            to_currency: order.output_currency.clone(),
            amount: order.input_amount,
            price: pool.calculate_spot_price(),
            turn: current_turn,
            extra: Map::new(),
        };
        
        self.trade_history.push_back(trade.clone());
        Ok(trade)
    }
    
    /// Execute a direct AMM swap (bypassing brokerage_accounts for raw amount conversion).
    /// 
    /// # Arguments
    /// * `input_currency` - Input currency code
    /// * `output_currency` - Output currency code
    /// * `input_amount` - Amount to swap
    /// 
    /// # Returns
    /// Result with output amount or error (no liquidity pool, insufficient liquidity)
    /// 
    /// # Rules
    /// - Used by Syndic to convert seized fx_balances without BorrowChecker panic
    /// - Direct AMM pool interaction, no brokerage account debit/credit
    /// - Caller must manually handle cash accounting
    pub fn execute_direct_swap(
        &mut self,
        input_currency: &str,
        output_currency: &str,
        input_amount: f64,
    ) -> Result<f64, String> {
        if input_amount <= 0.0 {
            return Err("Input amount must be positive".to_string());
        }
        
        // Find liquidity pool (check both A-B and B-A formats)
        let currency_pair_ab = format!("{}-{}", input_currency, output_currency);
        let currency_pair_ba = format!("{}-{}", output_currency, input_currency);
        
        let (pool, is_input_source) = if let Some(pool) = self.liquidity_pools.get_mut(&currency_pair_ab) {
            (pool, true) // input is source
        } else if let Some(pool) = self.liquidity_pools.get_mut(&currency_pair_ba) {
            (pool, false) // input is target
        } else {
            return Err(format!("No liquidity pool for currency pair {} or {}", currency_pair_ab, currency_pair_ba));
        };
        
        // Execute swap
        let output_amount = pool.execute_swap(input_amount, is_input_source);
        
        Ok(output_amount)
    }
}

// ============================================================================
// PHASE 5: TRADE DEFICIT SETTLEMENT
// ============================================================================

/// Result of trade deficit settlement for a single country.
#[derive(Debug, Clone, Default)]
pub struct TradeSettlementResult {
    /// Country name.
    pub country_id: String,
    /// Trade deficit amount (positive = deficit).
    pub deficit: f64,
    /// Amount settled via Forex reserves.
    pub forex_settled: f64,
    /// Amount settled via Gold sales.
    pub gold_settled: f64,
    /// Whether country entered sovereign default.
    pub sovereign_default: bool,
}

/// Settles trade deficits for all countries after `balance_global_trade`.
///
/// This is the Phase 10 orchestrator. For each country with a trade deficit
/// (negative trade balance), it attempts to settle in order:
/// 1. **Forex:** Use CB `fx_reserves` to swap domestic for foreign currency.
/// 2. **Gold:** Sell physical gold from CB vault to obtain foreign currency.
/// 3. **Sovereign Default:** If both fail, lock country out of Forex market.
///
/// # Arguments
/// * `state` - Mutable reference to global game state (forex, gold, vaults, currencies).
/// * `trade_balances` - Map of country_id → trade balance (negative = deficit).
///
/// # Returns
/// Vector of `TradeSettlementResult` for each country that had a deficit.
///
/// # Rules
/// * Only countries with `trade_balance < 0` are processed.
/// * Forex settlement: CB swaps domestic currency for the dominant foreign
///   currency (first non-domestic key in `fx_reserves`).
/// * Gold settlement: `execute_cb_trade` with `Sell` order — gold leaves CB
///   vault, foreign currency credited to `fx_reserves`.
/// * Sovereign default: `sovereign_default_turns_remaining` set to 12 (6 months),
///   `forex_market.lock_country()` called.
/// * Double-entry: All flows are physical — no phantom money.
pub fn settle_trade_deficits(
    state: &mut crate::state::GameState,
    trade_balances: &std::collections::HashMap<String, f64>,
    current_turn: u32,
) -> Vec<TradeSettlementResult> {
    let mut results = Vec::new();

    // Collect country IDs with deficits to avoid borrow issues
    let deficit_countries: Vec<(String, f64)> = trade_balances
        .iter()
        .filter(|(_, &balance)| balance < 0.0)
        .map(|(k, &v)| (k.clone(), v))
        .collect();

    for (country_id, deficit) in deficit_countries {
        let mut result = TradeSettlementResult {
            country_id: country_id.clone(),
            deficit: -deficit, // Store as positive
            ..Default::default()
        };

        // Get the country's domestic currency code
        let domestic_currency = {
            let country = state.countries.get(&country_id);
            if country.is_none() {
                results.push(result);
                continue;
            }
            country.unwrap().macro_indicators.currency.clone()
        };

        let country = state.countries.get_mut(&country_id).unwrap();
        let cb_id = country.central_bank.id.clone();

        // Determine the foreign currency to settle in
        // Use the first fx_reserve key that isn't the domestic currency
        let foreign_currency = country.central_bank.fx_reserves.keys()
            .find(|k| *k != &domestic_currency)
            .cloned()
            .unwrap_or_else(|| "IEU".to_string());

        let deficit_amount = -deficit;

        // Step 1: Try Forex reserves
        let available_fx = *country.central_bank.fx_reserves
            .get(&foreign_currency)
            .unwrap_or(&0.0);

        if available_fx >= deficit_amount {
            // Settle entirely via Forex
            *country.central_bank.fx_reserves.get_mut(&foreign_currency).unwrap() -= deficit_amount;
            result.forex_settled = deficit_amount;
            results.push(result);
            continue;
        }

        // Partial Forex settlement
        let mut settled = available_fx;
        if available_fx > 0.0 {
            *country.central_bank.fx_reserves.get_mut(&foreign_currency).unwrap() -= available_fx;
        }
        let remaining = deficit_amount - available_fx;

        // Step 2: Try Gold sales
        let cb_gold = *state.vaults.get(&cb_id).unwrap_or(&0.0);
        let gold_price = state.gold_exchange.gold_price_in_ieu;

        // Calculate how much gold we need to sell
        let currency_rate = state.currencies.get(&foreign_currency)
            .map(|c| c.exchange_rate)
            .unwrap_or(1.0);
        let gold_needed = remaining / (gold_price * currency_rate);

        if cb_gold >= gold_needed && gold_needed > 0.0 {
            // Sell gold via CB trade
            let gold_order = crate::state::gold::GoldOrder {
                id: format!("GOLD-SETTLE-{}", country_id),
                entity_id: cb_id.clone(),
                order_type: crate::state::forex::ForexOrderType::Sell,
                gold_amount: gold_needed,
                payment_currency: foreign_currency.clone(),
                limit_price_in_ieu: None,
                expiry_turn: 0,
                extra: Map::new(),
            };

            // We need to extract mutable references carefully
            // Split state into its components
            let currencies_clone = state.currencies.clone();
            let gold_exchange = &mut state.gold_exchange;
            let vaults = &mut state.vaults;
            let country = state.countries.get_mut(&country_id).unwrap();
            let fx_reserves = &mut country.central_bank.fx_reserves;
            let physical_gold_reserves = &mut country.central_bank.physical_gold_reserves;

            let gold_result = gold_exchange.execute_cb_trade(
                gold_order,
                &currencies_clone,
                vaults,
                fx_reserves,
                physical_gold_reserves,
                &cb_id,
                current_turn,
            );

            if let Ok(_trade) = gold_result {
                settled += remaining;
                result.gold_settled = remaining;
            }
        }

        result.forex_settled = settled.min(deficit_amount);

        // Step 3: Sovereign default if not fully settled
        if result.forex_settled + result.gold_settled < deficit_amount - 0.01 {
            let country = state.countries.get_mut(&country_id).unwrap();
            country.sovereign_default_turns_remaining = 12; // 6 months
            state.forex_market.lock_country(&country_id);
            result.sovereign_default = true;
        }

        results.push(result);
    }

    results
}

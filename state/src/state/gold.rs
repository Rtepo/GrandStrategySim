//! Gold exchange module for physical gold trading.
//!
//! This module implements the GlobalGoldExchange as a global singleton for physical
//! commodity trading with strict double-entry accounting for both gold and fiat.

use serde::{Deserialize, Serialize};
use serde_json::Map;
use std::collections::{BTreeMap, HashMap, VecDeque};
use uuid;

use crate::state::forex::ForexOrderType;

/// Gold order (buy/sell physical gold).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GoldOrder {
    /// Order ID.
    #[serde(default)]
    pub id: String,

    /// Entity placing the order.
    #[serde(default)]
    pub entity_id: String,

    /// Order type (buy/sell gold).
    #[serde(default)]
    pub order_type: ForexOrderType, // Reuse Buy/Sell enum

    /// Amount of gold (in units).
    #[serde(default)]
    pub gold_amount: f64,

    /// Payment currency (e.g., "USD", "PLN").
    #[serde(default)]
    pub payment_currency: String,

    /// Limit price in IEU (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price_in_ieu: Option<f64>,

    /// Turn when order expires.
    #[serde(default)]
    pub expiry_turn: u32,

    /// Any additional order fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

/// Gold trade record for audit.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GoldTrade {
    /// Trade ID.
    #[serde(default)]
    pub id: String,

    /// Buyer entity ID.
    #[serde(default)]
    pub buyer_id: String,

    /// Seller entity ID (or "GOLD_POOL").
    #[serde(default)]
    pub seller_id: String,

    /// Amount of gold traded.
    #[serde(default)]
    pub gold_amount: f64,

    /// Gold price in IEU at execution.
    #[serde(default)]
    pub price_in_ieu: f64,

    /// Payment currency used.
    #[serde(default)]
    pub payment_currency: String,

    /// Payment amount in payment currency.
    #[serde(default)]
    pub payment_amount: f64,

    /// Turn when trade occurred.
    #[serde(default)]
    pub turn: u32,

    /// Any additional trade fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

/// Global Gold Exchange - physical commodity market.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct GlobalGoldExchange {
    /// Order book: Maps currency -> Vec<GoldOrder> (buy/sell gold for fiat).
    #[serde(default)]
    pub order_book: BTreeMap<String, Vec<GoldOrder>>,

    /// Trade history for audit.
    #[serde(default)]
    pub trade_history: VecDeque<GoldTrade>,

    /// Gold price in IEU (softly pegged, drifts with global inflation).
    #[serde(default)]
    pub gold_price_in_ieu: f64,

    /// Global inflation rate (affects gold price drift).
    #[serde(default)]
    pub global_inflation_rate: f64,

    /// Phase E.1: Fiat reserves held by the gold pool (currency_code -> amount).
    /// Tracks the fiat payments received from gold buyers and paid to gold sellers.
    #[serde(default)]
    pub fiat_reserves: HashMap<String, f64>,

    /// Any additional gold exchange fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

impl GlobalGoldExchange {
    /// Update gold price based on global inflation.
    ///
    /// # Arguments
    /// * `current_turn` - Current simulation turn
    ///
    /// # Rules
    /// - Gold price in IEU slowly drifts upward with global inflation
    /// - Formula: new_price = old_price * (1 + global_inflation_rate)
    /// - Soft peg: Central banks can temporarily override via interventions
    pub fn update_gold_price(&mut self, _current_turn: u32) {
        let drift_factor = 1.0 + self.global_inflation_rate;
        self.gold_price_in_ieu *= drift_factor;
    }

    /// Execute a gold trade (requires physical delivery, enforces double-entry, multi-currency wallets).
    ///
    /// # Arguments
    /// * `order` - Gold order to execute
    /// * `currencies` - Global currency registry (for IEU conversion)
    /// * `vaults` - Global vault registry (entity_id -> gold_stored)
    /// * `brokerage_accounts` - Global brokerage accounts (for payment transfer)
    /// * `domestic_currency` - Domestic currency code for the entity's country
    ///
    /// # Returns
    /// Result with executed trade or error (insufficient gold, insufficient payment)
    ///
    /// # Rules
    /// - Physical gold must be transferred between vaults (debit seller, credit buyer)
    /// - Payment must be transferred between accounts (debit buyer, credit seller)
    /// - Gold price is in IEU, converted to payment currency
    /// - Multi-currency wallets: use debit_currency/credit_currency for payment
    /// - Close-loop transaction (no phantom trades)
    pub fn execute_trade(
        &mut self,
        order: GoldOrder,
        currencies: &HashMap<String, crate::state::Currency>,
        vaults: &mut BTreeMap<String, f64>,
        brokerage_accounts: &mut BTreeMap<String, &mut crate::securities::BrokerageAccount>,
        domestic_currency: &str,
        current_turn: u32,
    ) -> Result<GoldTrade, String> {
        // Calculate payment amount
        let currency_rate = currencies
            .get(&order.payment_currency)
            .map(|c| c.exchange_rate)
            .unwrap_or(1.0);

        let payment_amount = order.gold_amount * self.gold_price_in_ieu * currency_rate;

        // Get buyer's brokerage account
        let buyer_account = brokerage_accounts
            .get_mut(&order.entity_id)
            .ok_or("Buyer has no brokerage account")?;

        // Check buyer has sufficient payment currency (using multi-currency wallet)
        let buyer_balance =
            buyer_account.get_currency_balance(&order.payment_currency, domestic_currency);
        if buyer_balance < payment_amount {
            return Err(format!(
                "Insufficient {} payment: have {}, need {}",
                order.payment_currency, buyer_balance, payment_amount
            ));
        }

        // Match with counterparty (simplified: use global pool as seller)
        let seller_id = "GOLD_POOL".to_string();

        // Check seller has sufficient gold (if selling)
        if matches!(order.order_type, ForexOrderType::Sell) {
            let seller_gold = vaults.get(&order.entity_id).unwrap_or(&0.0);
            if *seller_gold < order.gold_amount {
                return Err("Insufficient gold in vault".to_string());
            }
        }

        // Execute double-entry transfer (using multi-currency wallet + fiat reserves)
        if matches!(order.order_type, ForexOrderType::Buy) {
            // Buyer: debit payment, credit gold
            buyer_account.debit_currency(
                &order.payment_currency,
                payment_amount,
                domestic_currency,
            );
            *vaults.entry(order.entity_id.clone()).or_insert(0.0) += order.gold_amount;

            // Seller (pool): credit payment to fiat_reserves, debit gold
            *self
                .fiat_reserves
                .entry(order.payment_currency.clone())
                .or_insert(0.0) += payment_amount;
            *vaults.entry(seller_id.clone()).or_insert(0.0) -= order.gold_amount;
        } else {
            // Seller: debit gold, credit payment
            *vaults.entry(order.entity_id.clone()).or_insert(0.0) -= order.gold_amount;

            // Check pool has sufficient fiat reserves
            let pool_fiat = self
                .fiat_reserves
                .get(&order.payment_currency)
                .unwrap_or(&0.0);
            if *pool_fiat < payment_amount {
                return Err(format!(
                    "Gold pool insufficient {} reserves: have {}, need {}",
                    order.payment_currency, pool_fiat, payment_amount
                ));
            }

            // Debit pool fiat_reserves, credit seller
            *self
                .fiat_reserves
                .entry(order.payment_currency.clone())
                .or_insert(0.0) -= payment_amount;
            buyer_account.credit_currency(
                &order.payment_currency,
                payment_amount,
                domestic_currency,
            );

            // Buyer (pool): debit payment (already done), credit gold
            *vaults.entry(seller_id.clone()).or_insert(0.0) += order.gold_amount;
        }

        let trade = GoldTrade {
            id: format!("GOLD-{}", uuid::Uuid::new_v4()),
            buyer_id: order.entity_id.clone(),
            seller_id,
            gold_amount: order.gold_amount,
            price_in_ieu: self.gold_price_in_ieu,
            payment_currency: order.payment_currency.clone(),
            payment_amount,
            turn: current_turn,
            extra: Map::new(),
        };

        self.trade_history.push_back(trade.clone());
        Ok(trade)
    }

    /// Execute a Central Bank gold trade (bypasses brokerage_accounts, uses fx_reserves directly).
    ///
    /// # Arguments
    /// * `order` - Gold order to execute
    /// * `currencies` - Global currency registry (for IEU conversion)
    /// * `vaults` - Global vault registry (entity_id -> gold_stored)
    /// * `fx_reserves` - Central Bank's foreign exchange reserves (mutable reference)
    /// * `physical_gold_reserves` - Central Bank's physical gold reserves (mutable reference)
    /// * `cb_id` - Central Bank entity ID (for vault access)
    ///
    /// # Returns
    /// Result with executed trade or error (insufficient gold, insufficient fx reserves)
    ///
    /// # Rules
    /// - Central Banks are sovereign entities, not retail traders
    /// - Bypasses brokerage_accounts requirement (CB has no BrokerageAccount)
    /// - Direct double-entry against fx_reserves and physical_gold_reserves
    /// - Physical gold transferred between vaults
    /// - Payment transferred from fx_reserves to Gold Pool
    /// - Used for CB interventions and gold reserve management
    pub fn execute_cb_trade(
        &mut self,
        order: GoldOrder,
        currencies: &HashMap<String, crate::state::Currency>,
        vaults: &mut BTreeMap<String, f64>,
        fx_reserves: &mut HashMap<String, f64>,
        physical_gold_reserves: &mut f64,
        cb_id: &str,
        current_turn: u32,
    ) -> Result<GoldTrade, String> {
        // Calculate payment amount
        let currency_rate = currencies
            .get(&order.payment_currency)
            .map(|c| c.exchange_rate)
            .unwrap_or(1.0);

        let payment_amount = order.gold_amount * self.gold_price_in_ieu * currency_rate;

        // Match with counterparty (simplified: use global pool as seller)
        let seller_id = "GOLD_POOL".to_string();

        // Execute double-entry transfer (using fiat_reserves for pool ledger)
        if matches!(order.order_type, ForexOrderType::Buy) {
            // CB buying gold: debit fx_reserves, credit gold vault
            let available_fx = fx_reserves.get(&order.payment_currency).unwrap_or(&0.0);
            if *available_fx < payment_amount {
                return Err(format!(
                    "Insufficient {} fx reserves: have {}, need {}",
                    order.payment_currency, available_fx, payment_amount
                ));
            }

            // Debit fx_reserves
            *fx_reserves.get_mut(&order.payment_currency).unwrap() -= payment_amount;

            // Credit gold vault
            *vaults.entry(cb_id.to_string()).or_insert(0.0) += order.gold_amount;
            *physical_gold_reserves = *vaults.get(cb_id).unwrap();

            // Seller (pool): credit payment to fiat_reserves, debit gold
            *self
                .fiat_reserves
                .entry(order.payment_currency.clone())
                .or_insert(0.0) += payment_amount;
            *vaults.entry(seller_id.clone()).or_insert(0.0) -= order.gold_amount;
        } else {
            // CB selling gold: debit gold vault, credit fx_reserves
            let cb_gold = vaults.get(cb_id).unwrap_or(&0.0);
            if *cb_gold < order.gold_amount {
                return Err("Insufficient physical gold reserves".to_string());
            }

            // Check pool has sufficient fiat reserves
            let pool_fiat = self
                .fiat_reserves
                .get(&order.payment_currency)
                .unwrap_or(&0.0);
            if *pool_fiat < payment_amount {
                return Err(format!(
                    "Gold pool insufficient {} reserves: have {}, need {}",
                    order.payment_currency, pool_fiat, payment_amount
                ));
            }

            // Debit gold vault
            *vaults.entry(cb_id.to_string()).or_insert(0.0) -= order.gold_amount;
            *physical_gold_reserves = *vaults.get(cb_id).unwrap();

            // Debit pool fiat_reserves, credit fx_reserves
            *self
                .fiat_reserves
                .entry(order.payment_currency.clone())
                .or_insert(0.0) -= payment_amount;
            *fx_reserves
                .entry(order.payment_currency.clone())
                .or_insert(0.0) += payment_amount;

            // Buyer (pool): debit payment (already done), credit gold
            *vaults.entry(seller_id.clone()).or_insert(0.0) += order.gold_amount;
        }

        let trade = GoldTrade {
            id: format!("CB-GOLD-{}", uuid::Uuid::new_v4()),
            buyer_id: order.entity_id.clone(),
            seller_id,
            gold_amount: order.gold_amount,
            price_in_ieu: self.gold_price_in_ieu,
            payment_currency: order.payment_currency.clone(),
            payment_amount,
            turn: current_turn,
            extra: Map::new(),
        };

        self.trade_history.push_back(trade.clone());
        Ok(trade)
    }
}

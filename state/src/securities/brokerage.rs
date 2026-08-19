//! Brokerage account module for individual investor accounts.
//!
//! This module implements the BrokerageAccount struct which holds cash,
//! portfolios, pending orders, and frozen cash for trading securities.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use serde_json::Value;

use crate::securities::exchange::Order;
use crate::securities::derivatives::FuturesContract;

/// Margin account for derivative trading.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename = "rachunek_marżowy")]
pub struct MarginAccount {
    /// Initial margin requirement (e.g., 10% of notional).
    #[serde(rename = "marża_początkowa", default)]
    pub initial_margin: f64,
    
    /// Maintenance margin requirement (e.g., 5% of notional).
    #[serde(rename = "marża_utrzymania", default)]
    pub maintenance_margin: f64,
    
    /// Locked margin cash (collateral for open positions).
    #[serde(rename = "zablokowana_marża", default)]
    pub locked_margin: f64,
    
    /// Unrealized P&L from mark-to-market.
    #[serde(rename = "p&l_nierozliczone", default)]
    pub unrealized_pnl: f64,
    
    /// Margin call status (true if below maintenance).
    #[serde(rename = "wezwanie_do_marży", default)]
    pub margin_call_active: bool,
    
    /// Any additional margin fields.
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

/// Individual brokerage account for holding securities and cash.
/// Attached to Companies, Demographics, and Institutional Investors.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename = "rachunek_maklerski")]
pub struct BrokerageAccount {
    /// Cash available for trading (domestic fiat currency only).
    #[serde(rename = "gotówka")]
    pub cash: f64,
    
    /// Phase E.1: Foreign currency balances (currency_code -> amount).
    /// Used for Forex trading - cannot mix PLN and USD in the same scalar field.
    #[serde(rename = "saldo_dewizowe", default)]
    pub fx_balances: HashMap<String, f64>,
    
    /// Portfolio: Maps company_id -> share count.
    /// This is the source of truth for ownership.
    #[serde(rename = "portfel")]
    pub portfolio: BTreeMap<String, u64>,
    
    /// Pending orders: Maps order_id -> Order.
    #[serde(rename = "zlecenia_oczekujące")]
    pub pending_orders: BTreeMap<String, Order>,
    
    /// Frozen cash (reserved for open orders).
    #[serde(rename = "zamrożona_gotówka")]
    pub frozen_cash: f64,
    
    /// KNF freeze status: when true, cannot place new Buy/Sell orders.
    /// Dividends can still be received (operational preservation).
    #[serde(rename = "zamrożony_przez_knf")]
    pub is_frozen: bool,
    
    /// Phase D.5: Margin account for derivative trading.
    #[serde(rename = "rachunek_marżowy", skip_serializing_if = "Option::is_none")]
    pub margin_account: Option<MarginAccount>,
    
    /// Any additional brokerage fields.
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

impl BrokerageAccount {
    /// Ensures cash + frozen_cash never goes negative.
    pub fn validate_cash_invariant(&self) -> bool {
        self.cash >= 0.0 && self.frozen_cash >= 0.0
    }
    
    /// Freezes cash for an order.
    pub fn freeze_cash(&mut self, amount: f64) -> Result<(), String> {
        if self.cash < amount {
            return Err("Insufficient cash".to_string());
        }
        self.cash -= amount;
        self.frozen_cash += amount;
        Ok(())
    }
    
    /// Releases frozen cash after order execution/cancellation.
    pub fn release_cash(&mut self, amount: f64) {
        self.frozen_cash = (self.frozen_cash - amount).max(0.0);
        self.cash += amount;
    }
    
    /// Checks if the account can place new orders (not frozen by KNF).
    pub fn can_place_orders(&self) -> bool {
        !self.is_frozen
    }
    
    /// Phase E.1: Get balance for a specific currency (domestic or foreign).
    /// 
    /// # Arguments
    /// * `currency_code` - Currency code (e.g., "PLN", "USD")
    /// * `domestic_currency` - Domestic currency code for this account
    /// 
    /// # Returns
    /// Balance in the specified currency
    /// 
    /// # Rules
    /// - If currency matches domestic, return cash field
    /// - If currency is foreign, return fx_balances[currency_code]
    pub fn get_currency_balance(&self, currency_code: &str, domestic_currency: &str) -> f64 {
        if currency_code == domestic_currency {
            self.cash
        } else {
            *self.fx_balances.get(currency_code).unwrap_or(&0.0)
        }
    }
    
    /// Phase E.1: Debit balance for a specific currency.
    /// 
    /// # Arguments
    /// * `currency_code` - Currency code to debit
    /// * `amount` - Amount to debit
    /// * `domestic_currency` - Domestic currency code for this account
    /// 
    /// # Rules
    /// - If currency matches domestic, debit cash field
    /// - If currency is foreign, debit fx_balances[currency_code]
    pub fn debit_currency(&mut self, currency_code: &str, amount: f64, domestic_currency: &str) {
        if currency_code == domestic_currency {
            self.cash -= amount;
        } else {
            *self.fx_balances.entry(currency_code.to_string()).or_insert(0.0) -= amount;
        }
    }
    
    /// Phase E.1: Credit balance for a specific currency.
    /// 
    /// # Arguments
    /// * `currency_code` - Currency code to credit
    /// * `amount` - Amount to credit
    /// * `domestic_currency` - Domestic currency code for this account
    /// 
    /// # Rules
    /// - If currency matches domestic, credit cash field
    /// - If currency is foreign, credit fx_balances[currency_code]
    pub fn credit_currency(&mut self, currency_code: &str, amount: f64, domestic_currency: &str) {
        if currency_code == domestic_currency {
            self.cash += amount;
        } else {
            *self.fx_balances.entry(currency_code.to_string()).or_insert(0.0) += amount;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cash_invariant() {
        let account = BrokerageAccount {
            cash: 1000.0,
            frozen_cash: 500.0,
            ..Default::default()
        };
        assert!(account.validate_cash_invariant());
    }

    #[test]
    fn test_freeze_cash() {
        let mut account = BrokerageAccount {
            cash: 1000.0,
            frozen_cash: 0.0,
            ..Default::default()
        };
        assert!(account.freeze_cash(500.0).is_ok());
        assert_eq!(account.cash, 500.0);
        assert_eq!(account.frozen_cash, 500.0);
    }

    #[test]
    fn test_freeze_cash_insufficient() {
        let mut account = BrokerageAccount {
            cash: 100.0,
            frozen_cash: 0.0,
            ..Default::default()
        };
        assert!(account.freeze_cash(500.0).is_err());
    }

    #[test]
    fn test_release_cash() {
        let mut account = BrokerageAccount {
            cash: 500.0,
            frozen_cash: 500.0,
            ..Default::default()
        };
        account.release_cash(300.0);
        assert_eq!(account.cash, 800.0);
        assert_eq!(account.frozen_cash, 200.0);
    }

    #[test]
    fn test_knf_freeze() {
        let mut account = BrokerageAccount {
            cash: 1000.0,
            frozen_cash: 0.0,
            is_frozen: false,
            ..Default::default()
        };
        assert!(account.can_place_orders());
        account.is_frozen = true;
        assert!(!account.can_place_orders());
    }
}

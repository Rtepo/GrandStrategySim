//! Brokerage account module for individual investor accounts.
//!
//! This module implements the BrokerageAccount struct which holds cash,
//! portfolios, pending orders, and frozen cash for trading securities.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use serde_json::Value;

use crate::securities::exchange::Order;

/// Phase 55: A single position lot in a brokerage portfolio.
///
/// Each lot records the acquisition cost basis and turn, enabling
/// accurate FIFO capital gains computation. Sells consume the oldest
/// lots first (FIFO matching).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct PositionLot {
    /// Number of shares/units in this lot.
    #[serde(default)]
    pub quantity: u64,
    /// Average acquisition price per share for this lot.
    #[serde(default)]
    pub cost_basis: f64,
    /// Turn when this lot was acquired (for holding-period tracking).
    #[serde(default)]
    pub acquisition_turn: u32,
}

/// Margin account for derivative trading.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct MarginAccount {
    /// Initial margin requirement (e.g., 10% of notional).
    #[serde(default)]
    pub initial_margin: f64,
    
    /// Maintenance margin requirement (e.g., 5% of notional).
    #[serde(default)]
    pub maintenance_margin: f64,
    
    /// Locked margin cash (collateral for open positions).
    #[serde(default)]
    pub locked_margin: f64,
    
    /// Unrealized P&L from mark-to-market.
    #[serde(default)]
    pub unrealized_pnl: f64,
    
    /// Margin call status (true if below maintenance).
    #[serde(default)]
    pub margin_call_active: bool,
    
    /// Any additional margin fields.
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

/// Individual brokerage account for holding securities and cash.
/// Attached to Companies, Demographics, and Institutional Investors.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct BrokerageAccount {
    /// Cash available for trading (domestic fiat currency only).

    pub cash: f64,
    
    /// Phase E.1: Foreign currency balances (currency_code -> amount).
    /// Used for Forex trading - cannot mix PLN and USD in the same scalar field.
    #[serde(default)]
    pub fx_balances: HashMap<String, f64>,
    
    /// Portfolio: Maps instrument_id -> list of position lots (FIFO order).
    /// Each lot tracks its own cost basis and acquisition turn for
    /// accurate capital gains computation.
    /// Phase 55: Breaking schema change from `BTreeMap<String, u64>`.
    /// Old saves with bare u64 values will fail to deserialize — this is
    /// intentional to avoid zero-cost-basis migration that would create
    /// false taxable gains.

    pub portfolio: BTreeMap<String, Vec<PositionLot>>,
    
    /// Pending orders: Maps order_id -> Order.

    pub pending_orders: BTreeMap<String, Order>,
    
    /// Frozen cash (reserved for open orders).

    pub frozen_cash: f64,
    
    /// KNF freeze status: when true, cannot place new Buy/Sell orders.
    /// Dividends can still be received (operational preservation).

    pub is_frozen: bool,
    
    /// Phase D.5: Margin account for derivative trading.
    #[serde(skip_serializing_if = "Option::is_none")]
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

    /// Phase 55: Get the total quantity held for an instrument (sum of all lots).
    pub fn get_quantity(&self, instrument_id: &str) -> u64 {
        self.portfolio
            .get(instrument_id)
            .map(|lots| lots.iter().map(|l| l.quantity).sum())
            .unwrap_or(0)
    }

    /// Phase 55: Add a new position lot for an instrument (buy/acquire).
    pub fn add_lot(&mut self, instrument_id: &str, quantity: u64, cost_basis: f64, turn: u32) {
        if quantity == 0 {
            return;
        }
        let lots = self.portfolio.entry(instrument_id.to_string()).or_default();
        // Merge with the last lot if it has the same cost basis and turn.
        if let Some(last) = lots.last_mut() {
            if (last.cost_basis - cost_basis).abs() < 1e-6 && last.acquisition_turn == turn {
                last.quantity += quantity;
                return;
            }
        }
        lots.push(PositionLot {
            quantity,
            cost_basis,
            acquisition_turn: turn,
        });
    }

    /// Phase 55: Sell shares using FIFO lot matching.
    ///
    /// Consumes the oldest lots first and returns the realized gain/loss
    /// (sale proceeds - cost basis of consumed lots).
    ///
    /// # Arguments
    /// * `instrument_id` - The instrument to sell.
    /// * `quantity` - Number of shares to sell.
    /// * `sale_price` - Price per share.
    ///
    /// # Returns
    /// `Some((gain_or_loss, shares_sold))` if successful, `None` if insufficient holdings.
    /// `gain_or_loss` is positive for gains, negative for losses.
    pub fn sell_fifo(
        &mut self,
        instrument_id: &str,
        quantity: u64,
        sale_price: f64,
    ) -> Option<(f64, u64)> {
        let lots = self.portfolio.get_mut(instrument_id)?;
        let mut remaining = quantity;
        let mut total_cost = 0.0;
        let mut total_sold = 0u64;

        while remaining > 0 {
            if lots.is_empty() {
                break;
            }
            let front = &mut lots[0];
            if front.quantity == 0 {
                lots.remove(0);
                continue;
            }
            let sell_from_lot = front.quantity.min(remaining);
            total_cost += sell_from_lot as f64 * front.cost_basis;
            front.quantity -= sell_from_lot;
            remaining -= sell_from_lot;
            total_sold += sell_from_lot;
            if front.quantity == 0 {
                lots.remove(0);
            }
        }

        if total_sold == 0 {
            return None;
        }

        let proceeds = total_sold as f64 * sale_price;
        let gain_or_loss = proceeds - total_cost;
        Some((gain_or_loss, total_sold))
    }

    /// Phase 55: Get the weighted average cost basis for an instrument.
    pub fn get_average_cost_basis(&self, instrument_id: &str) -> f64 {
        let lots = match self.portfolio.get(instrument_id) {
            Some(l) if !l.is_empty() => l,
            _ => return 0.0,
        };
        let total_qty: u64 = lots.iter().map(|l| l.quantity).sum();
        if total_qty == 0 {
            return 0.0;
        }
        let total_cost: f64 = lots
            .iter()
            .map(|l| l.quantity as f64 * l.cost_basis)
            .sum();
        total_cost / total_qty as f64
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

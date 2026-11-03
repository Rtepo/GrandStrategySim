//! Phase 54: Banking history tracking for sparkline tooltips.
//!
//! Stores a rolling window of per-turn banking aggregates (reserves, deposits,
//! loans) so the UI can render historical sparklines without relying on
//! localStorage. The engine is the single source of truth.
//!
//! # Rules
//! * Capped at `MAX_TURNS` (50) entries — oldest entries are dropped.
//! * Serialized with `#[serde(default)]` so old saves load with empty history.
//! * Updated once per turn at the end of the banking phase.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum number of historical turns to retain.
const MAX_TURNS: usize = 50;

/// v4.30.0 (TASK-026): All ten KPI aggregates sampled each turn so every top
/// banking card can render a real sparkline/historical chart.
#[derive(Debug, Clone, Default)]
pub struct BankingKpiSample {
    /// Total bank reserves parked at the central bank.
    pub total_reserves: f64,
    /// Total commercial-bank deposits.
    pub total_deposits: f64,
    /// Total outstanding corporate loan balance.
    pub total_loans: f64,
    /// Total consumer debt outstanding.
    pub total_consumer_debt: f64,
    /// Number of DSPW-flagged banks.
    pub dspw_bank_count: f64,
    /// Central-bank reference (policy) rate.
    pub cb_rate: f64,
    /// M0 monetary base.
    pub m0: f64,
    /// M3 broad money supply.
    pub m3: f64,
    /// Central-bank FX reserves.
    pub cb_fx_reserves: f64,
    /// Central-bank gold reserves (tons).
    pub cb_gold_reserves: f64,
}

/// v4.30.0 (TASK-026): One turn of per-bank financial results — net interest
/// income, provisioning/NPL cost, and net profit — for the expanded-bank
/// 24-turn multi-line chart.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BankProfitPoint {
    /// Turn the results were recorded on.
    pub turn: u32,
    /// Interest income minus interest expense (per-turn run-rate).
    pub net_interest_income: f64,
    /// Calibrated NPL provisioning charge for the turn — 0.2%–1.5% of
    /// the performing book via `economy::banking::npl_provisioning`
    /// (v4.31.0 TASK-030; was the raw impaired balance, ~10%/turn).
    pub provisioning_cost: f64,
    /// Net profit for the turn from the canonical income statement:
    /// interest_income + fee_income - interest_expense - npl_cost
    /// - operating_wages (v4.31.0 TASK-030; was `realized_profit_this_turn`
    /// which for banks reads as `-wages`).
    pub net_profit: f64,
}

/// Rolling history of banking aggregates for sparkline rendering.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BankingHistory {
    /// Turn numbers corresponding to each entry.
    #[serde(default)]
    pub turns: Vec<u32>,
    /// Total bank reserves per turn.
    #[serde(default)]
    pub total_reserves: Vec<f64>,
    /// Total bank deposits per turn.
    #[serde(default)]
    pub total_deposits: Vec<f64>,
    /// Total bank loans per turn.
    #[serde(default)]
    pub total_loans: Vec<f64>,
    /// v4.30.0 (TASK-026): consumer debt outstanding per turn.
    #[serde(default)]
    pub total_consumer_debt: Vec<f64>,
    /// v4.30.0 (TASK-026): DSPW bank count per turn.
    #[serde(default)]
    pub dspw_bank_count: Vec<f64>,
    /// v4.30.0 (TASK-026): central-bank reference rate per turn.
    #[serde(default)]
    pub cb_rate: Vec<f64>,
    /// v4.30.0 (TASK-026): M0 monetary base per turn.
    #[serde(default)]
    pub m0: Vec<f64>,
    /// v4.30.0 (TASK-026): M3 broad money per turn.
    #[serde(default)]
    pub m3: Vec<f64>,
    /// v4.30.0 (TASK-026): CB FX reserves per turn.
    #[serde(default)]
    pub cb_fx_reserves: Vec<f64>,
    /// v4.30.0 (TASK-026): CB gold reserves per turn.
    #[serde(default)]
    pub cb_gold_reserves: Vec<f64>,
    /// v4.30.0 (TASK-026): per-bank financial-results history, keyed by
    /// company id — last MAX_TURNS entries each.
    #[serde(default)]
    pub bank_profits: HashMap<String, Vec<BankProfitPoint>>,
}

impl BankingHistory {
    /// Record a new turn's banking aggregates. Drops the oldest entry if the
    /// window is full.
    pub fn record(&mut self, turn: u32, s: &BankingKpiSample) {
        self.turns.push(turn);
        self.total_reserves.push(s.total_reserves);
        self.total_deposits.push(s.total_deposits);
        self.total_loans.push(s.total_loans);
        self.total_consumer_debt.push(s.total_consumer_debt);
        self.dspw_bank_count.push(s.dspw_bank_count);
        self.cb_rate.push(s.cb_rate);
        self.m0.push(s.m0);
        self.m3.push(s.m3);
        self.cb_fx_reserves.push(s.cb_fx_reserves);
        self.cb_gold_reserves.push(s.cb_gold_reserves);
        // Trim to MAX_TURNS by removing from the front.
        while self.turns.len() > MAX_TURNS {
            self.turns.remove(0);
            self.total_reserves.remove(0);
            self.total_deposits.remove(0);
            self.total_loans.remove(0);
            self.total_consumer_debt.remove(0);
            self.dspw_bank_count.remove(0);
            self.cb_rate.remove(0);
            self.m0.remove(0);
            self.m3.remove(0);
            self.cb_fx_reserves.remove(0);
            self.cb_gold_reserves.remove(0);
        }
    }

    /// v4.30.0 (TASK-026): record one bank's per-turn financial results.
    pub fn record_bank_profit(&mut self, bank_id: &str, point: BankProfitPoint) {
        let series = self.bank_profits.entry(bank_id.to_string()).or_default();
        series.push(point);
        while series.len() > MAX_TURNS {
            series.remove(0);
        }
    }

    /// Returns true if there is at least one historical data point.
    pub fn has_data(&self) -> bool {
        !self.turns.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(i: u32) -> BankingKpiSample {
        BankingKpiSample {
            total_reserves: i as f64 * 100.0,
            total_deposits: i as f64 * 200.0,
            total_loans: i as f64 * 50.0,
            total_consumer_debt: i as f64 * 10.0,
            dspw_bank_count: 3.0,
            cb_rate: 0.05,
            m0: i as f64 * 1000.0,
            m3: i as f64 * 3000.0,
            cb_fx_reserves: i as f64 * 40.0,
            cb_gold_reserves: i as f64 * 5.0,
        }
    }

    #[test]
    fn test_record_and_trim() {
        let mut h = BankingHistory::default();
        for i in 0..60 {
            h.record(i, &sample(i));
        }
        assert_eq!(h.turns.len(), MAX_TURNS);
        // First entry should be turn 10 (0-9 were trimmed).
        assert_eq!(h.turns[0], 10);
        assert_eq!(h.total_reserves[0], 1000.0);
        assert_eq!(h.m3[0], 30_000.0);
        assert_eq!(h.cb_rate[0], 0.05);
    }

    #[test]
    fn test_default_is_empty() {
        let h = BankingHistory::default();
        assert!(!h.has_data());
        assert!(h.turns.is_empty());
        assert!(h.bank_profits.is_empty());
    }

    #[test]
    fn test_has_data_after_record() {
        let mut h = BankingHistory::default();
        h.record(1, &sample(1));
        assert!(h.has_data());
        assert_eq!(h.turns, vec![1]);
    }

    #[test]
    fn test_bank_profit_trim() {
        let mut h = BankingHistory::default();
        for i in 0..60 {
            h.record_bank_profit(
                "bank-1",
                BankProfitPoint {
                    turn: i,
                    net_interest_income: i as f64,
                    provisioning_cost: 1.0,
                    net_profit: i as f64 - 1.0,
                },
            );
        }
        let s = &h.bank_profits["bank-1"];
        assert_eq!(s.len(), MAX_TURNS);
        assert_eq!(s[0].turn, 10);
    }
}

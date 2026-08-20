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

/// Maximum number of historical turns to retain.
const MAX_TURNS: usize = 50;

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
}

impl BankingHistory {
    /// Record a new turn's banking aggregates. Drops the oldest entry if the
    /// window is full.
    pub fn record(&mut self, turn: u32, reserves: f64, deposits: f64, loans: f64) {
        self.turns.push(turn);
        self.total_reserves.push(reserves);
        self.total_deposits.push(deposits);
        self.total_loans.push(loans);
        // Trim to MAX_TURNS by removing from the front.
        while self.turns.len() > MAX_TURNS {
            self.turns.remove(0);
            self.total_reserves.remove(0);
            self.total_deposits.remove(0);
            self.total_loans.remove(0);
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

    #[test]
    fn test_record_and_trim() {
        let mut h = BankingHistory::default();
        for i in 0..60 {
            h.record(i, i as f64 * 100.0, i as f64 * 200.0, i as f64 * 50.0);
        }
        assert_eq!(h.turns.len(), MAX_TURNS);
        // First entry should be turn 10 (0-9 were trimmed).
        assert_eq!(h.turns[0], 10);
        assert_eq!(h.total_reserves[0], 1000.0);
    }

    #[test]
    fn test_default_is_empty() {
        let h = BankingHistory::default();
        assert!(!h.has_data());
        assert!(h.turns.is_empty());
    }

    #[test]
    fn test_has_data_after_record() {
        let mut h = BankingHistory::default();
        h.record(1, 100.0, 200.0, 50.0);
        assert!(h.has_data());
        assert_eq!(h.turns, vec![1]);
    }
}

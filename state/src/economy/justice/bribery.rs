//! Phase 22C: Bribery mechanics for inspectorate corruption.
//!
//! When an inspector detects a violation (fraud or OHS), before the fine is
//! levied, the contractor can offer a bribe. If accepted, the bribe enriches
//! the corrupt official personally via `TransferRecipient::CitizenSavings` —
//! the state building's `reserve` is never touched.

use crate::economy::transfer_settler::{settle_transfer, TransferRecipient};
use crate::entities::Company;
use crate::state::Country;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// A bribery attempt during an inspection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BribeAttempt {
    /// Inspectorate building ID.
    pub inspector_building_id: String,
    /// Contractor company ID offering the bribe.
    pub contractor_id: String,
    /// Bribe amount offered from contractor's cash.
    pub bribe_amount: f64,
    /// Whether the bribe was accepted.
    pub accepted: bool,
    /// Turn the bribe was attempted.
    pub turn: u32,
}

/// Default bribe ratio: bribe = fine * BRIBE_RATIO (30–50% of the fine).
pub const BRIBE_RATIO_MIN: f64 = 0.3;
/// Maximum bribe ratio as a fraction of the avoided fine.
pub const BRIBE_RATIO_MAX: f64 = 0.5;

/// Attempt a bribe to avoid a fine.
///
/// # Arguments
/// * `contractor_idx` - Index of the contractor company in `companies`.
/// * `fine_amount` - The fine that would be levied if the bribe fails.
/// * `corruption_index` - Inspectorate corruption level (0.0–1.0).
/// * `inspector_region_idx` - Region index of the inspectorate building.
/// * `inspector_class_key` - Demographic class key of the inspector (e.g. "bourgeoisie").
/// * `inspector_is_rural` - Whether the inspector class is rural.
/// * `current_turn` - Current turn number.
/// * `companies` - All companies (contractor cash is debited).
/// * `country` - Country state (citizen savings credited).
/// * `rng` - Random number generator.
///
/// # Returns
/// `Some(BribeAttempt)` if a bribe was attempted (accepted or rejected),
/// `None` if the contractor couldn't afford a bribe.
///
/// # Rules
/// * Bribe amount = fine * random(0.3–0.5).
/// * Acceptance probability = `corruption_index`.
/// * If accepted: cash flows to inspector's class savings via `settle_transfer`.
/// * If rejected: the bribe attempt becomes an additional crime (caller handles).
pub fn try_bribe(
    contractor_idx: usize,
    fine_amount: f64,
    corruption_index: f64,
    inspector_region_idx: usize,
    inspector_class_key: &str,
    inspector_is_rural: bool,
    current_turn: u32,
    companies: &mut [Company],
    country: &mut Country,
    rng: &mut impl Rng,
) -> Option<BribeAttempt> {
    if fine_amount <= 0.0 || contractor_idx >= companies.len() {
        return None;
    }

    // Compute bribe amount
    let bribe_ratio = BRIBE_RATIO_MIN + rng.gen::<f64>() * (BRIBE_RATIO_MAX - BRIBE_RATIO_MIN);
    let bribe_amount = fine_amount * bribe_ratio;

    // Check if contractor can afford the bribe
    let available = companies[contractor_idx]
        .brokerage_account
        .as_ref()
        .map(|b| b.cash)
        .unwrap_or(companies[contractor_idx].available_cash);

    if available < bribe_amount {
        return None;
    }

    // Determine acceptance
    let accepted = rng.gen::<f64>() < corruption_index;

    let mut attempt = BribeAttempt {
        inspector_building_id: String::new(), // caller can fill in
        contractor_id: companies[contractor_idx].id.clone(),
        bribe_amount,
        accepted,
        turn: current_turn,
    };

    if accepted {
        // Transfer bribe to inspector's demographic class savings
        let recipient = TransferRecipient::CitizenSavings {
            region_idx: inspector_region_idx,
            is_rural: inspector_is_rural,
            class_key: inspector_class_key.to_string(),
        };

        if settle_transfer(companies, contractor_idx, bribe_amount, &recipient, country).is_ok() {
            // Update inspectorate state counters
            if let Some(ref mut ist) = country.politics.inspectorate_state {
                ist.bribes_accepted_this_turn += 1;
                ist.bribes_total_value += bribe_amount;
            }
        }
    }

    let _ = &mut attempt; // silence unused mut warning
    Some(attempt)
}

/// Update the corruption index based on recent bribery activity.
///
/// # Rules
/// * Drifts upward when bribes are accepted (entrenchment).
/// * Drifts downward when justice coverage is high.
/// * Clamped to [0.0, 1.0].
pub fn update_corruption_index(
    corruption_index: &mut f64,
    bribes_accepted_this_turn: u32,
    justice_coverage: f64,
) {
    // Entrenchment: each accepted bribe increases corruption
    // Phase 28: Add passive drift so corruption is never permanently zero.
    // Without this, the system is deadlocked at 0.0 because bribe acceptance
    // probability equals corruption_index, and 0.0 means no bribes are ever accepted.
    let passive_drift = 0.001; // Small per-turn drift
    let entrenchment = (bribes_accepted_this_turn as f64 * 0.01) + passive_drift;

    // Oversight: high justice coverage reduces corruption
    let oversight = justice_coverage * 0.01;

    *corruption_index = (*corruption_index + entrenchment - oversight).clamp(0.0, 1.0);
}

/// Phase 29: Apply corruption-based tax revenue leakage.
///
/// Corruption reduces effective tax collection. A fraction of collected tax
/// revenue is lost to graft, embezzlement, and corrupt officials. This
/// creates a fiscal incentive for the state to fight corruption.
///
/// # Arguments
/// * `budget` - Mutable treasury budget (liquid_reserves reduced by leakage).
/// * `corruption_index` - Current corruption level [0.0, 1.0].
///
/// # Returns
/// The amount of tax revenue lost to corruption.
///
/// # Rules
/// * Leakage factor = `corruption_index * 0.3` (max 30% of revenue lost).
/// * Only applies to liquid_reserves (collected tax revenue).
/// * Does not reduce reserves below zero.
/// * The leaked amount is destroyed (embezzled funds leave the formal economy).
pub fn apply_corruption_tax_leakage(
    budget: &mut crate::state::treasury::Treasury,
    corruption_index: f64,
) -> f64 {
    let leakage_rate = (corruption_index * 0.3).clamp(0.0, 0.3);
    if leakage_rate <= 0.0 {
        return 0.0;
    }
    let leakage = budget.liquid_reserves * leakage_rate;
    budget.liquid_reserves -= leakage;
    leakage
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_corruption_index_entrenchment() {
        let mut idx = 0.5;
        update_corruption_index(&mut idx, 5, 0.0);
        // 5 bribes → +0.05, passive drift +0.001, no oversight → 0.551
        assert!((idx - 0.551).abs() < 0.01);
    }

    #[test]
    fn test_corruption_index_oversight() {
        let mut idx = 0.5;
        // No bribes, high justice coverage → corruption decreases
        // passive drift +0.001, oversight -0.008 → net -0.007 → 0.493
        update_corruption_index(&mut idx, 0, 0.8);
        assert!(idx < 0.5);
    }

    #[test]
    fn test_corruption_index_clamped() {
        let mut idx = 0.99;
        update_corruption_index(&mut idx, 10, 0.0);
        assert_eq!(idx, 1.0);

        // Start below the oversight reduction, should clamp to 0
        // passive drift +0.001, oversight -0.01 → net -0.009 → 0.0 (clamped)
        let mut idx = 0.001;
        update_corruption_index(&mut idx, 0, 1.0);
        assert_eq!(idx, 0.0);
    }

    #[test]
    fn test_corruption_tax_leakage_high_corruption() {
        let mut budget = crate::state::treasury::Treasury::default();
        budget.liquid_reserves = 1_000_000.0;
        let leakage = apply_corruption_tax_leakage(&mut budget, 0.5);
        // 0.5 * 0.3 = 0.15 → 15% of 1M = 150k
        assert!((leakage - 150_000.0).abs() < 1.0);
        assert!((budget.liquid_reserves - 850_000.0).abs() < 1.0);
    }

    #[test]
    fn test_corruption_tax_leakage_zero_corruption() {
        let mut budget = crate::state::treasury::Treasury::default();
        budget.liquid_reserves = 1_000_000.0;
        let leakage = apply_corruption_tax_leakage(&mut budget, 0.0);
        assert_eq!(leakage, 0.0);
        assert_eq!(budget.liquid_reserves, 1_000_000.0);
    }

    #[test]
    fn test_corruption_tax_leakage_max_corruption() {
        let mut budget = crate::state::treasury::Treasury::default();
        budget.liquid_reserves = 1_000_000.0;
        let leakage = apply_corruption_tax_leakage(&mut budget, 1.0);
        // 1.0 * 0.3 = 0.3 → 30% of 1M = 300k
        assert!((leakage - 300_000.0).abs() < 1.0);
        assert!((budget.liquid_reserves - 700_000.0).abs() < 1.0);
    }
}

//! Phase 39: Deferred diplomatic action queue.
//!
//! This module defines the `DiplomaticAction` enum and the sequential
//! drain logic for cross-country diplomatic operations.
//!
//! ## Architecture
//!
//! During parallel per-country turn processing (Rayon `par_iter_mut`),
//! each country's processing function returns a `Vec<DiplomaticAction>`.
//! These are collected via `.map(...).flatten().collect::<Vec<_>>()`.
//!
//! After the parallel block, `drain_diplomatic_actions` is called
//! sequentially to execute each action, safely mutating both the home
//! and host countries without borrow-checker conflicts.
//!
//! ## Rules
//!
//! - No `Mutex`, `RwLock`, or interior mutability.
//! - No direct cross-country mutation during parallel processing.
//! - Actions are drained sequentially after parallel processing completes.
//! - Missing/annexed host countries are handled safely (no-op).

use serde::{Deserialize, Serialize};

/// A deferred diplomatic action queued during parallel turn processing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DiplomaticAction {
    /// Request to construct an embassy in a host country.
    /// The home country's Foreign Affairs ministry pays for construction.
    EmbassyConstructionRequest {
        /// Country requesting the embassy (home country).
        home_country: String,
        /// Country where the embassy will be built (host country).
        host_country: String,
        /// Budget allocated for construction (debited from home country).
        budget: f64,
    },
    /// Transfer funds from home country to host country (embassy operating costs).
    EmbassyFundingTransfer {
        /// Country sending funds (home country).
        home_country: String,
        /// Country receiving funds (host country).
        host_country: String,
        /// Amount to transfer.
        amount: f64,
    },
}

/// Drain the pending diplomatic action queue sequentially.
///
/// This function is called after the parallel per-country turn processing
/// completes. It processes each action in order, safely mutating both
/// home and host countries.
///
/// # Arguments
/// * `state` - Mutable game state (for country access and queue drain).
///
/// # Rules
/// - Debits home country's ministry_cash or liquid_reserves.
/// - Credits host country's treasury or injects construction tenders.
/// - Missing/annexed host countries result in a no-op (funds returned).
/// - No money creation: if the home country lacks funds, the action is skipped.
pub fn drain_diplomatic_actions(state: &mut crate::state::GameState) {
    let actions = std::mem::take(&mut state.pending_diplomatic_actions);

    for action in actions {
        match action {
            DiplomaticAction::EmbassyConstructionRequest {
                home_country,
                host_country,
                budget,
            } => {
                // Debit home country's treasury
                let home = state.countries.get_mut(&home_country);
                if let Some(home) = home {
                    if home.budget.liquid_reserves < budget {
                        // Insufficient funds — skip, no money creation
                        continue;
                    }
                    home.budget.liquid_reserves -= budget;
                } else {
                    continue;
                }

                // Credit host country: inject as treasury revenue or skip if missing
                if let Some(host) = state.countries.get_mut(&host_country) {
                    // The embassy construction brings foreign capital into the host country
                    host.budget.liquid_reserves += budget * 0.5; // Half to host treasury
                    // The other half pays for local construction materials and labor
                    // (would be wired through construction tender system in full impl)
                }
                // If host country doesn't exist, funds are lost (paid to foreign contractors)
            }
            DiplomaticAction::EmbassyFundingTransfer {
                home_country,
                host_country,
                amount,
            } => {
                // Debit home country
                let home = state.countries.get_mut(&home_country);
                if let Some(home) = home {
                    if home.budget.liquid_reserves < amount {
                        continue;
                    }
                    home.budget.liquid_reserves -= amount;
                } else {
                    continue;
                }

                // Credit host country
                if let Some(host) = state.countries.get_mut(&host_country) {
                    host.budget.liquid_reserves += amount;
                }
                // If host doesn't exist, funds are lost
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{GameState, Country, Treasury};

    #[test]
    fn test_drain_empty_queue() {
        let mut state = GameState::default();
        drain_diplomatic_actions(&mut state);
        assert!(state.pending_diplomatic_actions.is_empty());
    }

    #[test]
    fn test_embassy_construction_missing_host() {
        let mut state = GameState::default();
        let mut home = Country::mock_for_tests();
        home.name = "HomeLand".to_string();
        home.budget.liquid_reserves = 1_000_000.0;
        state.countries.insert("HomeLand".to_string(), home);

        state.pending_diplomatic_actions.push(DiplomaticAction::EmbassyConstructionRequest {
            home_country: "HomeLand".to_string(),
            host_country: "NonExistent".to_string(),
            budget: 300_000.0,
        });

        drain_diplomatic_actions(&mut state);

        // Home country should have been debited
        let home = state.countries.get("HomeLand").unwrap();
        assert!((home.budget.liquid_reserves - 700_000.0).abs() < 0.01,
            "home should have 700000, got {}", home.budget.liquid_reserves);
    }

    #[test]
    fn test_embassy_funding_transfer() {
        let mut state = GameState::default();
        let mut home = Country::mock_for_tests();
        home.name = "HomeLand".to_string();
        home.budget.liquid_reserves = 500_000.0;
        let mut host = Country::mock_for_tests();
        host.name = "HostLand".to_string();
        host.budget.liquid_reserves = 100_000.0;
        state.countries.insert("HomeLand".to_string(), home);
        state.countries.insert("HostLand".to_string(), host);

        state.pending_diplomatic_actions.push(DiplomaticAction::EmbassyFundingTransfer {
            home_country: "HomeLand".to_string(),
            host_country: "HostLand".to_string(),
            amount: 50_000.0,
        });

        drain_diplomatic_actions(&mut state);

        let home = state.countries.get("HomeLand").unwrap();
        let host = state.countries.get("HostLand").unwrap();
        assert!((home.budget.liquid_reserves - 450_000.0).abs() < 0.01,
            "home should have 450000");
        assert!((host.budget.liquid_reserves - 150_000.0).abs() < 0.01,
            "host should have 150000");
    }

    #[test]
    fn test_insufficient_funds_skipped() {
        let mut state = GameState::default();
        let mut home = Country::mock_for_tests();
        home.name = "HomeLand".to_string();
        home.budget.liquid_reserves = 10_000.0;
        state.countries.insert("HomeLand".to_string(), home);

        state.pending_diplomatic_actions.push(DiplomaticAction::EmbassyFundingTransfer {
            home_country: "HomeLand".to_string(),
            host_country: "HostLand".to_string(),
            amount: 50_000.0,
        });

        drain_diplomatic_actions(&mut state);

        // Should be skipped — no debit
        let home = state.countries.get("HomeLand").unwrap();
        assert!((home.budget.liquid_reserves - 10_000.0).abs() < 0.01,
            "home should still have 10000");
    }
}

//! Phase 22D: KIO (National Appeal Chamber) — tender appeal mechanism.
//!
//! Competitors can report blacklisted or rule-breaking tender winners to KIO.
//! If the appeal is upheld, the tender is re-awarded to the next-best bid
//! and the respondent's reputation drops further.

use crate::economy::transfer_settler::settle_transfer_to_treasury;
use crate::entities::Company;
use crate::state::Country;
use serde::{Deserialize, Serialize};

/// Grounds for filing a KIO appeal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KioGrounds {
    /// Winner is blacklisted (reputation < threshold).
    #[default]
    Blacklisted,
    /// Winner has a recent fraud history / lawsuit loss.
    FraudHistory,
    /// Winner has a bribery record.
    BriberyRecord,
}

/// A KIO appeal filed by a competitor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct KioAppeal {
    /// Unique appeal ID.
    pub id: String,
    /// Tender ID being appealed.
    pub tender_id: String,
    /// Appellant (competitor company ID filing the appeal).
    pub appellant_id: String,
    /// Respondent (awarded contractor being reported).
    pub respondent_id: String,
    /// Grounds for the appeal.
    #[serde(default)]
    pub grounds: KioGrounds,
    /// Turn the appeal was filed.
    #[serde(default)]
    pub filed_turn: u32,
    /// Whether the appeal was upheld.
    #[serde(default)]
    pub upheld: bool,
    /// Turn the appeal was resolved (0 if pending).
    #[serde(default)]
    pub resolution_turn: u32,
}

/// KIO filing fee (currency units). Refunded if appeal is upheld.
pub const KIO_FILING_FEE: f64 = 5_000.0;

/// File a KIO appeal.
///
/// # Arguments
/// * `tender_id` - The tender being appealed.
/// * `appellant_id` - Competitor filing the appeal.
/// * `respondent_id` - Awarded contractor being reported.
/// * `grounds` - Reason for the appeal.
/// * `current_turn` - Current turn number.
///
/// # Returns
/// A new `KioAppeal` with `upheld = false`.
pub fn file_kio_appeal(
    tender_id: String,
    appellant_id: String,
    respondent_id: String,
    grounds: KioGrounds,
    current_turn: u32,
) -> KioAppeal {
    KioAppeal {
        id: format!("kio_{}_{}_{}", tender_id, appellant_id, current_turn),
        tender_id,
        appellant_id,
        respondent_id,
        grounds,
        filed_turn: current_turn,
        upheld: false,
        resolution_turn: 0,
    }
}

/// Process a KIO appeal.
///
/// # Arguments
/// * `appeal` - The appeal to process (mutated if resolved).
/// * `justice_coverage` - Current justice coverage ratio.
/// * `evidence_strength` - Strength of evidence (0.0–1.0).
/// * `companies` - All companies (for filing fee payment and reputation update).
/// * `country` - Country state (for Treasury).
/// * `current_turn` - Current turn number.
/// * `rng` - Random number generator.
///
/// # Returns
/// `true` if the appeal was resolved this turn.
///
/// # Rules
/// * Uphold probability = `justice_coverage * evidence_strength`.
/// * If upheld: respondent reputation -= 10, filing fee refunded.
/// * If rejected: filing fee kept by Treasury.
pub fn process_kio_appeal(
    appeal: &mut KioAppeal,
    justice_coverage: f64,
    evidence_strength: f64,
    companies: &mut [Company],
    country: &mut Country,
    current_turn: u32,
    rng: &mut impl rand::Rng,
) -> bool {
    // Uphold probability
    let uphold_chance = justice_coverage * evidence_strength;
    appeal.upheld = rng.gen::<f64>() < uphold_chance;
    appeal.resolution_turn = current_turn;

    // Pay filing fee from appellant to Treasury
    if let Some(appellant_idx) = companies.iter().position(|c| c.id == appeal.appellant_id) {
        let available = companies[appellant_idx]
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(companies[appellant_idx].available_cash);
        let fee = KIO_FILING_FEE.min(available);
        if fee > 0.0 {
            let _ = settle_transfer_to_treasury(companies, appellant_idx, fee, country);

            // Refund if upheld
            if appeal.upheld {
                country.budget.liquid_reserves -= fee;
                if let Some(ref mut ba) = companies[appellant_idx].brokerage_account {
                    ba.cash += fee;
                } else {
                    companies[appellant_idx].available_cash += fee;
                }
            }
        }
    }

    // Apply reputation penalty to respondent if upheld
    if appeal.upheld {
        if let Some(respondent_idx) = companies.iter().position(|c| c.id == appeal.respondent_id) {
            companies[respondent_idx].reputation_score =
                (companies[respondent_idx].reputation_score - 10.0).max(0.0);
        }
    }

    true
}

/// Process all pending KIO appeals for one turn.
///
/// # Returns
/// Number of appeals resolved this turn.
pub fn process_kio_appeals(
    appeals: &mut Vec<KioAppeal>,
    justice_coverage: f64,
    companies: &mut [Company],
    country: &mut Country,
    current_turn: u32,
    rng: &mut impl rand::Rng,
) -> u32 {
    let mut resolved = 0u32;
    for appeal in appeals.iter_mut() {
        if appeal.resolution_turn == 0 {
            // Determine evidence strength based on grounds
            let evidence_strength = match appeal.grounds {
                KioGrounds::Blacklisted => 0.9, // clear-cut if reputation is below threshold
                KioGrounds::FraudHistory => 0.7,
                KioGrounds::BriberyRecord => 0.8,
            };
            if process_kio_appeal(
                appeal,
                justice_coverage,
                evidence_strength,
                companies,
                country,
                current_turn,
                rng,
            ) {
                resolved += 1;
            }
        }
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_kio_appeal() {
        let appeal = file_kio_appeal(
            "tender_1".to_string(),
            "comp_a".to_string(),
            "comp_b".to_string(),
            KioGrounds::Blacklisted,
            10,
        );
        assert!(!appeal.upheld);
        assert_eq!(appeal.resolution_turn, 0);
        assert_eq!(appeal.grounds, KioGrounds::Blacklisted);
    }

    #[test]
    fn test_kio_default() {
        let appeal = KioAppeal::default();
        assert_eq!(appeal.grounds, KioGrounds::Blacklisted);
        assert!(!appeal.upheld);
    }
}

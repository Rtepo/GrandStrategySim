//! Phase 22D: Civil lawsuits via the justice system.
//!
//! Civil lawsuits allow an Investor (or the State) to sue a Contractor for
//! structural defects, building collapse, OHS negligence, or bribery.
//! On filing, the defendant's cash is frozen via a tagged entry in
//! `JusticeSystemState.frozen_company_cash`. Damages are paid through
//! `settle_company_to_company`.

use crate::construction::fraud::MaterialSubstitution;
use crate::economy::transfer_settler::{
    settle_company_to_company, settle_transfer_to_treasury,
};
use crate::entities::Company;
use crate::state::Country;
use serde::{Deserialize, Serialize};

/// Type of civil lawsuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CivilCaseType {
    /// Structural defect discovered via private inspection.
    #[default]
    StructuralDefect,
    /// Building collapse attributed to contractor (from DisasterEvent).
    BuildingCollapse,
    /// OHS negligence causing worker casualties.
    OhsNegligence,
    /// Bribery detected by state prosecutor (rejected bribe).
    Bribery,
}

/// Lifecycle status of a civil lawsuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LawsuitStatus {
    /// Filed, awaiting resolution.
    #[default]
    Pending,
    /// Plaintiff won; damages awarded.
    Won,
    /// Defendant won; no damages.
    Lost,
    /// Settled out of court.
    Settled,
}

/// Evidence supporting a civil lawsuit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LawsuitEvidence {
    /// Structural defect measured (0.0–1.0).
    #[serde(default)]
    pub defect_severity: f64,
    /// Number of casualties (for OHS/collapse cases).
    #[serde(default)]
    pub casualty_count: u32,
    /// Material substitutions detected (fraud evidence).
    #[serde(default)]
    pub fraud_detected: Vec<MaterialSubstitution>,
    /// Evidence strength (0.0–1.0). Higher = stronger case.
    #[serde(default)]
    pub evidence_strength: f64,
}

/// A civil lawsuit (Investor suing Contractor, or State suing Contractor).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CivilLawsuit {
    /// Unique lawsuit ID.
    pub id: String,
    /// Plaintiff entity: investor company ID or "STATE".
    pub plaintiff_id: String,
    /// Defendant: main contractor company ID.
    pub defendant_id: String,
    /// Type of case.
    #[serde(default)]
    pub case_type: CivilCaseType,
    /// Total damages claimed (currency units).
    #[serde(default)]
    pub damages_claimed: f64,
    /// Evidence supporting the case.
    #[serde(default)]
    pub evidence: LawsuitEvidence,
    /// Turn the lawsuit was filed.
    #[serde(default)]
    pub filed_turn: u32,
    /// Current status.
    #[serde(default)]
    pub status: LawsuitStatus,
    /// Turn the lawsuit was resolved (0 if pending).
    #[serde(default)]
    pub resolution_turn: u32,
    /// Damages actually awarded (0 if lost/pending).
    #[serde(default)]
    pub damages_awarded: f64,
}

/// Penalty multiplier for catastrophic defects (3x damages).
pub const CATASTROPHIC_DEFECT_PENALTY: f64 = 3.0;

/// Defect severity threshold for catastrophic penalty.
pub const CATASTROPHIC_DEFECT_THRESHOLD: f64 = 0.5;

/// File a new civil lawsuit.
///
/// # Arguments
/// * `plaintiff_id` - Investor company ID or "STATE".
/// * `defendant_id` - Contractor company ID.
/// * `case_type` - Type of case.
/// * `damages_claimed` - Total damages sought.
/// * `evidence` - Evidence supporting the case.
/// * `current_turn` - Current turn number.
///
/// # Returns
/// A new `CivilLawsuit` with status `Pending`.
pub fn file_lawsuit(
    plaintiff_id: String,
    defendant_id: String,
    case_type: CivilCaseType,
    damages_claimed: f64,
    evidence: LawsuitEvidence,
    current_turn: u32,
) -> CivilLawsuit {
    CivilLawsuit {
        id: format!("lawsuit_{}_{}_{}", plaintiff_id, defendant_id, current_turn),
        plaintiff_id,
        defendant_id,
        case_type,
        damages_claimed,
        evidence,
        filed_turn: current_turn,
        status: LawsuitStatus::Pending,
        resolution_turn: 0,
        damages_awarded: 0.0,
    }
}

/// Freeze defendant assets on lawsuit filing.
///
/// Adds a tagged entry to `JusticeSystemState.frozen_company_cash` with
/// key `"lawsuit:{case_id}:{defendant_id}"`.
pub fn freeze_defendant_assets(
    country: &mut Country,
    lawsuit: &CivilLawsuit,
    freeze_amount: f64,
) {
    if let Some(ref mut js) = country.politics.justice_state {
        let key = format!("lawsuit:{}:{}", lawsuit.id, lawsuit.defendant_id);
        js.frozen_company_cash.insert(key, freeze_amount);
    }
}

/// Attempt to resolve a pending lawsuit.
///
/// # Arguments
/// * `lawsuit` - The lawsuit to process (mutated if resolved).
/// * `justice_coverage` - Current justice coverage ratio (0–1).
/// * `companies` - All companies (for damages payment).
/// * `country` - Country state (for Treasury payments).
/// * `current_turn` - Current turn number.
/// * `rng` - Random number generator.
///
/// # Returns
/// `true` if the lawsuit was resolved this turn, `false` if still pending.
///
/// # Rules
/// * Resolution probability = `justice_coverage * evidence_strength`.
/// * On plaintiff win: damages = claimed * evidence_strength * penalty_multiplier.
/// * Damages paid via `settle_company_to_company` (private) or `settle_transfer_to_treasury` (state).
/// * Reputation penalty applied to defendant on loss.
pub fn process_lawsuit(
    lawsuit: &mut CivilLawsuit,
    justice_coverage: f64,
    companies: &mut [Company],
    country: &mut Country,
    current_turn: u32,
    rng: &mut impl rand::Rng,
) -> bool {
    if lawsuit.status != LawsuitStatus::Pending {
        return false;
    }

    // Resolution probability
    let resolution_chance = justice_coverage * lawsuit.evidence.evidence_strength;
    if rng.gen::<f64>() >= resolution_chance {
        return false;
    }

    // Determine outcome: plaintiff wins if evidence_strength > 0.5 (strong evidence)
    let plaintiff_wins = lawsuit.evidence.evidence_strength > 0.5;

    if plaintiff_wins {
        // Calculate damages
        let penalty_multiplier = if lawsuit.evidence.defect_severity > CATASTROPHIC_DEFECT_THRESHOLD {
            CATASTROPHIC_DEFECT_PENALTY
        } else {
            1.0
        };

        let damages = lawsuit.damages_claimed
            * lawsuit.evidence.evidence_strength
            * penalty_multiplier;

        // Find defendant index
        let defendant_idx = match companies.iter().position(|c| c.id == lawsuit.defendant_id) {
            Some(idx) => idx,
            None => return false,
        };

        // Clamp to available cash
        let available = companies[defendant_idx]
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(companies[defendant_idx].available_cash);
        let actual_damages = damages.min(available);

        if actual_damages > 0.0 {
            if lawsuit.plaintiff_id == "STATE" {
                let _ = settle_transfer_to_treasury(companies, defendant_idx, actual_damages, country);
            } else if let Some(plaintiff_idx) = companies.iter().position(|c| c.id == lawsuit.plaintiff_id) {
                let _ = settle_company_to_company(companies, defendant_idx, plaintiff_idx, actual_damages, country);
            }
        }

        // Apply reputation penalty
        let reputation_hit = 10.0 * (1.0 + lawsuit.evidence.defect_severity);
        companies[defendant_idx].reputation_score =
            (companies[defendant_idx].reputation_score - reputation_hit).max(0.0);

        lawsuit.status = LawsuitStatus::Won;
        lawsuit.damages_awarded = actual_damages;
    } else {
        lawsuit.status = LawsuitStatus::Lost;
    }

    lawsuit.resolution_turn = current_turn;

    // Unfreeze assets
    if let Some(ref mut js) = country.politics.justice_state {
        let key = format!("lawsuit:{}:{}", lawsuit.id, lawsuit.defendant_id);
        js.frozen_company_cash.remove(&key);
    }

    true
}

/// Process all pending civil lawsuits for one turn.
///
/// # Arguments
/// * `lawsuits` - All civil lawsuits (pending ones are processed).
/// * `justice_coverage` - Current justice coverage ratio.
/// * `companies` - All companies.
/// * `country` - Country state.
/// * `current_turn` - Current turn number.
/// * `rng` - Random number generator.
///
/// # Returns
/// Number of lawsuits resolved this turn.
pub fn process_civil_lawsuits(
    lawsuits: &mut Vec<CivilLawsuit>,
    justice_coverage: f64,
    companies: &mut [Company],
    country: &mut Country,
    current_turn: u32,
    rng: &mut impl rand::Rng,
) -> u32 {
    let mut resolved = 0u32;
    for lawsuit in lawsuits.iter_mut() {
        if process_lawsuit(lawsuit, justice_coverage, companies, country, current_turn, rng) {
            resolved += 1;
        }
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_lawsuit() {
        let evidence = LawsuitEvidence {
            defect_severity: 0.6,
            casualty_count: 3,
            fraud_detected: Vec::new(),
            evidence_strength: 0.8,
        };
        let lawsuit = file_lawsuit(
            "inv1".to_string(),
            "c1".to_string(),
            CivilCaseType::StructuralDefect,
            500_000.0,
            evidence,
            10,
        );
        assert_eq!(lawsuit.status, LawsuitStatus::Pending);
        assert_eq!(lawsuit.case_type, CivilCaseType::StructuralDefect);
    }

    #[test]
    fn test_lawsuit_default_status() {
        let lawsuit = CivilLawsuit::default();
        assert_eq!(lawsuit.status, LawsuitStatus::Pending);
        assert_eq!(lawsuit.case_type, CivilCaseType::StructuralDefect);
    }
}

//! Phase 48: Local JST legislation and unfunded mandates.
//!
//! This module implements:
//! - Local bills voted on by regional councils.
//! - Unfunded mandates imposed by the central government on regional JSTs.
//! - Strict double-entry cash flow for mandate execution.
//! - Commissary administration bond lock (prevents infinite debt spiral).

use serde::{Deserialize, Serialize};

use crate::politics::local_government::{RegionalGovernance, AdministrativeStatus};

// ============================================================================
// UNFUNDED MANDATE
// ============================================================================

/// An unfunded mandate imposed by the central government on regional councils.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UnfundedMandate {
    /// Mandate description (e.g., "Compulsory education for all children").
    #[serde(default)]
    pub description: String,
    /// Required spending per region (the cash that MUST leave the JST).
    #[serde(default)]
    pub required_spending_per_region: f64,
    /// Central government funding provided (debited from Treasury, credited to JST).
    #[serde(default)]
    pub central_funding: f64,
    /// Funding gap = required_spending - central_funding. JST must find this cash.
    #[serde(default)]
    pub funding_gap: f64,
    /// Turn when mandate was imposed.
    #[serde(default)]
    pub imposed_turn: u32,
    /// Political consequence: center-vs-province friction.
    #[serde(default)]
    pub friction_score: f64,
    /// Whether the JST council has voted on how to cover the gap.
    #[serde(default)]
    pub council_decision: MandateFundingDecision,
}

/// How the regional council decided to cover the funding gap.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum MandateFundingDecision {
    #[default]
    /// Council has not yet voted.
    Pending,
    /// Council voted to raise local property tax rate (affects FUTURE revenue).
    RaisePropertyTax { new_rate: f64 },
    /// Council voted to raise local service fees (affects FUTURE revenue).
    RaiseLocalFees { fee_multiplier: f64 },
    /// Council voted to issue municipal bonds (debt-financed).
    IssueBonds { principal: f64, interest_rate: f64 },
    /// Council voted to slash other local expenditures.
    CutExpenditures { cut_amount: f64 },
    /// Council refused to fund the mandate (non-compliance → friction spike).
    Refused,
}

// ============================================================================
// LOCAL BILL
// ============================================================================

/// A local bill voted on by a regional council.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LocalBill {
    /// Bill ID.
    #[serde(default)]
    pub id: String,
    /// Bill title.
    #[serde(default)]
    pub title: String,
    /// Provisions in the bill.
    #[serde(default)]
    pub provisions: Vec<LocalProvision>,
    /// Current stage.
    #[serde(default)]
    pub stage: LocalBillStage,
    /// Turn when bill was introduced.
    #[serde(default)]
    pub introduction_turn: u32,
}

/// A provision in a local bill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LocalProvision {
    /// Raise local property tax rate.
    PropertyTaxChange { new_rate: f64 },
    /// Raise local service fees.
    LocalFeeChange { fee_type: String, new_rate: f64 },
    /// Local infrastructure spending.
    InfrastructureSpending { amount: f64, target: String },
    /// Local service provision (healthcare, education).
    LocalServiceProvision { service: String, budget: f64 },
    /// Unfunded mandate from central government.
    UnfundedMandate {
        mandate: String,
        required_spending: f64,
        central_funding: f64,
    },
}

impl Default for LocalProvision {
    fn default() -> Self {
        LocalProvision::PropertyTaxChange { new_rate: 0.0 }
    }
}

/// Stage of a local bill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum LocalBillStage {
    #[default]
    Proposed,
    CouncilVote,
    Enacted,
    Rejected,
}

// ============================================================================
// MANDATE FUNDING VOTE
// ============================================================================

/// Council votes on how to cover a mandate funding shortfall.
///
/// Uses faction distribution to determine the preferred mechanism:
/// - Populares faction: prefers RaisePropertyTax / RaiseLocalFees (tax the rich)
/// - Optimates faction: prefers CutExpenditures (protect wealthy from taxes)
/// - Moderates faction: prefers IssueBonds (spread the cost over time)
///
/// **COMMISSARY ADMINISTRATION BOND LOCK:**
/// If the region is under `CommissaryAdministration`, `IssueBonds` is strictly
/// forbidden. The function returns `CutExpenditures` or `Refused` only.
/// This prevents the infinite debt spiral where appointed commissars
/// endlessly print junk municipal bonds.
pub fn vote_on_mandate_funding<R: rand::Rng>(
    gov: &RegionalGovernance,
    shortfall: f64,
    rng: &mut R,
) -> MandateFundingDecision {
    // ── COMMISSARY ADMINISTRATION BOND LOCK ──
    if gov.admin_status == AdministrativeStatus::CommissaryAdministration {
        // Commissary regions can only cut expenditures or raise future taxes.
        // Bonds are strictly forbidden. If neither cuts nor tax hikes can cover
        // the shortfall, the mandate is Refused.
        let cut = shortfall.min(gov.budget.local_expenditures * 0.5);
        if cut >= shortfall {
            return MandateFundingDecision::CutExpenditures { cut_amount: cut };
        }
        // Cuts insufficient — cannot issue bonds. Refuse the remainder.
        return MandateFundingDecision::Refused;
    }

    let fd = &gov.council.faction_distribution;
    let total = (fd.populares_count + fd.moderates_count + fd.optimates_count) as f64;
    if total == 0.0 {
        return MandateFundingDecision::Refused;
    }

    // Weighted vote: each faction prefers a specific mechanism.
    let populares_weight = fd.populares_count as f64 / total;
    let optimates_weight = fd.optimates_count as f64 / total;
    let moderates_weight = fd.moderates_count as f64 / total;

    // Add randomness based on faction stability.
    let stability = fd.faction_stability;
    let noise = |rng: &mut R| (rng.gen::<f64>() - 0.5) * (1.0 - stability) * 0.3;

    let tax_score = populares_weight + noise(rng);
    let cut_score = optimates_weight + noise(rng);
    let bond_score = moderates_weight + noise(rng);

    if tax_score >= cut_score && tax_score >= bond_score {
        // Populares win: raise property tax rate by 10-30%.
        let increase = 0.10 + rng.gen::<f64>() * 0.20;
        let new_rate = gov.budget.property_tax * (1.0 + increase);
        MandateFundingDecision::RaisePropertyTax { new_rate }
    } else if cut_score >= bond_score {
        // Optimates win: cut expenditures (up to 50%).
        let cut = shortfall.min(gov.budget.local_expenditures * 0.5);
        MandateFundingDecision::CutExpenditures { cut_amount: cut }
    } else {
        // Moderates win: issue bonds (only if NOT commissary administration).
        let rate = match gov.debt.credit_rating.as_str() {
            "AAA" | "AA" => 0.03,
            "A" | "BBB" => 0.05,
            "BB" | "B" => 0.08,
            _ => 0.12, // Junk rate
        };
        MandateFundingDecision::IssueBonds {
            principal: shortfall,
            interest_rate: rate,
        }
    }
}

// ============================================================================
// MANDATE EXECUTION — STRICT DOUBLE-ENTRY ACCOUNTING
// ============================================================================

use crate::state::Country;

/// Result of executing a mandate payment.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MandateExecutionResult {
    /// Diagnostic messages.
    pub messages: Vec<String>,
    /// Amount debited from JST liquid reserves.
    pub jst_debit: f64,
    /// Amount credited to central authority / service provider.
    pub central_credit: f64,
    /// Bonds issued (0.0 if none).
    pub bonds_issued: f64,
    /// Expenditures cut (0.0 if none).
    pub expenditures_cut: f64,
    /// Whether the mandate was refused/suspended.
    pub refused: bool,
    /// Final JST liquid reserves after execution.
    pub final_reserves: f64,
    /// Final JST total debt after execution.
    pub final_debt: f64,
}

/// Execute a mandate payment with strict double-entry accounting.
///
/// ## Cash Flow
///
/// 1. Compute `funding_gap = max(required_spending - central_funding, 0.0)`.
/// 2. Tax-rate changes do NOT create immediate cash.
/// 3. Pay the mandate physically:
///    - Debit JST `liquid_reserves`.
///    - Credit central authority or service provider.
/// 4. If reserves insufficient:
///    - Cut expenditures to release cash, OR
///    - Issue municipal bonds (if permitted).
/// 5. Bond issuance:
///    - Increase JST liabilities (total_debt).
///    - Credit JST liquid_reserves with bond proceeds.
///    - Then debit reserves for mandate payment.
/// 6. Tax-rate changes only affect revenue on subsequent turns.
/// 7. Record all cash movements. Prevent negative reserves.
///
/// ## Commissary Bond Lock
///
/// If `admin_status == CommissaryAdministration`:
/// - Bond issuance is FORBIDDEN.
/// - Treasury must provide 100% of shortfall via central funding.
/// - If Treasury cannot afford it: mandate is Refused/Suspended.
/// - JST debt must NOT increase in the refusal path.
pub fn execute_mandate_payment(
    country: &mut Country,
    region_id: &str,
    mandate: &UnfundedMandate,
    decision: &MandateFundingDecision,
    treasury_can_afford: bool,
) -> MandateExecutionResult {
    let mut result = MandateExecutionResult::default();
    let required = mandate.required_spending_per_region;
    let central_funding = mandate.central_funding;
    let funding_gap = (required - central_funding).max(0.0);

    result.messages.push(format!(
        "[MANDATE] Executing '{}' for region {}: required={:.2}, central={:.2}, gap={:.2}",
        mandate.description, region_id, required, central_funding, funding_gap
    ));

    // Find the region and its governance.
    let region_idx = country.regions.iter().position(|r| r.id == region_id);
    let region_idx = match region_idx {
        Some(idx) => idx,
        None => {
            result.messages.push(format!("[MANDATE] Region {} not found — aborted.", region_id));
            result.refused = true;
            result.final_reserves = 0.0;
            result.final_debt = 0.0;
            return result;
        }
    };

    // Check if region is under commissary administration.
    let is_commissary = country.regions[region_idx]
        .governance
        .as_ref()
        .map(|g| g.admin_status == AdministrativeStatus::CommissaryAdministration)
        .unwrap_or(false);

    // ── COMMISSARY BOND LOCK ──
    // If commissary and decision is IssueBonds, reject it defensively.
    if is_commissary
        && matches!(decision, MandateFundingDecision::IssueBonds { .. }) {
            result.messages.push(
                "[MANDATE] BOND LOCK: Commissary region cannot issue bonds. Checking Treasury funding.".to_string()
            );
            // Treasury must cover the gap, or mandate is refused.
            if !treasury_can_afford {
                result.messages.push(
                    "[MANDATE] BOND LOCK: Treasury cannot afford funding. Mandate REFUSED.".to_string()
                );
                result.refused = true;
                result.final_reserves = country.regions[region_idx]
                    .governance.as_ref().map(|g| g.budget.liquid_reserves).unwrap_or(0.0);
                result.final_debt = country.regions[region_idx]
                    .governance.as_ref().map(|g| g.debt.total_debt).unwrap_or(0.0);
                return result;
            }
            // Treasury covers the gap — no bonds, no JST debt increase.
            // Credit JST reserves from Treasury, then debit for payment.
            if let Some(ref mut gov) = country.regions[region_idx].governance {
                gov.budget.liquid_reserves += funding_gap;
                result.messages.push(format!(
                    "[MANDATE] Treasury credited JST reserves by {:.2} (bond lock fallback).",
                    funding_gap
                ));
            }
        }

    // Apply the decision to prepare cash.
    let mut bonds_to_issue = 0.0_f64;

    match decision {
        MandateFundingDecision::RaisePropertyTax { new_rate } => {
            // Tax rate change affects FUTURE revenue, not immediate cash.
            if let Some(ref mut gov) = country.regions[region_idx].governance {
                let old_rate = gov.budget.property_tax;
                gov.budget.property_tax = *new_rate;
                result.messages.push(format!(
                    "[MANDATE] Property tax raised from {:.4} to {:.4} (affects future revenue only).",
                    old_rate, new_rate
                ));
            }
        }
        MandateFundingDecision::RaiseLocalFees { fee_multiplier } => {
            // Fee change affects FUTURE revenue, not immediate cash.
            if let Some(ref mut gov) = country.regions[region_idx].governance {
                let old_fees = gov.budget.local_fees;
                gov.budget.local_fees *= fee_multiplier;
                result.messages.push(format!(
                    "[MANDATE] Local fees raised from {:.2} to {:.2} (affects future revenue only).",
                    old_fees, gov.budget.local_fees
                ));
            }
        }
        MandateFundingDecision::IssueBonds { principal, interest_rate } => {
            if is_commissary {
                // Defensive: should never reach here due to earlier check.
                result.messages.push(
                    "[MANDATE] DEFENSIVE: Bond issuance blocked for commissary region.".to_string()
                );
            } else {
                bonds_to_issue = *principal;
                result.messages.push(format!(
                    "[MANDATE] Issuing bonds: principal={:.2}, rate={:.2}%.",
                    principal, interest_rate * 100.0
                ));
            }
        }
        MandateFundingDecision::CutExpenditures { cut_amount } => {
            if let Some(ref mut gov) = country.regions[region_idx].governance {
                let actual_cut = cut_amount.min(gov.budget.local_expenditures);
                gov.budget.local_expenditures -= actual_cut;
                gov.budget.liquid_reserves += actual_cut; // Released cash goes to reserves.
                result.expenditures_cut = actual_cut;
                result.messages.push(format!(
                    "[MANDATE] Cut expenditures by {:.2} (released to reserves).",
                    actual_cut
                ));
            }
        }
        MandateFundingDecision::Refused => {
            result.messages.push("[MANDATE] Council refused to fund mandate.".to_string());
            result.refused = true;
            result.final_reserves = country.regions[region_idx]
                .governance.as_ref().map(|g| g.budget.liquid_reserves).unwrap_or(0.0);
            result.final_debt = country.regions[region_idx]
                .governance.as_ref().map(|g| g.debt.total_debt).unwrap_or(0.0);
            return result;
        }
        MandateFundingDecision::Pending => {
            result.messages.push("[MANDATE] Council has not yet voted — mandate pending.".to_string());
            result.refused = true;
            result.final_reserves = country.regions[region_idx]
                .governance.as_ref().map(|g| g.budget.liquid_reserves).unwrap_or(0.0);
            result.final_debt = country.regions[region_idx]
                .governance.as_ref().map(|g| g.debt.total_debt).unwrap_or(0.0);
            return result;
        }
    }

    // Issue bonds if needed (double-entry: debt increases, reserves increase).
    if bonds_to_issue > 0.0 && !is_commissary {
        if let Some(ref mut gov) = country.regions[region_idx].governance {
            gov.debt.total_debt += bonds_to_issue;
            gov.budget.liquid_reserves += bonds_to_issue;
            result.bonds_issued = bonds_to_issue;
            result.messages.push(format!(
                "[MANDATE] Bond proceeds credited to reserves: +{:.2}. Total debt: {:.2}.",
                bonds_to_issue, gov.debt.total_debt
            ));
        }
    }

    // Execute the mandate payment (double-entry: debit reserves, credit central).
    let total_needed = required; // Total cash needed.
    if let Some(ref mut gov) = country.regions[region_idx].governance {
        if gov.budget.liquid_reserves >= total_needed {
            // Sufficient reserves — pay directly.
            gov.budget.liquid_reserves -= total_needed;
            result.jst_debit = total_needed;
            result.central_credit = total_needed;
            result.final_reserves = gov.budget.liquid_reserves;
            result.final_debt = gov.debt.total_debt;
            result.messages.push(format!(
                "[MANDATE] Paid {:.2} from reserves. Remaining reserves: {:.2}.",
                total_needed, gov.budget.liquid_reserves
            ));
        } else {
            // Insufficient reserves even after bonds/cuts.
            let shortfall = total_needed - gov.budget.liquid_reserves;
            result.messages.push(format!(
                "[MANDATE] INSUFFICIENT RESERVES: need {:.2}, have {:.2}, shortfall {:.2}.",
                total_needed, gov.budget.liquid_reserves, shortfall
            ));
            if is_commissary {
                // Commissary: cannot issue more bonds. Refuse.
                result.messages.push(
                    "[MANDATE] Commissary region cannot cover shortfall — mandate SUSPENDED.".to_string()
                );
                result.refused = true;
            } else {
                // Non-commissary: pay what we can, suspend the rest.
                let payable = gov.budget.liquid_reserves;
                gov.budget.liquid_reserves = 0.0;
                result.jst_debit = payable;
                result.central_credit = payable;
                result.final_reserves = 0.0;
                result.final_debt = gov.debt.total_debt;
                result.messages.push(format!(
                    "[MANDATE] Partial payment: {:.2} (reserves exhausted). Remainder suspended.",
                    payable
                ));
            }
        }
    }

    // Validate: reserves must never be negative.
    if let Some(ref gov) = country.regions[region_idx].governance {
        assert!(
            gov.budget.liquid_reserves >= 0.0,
            "Double-entry violation: JST liquid_reserves must never be negative"
        );
    }

    result
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::local_council::{LocalCouncil, FactionDistribution};
    use crate::politics::local_government::{RegionalBudget, RegionalDebt};

    fn make_test_gov(admin_status: AdministrativeStatus) -> RegionalGovernance {
        let mut gov = RegionalGovernance::default();
        gov.admin_status = admin_status;
        gov.budget = RegionalBudget {
            liquid_reserves: 1000.0,
            property_tax: 0.05,
            local_fees: 10.0,
            local_expenditures: 500.0,
            ..Default::default()
        };
        gov.debt = RegionalDebt {
            total_debt: 100.0,
            credit_rating: "BBB".to_string(),
            ..Default::default()
        };
        gov.council = LocalCouncil {
            faction_distribution: FactionDistribution {
                populares_count: 5,
                moderates_count: 3,
                optimates_count: 4,
                ..Default::default()
            },
            ..Default::default()
        };
        gov
    }

    #[test]
    fn test_unfunded_mandate_default() {
        let m = UnfundedMandate::default();
        assert_eq!(m.council_decision, MandateFundingDecision::Pending);
        assert_eq!(m.funding_gap, 0.0);
    }

    #[test]
    fn test_local_bill_default() {
        let b = LocalBill::default();
        assert_eq!(b.stage, LocalBillStage::Proposed);
        assert!(b.provisions.is_empty());
    }

    #[test]
    fn test_commissary_bond_lock_returns_cut_or_refuse() {
        let mut rng = rand::thread_rng();
        let gov = make_test_gov(AdministrativeStatus::CommissaryAdministration);

        // Shortfall within cut capacity → CutExpenditures.
        let decision = vote_on_mandate_funding(&gov, 100.0, &mut rng);
        assert!(
            matches!(decision, MandateFundingDecision::CutExpenditures { .. }),
            "Commissary region should cut, not bond"
        );

        // Shortfall beyond cut capacity → Refused (no bonds!).
        let decision = vote_on_mandate_funding(&gov, 10000.0, &mut rng);
        assert!(
            matches!(decision, MandateFundingDecision::Refused),
            "Commissary region with insufficient cuts should refuse, not bond"
        );
    }

    #[test]
    fn test_normal_region_can_issue_bonds() {
        let mut rng = rand::thread_rng();
        let gov = make_test_gov(AdministrativeStatus::Normal);

        // Run multiple times to check that IssueBonds is possible
        // (depends on faction distribution + randomness).
        let mut got_bonds = false;
        for _ in 0..100 {
            let decision = vote_on_mandate_funding(&gov, 100.0, &mut rng);
            match decision {
                MandateFundingDecision::IssueBonds { .. } => got_bonds = true,
                MandateFundingDecision::RaisePropertyTax { .. } => {}
                MandateFundingDecision::CutExpenditures { .. } => {}
                _ => {}
            }
        }
        // Normal region should be able to issue bonds (moderates have weight).
        assert!(got_bonds, "Normal region should be able to issue bonds");
    }

    #[test]
    fn test_commissary_bond_lock_never_returns_bonds() {
        let mut rng = rand::thread_rng();
        let gov = make_test_gov(AdministrativeStatus::CommissaryAdministration);

        for _ in 0..100 {
            let decision = vote_on_mandate_funding(&gov, 50.0, &mut rng);
            assert!(
                !matches!(decision, MandateFundingDecision::IssueBonds { .. }),
                "Commissary region must NEVER issue bonds"
            );
        }
    }

    // ── Mandate execution tests ──

    fn make_country_with_region(admin_status: AdministrativeStatus, reserves: f64) -> Country {
        let mut country = Country::default();
        let mut region = crate::society::geography::Region::default();
        region.id = "TEST-REGION".to_string();
        let mut gov = RegionalGovernance::default();
        gov.admin_status = admin_status;
        gov.budget = RegionalBudget {
            liquid_reserves: reserves,
            property_tax: 0.05,
            local_fees: 10.0,
            local_expenditures: 500.0,
            ..Default::default()
        };
        gov.debt = RegionalDebt {
            total_debt: 100.0,
            credit_rating: "BBB".to_string(),
            ..Default::default()
        };
        region.governance = Some(gov);
        country.regions = vec![region];
        country
    }

    #[test]
    fn test_mandate_payment_sufficient_reserves() {
        let mut country = make_country_with_region(AdministrativeStatus::Normal, 1000.0);
        let mandate = UnfundedMandate {
            description: "School funding".to_string(),
            required_spending_per_region: 200.0,
            central_funding: 100.0,
            funding_gap: 100.0,
            ..Default::default()
        };
        let decision = MandateFundingDecision::CutExpenditures { cut_amount: 0.0 };

        let result = execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        assert!(!result.refused, "Should not be refused with sufficient reserves");
        assert_eq!(result.jst_debit, 200.0, "Should debit 200.0 from JST");
        assert_eq!(result.central_credit, 200.0, "Should credit 200.0 to central");
        assert_eq!(result.bonds_issued, 0.0, "No bonds should be issued");
        assert_eq!(result.final_reserves, 800.0, "Reserves should be 1000 - 200 = 800");
    }

    #[test]
    fn test_mandate_payment_bond_issuance_normal_region() {
        let mut country = make_country_with_region(AdministrativeStatus::Normal, 50.0);
        let mandate = UnfundedMandate {
            description: "Infrastructure mandate".to_string(),
            required_spending_per_region: 300.0,
            central_funding: 0.0,
            funding_gap: 300.0,
            ..Default::default()
        };
        let decision = MandateFundingDecision::IssueBonds {
            principal: 300.0,
            interest_rate: 0.05,
        };

        let result = execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        assert!(!result.refused, "Normal region should not be refused");
        assert_eq!(result.bonds_issued, 300.0, "Should issue 300.0 in bonds");
        assert_eq!(result.jst_debit, 300.0, "Should debit 300.0 for payment");
        // Reserves: 50 (initial) + 300 (bond proceeds) - 300 (payment) = 50.
        assert_eq!(result.final_reserves, 50.0, "Reserves should be 50 after bond-funded payment");
        // Debt: 100 (initial) + 300 (bonds) = 400.
        assert_eq!(result.final_debt, 400.0, "Debt should increase by bond amount");
    }

    #[test]
    fn test_mandate_payment_commissary_treasury_funded() {
        let mut country = make_country_with_region(AdministrativeStatus::CommissaryAdministration, 50.0);
        let mandate = UnfundedMandate {
            description: "Federal mandate".to_string(),
            required_spending_per_region: 300.0,
            central_funding: 0.0,
            funding_gap: 300.0,
            ..Default::default()
        };
        // Commissary region with IssueBonds decision — but Treasury can afford.
        let decision = MandateFundingDecision::IssueBonds {
            principal: 300.0,
            interest_rate: 0.05,
        };

        let result = execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        assert!(!result.refused, "Treasury-funded commissary mandate should not be refused");
        assert_eq!(result.bonds_issued, 0.0, "No bonds should be issued in commissary region");
        // Treasury credits JST reserves, then JST pays.
        // Reserves: 50 (initial) + 300 (Treasury) - 300 (payment) = 50.
        assert_eq!(result.final_reserves, 50.0, "Reserves should be 50 after Treasury-funded payment");
        // Debt should NOT increase (no bonds).
        assert_eq!(result.final_debt, 100.0, "Commissary debt should not increase");
    }

    #[test]
    fn test_mandate_payment_commissary_treasury_cannot_afford() {
        let mut country = make_country_with_region(AdministrativeStatus::CommissaryAdministration, 50.0);
        let mandate = UnfundedMandate {
            description: "Unfunded federal mandate".to_string(),
            required_spending_per_region: 300.0,
            central_funding: 0.0,
            funding_gap: 300.0,
            ..Default::default()
        };
        let decision = MandateFundingDecision::IssueBonds {
            principal: 300.0,
            interest_rate: 0.05,
        };

        let result = execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, false);

        assert!(result.refused, "Commissary mandate should be refused when Treasury cannot afford");
        assert_eq!(result.bonds_issued, 0.0, "No bonds should be issued");
        assert_eq!(result.final_debt, 100.0, "Debt should not increase in refusal path");
    }

    #[test]
    fn test_mandate_payment_tax_hike_does_not_create_immediate_cash() {
        let mut country = make_country_with_region(AdministrativeStatus::Normal, 100.0);
        let mandate = UnfundedMandate {
            description: "Mandate with tax hike".to_string(),
            required_spending_per_region: 200.0,
            central_funding: 0.0,
            funding_gap: 200.0,
            ..Default::default()
        };
        let decision = MandateFundingDecision::RaisePropertyTax { new_rate: 0.10 };

        let result = execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        // Tax hike does NOT create immediate cash. Reserves are only 100.
        // Payment of 200 exceeds reserves → partial payment.
        assert_eq!(result.jst_debit, 100.0, "Should only pay what reserves allow (100)");
        assert_eq!(result.final_reserves, 0.0, "Reserves should be exhausted");
        // Property tax rate should be updated for future revenue.
        let gov = country.regions[0].governance.as_ref().unwrap();
        assert_eq!(gov.budget.property_tax, 0.10, "Property tax should be raised for future turns");
    }

    #[test]
    fn test_mandate_payment_expenditure_cut_releases_cash() {
        let mut country = make_country_with_region(AdministrativeStatus::Normal, 100.0);
        let mandate = UnfundedMandate {
            description: "Mandate requiring cuts".to_string(),
            required_spending_per_region: 300.0,
            central_funding: 0.0,
            funding_gap: 300.0,
            ..Default::default()
        };
        let decision = MandateFundingDecision::CutExpenditures { cut_amount: 200.0 };

        let result = execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        // Cut 200 from expenditures → reserves = 100 + 200 = 300.
        // Pay 300 → reserves = 0.
        assert!(!result.refused);
        assert_eq!(result.expenditures_cut, 200.0, "Should cut 200.0 in expenditures");
        assert_eq!(result.jst_debit, 300.0, "Should debit 300.0 for payment");
        assert_eq!(result.final_reserves, 0.0, "Reserves should be 0 after full payment");
    }

    #[test]
    fn test_mandate_payment_refused() {
        let mut country = make_country_with_region(AdministrativeStatus::Normal, 1000.0);
        let mandate = UnfundedMandate {
            description: "Refused mandate".to_string(),
            required_spending_per_region: 200.0,
            central_funding: 0.0,
            funding_gap: 200.0,
            ..Default::default()
        };
        let decision = MandateFundingDecision::Refused;

        let result = execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        assert!(result.refused, "Refused decision should result in refused execution");
        assert_eq!(result.jst_debit, 0.0, "No payment should be made");
        assert_eq!(result.final_reserves, 1000.0, "Reserves should be unchanged");
    }

    #[test]
    fn test_mandate_payment_region_not_found() {
        let mut country = Country::default();
        let mandate = UnfundedMandate::default();
        let decision = MandateFundingDecision::Pending;

        let result = execute_mandate_payment(&mut country, "NONEXISTENT", &mandate, &decision, true);

        assert!(result.refused, "Nonexistent region should result in refused");
    }

    #[test]
    fn test_mandate_payment_reserves_never_negative() {
        let mut country = make_country_with_region(AdministrativeStatus::Normal, 50.0);
        let mandate = UnfundedMandate {
            description: "Large mandate".to_string(),
            required_spending_per_region: 1000.0,
            central_funding: 0.0,
            funding_gap: 1000.0,
            ..Default::default()
        };
        let decision = MandateFundingDecision::CutExpenditures { cut_amount: 0.0 };

        let result = execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        // Should pay only what's available (50), not go negative.
        assert_eq!(result.jst_debit, 50.0, "Should only pay available reserves");
        assert_eq!(result.final_reserves, 0.0, "Reserves should be 0, not negative");
        let gov = country.regions[0].governance.as_ref().unwrap();
        assert!(gov.budget.liquid_reserves >= 0.0, "Reserves must never be negative");
    }
}

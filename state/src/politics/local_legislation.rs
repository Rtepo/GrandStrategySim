//! Phase 48: Local JST legislation and unfunded mandates.
//!
//! This module implements:
//! - Local bills voted on by regional councils.
//! - Unfunded mandates imposed by the central government on regional JSTs.
//! - Strict double-entry cash flow for mandate execution.
//! - Commissary administration bond lock (prevents infinite debt spiral).

use serde::{Deserialize, Serialize};

use crate::politics::local_government::{AdministrativeStatus, RegionalGovernance};

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
    /// v4.19 WS5c: Target region. Legacy records abused `description` as
    /// the region key; new records carry an explicit region id so the
    /// description stays human-readable.
    #[serde(default)]
    pub region_id: String,
    /// v4.19 WS5c: National bill id that imposed this mandate (provenance).
    #[serde(default)]
    pub source_bill_id: String,
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
            result.messages.push(format!(
                "[MANDATE] Region {} not found — aborted.",
                region_id
            ));
            result.refused = true;
            result.final_reserves = 0.0;
            result.final_debt = 0.0;
            return result;
        }
    };

    // Phase D.6: Debit central Treasury for central_funding and credit JST.
    // This fixes the fiat-creation bug where central_funding was recorded in
    // the result but never actually debited from the Treasury.
    let treasury_available = country.budget.liquid_reserves.max(0.0);
    let actual_central_funding = central_funding.min(treasury_available);
    if actual_central_funding > 0.0 {
        country.budget.liquid_reserves -= actual_central_funding;
        if let Some(ref mut gov) = country.regions[region_idx].governance {
            gov.budget.liquid_reserves += actual_central_funding;
        }
        result.messages.push(format!(
            "[MANDATE] Treasury debited {:.2} for central funding, credited to JST reserves.",
            actual_central_funding
        ));
        if actual_central_funding < central_funding {
            result.messages.push(format!(
                "[MANDATE] Treasury shortfall: central funding clamped from {:.2} to {:.2}.",
                central_funding, actual_central_funding
            ));
        }
    }

    // Check if region is under commissary administration.
    let is_commissary = country.regions[region_idx]
        .governance
        .as_ref()
        .map(|g| g.admin_status == AdministrativeStatus::CommissaryAdministration)
        .unwrap_or(false);

    // ── COMMISSARY BOND LOCK ──
    // If commissary and decision is IssueBonds, reject it defensively.
    if is_commissary && matches!(decision, MandateFundingDecision::IssueBonds { .. }) {
        result.messages.push(
            "[MANDATE] BOND LOCK: Commissary region cannot issue bonds. Checking Treasury funding."
                .to_string(),
        );
        // Treasury must cover the gap, or mandate is refused.
        if !treasury_can_afford {
            result.messages.push(
                "[MANDATE] BOND LOCK: Treasury cannot afford funding. Mandate REFUSED.".to_string(),
            );
            result.refused = true;
            result.final_reserves = country.regions[region_idx]
                .governance
                .as_ref()
                .map(|g| g.budget.liquid_reserves)
                .unwrap_or(0.0);
            result.final_debt = country.regions[region_idx]
                .governance
                .as_ref()
                .map(|g| g.debt.total_debt)
                .unwrap_or(0.0);
            return result;
        }
        // Treasury covers the gap — no bonds, no JST debt increase.
        // Phase D.6: Debit Treasury and credit JST reserves (strict double-entry).
        let treasury_available = country.budget.liquid_reserves.max(0.0);
        let actual_gap_funding = funding_gap.min(treasury_available);
        if actual_gap_funding < funding_gap {
            result.messages.push(format!(
                "[MANDATE] BOND LOCK: Treasury shortfall: gap funding clamped from {:.2} to {:.2}.",
                funding_gap, actual_gap_funding
            ));
        }
        country.budget.liquid_reserves -= actual_gap_funding;
        if let Some(ref mut gov) = country.regions[region_idx].governance {
            gov.budget.liquid_reserves += actual_gap_funding;
            result.messages.push(format!(
                "[MANDATE] Treasury debited {:.2} for bond lock fallback, credited to JST reserves.",
                actual_gap_funding
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
        MandateFundingDecision::IssueBonds {
            principal,
            interest_rate,
        } => {
            if is_commissary {
                // Defensive: should never reach here due to earlier check.
                result.messages.push(
                    "[MANDATE] DEFENSIVE: Bond issuance blocked for commissary region.".to_string(),
                );
            } else {
                bonds_to_issue = *principal;
                result.messages.push(format!(
                    "[MANDATE] Issuing bonds: principal={:.2}, rate={:.2}%.",
                    principal,
                    interest_rate * 100.0
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
            result
                .messages
                .push("[MANDATE] Council refused to fund mandate.".to_string());
            result.refused = true;
            result.final_reserves = country.regions[region_idx]
                .governance
                .as_ref()
                .map(|g| g.budget.liquid_reserves)
                .unwrap_or(0.0);
            result.final_debt = country.regions[region_idx]
                .governance
                .as_ref()
                .map(|g| g.debt.total_debt)
                .unwrap_or(0.0);
            return result;
        }
        MandateFundingDecision::Pending => {
            result
                .messages
                .push("[MANDATE] Council has not yet voted — mandate pending.".to_string());
            result.refused = true;
            result.final_reserves = country.regions[region_idx]
                .governance
                .as_ref()
                .map(|g| g.budget.liquid_reserves)
                .unwrap_or(0.0);
            result.final_debt = country.regions[region_idx]
                .governance
                .as_ref()
                .map(|g| g.debt.total_debt)
                .unwrap_or(0.0);
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
                    "[MANDATE] Commissary region cannot cover shortfall — mandate SUSPENDED."
                        .to_string(),
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

    // v4.20 WS4: Settle the counterparty leg. `central_credit` was recorded
    // on the result but never actually credited anywhere — every executed
    // mandate silently destroyed `jst_debit` fiat (the payee was missing).
    // The mandate buys mandated public services/works, so the payment lands
    // in `ministry_public_service_pool` — an M0-counted pocket consumed by
    // the State Employer payroll path next turn.
    if result.central_credit > 0.0 {
        country.ministry_public_service_pool += result.central_credit;
        result.messages.push(format!(
            "[MANDATE] Credited {:.2} to public-service pool (mandated works delivered).",
            result.central_credit
        ));
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

/// Phase D.10: Process all active mandates in the turn loop.
///
/// For each active mandate:
/// 1. Determine the affected region.
/// 2. Vote on funding mechanism via `vote_on_mandate_funding`.
/// 3. Execute the mandate payment via `execute_mandate_payment`.
/// 4. Remove completed/refused mandates from `active_mandates`.
///
/// This function is called from the turn loop after fiscal transfers,
/// equalization, and before JST spending.
///
/// v4.20 WS4: The pass no longer early-returns on an empty mandate list —
/// regional councils convene every turn (`process_regional_council_sessions`)
/// AFTER the execution pass, so mandates adopted this turn persist in
/// `active_mandates` for a full turn (they are funded by NEXT turn's pass)
/// and `RegionDetail.active_mandates` reads a live record between turns.
pub fn process_active_mandates(country: &mut Country) {
    let current_turn = country_turn(country);

    if country.politics.active_mandates.is_empty() {
        process_regional_council_sessions(country, current_turn);
        return;
    }

    let mut rng = crate::engine::seeded_rng::thread_rng();
    let mandates = std::mem::take(&mut country.politics.active_mandates);
    let mut remaining_mandates = Vec::new();

    for mandate in mandates {
        // Find the region for this mandate.
        // v4.19 WS5c: Prefer the explicit region_id on new mandates; fall
        // back to the legacy description-as-key matching for old saves.
        let region_idx = if !mandate.region_id.is_empty() {
            country.regions.iter().position(|r| r.id == mandate.region_id)
        } else {
            country.regions.iter().position(|r| {
                r.governance
                    .as_ref()
                    .map(|g| g.id.contains(&mandate.description))
                    .unwrap_or(false)
                    || r.id == mandate.description
            })
        };

        let region_idx = match region_idx {
            Some(idx) => idx,
            None => {
                // Mandate targets a region we can't find — keep it for retry.
                remaining_mandates.push(mandate);
                continue;
            }
        };

        let region_id = country.regions[region_idx].id.clone();
        let shortfall = (mandate.required_spending_per_region - mandate.central_funding).max(0.0);

        // Vote on funding.
        let decision = {
            let gov = country.regions[region_idx].governance.as_ref();
            match gov {
                Some(g) => vote_on_mandate_funding(g, shortfall, &mut rng),
                None => MandateFundingDecision::Refused,
            }
        };

        // Check if Treasury can afford the central funding.
        let treasury_can_afford = country.budget.liquid_reserves >= mandate.central_funding;

        // Execute the mandate payment.
        let result = execute_mandate_payment(
            country,
            &region_id,
            &mandate,
            &decision,
            treasury_can_afford,
        );

        // Keep the mandate if it was refused or suspended (retry next turn).
        // Remove it if it was successfully paid.
        if !result.refused {
            // Mandate was executed (fully or partially). Remove from active.
            continue;
        }
        // Mandate was refused — keep for retry.
        remaining_mandates.push(mandate);
    }

    country.politics.active_mandates = remaining_mandates;

    // v4.20 WS4: regional councils convene after the execution pass so a
    // mandate adopted this turn is visible as `active` until NEXT turn's
    // pass funds it (snapshot reads between turns see a non-empty list).
    process_regional_council_sessions(country, current_turn);
}

/// v4.19 WS5c: Impose per-region mandates when a national bill carrying
/// regional-obligation provisions is enacted.
///
/// Real-world analogue: national infrastructure/education/health statutes
/// oblige regions (JST councils) to fund implementation — the classic
/// "unfunded mandate". Each governed region gets its own mandate record
/// whose required spending is derived from that region's actual
/// `local_expenditures` budget, with the Treasury covering a bounded share.
///
/// # Arguments
/// * `country` - Mutable country (regions, treasury, `politics.active_mandates`)
/// * `bill` - The just-enacted bill (provisions + title for description)
/// * `current_turn` - Turn the mandate is imposed
///
/// # Rules
/// * Only governed regions (with `governance`) receive mandates.
/// * `required_spending_per_region` = region's local_expenditures × share.
/// * Central funding = 50% of required, with the total across all regions
///   capped at 2% of Treasury liquid reserves (bounded, real affordability).
/// * Skip provisions that don't impose regional obligations.
/// * Deterministic: no RNG.
pub fn impose_mandates_for_bill(
    country: &mut Country,
    bill: &crate::politics::legislation::Bill,
    current_turn: u32,
) {
    use crate::politics::legislation::BillProvision;

    // Collect (label, required-share-of-local-budget) obligations.
    let mut obligations: Vec<(String, f64)> = Vec::new();
    for clause in &bill.core_clauses {
        let share = match &clause.provision {
            Some(BillProvision::InfrastructureMandate { allocation_pct }) => {
                Some((format!("{} — infrastructure works", bill.title), *allocation_pct))
            }
            Some(BillProvision::EducationLaw(_)) => {
                Some((format!("{} — compulsory education provision", bill.title), 0.08))
            }
            Some(BillProvision::HealthcareLaw(_)) => {
                Some((format!("{} — public health provision", bill.title), 0.08))
            }
            _ => None,
        };
        if let Some((label, s)) = share {
            obligations.push((label, s.max(0.0)));
        }
    }
    if obligations.is_empty() {
        return;
    }

    // Total central co-funding is capped at 2% of Treasury liquid reserves,
    // then distributed pro-rata so regions share a realistic pool.
    let governed: Vec<usize> = country
        .regions
        .iter()
        .enumerate()
        .filter(|(_, r)| r.governance.is_some())
        .map(|(i, _)| i)
        .collect();
    if governed.is_empty() {
        return;
    }

    for (label, share) in obligations {
        // Per-region required spending from each region's real local budget.
        let requireds: Vec<(usize, f64)> = governed
            .iter()
            .map(|&i| {
                let local_budget = country.regions[i]
                    .governance
                    .as_ref()
                    .map(|g| g.budget.local_expenditures)
                    .unwrap_or(0.0);
                (i, local_budget * share)
            })
            .filter(|(_, req)| *req > 0.0)
            .collect();
        let total_required: f64 = requireds.iter().map(|(_, r)| *r).sum();
        if total_required <= 0.0 {
            continue;
        }
        // Central share: 50% pro-rata, capped by real treasury affordability.
        let pool = (total_required * 0.5).min(country.budget.liquid_reserves * 0.02).max(0.0);
        for (i, required) in requireds {
            let central = pool * (required / total_required);
            country.politics.active_mandates.push(UnfundedMandate {
                description: label.clone(),
                required_spending_per_region: required,
                central_funding: central,
                funding_gap: (required - central).max(0.0),
                imposed_turn: current_turn,
                friction_score: 0.0,
                council_decision: MandateFundingDecision::Pending,
                region_id: country.regions[i].id.clone(),
                source_bill_id: bill.id.clone(),
            });
        }
    }
}

// ============================================================================
// v4.20 WS4: REGIONAL COUNCIL SESSIONS (resolutions + roll calls)
// ============================================================================

use crate::politics::local_council::{
    calculate_vote_probability, CouncilResolution, CouncilRollCall, Faction,
};

/// A council agenda item drafted from real regional conditions.
struct CouncilAgendaItem {
    /// Resolution title (stable per condition — the UI groups roll calls
    /// by `resolution_title + turn`).
    title: String,
    /// Contract category: "infrastructure" | "sanitation" | "budget".
    resolution_type: &'static str,
    /// Local cash the resolution appropriates when adopted (0 = no
    /// execution mandate).
    required_spending: f64,
    /// Ideological alignment per faction: (Populares, Moderates, Optimates).
    alignment: (f64, f64, f64),
    /// Dedup key for the execution mandate this resolution can impose —
    /// a second mandate for the same purpose is never stacked while one is
    /// still active (the council simply re-votes the resolution).
    mandate_key: &'static str,
}

/// Best available current-turn proxy when only `&mut Country` is at hand:
/// telemetry is appended at end-of-turn, so the latest sample is turn −1.
/// The explicit `process_regional_council_sessions(country, turn)` hook
/// exists for the orchestrator to pass `task.ctx.turn` directly.
fn country_turn(country: &Country) -> u32 {
    country
        .macro_indicators
        .telemetry_history
        .latest()
        .map(|s| s.turn + 1)
        .unwrap_or(0)
}

/// Convene every regional council: draft an agenda from the region's REAL
/// conditions, run a faction/trait-driven vote per sitting councilor with a
/// full roll call, and give adopted spending resolutions an execution arm
/// (an `UnfundedMandate` that next turn's `process_active_mandates` pass
/// will fund through the normal council-funding vote + double-entry
/// payment path).
///
/// # Agenda sources (no synthetic items)
/// * Infrastructure gap — `region.treasury.infrastructure_level` below the
///   60-point JST service standard. Spending = 2% of regional GDP × gap
///   fraction (the JST annual-expenditure coefficient).
/// * Sanitation backlog — `region.waste_grid` uncollected tons, landfill
///   utilization > 85%, or collection routes below 50% condition.
///   Spending = backlog tons × 1% of the national average wage per ton
///   plus a 10% local-budget expansion tranche per degraded subsystem.
/// * Budget pressure — `budget_balance < 0` or reserves < 10% of annual
///   local expenditures. Austerity ordinance; adoption cuts
///   `local_expenditures` 5% (real, immediate effect — no mandate).
///
/// # Determinism
/// Councilor votes use `calculate_vote_probability` (trait-aware) driven by
/// the engine's seeded RNG; abstention is a fixed 8-point band above the
/// support probability. No HashMap iteration affects state.
///
/// # Arguments
/// * `country` — regions, councils, budgets, `politics.active_mandates`.
/// * `current_turn` — global turn stamped on resolutions, roll calls, and
///   `imposed_turn` of generated mandates.
pub fn process_regional_council_sessions(country: &mut Country, current_turn: u32) {
    use rand::Rng;
    let mut rng = crate::engine::seeded_rng::thread_rng();

    let avg_wage = country.macro_indicators.average_wage.max(1.0);
    // Mandates already in flight — a council never stacks a second
    // execution mandate for a purpose that is still pending.
    let active_sources: std::collections::HashSet<String> = country
        .politics
        .active_mandates
        .iter()
        .map(|m| m.source_bill_id.clone())
        .collect();
    let mut new_mandates: Vec<UnfundedMandate> = Vec::new();

    for region in country.regions.iter_mut() {
        let region_id = region.id.clone();
        let region_gdp = region.gdp.max(0.0);
        // Real regional conditions — read before mutating governance.
        let infra_level = region.treasury.infrastructure_level;
        let uncollected_tons = region.waste_grid.total_uncollected();
        let landfill_fill = region.waste_grid.landfill_utilization;
        let route_condition = region.waste_grid.route_condition;

        let gov = match region.governance.as_mut() {
            Some(g) => g,
            None => continue,
        };
        if gov.council.councilors.is_empty() {
            continue;
        }

        // ── AGENDA — one item per unmet need (deterministic order) ──
        let mut agenda: Vec<CouncilAgendaItem> = Vec::new();

        // 1) Infrastructure gap.
        let infra_gap = (60.0 - infra_level).max(0.0);
        if infra_gap > 5.0 && region_gdp > 0.0 {
            agenda.push(CouncilAgendaItem {
                title: "Municipal Infrastructure Modernization Ordinance".to_string(),
                resolution_type: "infrastructure",
                required_spending: region_gdp * 0.02 * (infra_gap / 60.0),
                // Populares embrace public works; Optimates resist spending.
                alignment: (0.85, 0.60, 0.35),
                mandate_key: "infra",
            });
        }

        // 2) Sanitation backlog.
        let mut sanitation_cost = uncollected_tons * avg_wage * 0.01;
        if landfill_fill > 0.85 {
            // Landfill expansion tranche — 10% of the local budget.
            sanitation_cost += gov.budget.local_expenditures.max(0.0) * 0.10;
        }
        if route_condition < 0.5 {
            // Collection-route repair tranche.
            sanitation_cost += gov.budget.local_expenditures.max(0.0) * 0.05;
        }
        if sanitation_cost > 0.0 {
            agenda.push(CouncilAgendaItem {
                title: "Emergency Sanitation Services Resolution".to_string(),
                resolution_type: "sanitation",
                required_spending: sanitation_cost,
                alignment: (0.80, 0.65, 0.30),
                mandate_key: "sanitation",
            });
        }

        // 3) Budget pressure.
        let deficit = gov.budget.budget_balance < 0.0;
        let thin_reserves = gov.budget.local_expenditures > 0.0
            && gov.budget.liquid_reserves < gov.budget.local_expenditures * 0.10;
        if deficit || thin_reserves {
            agenda.push(CouncilAgendaItem {
                title: "Emergency Budget Stabilization Ordinance".to_string(),
                resolution_type: "budget",
                required_spending: 0.0,
                // Optimates embrace austerity; Populares resist cuts.
                alignment: (0.20, 0.55, 0.80),
                mandate_key: "budget",
            });
        }

        if agenda.is_empty() {
            continue;
        }

        // ── SESSION — debate + roll-call vote per agenda item ──
        for item in agenda {
            let mut votes_for: u32 = 0;
            let mut votes_against: u32 = 0;
            let mut abstentions: u32 = 0;
            let mut calls: Vec<CouncilRollCall> =
                Vec::with_capacity(gov.council.councilors.len());

            for councilor in &gov.council.councilors {
                let alignment = match councilor.faction {
                    Faction::Populares => item.alignment.0,
                    Faction::Moderates => item.alignment.1,
                    Faction::Optimates => item.alignment.2,
                };
                // Trait-aware support probability; councilors are not
                // bribed/blackmailed in a routine session, party discipline
                // is a mid 0.5 baseline, party wealth is not modeled here.
                let p = calculate_vote_probability(
                    councilor, false, alignment, false, false, 0.5, 0.0,
                )
                .clamp(0.02, 0.98);
                let roll: f64 = rng.gen();
                let vote = if roll < p {
                    votes_for += 1;
                    "for"
                } else if roll < p + 0.08 {
                    abstentions += 1;
                    "abstain"
                } else {
                    votes_against += 1;
                    "against"
                };
                calls.push(CouncilRollCall {
                    resolution_title: item.title.clone(),
                    member_name: councilor.name.clone(),
                    faction: councilor.faction.to_string(),
                    vote: vote.to_string(),
                    vip_id: councilor.id.clone(),
                    turn: current_turn,
                });
            }

            let adopted = votes_for > votes_against;
            gov.council.resolutions.push(CouncilResolution {
                title: item.title.clone(),
                resolution_type: item.resolution_type.to_string(),
                status: if adopted { "adopted" } else { "rejected" }.to_string(),
                votes_for,
                votes_against,
                abstentions,
                turn: current_turn,
            });
            gov.council.roll_calls.extend(calls);

            if adopted {
                gov.council.approval_rating = (gov.council.approval_rating + 0.5).min(100.0);
                if item.required_spending > 0.0 {
                    // Execution arm: the appropriation becomes an unfunded
                    // mandate on the JST budget. `central_funding = 0` — a
                    // self-appropriated local ordinance, fully JST-financed.
                    let source_id =
                        format!("COUNCIL-RES-{}-{}", region_id, item.mandate_key);
                    if !active_sources.contains(&source_id) {
                        new_mandates.push(UnfundedMandate {
                            description: item.title.clone(),
                            required_spending_per_region: item.required_spending,
                            central_funding: 0.0,
                            funding_gap: item.required_spending,
                            imposed_turn: current_turn,
                            friction_score: 0.0,
                            council_decision: MandateFundingDecision::Pending,
                            region_id: region_id.clone(),
                            source_bill_id: source_id,
                        });
                    }
                } else if item.resolution_type == "budget" {
                    // Austerity ordinance takes effect immediately — a 5%
                    // expenditure cut is a real budget decision, not a
                    // synthetic value.
                    gov.budget.local_expenditures *= 0.95;
                }
            } else {
                gov.council.approval_rating = (gov.council.approval_rating - 0.2).max(0.0);
            }
        }

        // Bounded history: 4 years of resolutions (96 turns), roll calls
        // capped at 20× per-resolution worst case (50 seats × 3 items).
        if gov.council.resolutions.len() > 96 {
            let drop = gov.council.resolutions.len() - 96;
            gov.council.resolutions.drain(0..drop);
        }
        if gov.council.roll_calls.len() > 960 {
            let drop = gov.council.roll_calls.len() - 960;
            gov.council.roll_calls.drain(0..drop);
        }
    }

    country.politics.active_mandates.extend(new_mandates);
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::local_council::{FactionDistribution, LocalCouncil};
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
        let mut rng = crate::engine::seeded_rng::thread_rng();
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
        let mut rng = crate::engine::seeded_rng::thread_rng();
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
        let mut rng = crate::engine::seeded_rng::thread_rng();
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
        make_country_with_region_and_treasury(admin_status, reserves, 0.0)
    }

    fn make_country_with_region_and_treasury(
        admin_status: AdministrativeStatus,
        reserves: f64,
        treasury_reserves: f64,
    ) -> Country {
        let mut country = Country::default();
        country.budget.liquid_reserves = treasury_reserves;
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

    // ── v4.20 WS4: Regional council sessions ──

    use crate::politics::local_council::Councilor;
    use crate::registries::enums::Commodity;

    /// A governed region whose council sits three real councilors, with an
    /// infrastructure gap (level 30 < 55) and a budget deficit — two agenda
    /// items that must always reach the floor.
    fn make_council_country() -> Country {
        let mut country = make_country_with_region(AdministrativeStatus::Normal, 5_000.0);
        let region = &mut country.regions[0];
        region.gdp = 1_000_000.0;
        region.treasury.infrastructure_level = 30.0;
        let gov = region.governance.as_mut().unwrap();
        gov.budget.budget_balance = -10.0; // deficit → austerity agenda item
        gov.council.total_seats = 3;
        gov.council.councilors = vec![
            Councilor {
                id: "C-1".to_string(),
                name: "Anna Nowak".to_string(),
                faction: Faction::Populares,
                ..Default::default()
            },
            Councilor {
                id: "C-2".to_string(),
                name: "Jan Krol".to_string(),
                faction: Faction::Moderates,
                ..Default::default()
            },
            Councilor {
                id: "C-3".to_string(),
                name: "Piotr Lis".to_string(),
                faction: Faction::Optimates,
                ..Default::default()
            },
        ];
        country
    }

    #[test]
    fn test_council_session_records_resolutions_and_roll_calls() {
        crate::engine::seeded_rng::set_seed(42);
        let mut country = make_council_country();
        process_regional_council_sessions(&mut country, 7);

        let gov = country.regions[0].governance.as_ref().unwrap();
        // Infrastructure gap + budget deficit → exactly two agenda items.
        assert_eq!(
            gov.council.resolutions.len(),
            2,
            "infrastructure + budget items must reach the floor"
        );
        let types: std::collections::HashSet<&str> = gov
            .council
            .resolutions
            .iter()
            .map(|r| r.resolution_type.as_str())
            .collect();
        assert!(types.contains("infrastructure"));
        assert!(types.contains("budget"));
        for r in &gov.council.resolutions {
            assert_eq!(r.turn, 7);
            assert_eq!(
                r.votes_for + r.votes_against + r.abstentions,
                3,
                "every sitting councilor casts a vote or abstains"
            );
            assert!(r.status == "adopted" || r.status == "rejected");
        }
        // One roll call per councilor per resolution — actual member data.
        assert_eq!(gov.council.roll_calls.len(), 2 * 3);
        for rc in &gov.council.roll_calls {
            assert!(!rc.member_name.is_empty());
            assert!(
                rc.faction == "Populares" || rc.faction == "Moderates" || rc.faction == "Optimates"
            );
            assert!(rc.vote == "for" || rc.vote == "against" || rc.vote == "abstain");
            assert_eq!(rc.turn, 7);
            assert!(gov
                .council
                .resolutions
                .iter()
                .any(|r| r.title == rc.resolution_title));
        }
        // Adopted spending resolutions become real unfunded mandates.
        let adopted_spenders = gov
            .council
            .resolutions
            .iter()
            .filter(|r| r.status == "adopted" && r.resolution_type != "budget")
            .count();
        assert_eq!(
            country.politics.active_mandates.len(),
            adopted_spenders,
            "each adopted spending resolution imposes exactly one mandate"
        );
        for m in &country.politics.active_mandates {
            assert!(m.source_bill_id.starts_with("COUNCIL-RES-TEST-REGION-"));
            assert_eq!(m.imposed_turn, 7);
            assert_eq!(m.council_decision, MandateFundingDecision::Pending);
        }
        crate::engine::seeded_rng::clear_seed();
    }

    #[test]
    fn test_process_active_mandates_convenes_council_on_empty_list() {
        crate::engine::seeded_rng::set_seed(9);
        let mut country = make_council_country();
        assert!(country.politics.active_mandates.is_empty());
        process_active_mandates(&mut country);
        let gov = country.regions[0].governance.as_ref().unwrap();
        assert!(
            !gov.council.resolutions.is_empty(),
            "council must convene even with no active mandates"
        );
        crate::engine::seeded_rng::clear_seed();
    }

    #[test]
    fn test_sanitation_resolution_driven_by_waste_backlog() {
        crate::engine::seeded_rng::set_seed(11);
        let mut country = make_council_country();
        {
            let region = &mut country.regions[0];
            region.treasury.infrastructure_level = 100.0; // no infra item
            let gov = region.governance.as_mut().unwrap();
            gov.budget.budget_balance = 0.0; // no deficit item
            region.waste_grid.add_uncollected(Commodity::MixedWaste, 500.0);
        }
        process_regional_council_sessions(&mut country, 3);
        let gov = country.regions[0].governance.as_ref().unwrap();
        assert_eq!(gov.council.resolutions.len(), 1);
        assert_eq!(gov.council.resolutions[0].resolution_type, "sanitation");
        assert_eq!(gov.council.roll_calls.len(), 3);
        crate::engine::seeded_rng::clear_seed();
    }

    #[test]
    fn test_council_mandate_dedup_no_stacking() {
        crate::engine::seeded_rng::set_seed(5);
        let mut country = make_council_country();
        // Pre-seed an in-flight council mandate for the same purpose.
        country.politics.active_mandates.push(UnfundedMandate {
            description: "prior infrastructure ordinance".to_string(),
            required_spending_per_region: 100.0,
            region_id: "TEST-REGION".to_string(),
            source_bill_id: "COUNCIL-RES-TEST-REGION-infra".to_string(),
            ..Default::default()
        });
        process_regional_council_sessions(&mut country, 8);
        let infra_mandates = country
            .politics
            .active_mandates
            .iter()
            .filter(|m| m.source_bill_id == "COUNCIL-RES-TEST-REGION-infra")
            .count();
        assert_eq!(infra_mandates, 1, "council never stacks a duplicate mandate");
        crate::engine::seeded_rng::clear_seed();
    }

    #[test]
    fn test_council_mandate_executes_and_credits_service_pool() {
        crate::engine::seeded_rng::set_seed(13);
        let mut country = make_council_country();
        country.politics.active_mandates.push(UnfundedMandate {
            description: "Municipal Infrastructure Modernization Ordinance".to_string(),
            required_spending_per_region: 200.0,
            central_funding: 0.0,
            funding_gap: 200.0,
            imposed_turn: 1,
            region_id: "TEST-REGION".to_string(),
            source_bill_id: "COUNCIL-RES-TEST-REGION-infra".to_string(),
            ..Default::default()
        });
        process_active_mandates(&mut country);
        // Either the council refused (mandate stays for retry) or the
        // payment executed — in which case the payee leg must have landed
        // in the M0-counted public-service pool (the void-sink fix).
        let still_active = !country.politics.active_mandates.is_empty();
        let credited = country.ministry_public_service_pool;
        assert!(
            still_active || credited == 200.0,
            "executed mandate must credit the service pool (got {credited})"
        );
        if !still_active {
            assert_eq!(credited, 200.0);
        }
        crate::engine::seeded_rng::clear_seed();
    }

    #[test]
    fn test_council_sessions_deterministic_under_seed() {
        let mut a = make_council_country();
        let mut b = make_council_country();
        crate::engine::seeded_rng::set_seed(77);
        process_regional_council_sessions(&mut a, 4);
        crate::engine::seeded_rng::set_seed(77);
        process_regional_council_sessions(&mut b, 4);
        let ga = a.regions[0].governance.as_ref().unwrap();
        let gb = b.regions[0].governance.as_ref().unwrap();
        assert_eq!(ga.council.resolutions, gb.council.resolutions);
        assert_eq!(ga.council.roll_calls, gb.council.roll_calls);
        crate::engine::seeded_rng::clear_seed();
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

        let result =
            execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        assert!(
            !result.refused,
            "Should not be refused with sufficient reserves"
        );
        assert_eq!(result.jst_debit, 200.0, "Should debit 200.0 from JST");
        assert_eq!(
            result.central_credit, 200.0,
            "Should credit 200.0 to central"
        );
        assert_eq!(result.bonds_issued, 0.0, "No bonds should be issued");
        assert_eq!(
            result.final_reserves, 800.0,
            "Reserves should be 1000 - 200 = 800"
        );
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

        let result =
            execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        assert!(!result.refused, "Normal region should not be refused");
        assert_eq!(result.bonds_issued, 300.0, "Should issue 300.0 in bonds");
        assert_eq!(result.jst_debit, 300.0, "Should debit 300.0 for payment");
        // Reserves: 50 (initial) + 300 (bond proceeds) - 300 (payment) = 50.
        assert_eq!(
            result.final_reserves, 50.0,
            "Reserves should be 50 after bond-funded payment"
        );
        // Debt: 100 (initial) + 300 (bonds) = 400.
        assert_eq!(
            result.final_debt, 400.0,
            "Debt should increase by bond amount"
        );
    }

    #[test]
    fn test_mandate_payment_commissary_treasury_funded() {
        let mut country = make_country_with_region_and_treasury(
            AdministrativeStatus::CommissaryAdministration,
            50.0,
            1000.0, // Treasury must have reserves to fund the gap (Phase D.6 fix).
        );
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

        let result =
            execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        assert!(
            !result.refused,
            "Treasury-funded commissary mandate should not be refused"
        );
        assert_eq!(
            result.bonds_issued, 0.0,
            "No bonds should be issued in commissary region"
        );
        // Treasury credits JST reserves, then JST pays.
        // Reserves: 50 (initial) + 300 (Treasury) - 300 (payment) = 50.
        assert_eq!(
            result.final_reserves, 50.0,
            "Reserves should be 50 after Treasury-funded payment"
        );
        // Debt should NOT increase (no bonds).
        assert_eq!(
            result.final_debt, 100.0,
            "Commissary debt should not increase"
        );
    }

    #[test]
    fn test_mandate_payment_commissary_treasury_cannot_afford() {
        let mut country =
            make_country_with_region(AdministrativeStatus::CommissaryAdministration, 50.0);
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

        let result =
            execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, false);

        assert!(
            result.refused,
            "Commissary mandate should be refused when Treasury cannot afford"
        );
        assert_eq!(result.bonds_issued, 0.0, "No bonds should be issued");
        assert_eq!(
            result.final_debt, 100.0,
            "Debt should not increase in refusal path"
        );
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

        let result =
            execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        // Tax hike does NOT create immediate cash. Reserves are only 100.
        // Payment of 200 exceeds reserves → partial payment.
        assert_eq!(
            result.jst_debit, 100.0,
            "Should only pay what reserves allow (100)"
        );
        assert_eq!(result.final_reserves, 0.0, "Reserves should be exhausted");
        // Property tax rate should be updated for future revenue.
        let gov = country.regions[0].governance.as_ref().unwrap();
        assert_eq!(
            gov.budget.property_tax, 0.10,
            "Property tax should be raised for future turns"
        );
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

        let result =
            execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        // Cut 200 from expenditures → reserves = 100 + 200 = 300.
        // Pay 300 → reserves = 0.
        assert!(!result.refused);
        assert_eq!(
            result.expenditures_cut, 200.0,
            "Should cut 200.0 in expenditures"
        );
        assert_eq!(result.jst_debit, 300.0, "Should debit 300.0 for payment");
        assert_eq!(
            result.final_reserves, 0.0,
            "Reserves should be 0 after full payment"
        );
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

        let result =
            execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        assert!(
            result.refused,
            "Refused decision should result in refused execution"
        );
        assert_eq!(result.jst_debit, 0.0, "No payment should be made");
        assert_eq!(
            result.final_reserves, 1000.0,
            "Reserves should be unchanged"
        );
    }

    #[test]
    fn test_mandate_payment_region_not_found() {
        let mut country = Country::default();
        let mandate = UnfundedMandate::default();
        let decision = MandateFundingDecision::Pending;

        let result =
            execute_mandate_payment(&mut country, "NONEXISTENT", &mandate, &decision, true);

        assert!(
            result.refused,
            "Nonexistent region should result in refused"
        );
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

        let result =
            execute_mandate_payment(&mut country, "TEST-REGION", &mandate, &decision, true);

        // Should pay only what's available (50), not go negative.
        assert_eq!(result.jst_debit, 50.0, "Should only pay available reserves");
        assert_eq!(
            result.final_reserves, 0.0,
            "Reserves should be 0, not negative"
        );
        let gov = country.regions[0].governance.as_ref().unwrap();
        assert!(
            gov.budget.liquid_reserves >= 0.0,
            "Reserves must never be negative"
        );
    }
}

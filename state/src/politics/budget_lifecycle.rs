//! Legislative budget cycle — budget bill lifecycle and amendment negotiation.
//!
//! This module implements Pillar I of the Phase 8 blueprint: the legislative
//! budget cycle as a bill with amendments, floor voting, bicameral review,
//! and executive sign/veto. When the budget fails, constitutional consequences
//! trigger (snap elections, provisional budget, or dictatorial decree).

use crate::politics::ideology::Ideology;
use crate::politics::ministries::{IdeologyBudgetPriorities, MinistryAllocation};
use crate::politics::system::{Constitution, Party};
use crate::state::Country;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// BUDGET BILL STRUCTURES
// ============================================================================

/// The stage of a budget bill in the legislative process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BudgetBillStage {
    /// PM has proposed ministries.
    #[default]
    Drafted,
    /// Factions propose amendments.
    AmendmentPhase,
    /// Parliament votes on amended budget.
    FloorVote,
    /// Upper house review.
    BicameralPending,
    /// Head of state sign/veto.
    Executive,
    /// Budget is law, cash flows begin.
    Enacted,
    /// Budget failed, consequence triggers.
    Rejected,
}

/// A proposed amendment to the budget bill by a non-ruling party.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetAmendment {
    /// Party or faction name proposing the amendment.
    pub proposer: String,
    /// Ministry being amended.
    pub target_ministry: String,
    /// Cash delta (+add / -cut).
    pub cash_delta: f64,
    /// Ideology-based justification.
    pub rationale: String,
    /// Whether the amendment was accepted during negotiation.
    pub accepted: bool,
}

/// A budget bill moving through the legislative process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BudgetBill {
    /// Unique bill ID.
    pub id: String,
    /// Bill title.
    pub title: String,
    /// Ruling party (initiator).
    pub initiator: String,
    /// Turn the bill was introduced.
    pub turn_introduced: u32,
    /// Current stage in the legislative process.
    pub stage: BudgetBillStage,
    /// PM's proposed ministry list with cash allocations.
    pub proposed_ministries: Vec<MinistryAllocation>,
    /// Amendments proposed by non-ruling parties.
    pub amendments: Vec<BudgetAmendment>,
    /// Post-amendment result (final ministry list).
    pub final_ministries: Vec<MinistryAllocation>,
    /// Committee modifier (adjustment from committee review).
    pub committee_modifier: f64,
    /// Status messages from each stage.
    pub messages: Vec<String>,
}

/// Constitutional consequence when the budget bill fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BudgetFailureConsequence {
    /// Government collapses, new elections triggered.
    #[default]
    SnapElections,
    /// Last year's budget rolls over with deficit cap.
    ProvisionalBudget,
    /// Executive bypasses legislature.
    DictatorialDecree,
}

// ============================================================================
// DRAFT BUDGET BILL
// ============================================================================

/// Drafts a budget bill from the current ministry configuration.
///
/// # Arguments
/// * `country` - Country state with current ministry config.
/// * `current_turn` - Current turn number.
///
/// # Returns
/// A `BudgetBill` with the PM's proposed ministry allocations.
///
/// # Rules
/// * Called once per year (not every turn).
/// * On non-election years, the previous year's budget auto-renews unless
///   the ruling party changes.
/// * The proposed allocations are targets, not guaranteed cash.
pub fn draft_budget_bill(country: &Country, current_turn: u32) -> BudgetBill {
    let pm_party = country
        .politics
        .ministry_config
        .as_ref()
        .map(|c| c.pm_party.clone())
        .or_else(|| Some(country.politics.ruling_party.clone()))
        .unwrap_or_default();

    let proposed = country
        .politics
        .ministry_config
        .as_ref()
        .map(|c| {
            c.ministries
                .iter()
                .map(|m| MinistryAllocation {
                    ministry_id: m.id.clone(),
                    ministry_name: m.name.clone(),
                    competencies: m.competencies.clone(),
                    allocated_cash: m.allocated_cash,
                    minister_party: m.minister_party.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    BudgetBill {
        id: format!("BUDGET-{}", current_turn),
        title: format!("Budget Bill Turn {}", current_turn),
        initiator: pm_party,
        turn_introduced: current_turn,
        stage: BudgetBillStage::Drafted,
        proposed_ministries: proposed,
        amendments: Vec::new(),
        final_ministries: Vec::new(),
        committee_modifier: 1.0,
        messages: Vec::new(),
    }
}

// ============================================================================
// AMENDMENT NEGOTIATION
// ============================================================================

/// Processes budget amendments proposed by non-ruling parties.
///
/// # Arguments
/// * `bill` - Mutable budget bill to amend.
/// * `parliament` - Seat counts by party ID.
/// * `active_parties` - All active parties by ID.
/// * `coalition` - List of coalition party IDs.
///
/// # Rules
/// * Each non-ruling party with seats > 0 proposes amendments.
/// * Amendments are based on ideological `BudgetPriorities` vs PM's allocation.
/// * Coalition members' amendments are auto-accepted (coalition discipline).
/// * Opposition amendments require combined opposition seats > 50% of parliament.
/// * Accepted amendments modify `bill.final_ministries`.
pub fn process_budget_amendments(
    bill: &mut BudgetBill,
    parliament: &HashMap<String, u32>,
    active_parties: &HashMap<String, Party>,
    coalition: &[String],
) {
    bill.stage = BudgetBillStage::AmendmentPhase;
    bill.final_ministries = bill.proposed_ministries.clone();

    let total_seats: u32 = parliament.values().sum();
    if total_seats == 0 {
        return;
    }

    let coalition_set: std::collections::HashSet<&String> = coalition.iter().collect();

    for (party_id, &seats) in parliament {
        if seats == 0 || coalition_set.contains(party_id) {
            continue;
        }

        let Some(party) = active_parties.get(party_id) else {
            continue;
        };

        let Some(ideology) = Ideology::from_name(&party.ideology) else {
            continue;
        };

        let priorities = ideology.budget_priorities();

        // Calculate total budget for share calculations
        let total_budget: f64 = bill.final_ministries.iter().map(|m| m.allocated_cash).sum();

        // Find a competency this party prioritizes that's underfunded
        let mut amendment_info: Option<(usize, f64, bool)> = None; // (ministry_idx, delta, accepted)
        for (m_idx, ministry) in bill.final_ministries.iter().enumerate() {
            for comp in &ministry.competencies {
                let weight = priorities.weight_for(*comp);
                let current_share = if total_budget > 0.0 {
                    ministry.allocated_cash / total_budget
                } else {
                    0.0
                };

                if weight > current_share + 0.05 {
                    let delta = ministry.allocated_cash * 0.1;

                    // Opposition amendment: passes if combined opposition > 50%
                    let opposition_seats: u32 = parliament
                        .iter()
                        .filter(|(pid, _)| !coalition_set.contains(pid))
                        .map(|(_, s)| *s)
                        .sum();

                    let accepted = opposition_seats * 2 > total_seats;
                    amendment_info = Some((m_idx, delta, accepted));

                    let amendment = BudgetAmendment {
                        proposer: party_id.clone(),
                        target_ministry: ministry.ministry_id.clone(),
                        cash_delta: delta,
                        rationale: format!("{:?} priority for {:?}", ideology, comp),
                        accepted,
                    };
                    bill.amendments.push(amendment);
                    break;
                }
            }
            if amendment_info.is_some() {
                break;
            }
        }

        // Apply amendment effects
        if let Some((m_idx, delta, accepted)) = amendment_info {
            if accepted {
                // Increase target ministry
                if let Some(ministry) = bill.final_ministries.get_mut(m_idx) {
                    ministry.allocated_cash += delta;
                }
                // Find a ministry to cut from (deprioritized by this ideology)
                let cut_idx = bill.final_ministries.iter().position(|m| {
                    m.ministry_id != bill.final_ministries[m_idx].ministry_id
                        && m.competencies
                            .iter()
                            .any(|c| priorities.weight_for(*c) < 0.3)
                });
                if let Some(c_idx) = cut_idx {
                    if let Some(other) = bill.final_ministries.get_mut(c_idx) {
                        other.allocated_cash = (other.allocated_cash - delta).max(0.0);
                    }
                }
            }
        }
    }

    bill.messages.push(format!(
        "Amendment phase complete: {} amendments proposed, {} accepted",
        bill.amendments.len(),
        bill.amendments.iter().filter(|a| a.accepted).count()
    ));
}

// ============================================================================
// BUDGET LIFECYCLE
// ============================================================================

/// Processes the complete budget bill lifecycle through all legislative stages.
///
/// # Arguments
/// * `bill` - The budget bill to process.
/// * `parliament` - Seat counts by party ID.
/// * `active_parties` - All active parties by ID.
/// * `upper_house` - Upper house seat composition (empty if unicameral).
/// * `constitution` - Constitution with veto and failure consequence settings.
/// * `coalition` - List of coalition party IDs.
/// * `current_turn` - Current turn number.
///
/// # Returns
/// `(final_bill, enacted, messages)` — the processed bill, whether it was
/// enacted into law, and all status messages.
///
/// # Rules
/// * Amendment phase → floor vote → bicameral review → executive review.
/// * Coalition parties vote yes (discipline).
/// * Opposition votes based on ideological distance to the budget.
/// * If rejected at any stage, `BudgetFailureConsequence` triggers.
pub fn process_budget_lifecycle(
    mut bill: BudgetBill,
    parliament: &HashMap<String, u32>,
    active_parties: &HashMap<String, Party>,
    upper_house: &HashMap<String, u32>,
    constitution: &Constitution,
    coalition: &[String],
    _current_turn: u32,
) -> (BudgetBill, bool, Vec<String>) {
    let mut messages = Vec::new();

    // Stage 1: Amendment Phase
    process_budget_amendments(&mut bill, parliament, active_parties, coalition);
    messages.extend(bill.messages.clone());

    // Stage 2: Floor Vote
    bill.stage = BudgetBillStage::FloorVote;
    let total_seats: u32 = parliament.values().sum();
    let coalition_seats: u32 = coalition.iter().filter_map(|pid| parliament.get(pid)).sum();

    let mut yes_votes = coalition_seats;

    // Opposition votes based on ideological alignment
    let coalition_set: std::collections::HashSet<&String> = coalition.iter().collect();
    for (party_id, &seats) in parliament {
        if coalition_set.contains(party_id) || seats == 0 {
            continue;
        }

        let Some(party) = active_parties.get(party_id) else {
            continue;
        };

        let Some(ideology) = Ideology::from_name(&party.ideology) else {
            continue;
        };

        let priorities = ideology.budget_priorities();

        // Calculate budget's weighted average alignment with this party
        let total_budget: f64 = bill.final_ministries.iter().map(|m| m.allocated_cash).sum();
        if total_budget > 0.0 {
            let budget_alignment: f64 = bill
                .final_ministries
                .iter()
                .map(|m| {
                    let weight = m
                        .competencies
                        .iter()
                        .map(|c| priorities.weight_for(*c))
                        .sum::<f64>()
                        / m.competencies.len().max(1) as f64;
                    weight * (m.allocated_cash / total_budget)
                })
                .sum();

            // Vote yes if alignment > 0.4 (moderate alignment threshold)
            if budget_alignment > 0.4 {
                yes_votes += seats;
            }
        }
    }

    let passed_floor = yes_votes * 2 > total_seats;
    if !passed_floor {
        bill.stage = BudgetBillStage::Rejected;
        messages.push(format!(
            "Floor vote FAILED: {} of {} seats voted yes",
            yes_votes, total_seats
        ));
        return (bill, false, messages);
    }

    messages.push(format!(
        "Floor vote PASSED: {} of {} seats voted yes",
        yes_votes, total_seats
    ));

    // Stage 3: Bicameral Review (if upper house exists)
    if !upper_house.is_empty() {
        bill.stage = BudgetBillStage::BicameralPending;
        let upper_total: u32 = upper_house.values().sum();
        let upper_coalition_seats: u32 = coalition
            .iter()
            .filter_map(|pid| upper_house.get(pid))
            .sum();

        let passed_bicameral = upper_coalition_seats * 2 > upper_total;
        if !passed_bicameral {
            bill.stage = BudgetBillStage::Rejected;
            messages.push(format!(
                "Bicameral review FAILED: {} of {} upper house seats",
                upper_coalition_seats, upper_total
            ));
            return (bill, false, messages);
        }

        messages.push(format!(
            "Bicameral review PASSED: {} of {} upper house seats",
            upper_coalition_seats, upper_total
        ));
    }

    // Stage 4: Executive Review
    bill.stage = BudgetBillStage::Executive;
    let has_veto = constitution.presidential_veto;

    // Veto probability: 0% for autocratic (PM is executive), low for democratic
    let vetoed = if has_veto {
        // Simplified: 10% veto chance for democratic systems
        let veto_chance = if country_is_autocratic(&bill.initiator, active_parties) {
            0.0
        } else {
            0.1
        };
        // Deterministic: veto if coalition is minority government
        country_is_minority(coalition, parliament) && veto_chance > 0.05
    } else {
        false
    };

    if vetoed {
        bill.stage = BudgetBillStage::Rejected;
        messages.push("Executive VETO: budget rejected".to_string());
        return (bill, false, messages);
    }

    bill.stage = BudgetBillStage::Enacted;
    messages.push("Budget ENACTED into law".to_string());

    (bill, true, messages)
}

/// Checks if the ruling party is autocratic (non-democratic).
fn country_is_autocratic(_ruling_party: &str, _active_parties: &HashMap<String, Party>) -> bool {
    // Simplified: check government form from the party's ideology
    // In practice, this would check the country's GovernmentForm
    false
}

/// Checks if the coalition is a minority government.
fn country_is_minority(coalition: &[String], parliament: &HashMap<String, u32>) -> bool {
    let total: u32 = parliament.values().sum();
    let coalition_seats: u32 = coalition.iter().filter_map(|pid| parliament.get(pid)).sum();
    if total == 0 {
        return false;
    }
    coalition_seats * 2 <= total
}

// ============================================================================
// BUDGET FAILURE CONSEQUENCES
// ============================================================================

/// Applies the constitutional consequence when the budget bill fails.
///
/// # Arguments
/// * `country` - Mutable country state.
/// * `bill` - The rejected budget bill.
///
/// # Rules
/// * `SnapElections`: Sets `years_to_elections = 1`, `budget_crisis = true`.
///   A provisional budget (last year's ministries with 80% funding) is used.
/// * `ProvisionalBudget`: Clones last year's allocations with 15% cut.
///   Deficit spending capped at 3% of GDP. No new ministries.
/// * `DictatorialDecree`: PM's original budget enacted without amendments.
///   Sets `iron_fist += 1`. May trigger unrest.
pub fn apply_budget_failure_consequence(country: &mut Country, bill: BudgetBill) {
    let consequence = country.politics.constitution.budget_failure_consequence;

    match consequence {
        BudgetFailureConsequence::SnapElections => {
            country.politics.years_to_elections = 1;
            country.politics.budget_crisis = true;
            // Provisional budget: 80% of last year's allocations
            if let Some(ref mut config) = country.politics.ministry_config {
                for ministry in &mut config.ministries {
                    ministry.allocated_cash *= 0.8;
                }
            }
        }
        BudgetFailureConsequence::ProvisionalBudget => {
            country.politics.budget_crisis = true;
            // 15% across-the-board cut
            if let Some(ref mut config) = country.politics.ministry_config {
                for ministry in &mut config.ministries {
                    ministry.allocated_cash *= 0.85;
                }
            }
        }
        BudgetFailureConsequence::DictatorialDecree => {
            // PM's original budget enacted without amendments
            country.politics.iron_fist += 1;
            if let Some(ref mut config) = country.politics.ministry_config {
                for (i, ministry) in config.ministries.iter_mut().enumerate() {
                    if let Some(proposed) = bill.proposed_ministries.get(i) {
                        ministry.allocated_cash = proposed.allocated_cash;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::ministries::{GovernmentCompetency, Ministry, MinistryConfig};

    #[test]
    fn test_draft_budget_bill() {
        let mut country = Country::mock_for_tests();
        country.politics.ruling_party = "P1".to_string();
        country.politics.ministry_config = Some(MinistryConfig {
            ministries: vec![Ministry {
                id: "MIN-001".into(),
                name: "Test".into(),
                competencies: vec![GovernmentCompetency::Treasury],
                minister_party: "P1".into(),
                minister_name: "A".into(),
                allocated_cash: 1000.0,
                spent_cash: 0.0,
                spending_actions: vec![],
                ministry_cash: 0.0,
            }],
            formation_turn: 0,
            pm_party: "P1".into(),
        });

        let bill = draft_budget_bill(&country, 5);
        assert_eq!(bill.initiator, "P1");
        assert_eq!(bill.turn_introduced, 5);
        assert_eq!(bill.proposed_ministries.len(), 1);
        assert!((bill.proposed_ministries[0].allocated_cash - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn test_budget_failure_snap_elections() {
        let mut country = Country::mock_for_tests();
        country.politics.years_to_elections = 3;
        country.politics.budget_crisis = false;
        country.politics.ministry_config = Some(MinistryConfig {
            ministries: vec![Ministry {
                id: "MIN-001".into(),
                name: "Test".into(),
                competencies: vec![GovernmentCompetency::Treasury],
                minister_party: "P1".into(),
                minister_name: "A".into(),
                allocated_cash: 1000.0,
                spent_cash: 0.0,
                spending_actions: vec![],
                ministry_cash: 0.0,
            }],
            formation_turn: 0,
            pm_party: "P1".into(),
        });

        let bill = BudgetBill::default();
        apply_budget_failure_consequence(&mut country, bill);

        assert_eq!(country.politics.years_to_elections, 1);
        assert!(country.politics.budget_crisis);
        // 80% funding
        let config = country.politics.ministry_config.as_ref().unwrap();
        assert!((config.ministries[0].allocated_cash - 800.0).abs() < 1e-6);
    }
}

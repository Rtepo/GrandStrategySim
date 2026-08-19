//! Bill lifecycle management integrating committees and legislative process

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use super::legislation::{Bill, LegislativeSession, LegislativeStage};
use super::committees::{Committee, CommitteeSystem};
use super::ideology::IdeologyCompass;
use super::local_council::{Councilor, CouncilorTrait, calculate_vote_probability};

/// Deterministic pseudo-random roll based on a seed string and probability.
///
/// # Arguments
/// * `seed` - Unique seed string (e.g. country name + bill ID + turn)
/// * `probability` - Threshold probability (0.0-1.0)
///
/// # Returns
/// true if the roll succeeds (hash normalized < probability)
///
/// # Rules
/// * Uses DefaultHasher for deterministic, reproducible results
/// * Same seed + probability always yields the same result
pub fn deterministic_roll(seed: &str, probability: f64) -> bool {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    let hash_val = hasher.finish();
    let normalized = (hash_val as f64) / (u64::MAX as f64);
    normalized < probability
}

/// Process bill through committee stage
/// 
/// # Arguments
/// * `bill` - Bill to process
/// * `committee` - Committee reviewing the bill
/// * `current_turn` - Current game turn
/// * `initiator_is_ruling` - Whether bill initiator is in ruling coalition
/// 
/// # Returns
/// (updated_bill, committee_with_assigned_bill)
/// 
/// # Rules
/// * Committee delays: 1 turn for minor bills, 2 for moderate, 3 for massive reforms
/// * Committee recommendation modifier affects floor vote
pub fn process_committee_stage(
    mut bill: Bill,
    committee: &mut Committee,
    current_turn: u32,
    initiator_is_ruling: bool,
) -> (Bill, bool) {
    bill.stage = LegislativeStage::Committee;
    bill.committee = Some(committee.id.clone());
    
    let complexity = bill.calculate_complexity();
    let delay = committee.calculate_delay(complexity);
    bill.committee_completion_turn = Some(current_turn + delay);
    
    // Calculate committee recommendation modifier
    let bill_ideology = bill.calculate_ideological_impact();
    bill.committee_modifier = committee.calculate_recommendation(
        &bill_ideology,
        &bill.initiator,
        initiator_is_ruling,
    );
    
    committee.assign_bill(bill.id.clone());
    
    (bill, true)
}

/// Process floor vote for a bill
/// 
/// # Arguments
/// * `bill` - Bill to vote on
/// * `councilors` - Councilors voting on the bill
/// * `parties` - Parties in the parliament (for discipline and wealth)
/// * `current_turn` - Current game turn
/// 
/// # Returns
/// (updated_bill, passed, messages)
/// 
/// # Rules
/// * Each councilor votes based on traits + concessions + ideological alignment
/// * Committee modifier affects vote probability
/// * Party discipline and wealth affect vote probability (trait-specific effects)
/// * Simple majority (50% + 1) required to pass
pub fn process_floor_vote(
    mut bill: Bill,
    councilors: &[Councilor],
    parties: &std::collections::HashMap<String, super::system::Party>,
    current_turn: u32,
) -> (Bill, bool, Vec<String>) {
    let mut messages = Vec::new();
    
    bill.stage = LegislativeStage::FloorVote;
    
    let bill_ideology = bill.calculate_ideological_impact();
    let mut votes_for = 0;
    let mut votes_against = 0;
    let mut total_votes = 0;
    
    for councilor in councilors {
        // Calculate ideological alignment (simplified as distance from bill ideology)
        let ideological_alignment = calculate_ideological_alignment(councilor, &bill_ideology);
        
        // Check if concession was offered to this councilor's faction
        let concession_offered = bill.concessions.iter()
            .any(|c| c.target == councilor.faction.to_string() || c.target == councilor.id);
        
        // Get party discipline and wealth for this councilor's party
        let (party_discipline, party_wealth) = if let Some(party) = parties.get(&councilor.party) {
            (party.organization.discipline, party.liquid_funds())
        } else {
            (0.5, 0.0)  // Default values if party not found
        };
        
        // Calculate vote probability with party discipline and wealth
        let vote_prob = calculate_vote_probability(
            councilor,
            concession_offered,
            ideological_alignment,
            false, // No bribery in floor vote (handled separately)
            false, // No blackmail in floor vote (handled separately)
            party_discipline,
            party_wealth,
        );
        
        // Apply committee modifier
        let adjusted_prob = (vote_prob + bill.committee_modifier).clamp(0.0, 1.0);
        
        // Deterministic roll (seeded by bill ID + councilor ID + turn)
        let roll_seed = format!("{}:{}:{}", bill.id, councilor.id, current_turn);
        if deterministic_roll(&roll_seed, adjusted_prob) {
            votes_for += 1;
        } else {
            votes_against += 1;
        }
        total_votes += 1;
    }
    
    let majority_threshold = total_votes / 2 + 1;
    let passed = votes_for >= majority_threshold;
    
    messages.push(format!(
        "[GŁOSOWANIE] Ustawa {}: {} za, {} przeciw (wymagane: {})",
        bill.title, votes_for, votes_against, majority_threshold
    ));
    
    if passed {
        bill.advance_stage(current_turn);
        messages.push(format!("[USTAWA] Ustawa {} przeszła głosowanie", bill.title));
    } else {
        bill.reject();
        messages.push(format!("[USTAWA] Ustawa {} została odrzucona", bill.title));
    }
    
    (bill, passed, messages)
}

/// Calculate ideological alignment between councilor and bill
/// 
/// # Arguments
/// * `councilor` - Councilor to check alignment for
/// * `bill_ideology` - Ideological vector of the bill
/// 
/// # Returns
/// Alignment score (0-1, higher = more aligned)
fn calculate_ideological_alignment(councilor: &Councilor, bill_ideology: &IdeologyCompass) -> f64 {
    // Simplified: use faction as proxy for ideology
    // In full implementation, councilors would have personal ideology
    match councilor.faction {
        super::local_council::Faction::Populares => {
            // Populares favor liberty and economy reform
            (bill_ideology.liberty + bill_ideology.economy) / 2.0
        }
        super::local_council::Faction::Moderates => {
            // Moderates favor balance
            1.0 - (bill_ideology.economy.abs() + bill_ideology.liberty.abs() + bill_ideology.tradition.abs()) / 3.0
        }
        super::local_council::Faction::Optimates => {
            // Optimates favor tradition
            bill_ideology.tradition
        }
    }
}

/// Process bicameral review (if applicable)
/// 
/// # Arguments
/// * `bill` - Bill to review
/// * `upper_house_composition` - Upper house seat distribution
/// * `current_turn` - Current game turn
/// 
/// # Returns
/// (updated_bill, passed, messages)
/// 
/// # Rules
/// * If country has unicameral parliament, automatically passes this stage
/// * If bicameral, requires upper house approval
pub fn process_bicameral_review(
    mut bill: Bill,
    upper_house_composition: &HashMap<String, u32>,
    current_turn: u32,
) -> (Bill, bool, Vec<String>) {
    let mut messages = Vec::new();
    
    if upper_house_composition.is_empty() {
        // Unicameral - skip to executive
        bill.advance_stage(current_turn);
        return (bill, true, messages);
    }
    
    bill.stage = LegislativeStage::BicameralPending;
    
    // Simplified upper house vote based on party composition
    let total_seats: u32 = upper_house_composition.values().sum();
    let initiator_seats = upper_house_composition.get(&bill.initiator).copied().unwrap_or(0);
    
    let majority_threshold = total_seats / 2 + 1;
    let passed = initiator_seats >= majority_threshold;
    
    messages.push(format!(
        "[IZBA WYŻSZA] Ustawa {}: {} miejsc (wymagane: {})",
        bill.title, initiator_seats, majority_threshold
    ));
    
    if passed {
        bill.advance_stage(current_turn);
        messages.push(format!("[USTAWA] Ustawa {} przeszła izbę wyższą", bill.title));
    } else {
        bill.reject();
        messages.push(format!("[USTAWA] Ustawa {} została odrzucona przez izbę wyższą", bill.title));
    }
    
    (bill, passed, messages)
}

/// Process executive review (if applicable)
/// 
/// # Arguments
/// * `bill` - Bill to review
/// * `has_veto_power` - Whether executive has veto power
/// * `current_turn` - Current game turn
/// 
/// # Returns
/// (updated_bill, enacted, messages)
/// 
/// # Rules
/// * If no veto power, automatically enacts
/// * If veto power, executive may veto (simplified: 20% chance of veto)
pub fn process_executive_review(
    mut bill: Bill,
    has_veto_power: bool,
    current_turn: u32,
) -> (Bill, bool, Vec<String>) {
    let mut messages = Vec::new();
    
    if !has_veto_power {
        bill.advance_stage(current_turn);
        return (bill, true, messages);
    }
    
    bill.stage = LegislativeStage::Executive;
    
    // Deterministic: 20% chance of veto (seeded by bill ID + turn)
    let veto_seed = format!("veto:{}:{}", bill.id, current_turn);
    let vetoed = deterministic_roll(&veto_seed, 0.2);
    
    if vetoed {
        bill.reject();
        messages.push(format!("[WETO] Ustawa {} została zawetowana przez głowę państwa", bill.title));
    } else {
        bill.advance_stage(current_turn);
        messages.push(format!("[USTAWA] Ustawa {} została podpisana przez głowę państwa", bill.title));
    }
    
    (bill, !vetoed, messages)
}

/// Process complete bill lifecycle from introduction to enactment/rejection
/// 
/// # Arguments
/// * `bill` - Bill to process
/// * `committee_system` - Committee system for committee stage
/// * `councilors` - Councilors for floor vote
/// * `parties` - Parties in the parliament (for discipline and wealth)
/// * `upper_house_composition` - Upper house composition (empty if unicameral)
/// * `has_veto_power` - Whether executive has veto power
/// * `current_turn` - Current game turn
/// * `initiator_is_ruling` - Whether bill initiator is in ruling coalition
/// 
/// # Returns
/// (final_bill, enacted, all_messages)
pub fn process_bill_lifecycle(
    bill: Bill,
    committee_system: &mut CommitteeSystem,
    councilors: &[Councilor],
    parties: &std::collections::HashMap<String, super::system::Party>,
    upper_house_composition: &HashMap<String, u32>,
    has_veto_power: bool,
    current_turn: u32,
    initiator_is_ruling: bool,
) -> (Bill, bool, Vec<String>) {
    let mut all_messages = Vec::new();
    let mut current_bill = bill;
    
    // Stage 1: Committee Review
    let committee_id = committee_system.get_committee_for_bill("general").cloned();
    if let Some(committee_id) = committee_id {
        if let Some(committee) = committee_system.get_committee_mut(&committee_id) {
            let (processed_bill, _) = process_committee_stage(current_bill, committee, current_turn, initiator_is_ruling);
            current_bill = processed_bill;
            all_messages.push(format!(
                "[KOMISJA] Ustawa {} skierowana do komisji {} (czas przeglądu: {} tur)",
                current_bill.title,
                committee.name,
                current_bill.committee_completion_turn.unwrap_or(current_turn) - current_turn
            ));
        }
    }
    
    // Stage 2: Floor Vote
    let (floor_bill, passed_floor, floor_messages) = process_floor_vote(current_bill, councilors, parties, current_turn);
    all_messages.extend(floor_messages);
    current_bill = floor_bill;
    
    if !passed_floor {
        return (current_bill, false, all_messages);
    }
    
    // Stage 3: Bicameral Review (if applicable)
    let (bicameral_bill, passed_bicameral, bicameral_messages) = 
        process_bicameral_review(current_bill, upper_house_composition, current_turn);
    all_messages.extend(bicameral_messages);
    current_bill = bicameral_bill;
    
    if !passed_bicameral {
        return (current_bill, false, all_messages);
    }
    
    // Stage 4: Executive Review
    let (executive_bill, enacted, executive_messages) = 
        process_executive_review(current_bill, has_veto_power, current_turn);
    all_messages.extend(executive_messages);
    current_bill = executive_bill;
    
    (current_bill, enacted, all_messages)
}

/// Process legislation for one turn — advance all bills through their lifecycle stages.
///
/// # Arguments
/// * `country` - Mutable country (for legislative session, committee system)
/// * `councilors` - Councilors for floor votes
/// * `parties` - Parties in parliament
/// * `current_turn` - Current game turn
///
/// # Returns
/// Vector of diagnostic messages
///
/// # Rules
/// * Bills advance through committee → floor vote → bicameral → executive stages.
/// * Enacted bills trigger `enact_law` to mutate physical economic configs.
/// * Uses deterministic rolls (no rand::random).
/// * Phase 32: If State of Emergency is active with parliament_suspended, skip all legislation.
/// * Phase 32: If Parliament struct exists, record votes in chamber's recent_votes.
pub fn process_legislation_turn(
    country: &mut crate::state::Country,
    councilors: &[Councilor],
    parties: &std::collections::HashMap<String, super::system::Party>,
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    // Phase 32: Check if parliament is suspended (State of Emergency).
    let parliament_suspended = country
        .politics
        .state_of_emergency
        .as_ref()
        .map(|soe| soe.can_bypass_parliament())
        .unwrap_or(false);

    if parliament_suspended {
        messages.push("[LEGISLATION] Parliament suspended (State of Emergency) — no bills processed.".to_string());
        return messages;
    }

    // Check if there's an active legislative session.
    let has_session = country
        .politics
        .legislative_session
        .as_ref()
        .map(|s| !s.active_bills.is_empty())
        .unwrap_or(false);

    if !has_session {
        // No active bills — this is normal, not an error.
        return messages;
    }

    // Get ruling party and upper house composition.
    let ruling_party = country.politics.ruling_party.clone();
    let upper_house_composition: HashMap<String, u32> = country.politics.upper_house.clone();
    let coalition = country.politics.coalition.clone();
    let lower_seats = country.politics.parliament.clone();
    let total_lower_seats: u32 = lower_seats.values().sum();
    let coalition_seats: u32 = coalition
        .iter()
        .filter_map(|p| lower_seats.get(p))
        .sum::<u32>()
        + lower_seats.get(&ruling_party).copied().unwrap_or(0);

    // Process bills in the legislative session.
    // We need to extract the session, process it, then put it back (borrow checker).
    let mut session = country.politics.legislative_session.take();
    let mut committee_system = country.politics.committee_system.take();

    if let Some(ref mut sess) = session {
        let bill_ids: Vec<String> = sess.active_bills.keys().cloned().collect();

        for bill_id in &bill_ids {
            // Get the bill (we need to remove it, process it, then reinsert or move to enacted/rejected).
            let mut bill = match sess.active_bills.remove(bill_id) {
                Some(b) => b,
                None => continue,
            };

            let bill_title = bill.title.clone();
            let stage = bill.stage.clone();

            match stage {
                LegislativeStage::Introduced => {
                    // Move to committee.
                    bill.stage = LegislativeStage::Committee;
                    // Assign committee if not already assigned.
                    if bill.committee.is_none() {
                        if let Some(ref cs) = committee_system {
                            bill.committee = Some(assign_committee(&bill, cs));
                        }
                    }
                    // Set committee completion turn (1-3 turns for normal, 0-1 for fast-track).
                    let committee_delay = if bill.is_fast_track() { 1 } else { 3 };
                    bill.committee_completion_turn = Some(current_turn + committee_delay);
                    let committee_name = bill.committee.clone().unwrap_or_default();
                    messages.push(format!("[LEGISLATION] Bill '{}' → Committee ({}).", bill_title, committee_name));
                    sess.active_bills.insert(bill_id.clone(), bill);
                }

                LegislativeStage::Committee => {
                    // Check if committee review is complete.
                    let completion = bill.committee_completion_turn.unwrap_or(0);
                    if current_turn >= completion {
                        // Process committee stage.
                        let recommendation = if let Some(ref cs) = committee_system {
                            process_committee_stage_phase32(&mut bill, cs, &lower_seats, &coalition, &ruling_party)
                        } else {
                            0.0
                        };

                        // Advance to floor vote.
                        bill.stage = LegislativeStage::FloorVote;
                        bill.committee_modifier = recommendation;
                        messages.push(format!(
                            "[LEGISLATION] Bill '{}' → Floor Vote (committee recommendation: {:.2}).",
                            bill_title, recommendation
                        ));
                    }
                    sess.active_bills.insert(bill_id.clone(), bill);
                }

                LegislativeStage::FloorVote => {
                    // Perform floor vote.
                    let (votes_for, votes_against, abstentions) = calculate_floor_vote(
                        &bill,
                        &lower_seats,
                        &coalition,
                        &ruling_party,
                        parties,
                        &bill_title,
                        current_turn,
                    );

                    let passed = votes_for * 2 > total_lower_seats;
                    let vote_record = super::parliament::VoteRecord {
                        bill_id: bill_id.clone(),
                        bill_title: bill_title.clone(),
                        votes_for,
                        votes_against,
                        abstentions,
                        passed,
                        turn: current_turn,
                    };

                    // Record vote in parliament struct if it exists.
                    if let Some(ref mut parl) = country.politics.parliament_struct {
                        parl.record_vote(vote_record.clone());
                    }

                    if passed {
                        // Check if bicameral.
                        let has_upper = !upper_house_composition.is_empty();
                        if has_upper {
                            bill.stage = LegislativeStage::BicameralPending;
                            messages.push(format!(
                                "[LEGISLATION] Bill '{}' PASSED lower house ({}:{}:{}) → Upper house.",
                                bill_title, votes_for, votes_against, abstentions
                            ));
                        } else {
                            bill.stage = LegislativeStage::Executive;
                            messages.push(format!(
                                "[LEGISLATION] Bill '{}' PASSED lower house ({}:{}:{}) → Executive review.",
                                bill_title, votes_for, votes_against, abstentions
                            ));
                        }
                        sess.active_bills.insert(bill_id.clone(), bill);
                    } else {
                        messages.push(format!(
                            "[LEGISLATION] Bill '{}' REJECTED ({}:{}:{}).",
                            bill_title, votes_for, votes_against, abstentions
                        ));
                        sess.rejected_bills.push(bill_id.clone());
                    }
                }

                LegislativeStage::BicameralPending => {
                    // Upper house vote.
                    let (votes_for, votes_against, _abstentions) = calculate_upper_house_vote(
                        &bill,
                        &upper_house_composition,
                        &coalition,
                        &ruling_party,
                        parties,
                        &bill_title,
                        current_turn,
                    );

                    let total_upper: u32 = upper_house_composition.values().sum();
                    let passed = if total_upper > 0 {
                        votes_for * 2 > total_upper
                    } else {
                        true // No upper house → auto-pass.
                    };

                    if passed {
                        bill.stage = LegislativeStage::Executive;
                        messages.push(format!(
                            "[LEGISLATION] Bill '{}' PASSED upper house ({}:{}) → Executive review.",
                            bill_title, votes_for, votes_against
                        ));
                        sess.active_bills.insert(bill_id.clone(), bill);
                    } else {
                        messages.push(format!(
                            "[LEGISLATION] Bill '{}' REJECTED by upper house ({}:{}).",
                            bill_title, votes_for, votes_against
                        ));
                        sess.rejected_bills.push(bill_id.clone());
                    }
                }

                LegislativeStage::Executive => {
                    // Executive review (President/Monarch signs or vetoes).
                    let sign_probability = calculate_executive_sign_probability(
                        &bill,
                        &country.politics,
                        parties,
                    );

                    let seed = format!("exec_review_{}_{}", bill_id, current_turn);
                    let signed = deterministic_roll(&seed, sign_probability);

                    if signed {
                        bill.stage = LegislativeStage::Enacted;
                        messages.push(format!(
                            "[LEGISLATION] Bill '{}' ENACTED by executive.",
                            bill_title
                        ));

                        // Phase 48: Apply all bill provisions via enact_bill → enact_law.
                        // This is the full enactment path — no half-measures.
                        let enact_msgs = super::legislation::enact_bill(country, &bill);
                        messages.extend(enact_msgs);

                        sess.enacted_laws.push(bill_id.clone());
                    } else {
                        bill.stage = LegislativeStage::Rejected;
                        messages.push(format!(
                            "[LEGISLATION] Bill '{}' VETOED by executive.",
                            bill_title
                        ));
                        sess.rejected_bills.push(bill_id.clone());
                    }
                }

                LegislativeStage::Enacted | LegislativeStage::Rejected => {
                    // Already terminal — should have been removed. Skip.
                }
            }
        }
    }

    // Restore session and committee system.
    country.politics.legislative_session = session;
    country.politics.committee_system = committee_system;

    messages
}

/// Check if a bill is fast-tracked (Phase 32).
/// This is determined by checking if the bill title or clauses contain crisis markers.
/// For now, we use a simple heuristic: bills with "crisis" or "emergency" in the title.
trait FastTrackBill {
    fn is_fast_track(&self) -> bool;
}

impl FastTrackBill for Bill {
    fn is_fast_track(&self) -> bool {
        let title_lower = self.title.to_lowercase();
        title_lower.contains("crisis") || title_lower.contains("emergency") || title_lower.contains("fast-track")
    }
}

/// Assign a committee to a bill based on its type/clauses.
fn assign_committee(bill: &Bill, cs: &CommitteeSystem) -> String {
    // Simple heuristic: use the first committee that matches.
    if let Some(first_committee) = cs.committees.keys().next() {
        first_committee.clone()
    } else {
        "Budget".to_string()
    }
}

/// Process committee stage for Phase 32 — calculate recommendation modifier.
fn process_committee_stage_phase32(
    bill: &mut Bill,
    _cs: &CommitteeSystem,
    lower_seats: &HashMap<String, u32>,
    coalition: &[String],
    ruling_party: &str,
) -> f64 {
    // Committee recommendation is based on coalition proportion in the committee.
    let total_seats: u32 = lower_seats.values().sum();
    if total_seats == 0 {
        return 0.0;
    }

    let coalition_seats: u32 = coalition
        .iter()
        .filter_map(|p| lower_seats.get(p))
        .sum::<u32>()
        + lower_seats.get(ruling_party).copied().unwrap_or(0);

    let coalition_share = coalition_seats as f64 / total_seats as f64;

    // Recommendation modifier: positive if coalition dominates, negative if not.
    (coalition_share - 0.5) * 0.4
}

/// Calculate floor vote results (deterministic).
fn calculate_floor_vote(
    bill: &Bill,
    lower_seats: &HashMap<String, u32>,
    coalition: &[String],
    ruling_party: &str,
    parties: &std::collections::HashMap<String, super::system::Party>,
    bill_title: &str,
    current_turn: u32,
) -> (u32, u32, u32) {
    let mut votes_for: u32 = 0;
    let mut votes_against: u32 = 0;
    let mut abstentions: u32 = 0;

    for (party_name, &seats) in lower_seats {
        let is_coalition = coalition.contains(party_name) || party_name == ruling_party;

        if is_coalition {
            // Coalition parties vote yes with high probability (discipline).
            let party = parties.get(party_name);
            let discipline = party.map(|p| p.organization.discipline).unwrap_or(0.7);
            let yes_prob = 0.7 + discipline * 0.25; // 0.70–0.95

            let seed = format!("floor_{}_{}_{}", bill_title, party_name, current_turn);
            if deterministic_roll(&seed, yes_prob) {
                votes_for += seats;
            } else {
                abstentions += seats / 3;
                votes_against += seats - seats / 3;
            }
        } else {
            // Opposition parties vote based on ideological alignment.
            let party = parties.get(party_name);
            let discipline = party.map(|p| p.organization.discipline).unwrap_or(0.5);

            // Opposition default: vote no, but some may be swayed.
            let no_prob = 0.6 + discipline * 0.2; // 0.60–0.80
            let yes_prob = 1.0 - no_prob;

            let seed = format!("floor_{}_{}_{}", bill_title, party_name, current_turn);
            if deterministic_roll(&seed, yes_prob) {
                votes_for += seats;
            } else {
                votes_against += seats;
            }
        }
    }

    (votes_for, votes_against, abstentions)
}

/// Calculate upper house vote results (deterministic).
fn calculate_upper_house_vote(
    bill: &Bill,
    upper_seats: &HashMap<String, u32>,
    coalition: &[String],
    ruling_party: &str,
    parties: &std::collections::HashMap<String, super::system::Party>,
    bill_title: &str,
    current_turn: u32,
) -> (u32, u32, u32) {
    // Same logic as floor vote but for upper house.
    calculate_floor_vote(bill, upper_seats, coalition, ruling_party, parties, bill_title, current_turn)
}

/// Calculate the probability that the executive signs a bill.
fn calculate_executive_sign_probability(
    bill: &Bill,
    politics: &super::system::Politics,
    parties: &std::collections::HashMap<String, super::system::Party>,
) -> f64 {
    // If the bill initiator is the ruling party, high sign probability.
    if bill.initiator == politics.ruling_party {
        return 0.9;
    }

    // If the bill initiator is a coalition partner, moderate sign probability.
    if politics.coalition.contains(&bill.initiator) {
        return 0.75;
    }

    // Opposition bill — low sign probability.
    0.3
}

// ============================================================================
// PHASE 32: PORK-BARREL VOTE BUYING
// ============================================================================

/// How pork-barrel spending is physically executed in the economy.
/// No ghost wallets — every credit goes to a real Company or ConstructionTender.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PorkBarrelMethod {
    /// Direct subsidy to companies in the target club's stronghold regions.
    /// Uses `settle_treasury_to_company()` for each recipient.
    CompanySubsidy {
        target_company_ids: Vec<String>,
        per_company_amount: f64,
    },
    /// State-funded construction tender in the target region.
    /// Creates a real `ConstructionTender` in the tender market.
    ConstructionProject {
        region_id: String,
        estimated_cost: f64,
    },
}

/// Pork-barrel offer to buy opposition votes.
/// Executed via REAL economic hooks — no ghost wallets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PorkBarrelOffer {
    /// Target club/party being bribed.
    pub target_club: String,
    /// Seats being bought.
    pub seats_bought: u32,
    /// Execution method: direct company subsidy or construction tender.
    pub method: PorkBarrelMethod,
    /// Total Treasury cost.
    pub budget_cost: f64,
    /// Political capital spent.
    pub political_capital_cost: f64,
    /// Vote probability bonus per seat (0.0–1.0).
    pub vote_bonus: f64,
}

/// Calculate the cost per seat bought (0.01% of GDP per seat).
pub fn pork_barrel_cost_per_seat(gdp: f64) -> f64 {
    (gdp * 0.0001).max(100.0) // Minimum 100 per seat.
}

/// Attempt to buy opposition votes using pork-barrel spending.
///
/// # Arguments
/// * `lower_seats` - Seat distribution in the lower house.
/// * `coalition` - Ruling coalition party IDs.
/// * `ruling_party` - Ruling party ID.
/// * `treasury_reserves` - Available Treasury cash.
/// * `political_capital` - Available political capital.
/// * `gdp` - Current GDP (for cost calculation).
///
/// # Returns
/// A list of pork-barrel offers and the total Treasury cost.
///
/// # Rules
/// * Only the ruling coalition can offer pork.
/// * Cost per seat = `GDP * 0.0001` (0.01% of GDP per seat).
/// * Vote bonus is capped at +0.3 per seat.
/// * Political capital cost = `seats_bought * 5.0`.
pub fn attempt_pork_barrel(
    lower_seats: &HashMap<String, u32>,
    coalition: &[String],
    ruling_party: &str,
    treasury_reserves: f64,
    political_capital: f64,
    gdp: f64,
) -> Vec<PorkBarrelOffer> {
    let mut offers = Vec::new();
    let cost_per_seat = pork_barrel_cost_per_seat(gdp);
    let pc_cost_per_seat = 5.0;

    // Find opposition parties with seats.
    let opposition: Vec<(String, u32)> = lower_seats
        .iter()
        .filter(|(name, _)| !coalition.contains(name) && **name != ruling_party)
        .map(|(n, &s)| (n.clone(), s))
        .collect();

    let mut remaining_treasury = treasury_reserves;
    let mut remaining_pc = political_capital;

    for (party_name, seats) in opposition {
        // Try to buy up to 30% of this party's seats.
        let target_seats = (seats as f64 * 0.3) as u32;
        if target_seats == 0 {
            continue;
        }

        let treasury_cost = cost_per_seat * target_seats as f64;
        let pc_cost = pc_cost_per_seat * target_seats as f64;

        // Check if we can afford it.
        if treasury_cost > remaining_treasury || pc_cost > remaining_pc {
            // Try fewer seats.
            let affordable_by_treasury = (remaining_treasury / cost_per_seat) as u32;
            let affordable_by_pc = (remaining_pc / pc_cost_per_seat) as u32;
            let affordable = affordable_by_treasury.min(affordable_by_pc).min(target_seats);
            if affordable == 0 {
                continue;
            }
            let actual_seats = affordable;
            let actual_treasury = cost_per_seat * actual_seats as f64;
            let actual_pc = pc_cost_per_seat * actual_seats as f64;

            offers.push(PorkBarrelOffer {
                target_club: party_name,
                seats_bought: actual_seats,
                method: PorkBarrelMethod::CompanySubsidy {
                    target_company_ids: Vec::new(), // Will be filled by caller with region companies.
                    per_company_amount: actual_treasury / actual_seats as f64,
                },
                budget_cost: actual_treasury,
                political_capital_cost: actual_pc,
                vote_bonus: 0.3 * (actual_seats as f64 / target_seats as f64).min(1.0),
            });
            remaining_treasury -= actual_treasury;
            remaining_pc -= actual_pc;
        } else {
            offers.push(PorkBarrelOffer {
                target_club: party_name,
                seats_bought: target_seats,
                method: PorkBarrelMethod::CompanySubsidy {
                    target_company_ids: Vec::new(),
                    per_company_amount: cost_per_seat,
                },
                budget_cost: treasury_cost,
                political_capital_cost: pc_cost,
                vote_bonus: 0.3,
            });
            remaining_treasury -= treasury_cost;
            remaining_pc -= pc_cost;
        }
    }

    offers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_roll_consistent() {
        let r1 = deterministic_roll("test_seed_1", 0.5);
        let r2 = deterministic_roll("test_seed_1", 0.5);
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_deterministic_roll_high_prob_always_succeeds() {
        assert!(deterministic_roll("any_seed", 1.0));
    }

    #[test]
    fn test_deterministic_roll_zero_prob_never_succeeds() {
        assert!(!deterministic_roll("any_seed", 0.0));
    }

    #[test]
    fn test_pork_barrel_cost_per_seat() {
        let cost = pork_barrel_cost_per_seat(1_000_000.0);
        assert_eq!(cost, 100.0); // 0.01% of 1M = 100.
    }

    #[test]
    fn test_pork_barrel_cost_minimum() {
        let cost = pork_barrel_cost_per_seat(100.0); // Very low GDP.
        assert_eq!(cost, 100.0); // Minimum 100.
    }

    #[test]
    fn test_attempt_pork_barrel_creates_offers() {
        let mut seats = HashMap::new();
        seats.insert("RulingParty".to_string(), 60);
        seats.insert("OppParty".to_string(), 30);
        seats.insert("OtherOpp".to_string(), 10);

        let offers = attempt_pork_barrel(
            &seats,
            &["AllyParty".to_string()],
            "RulingParty",
            100_000.0,
            100.0,
            1_000_000.0,
        );

        // Should create offers for opposition parties.
        assert!(!offers.is_empty());
        for offer in &offers {
            assert!(offer.seats_bought > 0);
            assert!(offer.budget_cost > 0.0);
            assert!(offer.political_capital_cost > 0.0);
            assert!(offer.vote_bonus <= 0.3);
        }
    }

    #[test]
    fn test_attempt_pork_barrel_no_offers_for_coalition() {
        let mut seats = HashMap::new();
        seats.insert("RulingParty".to_string(), 60);
        seats.insert("AllyParty".to_string(), 10);

        let offers = attempt_pork_barrel(
            &seats,
            &["AllyParty".to_string()],
            "RulingParty",
            100_000.0,
            100.0,
            1_000_000.0,
        );

        // No opposition parties → no offers.
        assert!(offers.is_empty());
    }

    #[test]
    fn test_attempt_pork_barrel_respects_treasury_limit() {
        let mut seats = HashMap::new();
        seats.insert("RulingParty".to_string(), 10);
        seats.insert("OppParty".to_string(), 90);

        // Very low treasury.
        let offers = attempt_pork_barrel(
            &seats,
            &[],
            "RulingParty",
            50.0, // Can't afford even 1 seat (cost=100).
            100.0,
            1_000_000.0,
        );

        // Should have no offers (can't afford any).
        assert!(offers.is_empty());
    }
}

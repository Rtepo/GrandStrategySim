//! Bill lifecycle management integrating committees and legislative process

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use super::committees::{Committee, CommitteeSystem};
use super::ideology::IdeologyCoordinates;
use super::legislation::{Bill, BillProvision, LegislativeStage};
use super::parliament::{BillDetail, ClubVoteTally, ProvisionRecord};
use super::local_council::{calculate_vote_probability, Councilor};

/// Deterministic pseudo-random uniform value in `[0, 1)` from a seed string.
///
/// # Arguments
/// * `seed` - Unique seed string (e.g. country name + bill ID + turn)
///
/// # Rules
/// * Uses DefaultHasher for deterministic, reproducible results
/// * Same seed always yields the same value
pub fn deterministic_uniform(seed: &str) -> f64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    let hash_val = hasher.finish();
    (hash_val as f64) / (u64::MAX as f64)
}

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
    deterministic_uniform(seed) < probability
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
    bill.committee_modifier =
        committee.calculate_recommendation(&bill_ideology, &bill.initiator, initiator_is_ruling);

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
        let concession_offered = bill
            .concessions
            .iter()
            .any(|c| c.target == councilor.faction.to_string() || c.target == councilor.id);

        // Get party discipline and wealth for this councilor's party
        let (party_discipline, party_wealth) = if let Some(party) = parties.get(&councilor.party) {
            (party.organization.discipline, party.liquid_funds())
        } else {
            (0.5, 0.0) // Default values if party not found
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
        "[VOTE] Bill {}: {} for, {} against (required: {})",
        bill.title, votes_for, votes_against, majority_threshold
    ));

    if passed {
        bill.advance_stage(current_turn);
        messages.push(format!("[BILL] Bill {} passed the vote", bill.title));
    } else {
        bill.reject();
        messages.push(format!("[BILL] Bill {} was rejected", bill.title));
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
fn calculate_ideological_alignment(councilor: &Councilor, bill_ideology: &IdeologyCoordinates) -> f64 {
    // Simplified: use faction as proxy for ideology
    // In full implementation, councilors would have personal ideology
    match councilor.faction {
        super::local_council::Faction::Populares => {
            // Populares favor liberty and economy reform
            (bill_ideology.liberty + bill_ideology.economy) / 2.0
        }
        super::local_council::Faction::Moderates => {
            // Moderates favor balance
            1.0 - (bill_ideology.economy.abs()
                + bill_ideology.liberty.abs()
                + bill_ideology.tradition.abs())
                / 3.0
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
    let initiator_seats = upper_house_composition
        .get(&bill.initiator)
        .copied()
        .unwrap_or(0);

    let majority_threshold = total_seats / 2 + 1;
    let passed = initiator_seats >= majority_threshold;

    messages.push(format!(
        "[UPPER CHAMBER] Bill {}: {} seats (required: {})",
        bill.title, initiator_seats, majority_threshold
    ));

    if passed {
        bill.advance_stage(current_turn);
        messages.push(format!(
            "[BILL] Bill {} passed the upper chamber",
            bill.title
        ));
    } else {
        bill.reject();
        messages.push(format!(
            "[BILL] Bill {} was rejected by the upper chamber",
            bill.title
        ));
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
        messages.push(format!(
            "[VETO] Bill {} was vetoed by the head of state",
            bill.title
        ));
    } else {
        bill.advance_stage(current_turn);
        messages.push(format!(
            "[BILL] Bill {} was signed by the head of state",
            bill.title
        ));
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
#[deprecated(
    note = "Phase 86: Use process_legislation_turn() for per-turn bill advancement. This instant lifecycle bypasses the per-turn pipeline."
)]
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
            let (processed_bill, _) =
                process_committee_stage(current_bill, committee, current_turn, initiator_is_ruling);
            current_bill = processed_bill;
            all_messages.push(format!(
                "[COMMITTEE] Bill {} sent to committee {} (review time: {} turns)",
                current_bill.title,
                committee.name,
                current_bill
                    .committee_completion_turn
                    .unwrap_or(current_turn)
                    - current_turn
            ));
        }
    }

    // Stage 2: Floor Vote
    let (floor_bill, passed_floor, floor_messages) =
        process_floor_vote(current_bill, councilors, parties, current_turn);
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
    _councilors: &[Councilor],
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
        messages.push(
            "[LEGISLATION] Parliament suspended (State of Emergency) — no bills processed."
                .to_string(),
        );
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
    // Prefer the chamber's live seat map (it reflects mid-term splinter clubs);
    // fall back to the legacy flat parliament map when no struct exists.
    let lower_seats = country
        .politics
        .parliament_struct
        .as_ref()
        .map(|p| p.lower_seat_map())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| country.politics.parliament.clone());
    let total_lower_seats: u32 = lower_seats.values().sum();
    let _coalition_seats: u32 = coalition
        .iter()
        .filter_map(|p| lower_seats.get(p))
        .sum::<u32>()
        + lower_seats.get(&ruling_party).copied().unwrap_or(0);

    // Process bills in the legislative session.
    // We need to extract the session, process it, then put it back (borrow checker).
    let mut session = country.politics.legislative_session.take();
    let mut committee_system = country.politics.committee_system.take();

    if let Some(ref mut sess) = session {
        // Determinism: iterate active bills in sorted id order — HashMap
        // iteration order is arbitrary and enactment order can matter when
        // two bills pass on the same turn.
        let mut bill_ids: Vec<String> = sess.active_bills.keys().cloned().collect();
        bill_ids.sort();

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
                    messages.push(format!(
                        "[LEGISLATION] Bill '{}' → Committee ({}).",
                        bill_title, committee_name
                    ));
                    sess.active_bills.insert(bill_id.clone(), bill);
                }

                LegislativeStage::Committee => {
                    // Check if committee review is complete.
                    let completion = bill.committee_completion_turn.unwrap_or(0);
                    if current_turn >= completion {
                        // Process committee stage.
                        let recommendation = if let Some(ref cs) = committee_system {
                            process_committee_stage_phase32(
                                &mut bill,
                                cs,
                                &lower_seats,
                                &coalition,
                                &ruling_party,
                            )
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
                    // Phase 86: Get health capacity and social unrest for attendance.
                    // social_unrest is 0–100 on macro_indicators; normalize to 0.0–1.0.
                    let social_unrest =
                        (country.macro_indicators.social_unrest / 100.0).clamp(0.0, 1.0);
                    // Health capacity ratio: derived from healthcare law universality.
                    // Default to 0.5 (moderate) if no healthcare law is set.
                    let health_capacity_ratio = country
                        .politics
                        .healthcare_law
                        .as_ref()
                        .map(|law| match law.universality {
                            crate::politics::laws::UniversalityLevel::Universal => 1.0,
                            crate::politics::laws::UniversalityLevel::MeansTested => 0.7,
                            crate::politics::laws::UniversalityLevel::Categorical => 0.6,
                            crate::politics::laws::UniversalityLevel::Limited => 0.3,
                        })
                        .unwrap_or(0.5);

                    // Perform floor vote with attendance and quorum.
                    let (votes_for, votes_against, abstentions, quorum_met, club_tallies) =
                        calculate_floor_vote(
                            &bill,
                            &lower_seats,
                            &coalition,
                            &ruling_party,
                            parties,
                            &bill_title,
                            current_turn,
                            health_capacity_ratio,
                            social_unrest,
                        );

                    // Phase 86: If quorum failed, bill stays in FloorVote for next turn.
                    if !quorum_met {
                        messages.push(format!(
                            "[QUORUM] Bill '{}' failed quorum — insufficient attendance. Bill held for next turn.",
                            bill_title
                        ));
                        sess.active_bills.insert(bill_id.clone(), bill);
                        continue;
                    }

                    // Phase 86: Three majority thresholds based on legislative weight.
                    let passed = match bill.weight {
                        super::legislative_weight::LegislativeWeight::Ordinary => {
                            // Relative: >50% of present (non-abstaining) MPs
                            let present = votes_for + votes_against;
                            if present == 0 {
                                false
                            } else {
                                votes_for * 2 > present
                            }
                        }
                        super::legislative_weight::LegislativeWeight::Organic => {
                            // Absolute: >50% of total seats
                            votes_for * 2 > total_lower_seats
                        }
                        super::legislative_weight::LegislativeWeight::Constitutional => {
                            // Qualified: 2/3 of total seats
                            votes_for * 3 > total_lower_seats * 2
                        }
                    };
                    let vote_record = super::parliament::VoteRecord {
                        bill_id: bill_id.clone(),
                        bill_title: bill_title.clone(),
                        bill_type: bill.bill_type,
                        votes_for,
                        votes_against,
                        abstentions,
                        passed,
                        turn: current_turn,
                        club_breakdown: club_tallies,
                        // Snapshot the bill's provisions/sponsor at vote time —
                        // the record must carry its own detail once the bill
                        // leaves the active session.
                        detail: Some(build_bill_detail(
                            &bill,
                            current_turn,
                            !upper_house_composition.is_empty(),
                        )),
                    };

                    // Record vote in parliament struct if it exists.
                    if let Some(ref mut parl) = country.politics.parliament_struct {
                        parl.record_vote(vote_record.clone());
                    }
                    // Real legislative telemetry — the parliamentary record the
                    // UI reads, not a synthetic history string.
                    country.politics.history.push(serde_json::json!({
                        "event": "floor_vote",
                        "bill_id": bill_id,
                        "title": bill_title,
                        "bill_type": bill.bill_type.as_str(),
                        "votes_for": votes_for,
                        "votes_against": votes_against,
                        "abstentions": abstentions,
                        "passed": passed,
                        "turn": current_turn,
                    }));

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
                        // Stamp the terminal outcome on the recorded vote —
                        // name the rejecting chamber so the UI can badge
                        // "Rejected by <chamber>" instead of a bare "Rejected".
                        if let Some(ref mut parl) = country.politics.parliament_struct {
                            let lower_name = parl
                                .chambers
                                .first()
                                .map(|c| c.name.clone())
                                .unwrap_or_else(|| "Lower House".to_string());
                            parl.mark_bill_terminal(
                                bill_id,
                                &format!("Rejected by {lower_name}"),
                                current_turn,
                            );
                        }
                    }
                }

                LegislativeStage::BicameralPending => {
                    // Upper house vote.
                    // Phase 86: Pass attendance parameters to upper house vote.
                    let social_unrest_norm =
                        (country.macro_indicators.social_unrest / 100.0).clamp(0.0, 1.0);
                    let health_cap = country
                        .politics
                        .healthcare_law
                        .as_ref()
                        .map(|law| match law.universality {
                            crate::politics::laws::UniversalityLevel::Universal => 1.0,
                            crate::politics::laws::UniversalityLevel::MeansTested => 0.7,
                            crate::politics::laws::UniversalityLevel::Categorical => 0.6,
                            crate::politics::laws::UniversalityLevel::Limited => 0.3,
                        })
                        .unwrap_or(0.5);
                    let (votes_for, votes_against, abstentions, upper_quorum_met, upper_tallies) =
                        calculate_upper_house_vote(
                            &bill,
                            &upper_house_composition,
                            &coalition,
                            &ruling_party,
                            parties,
                            &bill_title,
                            current_turn,
                            health_cap,
                            social_unrest_norm,
                        );

                    let total_upper: u32 = upper_house_composition.values().sum();

                    // Phase 86: If upper house quorum failed, bill stays in BicameralPending.
                    if !upper_quorum_met && total_upper > 0 {
                        messages.push(format!(
                            "[QUORUM] Bill '{}' failed upper house quorum — held for next turn.",
                            bill_title
                        ));
                        sess.active_bills.insert(bill_id.clone(), bill);
                        continue;
                    }

                    let passed = if total_upper > 0 {
                        // Phase 86: Apply same majority threshold as lower house.
                        match bill.weight {
                            super::legislative_weight::LegislativeWeight::Ordinary => {
                                let present = votes_for + votes_against;
                                if present == 0 {
                                    false
                                } else {
                                    votes_for * 2 > present
                                }
                            }
                            super::legislative_weight::LegislativeWeight::Organic => {
                                votes_for * 2 > total_upper
                            }
                            super::legislative_weight::LegislativeWeight::Constitutional => {
                                votes_for * 3 > total_upper * 2
                            }
                        }
                    } else {
                        true // No upper house → auto-pass.
                    };

                    // Record the upper-house vote in the upper chamber's own
                    // recent_votes log when a second chamber exists.
                    if total_upper > 0 {
                        let upper_record = super::parliament::VoteRecord {
                            bill_id: bill_id.clone(),
                            bill_title: bill_title.clone(),
                            bill_type: bill.bill_type,
                            votes_for,
                            votes_against,
                            abstentions,
                            passed,
                            turn: current_turn,
                            club_breakdown: upper_tallies,
                            detail: Some(build_bill_detail(&bill, current_turn, true)),
                        };
                        if let Some(ref mut parl) = country.politics.parliament_struct {
                            parl.record_upper_vote(upper_record);
                        }
                    }

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
                        if let Some(ref mut parl) = country.politics.parliament_struct {
                            let upper_name = parl
                                .chambers
                                .get(1)
                                .map(|c| c.name.clone())
                                .unwrap_or_else(|| "Upper House".to_string());
                            parl.mark_bill_terminal(
                                bill_id,
                                &format!("Rejected by {upper_name}"),
                                current_turn,
                            );
                        }
                    }
                }

                LegislativeStage::Executive => {
                    // Executive review (President/Monarch signs or vetoes).
                    let sign_probability =
                        calculate_executive_sign_probability(&bill, &country.politics, parties);

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

                        // v4.19 WS5c: Regional-obligation provisions impose
                        // unfunded mandates on governed regions (displayed in
                        // Regions → Mandates, executed by process_active_mandates).
                        super::local_legislation::impose_mandates_for_bill(
                            country,
                            &bill,
                            current_turn,
                        );

                        sess.enacted_laws.push(bill_id.clone());
                        // Stamp the effective turn on the parliamentary record —
                        // the law takes effect on the enactment turn.
                        if let Some(ref mut parl) = country.politics.parliament_struct {
                            parl.mark_bill_terminal(bill_id, "Enacted", current_turn);
                        }
                    } else {
                        bill.stage = LegislativeStage::Rejected;
                        messages.push(format!(
                            "[LEGISLATION] Bill '{}' VETOED by executive.",
                            bill_title
                        ));
                        sess.rejected_bills.push(bill_id.clone());
                        let hos = &country.politics.head_of_state.name;
                        let veto_label = if hos.is_empty() {
                            "Vetoed by Head of State".to_string()
                        } else {
                            format!("Vetoed by {hos}")
                        };
                        if let Some(ref mut parl) = country.politics.parliament_struct {
                            parl.mark_bill_terminal(bill_id, &veto_label, current_turn);
                        }
                    }
                }

                LegislativeStage::Enacted | LegislativeStage::Rejected => {
                    // Already terminal — should have been removed. Skip.
                }
            }
        }
    }

    // Prune chamber legislative queues of bill IDs that are no longer active
    // (enacted, rejected, or vetoed bills were removed from active_bills above).
    if let (Some(ref mut parl), Some(ref sess)) =
        (&mut country.politics.parliament_struct, &session)
    {
        for chamber in &mut parl.chambers {
            chamber
                .legislative_queue
                .retain(|id| sess.active_bills.contains_key(id));
        }
    }

    // v4.18: Reconcile each committee's `bills_under_review` roster with the
    // live session state — the roster is derived truth, rebuilt every turn
    // from the bills that actually sit in `LegislativeStage::Committee`
    // pointing at that committee. Sorted for deterministic serialization.
    if let (Some(ref mut cs), Some(ref sess)) = (&mut committee_system, &session) {
        for committee in cs.committees.values_mut() {
            let cid = committee.id.clone();
            let mut ids: Vec<String> = sess
                .active_bills
                .iter()
                .filter(|(_, b)| {
                    matches!(b.stage, LegislativeStage::Committee)
                        && b.committee.as_deref() == Some(cid.as_str())
                })
                .map(|(bid, _)| bid.clone())
                .collect();
            ids.sort();
            committee.bills_under_review = ids;
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
        title_lower.contains("crisis")
            || title_lower.contains("emergency")
            || title_lower.contains("fast-track")
    }
}

/// Assign a committee to a bill based on its type/clauses.
///
/// Deterministic: routes through `bill_type_assignments` when populated,
/// otherwise falls back to the lexicographically smallest committee id —
/// `HashMap::keys().next()` iteration order is arbitrary and would assign
/// different committees run-to-run.
fn assign_committee(bill: &Bill, cs: &CommitteeSystem) -> String {
    // v4.19 WS4: route on the bill's actual policy domain, not a title
    // keyword guess. The old fallback read `bill_type_assignments["general"]`
    // (nonexistent) and landed every non-budget bill in `committees.keys()
    // .min()` — alphabetically the Budget Committee — so seven committees
    // sat at zero bills while Budget swallowed everything.
    use crate::politics::legislation::BillType;
    let mapped: Option<&'static str> = match bill.bill_type {
        BillType::Budget | BillType::EmergencyFiscal | BillType::EmergencyBonds | BillType::TaxReform => {
            Some("budget_committee")
        }
        BillType::Healthcare => Some("health_committee"),
        BillType::Education => Some("education_committee"),
        BillType::Justice | BillType::Sentencing => Some("justice_committee"),
        BillType::Infrastructure | BillType::Transport => Some("infrastructure_committee"),
        BillType::Labor | BillType::FreeSpeech | BillType::Migration => {
            Some("social_affairs_committee")
        }
        BillType::Other => None,
    };
    if let Some(committee_id) = mapped {
        if cs.committees.contains_key(committee_id) {
            return committee_id.to_string();
        }
    }

    // Uncategorized bills: title keywords first, then the least-loaded
    // committee (fair rotation) — never a fixed default.
    let title = bill.title.to_lowercase();
    let keyword_hit = [
        ("budget", "budget_committee"),
        ("crisis", "budget_committee"),
        ("emergency", "budget_committee"),
        ("bond", "budget_committee"),
        ("tax", "budget_committee"),
        ("health", "health_committee"),
        ("hospital", "health_committee"),
        ("education", "education_committee"),
        ("school", "education_committee"),
        ("justice", "justice_committee"),
        ("court", "justice_committee"),
        ("sentenc", "justice_committee"),
        ("infrastructure", "infrastructure_committee"),
        ("transport", "infrastructure_committee"),
        ("rail", "infrastructure_committee"),
        ("defense", "defense_committee"),
        ("military", "defense_committee"),
        ("army", "defense_committee"),
        ("foreign", "foreign_affairs_committee"),
        ("treaty", "foreign_affairs_committee"),
        ("labor", "social_affairs_committee"),
        ("speech", "social_affairs_committee"),
        ("migration", "social_affairs_committee"),
    ]
    .iter()
    .find(|(kw, _)| title.contains(kw))
    .map(|(_, cid)| *cid);
    if let Some(committee_id) = keyword_hit {
        if cs.committees.contains_key(committee_id) {
            return committee_id.to_string();
        }
    }
    cs.committees
        .iter()
        .min_by(|(id_a, a), (id_b, b)| {
            a.bills_under_review
                .len()
                .cmp(&b.bills_under_review.len())
                .then_with(|| id_a.cmp(id_b))
        })
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| "budget_committee".to_string())
}

/// Process committee stage for Phase 32 — calculate recommendation modifier.
fn process_committee_stage_phase32(
    _bill: &mut Bill,
    _cs: &CommitteeSystem,
    lower_seats: &HashMap<String, u32>,
    coalition: &[String],
    ruling_party: &str,
) -> f64 {
    committee_recommendation_modifier(lower_seats, coalition, ruling_party)
}

/// The committee-stage recommendation modifier recorded on a bill when its
/// review completes — a deterministic function of the coalition's share of
/// the lower house. Extracted so the snapshot layer can preview the exact
/// same value for bills still under review (same inputs, same formula —
/// no fabrication).
pub fn committee_recommendation_modifier(
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

// ============================================================================
// v4.18: BILL DETAIL PROJECTION — provisions/effects for the UI drawer
// ============================================================================

/// Render a `BillProvision`'s concrete effect for the UI detail drawer.
/// Every value is read straight off the typed provision — this is a field
/// formatter, not prose generation.
///
/// # Returns
/// `(kind, detail)` — the `BillProvision` variant name and a compact
/// "field: value" rendering of its concrete parameters.
pub fn describe_provision(p: &BillProvision) -> (String, String) {
    match p {
        BillProvision::TaxRateChange {
            income_tax,
            vat,
            corporate_tax,
        } => {
            let mut parts = Vec::new();
            if let Some(v) = income_tax {
                parts.push(format!("income tax → {:.1}%", v * 100.0));
            }
            if let Some(v) = vat {
                parts.push(format!("VAT → {:.1}%", v * 100.0));
            }
            if let Some(v) = corporate_tax {
                parts.push(format!("corporate tax → {:.1}%", v * 100.0));
            }
            ("TaxRateChange".to_string(), parts.join(" · "))
        }
        BillProvision::PriceControl {
            commodity,
            min_price,
            max_price,
        } => {
            let mut parts = vec![commodity.clone()];
            if let Some(v) = min_price {
                parts.push(format!("min {:.2}", v));
            }
            if let Some(v) = max_price {
                parts.push(format!("max {:.2}", v));
            }
            ("PriceControl".to_string(), parts.join(" · "))
        }
        BillProvision::Subsidy {
            target,
            amount_per_unit,
        } => (
            "Subsidy".to_string(),
            format!("{} · {:.2} per unit", target, amount_per_unit),
        ),
        BillProvision::Deregulation { sector, scope } => (
            "Deregulation".to_string(),
            format!("{} · scope: {}", sector, scope),
        ),
        BillProvision::HealthcareLaw(law) => (
            "HealthcareLaw".to_string(),
            format!(
                "system {:?} · universality {:?}",
                law.healthcare_system, law.universality
            ),
        ),
        BillProvision::EducationLaw(law) => (
            "EducationLaw".to_string(),
            format!(
                "compulsory {} yrs (to age {}) · {:?} · {:?}",
                law.compulsory_education.compulsory_years,
                law.compulsory_education.end_age,
                law.education_model,
                law.school_system
            ),
        ),
        BillProvision::JusticeLaw(law) => (
            "JusticeLaw".to_string(),
            format!(
                "KRS separated={} · PG separated={} · corruption {:.2} · wait {:?}",
                law.krs_separated,
                law.prosecutor_general_separated,
                law.corruption_index,
                law.court_wait_time_target
            ),
        ),
        BillProvision::InfrastructureMandate { allocation_pct } => (
            "InfrastructureMandate".to_string(),
            format!("allocation {:.1}% of budget", allocation_pct * 100.0),
        ),
        BillProvision::FreeSpeechLaw(law) => (
            "FreeSpeechLaw".to_string(),
            format!(
                "speech {:?} · assembly {:?} · press {:?}",
                law.free_speech_level, law.assembly_rights, law.press_freedom
            ),
        ),
        BillProvision::TransportLaw(law) => (
            "TransportLaw".to_string(),
            format!(
                "ownership {:?} · public subsidy {:.0}%",
                law.ownership,
                law.public_subsidy_fraction * 100.0
            ),
        ),
        BillProvision::SentencingLaw(law) => (
            "SentencingLaw".to_string(),
            format!(
                "death_penalty={} · life={} · community_service={} · dualism={}",
                law.death_penalty_enabled,
                law.life_imprisonment_enabled,
                law.community_service_enabled,
                law.legal_dualism_enabled
            ),
        ),
        BillProvision::MigrationLawChange(law) => (
            "MigrationLawChange".to_string(),
            format!(
                "open_borders={} · visa_required={} · deportation {:?}",
                law.open_borders, law.visa_required, law.deportation_policy
            ),
        ),
        BillProvision::ChildLaborLaw(law) => (
            "ChildLaborLaw".to_string(),
            format!("child labor {:?}", law),
        ),
        BillProvision::Custom {
            description,
            effect_key,
            effect_value,
        } => (
            "Custom".to_string(),
            format!("{} · {}={:.2}", description, effect_key, effect_value),
        ),
    }
}

/// Build a `BillDetail` snapshot of a bill's substantive content — typed
/// provisions/effects, sponsor, stage and effective-turn data for the UI
/// detail drawer. Recorded onto `VoteRecord`s at vote time (bills leave
/// `active_bills` on enactment/rejection) and projected live for queued
/// bills in the snapshot builder.
///
/// `earliest_enactment_turn` is pure pipeline arithmetic on the bill's real
/// stage fields — committee wait → floor vote → optional upper house →
/// executive signature (each stage transition takes one turn).
pub fn build_bill_detail(
    bill: &Bill,
    current_turn: u32,
    has_upper_chamber: bool,
) -> BillDetail {
    let upper_hops = if has_upper_chamber { 1 } else { 0 };
    let earliest = match bill.stage {
        // Introduced → Committee (delay 1–3, assume standard 3) → +2 hops.
        LegislativeStage::Introduced => Some(current_turn + 3 + 2 + upper_hops),
        // Committee completes at `committee_completion_turn`, then floor →
        // (upper) → executive: +2 unicameral, +3 bicameral.
        LegislativeStage::Committee => Some(
            bill.committee_completion_turn.unwrap_or(current_turn) + 2 + upper_hops,
        ),
        LegislativeStage::FloorVote => Some(current_turn + 1 + upper_hops),
        LegislativeStage::BicameralPending => Some(current_turn + 1),
        LegislativeStage::Executive => Some(current_turn),
        LegislativeStage::Enacted | LegislativeStage::Rejected => None,
    };

    BillDetail {
        initiator: bill.initiator.clone(),
        bill_type: bill.bill_type.as_str().to_string(),
        weight: format!("{:?}", bill.weight),
        stage: format!("{:?}", bill.stage),
        introduction_turn: bill.introduction_turn,
        enacted_turn: None,
        earliest_enactment_turn: earliest,
        committee: bill.committee.clone(),
        provisions: bill
            .core_clauses
            .iter()
            .map(|c| {
                let (kind, detail) = c
                    .provision
                    .as_ref()
                    .map(describe_provision)
                    .unwrap_or_else(|| ("Descriptive".to_string(), String::new()));
                ProvisionRecord {
                    description: c.description.clone(),
                    kind,
                    detail,
                    budget_impact: c.budget_impact,
                    sunset_turn: c.sunset_turn,
                    mutated: c.mutated,
                    mutation_notes: c.mutation_notes.clone(),
                }
            })
            .collect(),
    }
}

/// Calculate floor vote results (deterministic).
/// Phase 86: Integrates dynamic attendance and quorum checks.
/// v4.18: Also returns per-club tallies so the parliamentary record keeps
/// real For/Against/Abstain bookkeeping by club, not just chamber totals.
fn calculate_floor_vote(
    bill: &Bill,
    lower_seats: &HashMap<String, u32>,
    coalition: &[String],
    ruling_party: &str,
    parties: &std::collections::HashMap<String, super::system::Party>,
    bill_title: &str,
    current_turn: u32,
    health_capacity_ratio: f64,
    social_unrest: f64,
) -> (u32, u32, u32, bool, Vec<ClubVoteTally>) {
    // Phase 86: Calculate attendance and check quorum.
    let attendance = super::attendance::calculate_attendance(
        lower_seats,
        parties,
        health_capacity_ratio,
        social_unrest,
        bill.weight,
        bill_title,
        current_turn,
    );

    // If quorum is not met, the vote is blocked.
    if !attendance.quorum_met {
        return (0, 0, 0, false, Vec::new());
    }

    let mut votes_for: u32 = 0;
    let mut votes_against: u32 = 0;
    let mut abstentions: u32 = 0;
    let mut club_breakdown: Vec<ClubVoteTally> = Vec::new();

    // Phase 86: Use present seats (from attendance) instead of total seats.
    for (party_name, &_total_seats) in lower_seats {
        let present_seats = attendance
            .present_by_party
            .get(party_name)
            .copied()
            .unwrap_or(0);
        if present_seats == 0 {
            continue; // No MPs from this party are present
        }

        let is_coalition = coalition.contains(party_name) || party_name == ruling_party;
        let mut tally = ClubVoteTally {
            club_id: party_name.clone(),
            ..Default::default()
        };

        if is_coalition {
            // Coalition parties vote yes with high probability (discipline).
            let party = parties.get(party_name);
            let discipline = party.map(|p| p.organization.discipline).unwrap_or(0.7);
            let yes_prob = 0.7 + discipline * 0.25; // 0.70–0.95

            let seed = format!("floor_{}_{}_{}", bill_title, party_name, current_turn);
            if deterministic_roll(&seed, yes_prob) {
                votes_for += present_seats;
                tally.votes_for = present_seats;
            } else {
                abstentions += present_seats / 3;
                votes_against += present_seats - present_seats / 3;
                tally.abstentions = present_seats / 3;
                tally.votes_against = present_seats - present_seats / 3;
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
                votes_for += present_seats;
                tally.votes_for = present_seats;
            } else {
                votes_against += present_seats;
                tally.votes_against = present_seats;
            }
        }
        club_breakdown.push(tally);
    }

    // Determinism: seat-map iteration order is hash-dependent; sort the
    // tallies by club id so serialized save files are byte-stable.
    club_breakdown.sort_by(|a, b| a.club_id.cmp(&b.club_id));

    (votes_for, votes_against, abstentions, true, club_breakdown)
}

/// Calculate upper house vote results (deterministic).
/// Phase 86: Integrates attendance and quorum (same model as floor vote).
fn calculate_upper_house_vote(
    bill: &Bill,
    upper_seats: &HashMap<String, u32>,
    coalition: &[String],
    ruling_party: &str,
    parties: &std::collections::HashMap<String, super::system::Party>,
    bill_title: &str,
    current_turn: u32,
    health_capacity_ratio: f64,
    social_unrest: f64,
) -> (u32, u32, u32, bool, Vec<ClubVoteTally>) {
    // Same logic as floor vote but for upper house.
    calculate_floor_vote(
        bill,
        upper_seats,
        coalition,
        ruling_party,
        parties,
        bill_title,
        current_turn,
        health_capacity_ratio,
        social_unrest,
    )
}

/// Calculate the probability that the executive signs a bill.
fn calculate_executive_sign_probability(
    bill: &Bill,
    politics: &super::system::Politics,
    _parties: &std::collections::HashMap<String, super::system::Party>,
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
// PHASE 96: BILL INTAKE — introduction into the parliamentary queue
// ============================================================================

/// Introduce a bill into the country's legislative session and queue it in the
/// lower chamber of `parliament_struct`.
///
/// This is the missing intake half of the Phase 32 pipeline: previously
/// `LegislativeSession::introduce_bill` had no call sites and
/// `politics.legislative_session` was never initialized, so
/// `process_legislation_turn` early-returned on every turn and no chamber ever
/// recorded a `VoteRecord`.
///
/// # Rules
/// * Countries with `GovernmentForm::chambers() == 0` (absolutist/dictatorial
///   regimes) cannot introduce bills — returns `false`.
/// * `legislative_session` is lazily initialized on first introduction.
/// * Only bills in `LegislativeStage::Introduced` are accepted (mirrors
///   `LegislativeSession::introduce_bill`).
/// * Duplicate bill IDs already in flight are skipped.
///
/// # Returns
/// `true` when the bill was added to `active_bills` (and queued on the lower
/// chamber when a parliament struct exists).
pub fn introduce_bill_to_session(country: &mut crate::state::Country, bill: Bill) -> bool {
    if country.politics.government_form.chambers() == 0 {
        return false; // No parliament exists to vote on legislation.
    }
    // No seated MPs → a bill would only produce a degenerate 0-0 rejection.
    // Check both the live chamber seat map and the legacy flat map.
    let seated = country
        .politics
        .parliament_struct
        .as_ref()
        .map(|p| p.lower_seats())
        .unwrap_or(0)
        .max(country.politics.parliament.values().sum::<u32>());
    if seated == 0 {
        return false;
    }
    if !matches!(bill.stage, LegislativeStage::Introduced) {
        return false;
    }
    let bill_id = bill.id.clone();
    let bill_type_key = bill.bill_type.as_str().to_string();
    let introduction_turn = bill.introduction_turn;
    let session = country
        .politics
        .legislative_session
        .get_or_insert_with(|| {
            super::legislation::LegislativeSession::new(introduction_turn / 24)
        });
    if session.active_bills.contains_key(&bill_id) {
        return false; // Already introduced — do not double-queue.
    }
    session.introduce_bill(bill);
    // Per-bill-type cooldown ledger: every successful introduction records
    // its type so recurring triggers (crisis fast-track, routine agenda
    // bills) can be throttled by `check_bill_type_cooldown`.
    session
        .last_introduced_by_type
        .insert(bill_type_key.clone(), introduction_turn);
    if let Some(ref mut parl) = country.politics.parliament_struct {
        parl.queue_bill(bill_id.clone());
    }
    #[cfg(feature = "diagnostic")]
    eprintln!(
        "PARLDBG introduced+queued bill_id={} type={} turn={}",
        bill_id, bill_type_key, introduction_turn
    );
    true
}

// ============================================================================
// PER-TYPE INTRODUCTION COOLDOWNS
// ============================================================================

/// The legislative cycle cadence in turns — the periodic budget bill runs on
/// `turn % LEGISLATIVE_CYCLE_TURNS == 0`, so one cycle is 4 turns.
pub const LEGISLATIVE_CYCLE_TURNS: u32 = 4;

/// Minimum turns between two introductions of the same emergency bill type
/// (crisis fast-track authorisations such as the Emergency Fiscal Response
/// Act or Emergency Bond Authorization Act).
///
/// Set to three legislative cycles (12 turns): a fast-track bill clears the
/// pipeline in ~3–5 turns (1-turn committee → floor vote → optional upper
/// house → executive), so the cooldown leaves roughly two full pipeline
/// lengths of slack before re-introduction while still permitting repeated
/// authorisation in a sustained crisis. The trigger condition (debt/deficit)
/// stays hot — it just stops spawning a bill every cycle.
pub const EMERGENCY_BILL_COOLDOWN_TURNS: u32 = LEGISLATIVE_CYCLE_TURNS * 3;

/// Minimum turns between two introductions of the same routine policy
/// domain. Same three-cycle spacing as the emergency cooldown — forces the
/// chamber agenda to rotate across policy domains instead of fixating on
/// whichever domain currently carries the highest platform weight.
pub const POLICY_DOMAIN_COOLDOWN_TURNS: u32 = LEGISLATIVE_CYCLE_TURNS * 3;

/// Phase offset inside the legislative cycle on which routine policy bills
/// are introduced. Budget bills occupy offset 0 (`turn % 4 == 0`); policy
/// bills use offset 2 so the two intake streams interleave and at most one
/// routine intake event happens per turn.
pub const POLICY_BILL_PHASE_OFFSET: u32 = LEGISLATIVE_CYCLE_TURNS / 2;

/// Cap on concurrently active session bills — about two full committee
/// batches. Prevents queue flooding while floor votes stall on repeated
/// quorum failures (bills held at FloorVote otherwise accumulate forever).
const MAX_ACTIVE_BILLS: usize = 8;

/// Check the per-bill-type cooldown ledger on the legislative session.
///
/// # Returns
/// `Err(last_turn)` when a bill of `bill_type` was introduced fewer than
/// `cooldown_turns` ago, `Ok(())` otherwise — including when no session
/// exists yet (nothing has ever been introduced, so nothing is on cooldown).
pub fn check_bill_type_cooldown(
    country: &crate::state::Country,
    bill_type: super::legislation::BillType,
    cooldown_turns: u32,
    current_turn: u32,
) -> Result<(), u32> {
    if cooldown_turns == 0 {
        return Ok(());
    }
    let last = country
        .politics
        .legislative_session
        .as_ref()
        .and_then(|s| s.last_introduced_by_type.get(bill_type.as_str()))
        .copied();
    match last {
        Some(t) if current_turn.saturating_sub(t) < cooldown_turns => Err(t),
        _ => Ok(()),
    }
}

/// Whether the parliament currently accepts new bills: at least one chamber,
/// a seated body, and not suspended / bypassed by a State of Emergency.
/// Shared gate for the budget, crisis, and routine-policy intake paths.
fn parliament_can_legislate(country: &crate::state::Country) -> bool {
    if country.politics.government_form.chambers() == 0 {
        return false;
    }
    let parliament_suspended = country
        .politics
        .parliament_struct
        .as_ref()
        .map(|p| p.suspended)
        .unwrap_or(true) // No parliament struct → no queue to feed.
        || country
            .politics
            .state_of_emergency
            .as_ref()
            .map(|soe| soe.can_bypass_parliament())
            .unwrap_or(false);
    if parliament_suspended {
        return false;
    }
    let total_seats: u32 = country
        .politics
        .parliament_struct
        .as_ref()
        .map(|p| p.lower_seats())
        .unwrap_or(0)
        .max(country.politics.parliament.values().sum::<u32>());
    total_seats > 0
}

/// Introduce the periodic budget bill into the real parliamentary queue.
///
/// Runs on the same cadence as the engine's budget lifecycle block
/// (`current_turn % 4 == 0`). The drafted `BudgetBill` is converted into a
/// legislative `Bill` (via `budget_bill_to_legislative_bill`) and introduced
/// into the session, so the budget progresses through committee → floor vote
/// → bicameral → executive inside `process_legislation_turn` and produces a
/// real `VoteRecord` in the chamber's `recent_votes`.
///
/// The engine's synchronous `process_budget_lifecycle` remains the economic
/// authority (it writes ministry allocations back and applies failure
/// consequences); the session bill carries descriptive clauses only.
///
/// # Returns
/// `Some(message)` when a bill was introduced this turn, `None` otherwise.
/// Fiscal-year anchor turn for the budget cycle (24 turns = 1 year).
/// The annual budget bill carries id `BUDGET-{year}` — a single unified
/// record per fiscal year instead of a fresh `BUDGET-{turn}` duplicate
/// every 4 turns.
pub const BUDGET_BILL_PERIOD_TURNS: u32 = 24;

pub fn introduce_budget_bill_if_due(
    country: &mut crate::state::Country,
    current_turn: u32,
) -> Option<String> {
    // v4.19 WS4: budgets are ANNUAL (every 24 turns). The intermediate
    // 4-turn checkpoint only re-submits when the session's own record of
    // this year's budget bill is terminal-rejected (upper-chamber vote or
    // executive veto) — and it re-queues the SAME `BUDGET-{year}` id, so
    // the chamber list keeps one unified bill record rather than
    // unexplained duplicate rows.
    let annual_due = current_turn.is_multiple_of(BUDGET_BILL_PERIOD_TURNS);
    let checkpoint = current_turn.is_multiple_of(4);
    if !annual_due && !checkpoint {
        return None;
    }
    if !parliament_can_legislate(country) {
        return None;
    }

    let year = current_turn / BUDGET_BILL_PERIOD_TURNS;
    let unified_id = format!("BUDGET-{year}");
    let resubmission = if !annual_due {
        // Off-cadence turn: only re-submit when this year's budget record
        // is rejected AND no live copy is still under consideration.
        let session = country.politics.legislative_session.as_ref()?;
        let rejected = session.rejected_bills.contains(&unified_id);
        let still_live = session.active_bills.contains_key(&unified_id);
        if !rejected || still_live {
            return None;
        }
        true
    } else {
        false
    };

    let mut budget_bill = super::budget_lifecycle::draft_budget_bill(country, current_turn);
    // Unify the record: the year's budget always carries `BUDGET-{year}`
    // so re-submissions reuse the same row rather than `BUDGET-{turn}` dups.
    budget_bill.id = unified_id.clone();
    if resubmission {
        budget_bill.title = format!("Budget Bill FY{} (Re-submitted)", 1900 + year);
    } else {
        budget_bill.title = format!("Budget Bill FY{}", 1900 + year);
    }
    let bill = super::budget_lifecycle::budget_bill_to_legislative_bill(country, &budget_bill);
    let bill_id = bill.id.clone();
    if introduce_bill_to_session(country, bill) {
        Some(format!(
            "[LEGISLATION] Bill '{}' introduced to Parliament.",
            bill_id
        ))
    } else {
        #[cfg(feature = "diagnostic")]
        eprintln!(
            "PARLDBG budget bill {} blocked at intake (turn {})",
            bill_id, current_turn
        );
        None
    }
}

// ============================================================================
// ROUTINE POLICY LEGISLATION — chamber agenda generator
// ============================================================================
//
// Between budget sessions and emergency fast-tracks, a functioning
// legislature also debates ordinary policy. Each cycle the lower chamber
// takes up one policy domain, picked by a deterministic weighted draw over
// seat-share × platform affinity, scaled by a per-domain macro-urgency
// multiplier (unemployment boosts labor bills, low treasury coverage boosts
// tax reform, unrest boosts justice/sentencing, missing or minimal service
// laws boost healthcare/education, ...). A per-domain cooldown forces the
// agenda to rotate instead of fixating.
//
// These bills travel the SAME pipeline as budget and crisis bills —
// committee → floor vote (real `VoteRecord`) → bicameral → executive — and
// carry typed provisions that map onto real `LawType` actuators, so an
// enacted labor act changes `politics.child_labor_law`, a transport act
// rewrites `commuting_config`, etc.

/// A policy domain the chamber can take up, together with its ideological
/// signature vector. The signature doubles as the clause's
/// `ideological_vector`, so the same affinity that places a domain on the
/// agenda also drives councilor/committee alignment when it is voted.
#[derive(Debug, Clone, Copy)]
struct PolicyDomainSpec {
    bill_type: super::legislation::BillType,
    /// Base title of the bill ("{} — Turn {turn}" suffix is appended).
    title: &'static str,
    signature: IdeologyCoordinates,
    /// Interest groups whose platform backing boosts this domain — matched
    /// against `Party::base` (the party's supporting-group platform).
    base_groups: &'static [&'static str],
}

/// The routine agenda: one entry per policy domain. Order is fixed for
/// determinism (weighted draw iterates this array).
const POLICY_DOMAINS: &[PolicyDomainSpec] = &[
    PolicyDomainSpec {
        bill_type: super::legislation::BillType::Labor,
        title: "Labor Standards Act",
        signature: IdeologyCoordinates {
            economy: -0.5,
            liberty: 0.1,
            tradition: -0.1,
        },
        base_groups: &["Trade Unions", "Worker"],
    },
    PolicyDomainSpec {
        bill_type: super::legislation::BillType::Infrastructure,
        title: "Infrastructure Investment Act",
        signature: IdeologyCoordinates {
            economy: -0.2,
            liberty: 0.0,
            tradition: 0.1,
        },
        base_groups: &["Bureaucrats", "Capitalists"],
    },
    PolicyDomainSpec {
        bill_type: super::legislation::BillType::TaxReform,
        title: "Fiscal Reform Act",
        signature: IdeologyCoordinates {
            economy: 0.3,
            liberty: 0.1,
            tradition: 0.0,
        },
        base_groups: &["Capitalists", "Petty Bourgeoisie"],
    },
    PolicyDomainSpec {
        bill_type: super::legislation::BillType::Healthcare,
        title: "Healthcare Coverage Act",
        signature: IdeologyCoordinates {
            economy: -0.4,
            liberty: 0.1,
            tradition: 0.0,
        },
        base_groups: &["Trade Unions", "Intelligentsia"],
    },
    PolicyDomainSpec {
        bill_type: super::legislation::BillType::Education,
        title: "Education Access Act",
        signature: IdeologyCoordinates {
            economy: -0.2,
            liberty: 0.3,
            tradition: -0.1,
        },
        base_groups: &["Intelligentsia", "Specialists"],
    },
    PolicyDomainSpec {
        bill_type: super::legislation::BillType::Justice,
        title: "Judicial Reform Act",
        signature: IdeologyCoordinates {
            economy: 0.0,
            liberty: 0.4,
            tradition: -0.2,
        },
        base_groups: &["Intelligentsia"],
    },
    PolicyDomainSpec {
        bill_type: super::legislation::BillType::Sentencing,
        title: "Penal Code Reform Act",
        signature: IdeologyCoordinates {
            economy: 0.0,
            liberty: -0.3,
            tradition: 0.5,
        },
        base_groups: &["Aristocracy", "Bureaucrats"],
    },
    PolicyDomainSpec {
        bill_type: super::legislation::BillType::FreeSpeech,
        title: "Speech & Assembly Act",
        signature: IdeologyCoordinates {
            economy: 0.0,
            liberty: 0.6,
            tradition: -0.2,
        },
        base_groups: &["Intelligentsia", "Specialists"],
    },
    PolicyDomainSpec {
        bill_type: super::legislation::BillType::Transport,
        title: "Public Transport Act",
        signature: IdeologyCoordinates {
            economy: -0.3,
            liberty: 0.0,
            tradition: 0.0,
        },
        base_groups: &["Trade Unions", "Petty Bourgeoisie"],
    },
];

/// Macro urgency multiplier for a policy domain — continuous functions of
/// real indicators, so an active crisis channel pushes the matching domain
/// up the agenda without any threshold table.
fn policy_domain_urgency(
    bill_type: super::legislation::BillType,
    country: &crate::state::Country,
) -> f64 {
    use super::legislation::BillType as BT;
    let unemployment = country.macro_indicators.labor_market.unemployment_rate;
    let unrest = country.macro_indicators.social_unrest;
    let growth = country.macro_indicators.gdp_breakdown.growth_rate();
    let monthly_spending = country.budget.nominal_budget / 12.0;
    // Same treasury-coverage metric `detect_crisis` uses.
    let treasury_coverage = if monthly_spending > 0.0 {
        country.budget.liquid_reserves / monthly_spending
    } else {
        f64::MAX
    };
    match bill_type {
        // The engine's established unemployment-action level is 8%
        // (counter_cyclical_response); each 8 points of unemployment
        // doubles labor urgency.
        BT::Labor => 1.0 + unemployment / 8.0,
        // Recession → counter-cyclical public works pressure.
        BT::Infrastructure => 1.0 + (-growth).max(0.0) * 10.0,
        // Treasury coverage below the 2-month crisis line → fiscal repair.
        BT::TaxReform => 1.0 + (2.0 - treasury_coverage).clamp(0.0, 2.0),
        // Coverage deficit: no law or a Limited system pressures expansion.
        BT::Healthcare => {
            let deficit = match country.politics.healthcare_law.as_ref().map(|l| l.universality) {
                None => 1.0,
                Some(crate::politics::laws::UniversalityLevel::Limited) => 0.75,
                Some(crate::politics::laws::UniversalityLevel::Categorical) => 0.5,
                Some(crate::politics::laws::UniversalityLevel::MeansTested) => 0.25,
                Some(crate::politics::laws::UniversalityLevel::Universal) => 0.0,
            };
            1.0 + deficit
        }
        // Missing education law → double urgency; short compulsory
        // schooling adds a small push.
        BT::Education => match country.politics.education_law.as_ref() {
            None => 2.0,
            Some(law) => 1.0 + (12.0 - law.compulsory_education.compulsory_years as f64).max(0.0) / 24.0,
        },
        // Unrest pressures institutional reform (justice) and order
        // legislation (sentencing) — 40 points of unrest doubles urgency.
        BT::Justice => 1.0 + unrest / 40.0,
        BT::Sentencing => 1.0 + unrest / 60.0,
        // Unrest also pushes speech/assembly questions onto the agenda.
        BT::FreeSpeech => 1.0 + unrest / 80.0,
        // Missing transport law → mild pressure to codify ownership.
        BT::Transport if country.politics.transport_law.is_none() => 1.5,
        _ => 1.0,
    }
}

/// Affinity of a parliamentary club/party to a policy domain signature —
/// same coordinate axes the floor-vote alignment uses, so agenda placement
/// foreshadows (but does not guarantee) the vote.
fn policy_domain_affinity(coords: &IdeologyCoordinates, signature: &IdeologyCoordinates) -> f64 {
    let dot = coords.economy * signature.economy
        + coords.liberty * signature.liberty
        + coords.tradition * signature.tradition;
    (0.5 + dot * 0.5).clamp(0.05, 1.0)
}

/// Seat-share-weighted platform support for every domain: each lower-house
/// club contributes `seats × affinity`, with a platform bonus when the
/// club's parent party lists a domain-relevant interest group in `base`.
/// Both coalition and opposition clubs contribute — the agenda reflects the
/// whole chamber, while the initiator is chosen separately below.
fn policy_domain_weights(country: &crate::state::Country) -> Vec<(usize, f64)> {
    // (club coordinates, seats, parent/party name for base lookup)
    let mut seat_blocks: Vec<(IdeologyCoordinates, u32, String)> = Vec::new();
    if let Some(ref parl) = country.politics.parliament_struct {
        for club in &parl.clubs {
            if club.seats > 0 {
                seat_blocks.push((
                    club.coordinates,
                    club.seats,
                    club.parent_party.clone().unwrap_or_else(|| club.id.clone()),
                ));
            }
        }
    }
    // Fallback: appointed/legacy chambers without clubs — weight active
    // parties by their seat-map or support share.
    if seat_blocks.is_empty() {
        let seat_map = &country.politics.parliament;
        for (name, party) in &country.politics.active_parties {
            let seats = seat_map.get(name).copied().unwrap_or(0);
            let weight = if seats > 0 {
                seats as f64
            } else {
                party.support
            };
            if weight > 0.0 {
                seat_blocks.push((
                    party.coordinates,
                    weight.max(1.0) as u32,
                    name.clone(),
                ));
            }
        }
    }

    POLICY_DOMAINS
        .iter()
        .enumerate()
        .map(|(idx, spec)| {
            let mut weight = 0.0;
            for (coords, seats, party_name) in &seat_blocks {
                let mut affinity = policy_domain_affinity(coords, &spec.signature);
                // Platform bonus: the party's declared interest-group base
                // backs this domain → +25% affinity (bounded platform boost).
                if let Some(party) = country.politics.active_parties.get(party_name) {
                    if party
                        .base
                        .iter()
                        .any(|g| spec.base_groups.contains(&g.as_str()))
                    {
                        affinity = (affinity * 1.25).min(1.0);
                    }
                }
                weight += *seats as f64 * affinity;
            }
            (idx, weight * policy_domain_urgency(spec.bill_type, country))
        })
        .collect()
}

/// Deterministic weighted draw over `(index, weight)` pairs. Seed-mixing by
/// country + turn keeps the pick reproducible.
fn weighted_domain_pick(weights: &[(usize, f64)], seed: &str) -> Option<usize> {
    let total: f64 = weights.iter().map(|(_, w)| w).sum();
    if total <= 0.0 {
        return None;
    }
    let u = deterministic_uniform(seed) * total;
    let mut acc = 0.0;
    for (idx, w) in weights {
        acc += *w;
        if u < acc {
            return Some(*idx);
        }
    }
    weights.last().map(|(idx, _)| *idx)
}

/// Pick the bill's sponsor: the lower-house club with the highest
/// `seats × affinity` for the chosen domain (ties break lexicographically
/// for determinism). Falls back to the ruling party when no clubs exist.
fn pick_policy_bill_sponsor(
    country: &crate::state::Country,
    signature: &IdeologyCoordinates,
) -> String {
    let mut sponsor: Option<(f64, String)> = None;
    if let Some(ref parl) = country.politics.parliament_struct {
        // Sort by id first so argmax is deterministic regardless of Vec order.
        let mut clubs: Vec<&super::parliament::ParliamentaryClub> = parl.clubs.iter().collect();
        clubs.sort_by(|a, b| a.id.cmp(&b.id));
        for club in clubs {
            if club.seats == 0 {
                continue;
            }
            let score = club.seats as f64 * policy_domain_affinity(&club.coordinates, signature);
            let better = match &sponsor {
                None => true,
                Some((best, _)) => score > *best,
            };
            if better {
                sponsor = Some((
                    score,
                    club.parent_party.clone().unwrap_or_else(|| club.id.clone()),
                ));
            }
        }
    }
    sponsor
        .map(|(_, name)| name)
        .unwrap_or_else(|| country.politics.ruling_party.clone())
}

/// Universality ladder step: expand coverage one notch (left/statist
/// initiators) or retrench one notch (market initiators).
fn step_universality(
    current: crate::politics::laws::UniversalityLevel,
    expand: bool,
) -> crate::politics::laws::UniversalityLevel {
    use crate::politics::laws::UniversalityLevel::*;
    match (current, expand) {
        (Limited, true) => Categorical,
        (Categorical, true) => MeansTested,
        (MeansTested, true) => Universal,
        (Universal, true) => Universal,
        (Universal, false) => MeansTested,
        (MeansTested, false) => Categorical,
        (Categorical, false) => Limited,
        (Limited, false) => Limited,
    }
}

/// Build the provision, clause description, and title for a policy bill.
/// Every domain maps onto a real `LawType` actuator — amendment parameters
/// are derived from the country's current law state and macro indicators,
/// never hardcoded snapshots.
fn build_policy_provision(
    domain: &PolicyDomainSpec,
    country: &crate::state::Country,
    ruling_coords: &IdeologyCoordinates,
) -> (super::legislation::BillProvision, String) {
    use super::legislation::BillProvision;
    match domain.bill_type {
        super::legislation::BillType::Labor => {
            // Labor-supply intervention: high unemployment → pull children
            // out of the workforce (Prohibited); otherwise codify Restricted.
            let unemployment = country.macro_indicators.labor_market.unemployment_rate;
            let law = if unemployment >= 8.0 {
                crate::politics::laws::ChildLaborLaw::Prohibited
            } else {
                crate::politics::laws::ChildLaborLaw::Restricted
            };
            (
                BillProvision::ChildLaborLaw(law.clone()),
                format!(
                    "Labor standards: child labor {:?} (unemployment {:.1}%)",
                    law, unemployment
                ),
            )
        }
        super::legislation::BillType::Infrastructure => {
            // Counter-cyclical sizing: deeper recession → larger mandate.
            let growth = country.macro_indicators.gdp_breakdown.growth_rate();
            let allocation_pct = (0.05 + (-growth).max(0.0)).clamp(0.02, 0.25);
            (
                BillProvision::InfrastructureMandate { allocation_pct },
                format!(
                    "Infrastructure mandate: +{:.0}% allocation (growth {:+.1}%)",
                    allocation_pct * 100.0,
                    growth * 100.0
                ),
            )
        }
        super::legislation::BillType::TaxReform => {
            let monthly_spending = country.budget.nominal_budget / 12.0;
            let coverage = if monthly_spending > 0.0 {
                country.budget.liquid_reserves / monthly_spending
            } else {
                f64::MAX
            };
            let pit = country.tax_rates.income_tax.rate;
            let cit = country.tax_rates.corporate_tax;
            if coverage < 2.0 {
                // Revenue repair: +2pp PIT, capped at 60%.
                let new_pit = (pit + 0.02).min(0.60);
                (
                    BillProvision::TaxRateChange {
                        income_tax: Some(new_pit),
                        vat: None,
                        corporate_tax: None,
                    },
                    format!(
                        "Revenue repair: income tax {:.1}% → {:.1}% (treasury {:.1} mo)",
                        pit * 100.0,
                        new_pit * 100.0,
                        coverage
                    ),
                )
            } else if ruling_coords.economy > 0.0 {
                // Market-leaning government: 1pp corporate tax cut.
                let new_cit = (cit - 0.01).max(0.02);
                (
                    BillProvision::TaxRateChange {
                        income_tax: None,
                        vat: None,
                        corporate_tax: Some(new_cit),
                    },
                    format!(
                        "Tax relief: corporate tax {:.1}% → {:.1}%",
                        cit * 100.0,
                        new_cit * 100.0
                    ),
                )
            } else {
                // Statist government with a healthy treasury: modest VAT bump.
                let vat_now = country
                    .tax_rates
                    .vat
                    .get("services")
                    .map(|b| b.rate)
                    .unwrap_or(0.0);
                let new_vat = (vat_now + 0.01).min(0.30);
                (
                    BillProvision::TaxRateChange {
                        income_tax: None,
                        vat: Some(new_vat),
                        corporate_tax: None,
                    },
                    format!(
                        "VAT adjustment: services {:.1}% → {:.1}%",
                        vat_now * 100.0,
                        new_vat * 100.0
                    ),
                )
            }
        }
        super::legislation::BillType::Healthcare => {
            // Amend the existing law (one universality step) or codify a
            // baseline system when none exists.
            let expand = ruling_coords.economy <= 0.0;
            let current = country.politics.healthcare_law.clone();
            let universality = step_universality(
                current
                    .as_ref()
                    .map(|l| l.universality)
                    .unwrap_or(crate::politics::laws::UniversalityLevel::Limited),
                expand,
            );
            let law = crate::politics::laws::HealthcareLaw {
                healthcare_system: current
                    .as_ref()
                    .map(|l| l.healthcare_system)
                    .unwrap_or(if expand {
                        crate::politics::laws::HealthcareSystem::Budgetary
                    } else {
                        crate::politics::laws::HealthcareSystem::InsuranceBased
                    }),
                funding: current
                    .as_ref()
                    .map(|l| l.funding.clone())
                    .unwrap_or(crate::politics::funding::ServiceFundingConfig {
                        funding_model: crate::politics::funding::FundingModel::Public,
                        budget_source: crate::politics::funding::BudgetSource::Central,
                        per_capita_funding: 0.0,
                        student_loans: None,
                    }),
                universality,
                priorities: current
                    .as_ref()
                    .map(|l| l.priorities.clone())
                    .unwrap_or(crate::politics::laws::HealthcarePriorities {
                        emergency_priority: true,
                        elderly_priority: true,
                        children_priority: true,
                        chronic_priority: true,
                    }),
            };
            (
                BillProvision::HealthcareLaw(law),
                format!("Healthcare coverage: universality {:?}", universality),
            )
        }
        super::legislation::BillType::Education => {
            // Extend compulsory schooling by one year (cap 12); choose the
            // delivery model from the sponsor's coordinates when no law exists.
            let current = country.politics.education_law.clone();
            let years = current
                .as_ref()
                .map(|l| l.compulsory_education.compulsory_years)
                .unwrap_or(9);
            let new_years = (years + 1).min(12);
            let law = crate::politics::laws::EducationLaw {
                education_model: current
                    .as_ref()
                    .map(|l| l.education_model)
                    .unwrap_or(if ruling_coords.economy < 0.0 {
                        crate::politics::laws::EducationModel::StateRun
                    } else {
                        crate::politics::laws::EducationModel::Mixed
                    }),
                school_system: current
                    .as_ref()
                    .map(|l| l.school_system)
                    .unwrap_or(crate::politics::laws::SchoolSystem::SixPlusThreePlusThree),
                funding: current
                    .as_ref()
                    .map(|l| l.funding.clone())
                    .unwrap_or(crate::politics::funding::ServiceFundingConfig {
                        funding_model: crate::politics::funding::FundingModel::Public,
                        budget_source: crate::politics::funding::BudgetSource::Central,
                        per_capita_funding: 0.0,
                        student_loans: None,
                    }),
                compulsory_education: crate::politics::laws::CompulsoryEducationConfig {
                    compulsory_years: new_years,
                    end_age: current
                        .as_ref()
                        .map(|l| l.compulsory_education.end_age)
                        .unwrap_or(15)
                        .max(6 + new_years),
                    enforcement: current
                        .as_ref()
                        .map(|l| l.compulsory_education.enforcement)
                        .unwrap_or(crate::politics::laws::EnforcementLevel::Moderate),
                },
                financing: current.as_ref().and_then(|l| l.financing.clone()),
            };
            (
                BillProvision::EducationLaw(law),
                format!("Education access: compulsory schooling {} years", new_years),
            )
        }
        super::legislation::BillType::Justice => {
            // Institutional reform: independent judiciary council under
            // liberty-leaning sponsors; anti-corruption drive either way.
            let current = country.politics.justice_law.clone().unwrap_or_default();
            let law = crate::politics::laws::JusticeLaw {
                krs_separated: if ruling_coords.liberty >= 0.0 {
                    true
                } else {
                    current.krs_separated
                },
                prosecutor_general_separated: current.prosecutor_general_separated,
                court_wait_time_target: if country.macro_indicators.social_unrest > 40.0 {
                    crate::politics::laws::CourtWaitTime::Expedited
                } else {
                    current.court_wait_time_target
                },
                pardon_authority: current.pardon_authority,
                corruption_index: (current.corruption_index.max(0.5) - 0.05).clamp(0.0, 1.0),
            };
            (
                BillProvision::JusticeLaw(law.clone()),
                format!(
                    "Judicial reform: corruption index {:.2} → {:.2}, KRS separated={}",
                    current.corruption_index, law.corruption_index, law.krs_separated
                ),
            )
        }
        super::legislation::BillType::Sentencing => {
            // Penal reform around the documented default ranges; community
            // service for liberty-leaning sponsors, tougher penalties under
            // authoritarian/traditionalist ones.
            let base = country
                .politics
                .sentencing_law
                .clone()
                .unwrap_or(crate::economy::sentencing::SentencingLaw {
                    misdemeanor_range_months: (1, 6),
                    felony_range_months: (5, 20),
                    capital_methods: Vec::new(),
                    death_penalty_enabled: false,
                    life_imprisonment_enabled: false,
                    community_service_enabled: true,
                    legal_dualism_enabled: false,
                    minority_sentence_multiplier: 2.0,
                    resident_sentence_multiplier: 1.5,
                    illegal_sentence_multiplier: 3.0,
                });
            let punitive = ruling_coords.tradition > 0.2 && ruling_coords.liberty < -0.2;
            let law = crate::economy::sentencing::SentencingLaw {
                death_penalty_enabled: punitive && ruling_coords.tradition > 0.5,
                life_imprisonment_enabled: base.life_imprisonment_enabled || punitive,
                community_service_enabled: ruling_coords.liberty >= 0.0,
                // Routine legislation never introduces legal dualism.
                legal_dualism_enabled: false,
                ..base
            };
            (
                BillProvision::SentencingLaw(law.clone()),
                format!(
                    "Penal reform: community_service={}, life_imprisonment={}, death_penalty={}",
                    law.community_service_enabled,
                    law.life_imprisonment_enabled,
                    law.death_penalty_enabled
                ),
            )
        }
        super::legislation::BillType::FreeSpeech => {
            use crate::politics::free_speech::{AssemblyRights, FreeSpeechLevel, PressFreedom};
            // Speech regime follows the sponsor's liberty axis: libertarian
            // sponsors expand to Full, authoritarians tighten to Suppressed.
            let (level, assembly, press) = if ruling_coords.liberty >= -0.1 {
                (FreeSpeechLevel::Full, AssemblyRights::Free, PressFreedom::Independent)
            } else if ruling_coords.liberty >= -0.4 {
                (
                    FreeSpeechLevel::Restricted,
                    AssemblyRights::Restricted,
                    PressFreedom::Mixed,
                )
            } else {
                (
                    FreeSpeechLevel::Suppressed,
                    AssemblyRights::Banned,
                    PressFreedom::StateControlled,
                )
            };
            let law = crate::politics::free_speech::FreeSpeechLaw {
                free_speech_level: level,
                assembly_rights: assembly,
                press_freedom: press,
            };
            (
                BillProvision::FreeSpeechLaw(law),
                format!("Speech & assembly regime: {:?}/{:?}/{:?}", level, assembly, press),
            )
        }
        super::legislation::BillType::Transport => {
            // Ownership question: market sponsors privatise; others keep the
            // public operator with at least a 50% ticket subsidy floor.
            let privatize = ruling_coords.economy > 0.3;
            let law = crate::economy::commuting::TransportLaw {
                ownership: if privatize {
                    crate::economy::commuting::TransportOwnership::Privatized
                } else {
                    crate::economy::commuting::TransportOwnership::Public
                },
                public_subsidy_fraction: if privatize {
                    0.0
                } else {
                    country.commuting_config.public_subsidy_fraction.max(0.5)
                },
            };
            (
                BillProvision::TransportLaw(law.clone()),
                format!(
                    "Transport: {:?}, subsidy {:.0}%",
                    law.ownership,
                    law.public_subsidy_fraction * 100.0
                ),
            )
        }
        _ => (
            BillProvision::Custom {
                description: domain.title.to_string(),
                effect_key: format!("policy_{}", domain.bill_type.as_str()),
                effect_value: 0.0,
            },
            domain.title.to_string(),
        ),
    }
}

/// Introduce a routine (non-emergency) policy bill on the off-budget phase
/// of the legislative cycle (`turn % 4 == 2`).
///
/// The domain is a deterministic weighted draw: seat-share-weighted
/// ideological affinity of every lower-house club (coalition AND
/// opposition — the whole chamber sets the agenda) times the domain's
/// macro-urgency multiplier. Domains inside their
/// [`POLICY_DOMAIN_COOLDOWN_TURNS`] window are excluded, forcing rotation.
///
/// The bill travels the normal pipeline — committee → floor vote
/// (`VoteRecord` in `recent_votes`) → bicameral → executive — and carries a
/// typed provision with a real `LawType` actuator.
///
/// # Returns
/// `Some(message)` when a bill was introduced this turn, `None` otherwise.
pub fn introduce_policy_bill_if_due(
    country: &mut crate::state::Country,
    current_turn: u32,
) -> Option<String> {
    if current_turn % LEGISLATIVE_CYCLE_TURNS != POLICY_BILL_PHASE_OFFSET {
        return None;
    }
    if !parliament_can_legislate(country) {
        return None;
    }
    // Backpressure: a chamber jammed on quorum failures shouldn't keep
    // stacking new bills.
    let active_count = country
        .politics
        .legislative_session
        .as_ref()
        .map(|s| s.active_bills.len())
        .unwrap_or(0);
    if active_count >= MAX_ACTIVE_BILLS {
        #[cfg(feature = "diagnostic")]
        eprintln!(
            "PARLDBG policy intake skipped: {} bills already active (turn {})",
            active_count, current_turn
        );
        return None;
    }

    // Domain weights, then drop domains still inside their cooldown window.
    let weights = policy_domain_weights(country);
    let eligible: Vec<(usize, f64)> = weights
        .into_iter()
        .filter(|(idx, _)| {
            check_bill_type_cooldown(
                country,
                POLICY_DOMAINS[*idx].bill_type,
                POLICY_DOMAIN_COOLDOWN_TURNS,
                current_turn,
            )
            .is_ok()
        })
        .collect();
    let seed = format!("policy_pick:{}:{}", country.name, current_turn);
    let domain_idx = match weighted_domain_pick(&eligible, &seed) {
        Some(idx) => idx,
        None => {
            #[cfg(feature = "diagnostic")]
            eprintln!(
                "PARLDBG policy intake skipped: all domains on cooldown (turn {})",
                current_turn
            );
            return None;
        }
    };
    let spec = &POLICY_DOMAINS[domain_idx];

    // Sponsor: the club with the strongest seats × affinity for this domain.
    let initiator = pick_policy_bill_sponsor(country, &spec.signature);
    let sponsor_coords = country
        .politics
        .active_parties
        .get(&initiator)
        .map(|p| p.coordinates)
        .unwrap_or(spec.signature);
    let (provision, description) = build_policy_provision(spec, country, &sponsor_coords);

    let bill = Bill::new(
        format!("POLICY-{}-{}", spec.bill_type.as_str(), current_turn),
        format!("{} — Turn {}", spec.title, current_turn),
        initiator.clone(),
        vec![super::legislation::Clause {
            description,
            ideological_vector: spec.signature,
            budget_impact: 0.0,
            provision: Some(provision),
            sunset_turn: None,
            mutated: false,
            mutation_notes: Vec::new(),
        }],
        current_turn,
    )
    .with_bill_type(spec.bill_type);
    let bill_id = bill.id.clone();

    if introduce_bill_to_session(country, bill) {
        Some(format!(
            "[LEGISLATION] Bill '{}' ({}) introduced to Parliament by {}.",
            bill_id,
            spec.bill_type.as_str(),
            initiator,
        ))
    } else {
        #[cfg(feature = "diagnostic")]
        eprintln!(
            "PARLDBG policy bill {} blocked at intake (turn {})",
            bill_id, current_turn
        );
        None
    }
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
            let affordable = affordable_by_treasury
                .min(affordable_by_pc)
                .min(target_seats);
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

    // ====================================================================
    // Cooldown ledger & routine policy intake
    // ====================================================================

    /// Minimal country with a seated (appointed-style) lower chamber:
    /// two clubs, 100 seats, no upper house.
    fn country_with_seated_parliament() -> crate::state::Country {
        use crate::politics::ideology::IdeologyCoordinates;
        use crate::politics::parliament::{Chamber, Parliament, ParliamentaryClub};

        let mut country = crate::state::Country::default();
        country.politics.parliament.insert("Alpha".to_string(), 60);
        country.politics.parliament.insert("Beta".to_string(), 40);

        let mut chamber = Chamber::default();
        chamber.id = "lower".to_string();
        chamber.total_seats = 100;
        chamber.seats = country.politics.parliament.clone();

        let mut parl = Parliament::default();
        parl.chambers.push(chamber);

        let coords_a = IdeologyCoordinates::new(-0.6, 0.4, -0.3);
        let coords_b = IdeologyCoordinates::new(0.7, 0.2, 0.5);
        for (name, seats, coords) in
            [("Alpha", 60u32, coords_a), ("Beta", 40u32, coords_b)]
        {
            parl.clubs.push(ParliamentaryClub {
                id: name.to_string(),
                name: name.to_string(),
                parent_party: Some(name.to_string()),
                seats,
                ideology: String::new(),
                coordinates: coords,
                discipline: 0.7,
                is_splinter: false,
                formation_turn: 0,
                chairperson_id: None,
                chairperson_name: String::new(),
            });
            country.politics.active_parties.insert(
                name.to_string(),
                crate::politics::system::Party {
                    coordinates: coords,
                    ..Default::default()
                },
            );
        }
        country.politics.parliament_struct = Some(parl);
        country.politics.ruling_party = "Alpha".to_string();
        country.politics.coalition = vec!["Alpha".to_string()];
        country
    }

    #[test]
    fn test_policy_bills_rotate_across_domains() {
        let mut country = country_with_seated_parliament();
        // Policy intake phase is turn % 4 == 2.
        for turn in [2u32, 6, 10] {
            assert!(
                introduce_policy_bill_if_due(&mut country, turn).is_some(),
                "policy bill should be introduced at turn {}",
                turn
            );
        }
        let sess = country.politics.legislative_session.as_ref().unwrap();
        let types: std::collections::HashSet<crate::politics::legislation::BillType> = sess
            .active_bills
            .values()
            .map(|b| b.bill_type)
            .collect();
        // The per-domain cooldown forbids re-picking a domain within 12
        // turns, so three introductions must yield three distinct types.
        assert_eq!(types.len(), 3);
        assert!(types.iter().all(|t| t.is_policy()));
    }

    #[test]
    fn test_bill_type_cooldown_blocks_repeat_introduction() {
        let mut country = country_with_seated_parliament();
        assert!(introduce_policy_bill_if_due(&mut country, 2).is_some());
        let picked = country
            .politics
            .legislative_session
            .as_ref()
            .unwrap()
            .active_bills
            .values()
            .next()
            .unwrap()
            .bill_type;
        assert_eq!(
            check_bill_type_cooldown(&country, picked, POLICY_DOMAIN_COOLDOWN_TURNS, 6),
            Err(2)
        );
        // Cooldown is elapsed once the full window has passed.
        assert!(
            check_bill_type_cooldown(&country, picked, POLICY_DOMAIN_COOLDOWN_TURNS, 14).is_ok()
        );
    }

    #[test]
    fn test_emergency_bill_cooldown_ledger() {
        // Mirrors what introduce_crisis_fast_track_bill does: a typed bill
        // writes the shared per-type ledger on introduction.
        let mut country = country_with_seated_parliament();
        let bill = Bill::new(
            "CRISIS-FISCAL-4".to_string(),
            "Emergency Fiscal Response Act — Turn 4".to_string(),
            "Alpha".to_string(),
            Vec::new(),
            4,
        )
        .with_bill_type(crate::politics::legislation::BillType::EmergencyFiscal);
        assert!(introduce_bill_to_session(&mut country, bill));
        // Same emergency type inside the window → still on cooldown.
        assert_eq!(
            check_bill_type_cooldown(
                &country,
                crate::politics::legislation::BillType::EmergencyFiscal,
                EMERGENCY_BILL_COOLDOWN_TURNS,
                8,
            ),
            Err(4)
        );
        // A different emergency type has its own ledger entry.
        assert!(
            check_bill_type_cooldown(
                &country,
                crate::politics::legislation::BillType::EmergencyBonds,
                EMERGENCY_BILL_COOLDOWN_TURNS,
                8,
            )
            .is_ok()
        );
    }

    #[test]
    fn test_policy_bill_reaches_floor_vote_with_type() {
        let mut country = country_with_seated_parliament();
        let parties = country.politics.active_parties.clone();
        assert!(introduce_policy_bill_if_due(&mut country, 2).is_some());
        // Advance the pipeline; the bill must travel committee → floor vote
        // and produce a typed VoteRecord (pass OR reject — both record).
        for turn in 2..20 {
            process_legislation_turn(&mut country, &[], &parties, turn);
        }
        let parl = country.politics.parliament_struct.as_ref().unwrap();
        let votes = &parl.chambers[0].recent_votes;
        assert!(
            !votes.is_empty(),
            "policy bill should reach a floor vote within the window"
        );
        assert!(votes[0].bill_type.is_policy());
    }
}

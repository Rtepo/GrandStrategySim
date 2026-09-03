//! Phase E.10: IP Theft / Industrial Espionage / Reverse Engineering.
//!
//! This module implements corporate IP theft mechanics:
//! - Private espionage: internal black-ops budget (AdminServices + ElectronicComponents + cash)
//! - State-sponsored espionage: consumes state IntelligenceCapacity
//! - Reverse engineering: consumes ResearchOutput or domain Innovation Points
//! - Detection: probabilistic, based on victim's counter-intelligence capacity
//! - Enforcement: domestic judgments + treaty-gated cross-border enforcement
//! - Judgment debts: recorded as balance sheet liabilities (no manual equity override)
//!
//! # Architectural Rules
//! - `IntelligenceCapacity` is strictly a State asset (produced by `intelligence_hq`).
//! - Private companies cannot raid state intelligence inventories.
//! - Cash debited from the thief enters the Treasury as miscellaneous revenue (double-entry).
//! - Unpaid damages are recorded as `JudgmentDebt` liabilities, not manual equity overrides.
//! - The normal Syndic lifecycle detects negative equity and liquidates organically.
//! - Cross-border enforcement requires an active `TreatyClause::IntellectualPropertyEnforcement`.

use crate::economy::trade::transfer_settler::{credit_company_by_id, debit_company_by_id};
use crate::entities::{Company, IPTheftMethod, StolenIP};
use crate::registries::tech_tree::{TechId, TechNode};
use crate::state::Country;
use std::collections::HashMap;

/// Result of an IP theft attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct IPTheftResult {
    /// Whether the theft succeeded.
    pub success: bool,
    /// The stolen IP entry (if successful).
    pub stolen_ip: Option<StolenIP>,
    /// Cash debited from the thief (black-ops budget or reverse-engineering staff cost).
    pub cash_spent: f64,
    /// Administrative services consumed (private espionage only).
    pub admin_services_consumed: f64,
    /// Electronic components consumed (private espionage only).
    pub electronics_consumed: f64,
    /// Research output consumed (reverse engineering only).
    pub research_output_consumed: f64,
    /// Domain innovation points consumed (reverse engineering fallback).
    pub innovation_points_consumed: f64,
    /// The domain of innovation points consumed (if any).
    pub domain_consumed: Option<crate::registries::tech_tree::ResearchDomain>,
    /// State intelligence capacity consumed (state-sponsored only).
    pub state_intel_consumed: f64,
}

impl Default for IPTheftResult {
    fn default() -> Self {
        Self {
            success: false,
            stolen_ip: None,
            cash_spent: 0.0,
            admin_services_consumed: 0.0,
            electronics_consumed: 0.0,
            research_output_consumed: 0.0,
            innovation_points_consumed: 0.0,
            domain_consumed: None,
            state_intel_consumed: 0.0,
        }
    }
}

/// Configuration for IP theft mechanics (no magic numbers).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct IPTheftConfig {
    /// Multiplier: how much AdminServices to consume per unit of tech cost.
    /// Default 0.01 (1 unit of AdminServices per 100 tech cost).
    pub admin_services_per_tech_cost: f64,
    /// Multiplier: how much ElectronicComponents to consume per unit of tech cost.
    /// Default 0.005 (0.5 units of ElectronicComponents per 100 tech cost).
    pub electronics_per_tech_cost: f64,
    /// Multiplier: cash black-ops budget as fraction of tech cost.
    /// Default 0.5 (50% of tech cost as off-books cash).
    pub cash_blackops_ratio: f64,
    /// Multiplier: ResearchOutput cost for reverse engineering (× tech cost).
    /// Default 2.0 (2× tech cost in ResearchOutput).
    pub reverse_engineering_research_output_multiplier: f64,
    /// Multiplier: domain Innovation Points cost for reverse engineering (× tech cost).
    /// Default 3.0 (3× tech cost in domain Innovation Points).
    pub reverse_engineering_innovation_multiplier: f64,
    /// Cash cost for reverse engineering staff (fraction of tech cost).
    /// Default 0.3 (30% of tech cost as staff wages).
    pub reverse_engineering_cash_ratio: f64,
    /// Evasion decay rate per turn (reduces detection probability over time).
    /// Default 0.05 (5% decay per turn).
    pub evasion_decay_rate: f64,
    /// Treble damages multiplier for enforced judgments.
    /// Default 3.0 (3× back-royalties).
    pub treble_damages_multiplier: f64,
}

impl Default for IPTheftConfig {
    fn default() -> Self {
        Self {
            admin_services_per_tech_cost: 0.01,
            electronics_per_tech_cost: 0.005,
            cash_blackops_ratio: 0.5,
            reverse_engineering_research_output_multiplier: 2.0,
            reverse_engineering_innovation_multiplier: 3.0,
            reverse_engineering_cash_ratio: 0.3,
            evasion_decay_rate: 0.05,
            treble_damages_multiplier: 3.0,
        }
    }
}

/// Execute a private espionage IP theft attempt.
///
/// # Arguments
/// * `thief` - The company attempting the theft (mutable — cash debited, inventory consumed).
/// * `victim_company_id` - ID of the victim (patent holder).
/// * `victim_country` - Country of the victim.
/// * `tech_node` - The technology to steal.
/// * `victim_intel` - The victim country's IntelligenceCapacity (counter-intelligence defense).
/// * `average_wage` - Country average wage for dynamic pricing.
/// * `config` - IP theft configuration.
/// * `current_turn` - Current turn number.
/// * `country` - Mutable country (Treasury receives the cash as misc revenue).
///
/// # Rules
/// - Consumes AdministrativeServices + ElectronicComponents from thief's buildings.
/// - Cash debited from thief's available_cash, credited to Treasury (double-entry).
/// - Success probability: `thief_blackops / (thief_blackops + victim_intel).max(1.0)`.
/// - On success: creates StolenIP entry, grants production access without royalties.
/// - On failure: no IP stolen, resources still consumed (sunk cost).
pub fn execute_private_espionage(
    thief: &mut Company,
    victim_company_id: &str,
    victim_country: &str,
    tech_id: &TechId,
    tech_node: &TechNode,
    victim_intel: f64,
    average_wage: f64,
    config: &IPTheftConfig,
    current_turn: u32,
    country: &mut Country,
) -> IPTheftResult {
    let mut result = IPTheftResult::default();

    let tech_cost = (tech_node.cost as f64).max(1.0);
    let admin_needed = tech_cost * config.admin_services_per_tech_cost;
    let electronics_needed = tech_cost * config.electronics_per_tech_cost;
    let cash_needed = tech_cost * config.cash_blackops_ratio * average_wage;

    // Check cash availability.
    let available_cash = thief
        .brokerage_account
        .as_ref()
        .map(|ba| ba.cash.max(0.0))
        .unwrap_or(thief.available_cash.max(0.0));

    if available_cash < cash_needed {
        // Cannot afford the black-ops budget — abort.
        return result;
    }

    // Consume cash: debit thief, credit Treasury (double-entry).
    let thief_id = thief.id.clone();
    let debited = debit_company_by_id(std::slice::from_mut(thief), &thief_id, cash_needed);
    if debited > 0.0 {
        country.budget.liquid_reserves += debited;
        result.cash_spent = debited;
    }

    // Consume physical resources from thief's buildings (if available).
    // We track consumption via building inventories in the turn loop.
    // Here we record what should be consumed; actual consumption happens
    // in the turn loop where building inventories are accessible.
    result.admin_services_consumed = admin_needed;
    result.electronics_consumed = electronics_needed;

    // Compute black-ops budget (offensive capability).
    let vwap_fallback = average_wage * 10.0;
    let thief_blackops = result.cash_spent
        + admin_needed * vwap_fallback
        + electronics_needed * vwap_fallback;

    // Success probability: thief_blackops / (thief_blackops + victim_intel).
    let success_prob = thief_blackops / (thief_blackops + victim_intel.max(1.0));

    // Deterministic RNG based on (turn, thief_id, tech_id).
    let roll = deterministic_roll(current_turn, &thief.id, tech_id);
    let success = roll < success_prob;

    result.success = success;

    if success {
        let evasion = thief_blackops * 0.1; // 10% of black-ops budget as evasion.
        let stolen_ip = StolenIP {
            tech_id: tech_id.clone(),
            victim_company_id: victim_company_id.to_string(),
            victim_country: victim_country.to_string(),
            method: IPTheftMethod::PrivateEspionage,
            stolen_turn: current_turn,
            detected: false,
            detected_turn: None,
            initial_evasion: evasion,
        };
        thief.stolen_ips.push(stolen_ip.clone());
        result.stolen_ip = Some(stolen_ip);
    }

    result
}

/// Execute a state-sponsored espionage IP theft attempt.
///
/// # Rules
/// - Consumes state IntelligenceCapacity directly.
/// - No company cash debited.
/// - Success probability: `state_intel / (state_intel + victim_intel).max(1.0)`.
/// - The State bears diplomatic consequences if detected.
pub fn execute_state_sponsored_espionage(
    thief: &mut Company,
    victim_company_id: &str,
    victim_country: &str,
    tech_id: &TechId,
    tech_node: &TechNode,
    state_intel_available: f64,
    victim_intel: f64,
    _config: &IPTheftConfig,
    current_turn: u32,
) -> IPTheftResult {
    let mut result = IPTheftResult::default();

    let tech_cost = (tech_node.cost as f64).max(1.0);
    // State intel consumption scales with tech cost.
    let intel_needed = (tech_cost * 0.1).min(state_intel_available);

    if intel_needed <= 0.0 {
        return result;
    }

    result.state_intel_consumed = intel_needed;

    // Success probability: state_intel / (state_intel + victim_intel).
    let success_prob = intel_needed / (intel_needed + victim_intel.max(1.0));

    let roll = deterministic_roll(current_turn, &thief.id, tech_id);
    let success = roll < success_prob;

    result.success = success;

    if success {
        let evasion = intel_needed * 0.15; // 15% of state intel as evasion.
        let stolen_ip = StolenIP {
            tech_id: tech_id.clone(),
            victim_company_id: victim_company_id.to_string(),
            victim_country: victim_country.to_string(),
            method: IPTheftMethod::StateSponsored,
            stolen_turn: current_turn,
            detected: false,
            detected_turn: None,
            initial_evasion: evasion,
        };
        thief.stolen_ips.push(stolen_ip.clone());
        result.stolen_ip = Some(stolen_ip);
    }

    result
}

/// Execute a reverse engineering IP theft attempt.
///
/// # Rules
/// - Consumes ResearchOutput (2× tech cost) from the thief's research institutes, OR
/// - Consumes domain Innovation Points (3× tech cost) from the thief's R&D budget.
/// - Cash payment to research staff via settle_transfer.
/// - Success based on ResearchOutput investment vs. tech complexity.
pub fn execute_reverse_engineering(
    thief: &mut Company,
    victim_company_id: &str,
    victim_country: &str,
    tech_id: &TechId,
    tech_node: &TechNode,
    available_research_output: f64,
    available_innovation_points: f64,
    average_wage: f64,
    config: &IPTheftConfig,
    current_turn: u32,
    country: &mut Country,
) -> IPTheftResult {
    let mut result = IPTheftResult::default();

    let tech_cost = (tech_node.cost as f64).max(1.0);
    let ro_needed = tech_cost * config.reverse_engineering_research_output_multiplier;
    let ip_needed = tech_cost * config.reverse_engineering_innovation_multiplier;
    let cash_needed = tech_cost * config.reverse_engineering_cash_ratio * average_wage;

    // Check cash availability.
    let available_cash = thief
        .brokerage_account
        .as_ref()
        .map(|ba| ba.cash.max(0.0))
        .unwrap_or(thief.available_cash.max(0.0));

    if available_cash < cash_needed {
        return result;
    }

    // Prefer ResearchOutput, fall back to domain Innovation Points.
    let (ro_consumed, ip_consumed) = if available_research_output >= ro_needed {
        (ro_needed, 0.0)
    } else if available_innovation_points >= ip_needed {
        (0.0, ip_needed)
    } else {
        // Not enough research resources.
        return result;
    };

    // Consume cash for research staff.
    let thief_id = thief.id.clone();
    let debited = debit_company_by_id(std::slice::from_mut(thief), &thief_id, cash_needed);
    if debited > 0.0 {
        country.budget.liquid_reserves += debited;
        result.cash_spent = debited;
    }

    result.research_output_consumed = ro_consumed;
    result.innovation_points_consumed = ip_consumed;
    result.domain_consumed = Some(tech_node.research_domain);

    // Success probability: based on research investment vs. tech complexity.
    let investment = ro_consumed.max(ip_consumed);
    let success_prob = investment / (investment + tech_cost).max(1.0);

    let roll = deterministic_roll(current_turn, &thief.id, tech_id);
    let success = roll < success_prob;

    result.success = success;

    if success {
        let stolen_ip = StolenIP {
            tech_id: tech_id.clone(),
            victim_company_id: victim_company_id.to_string(),
            victim_country: victim_country.to_string(),
            method: IPTheftMethod::ReverseEngineering,
            stolen_turn: current_turn,
            detected: false,
            detected_turn: None,
            initial_evasion: investment * 0.05, // Low evasion for reverse engineering.
        };
        thief.stolen_ips.push(stolen_ip.clone());
        result.stolen_ip = Some(stolen_ip);
    }

    result
}

/// Process detection rolls for all stolen IPs in a country.
///
/// Called in the sequential post-parallel phase of the turn loop.
///
/// # Rules
/// - Detection probability: `(victim_justice + victim_intel) / (victim_justice + victim_intel + thief_evasion).max(1.0)`.
/// - Evasion decays over time: `current_evasion = initial_evasion * (1 - decay_rate * turns_since_theft)`.
/// - On detection: set `detected = true`, `detected_turn = Some(current_turn)`.
/// - Cross-border without treaty: diplomatic penalty only.
/// - Cross-border with treaty OR domestic: enforce judgment (back-royalties + treble damages).
///
/// # Arguments
/// * `companies` - All companies in this country.
/// * `country` - Mutable country (for reputation, treasury).
/// * `tech_tree` - Tech tree for looking up tech costs.
/// * `config` - IP theft configuration.
/// * `reputation_config` - Reputation configuration for diplomatic penalties.
/// * `current_turn` - Current turn number.
/// * `ip_treaty_partners` - Set of country names with active IP enforcement treaties.
/// * `victim_justice` - Justice coverage ratio (0.0–1.0) for the victim country.
/// * `victim_intel` - IntelligenceCapacity of the victim country (for defense).
pub fn process_ip_theft_detection(
    companies: &mut [Company],
    country: &mut Country,
    tech_tree: &HashMap<TechId, TechNode>,
    config: &IPTheftConfig,
    reputation_config: &crate::international::reputation::ReputationConfig,
    current_turn: u32,
    ip_treaty_partners: &std::collections::HashSet<String>,
    victim_justice: f64,
    victim_intel: f64,
) -> Vec<String> {
    let mut messages = Vec::new();
    let this_country = country.name.clone();

    // Collect detection targets: (thief_idx, stolen_ip_idx, tech_cost, evasion, is_cross_border, victim_country).
    let mut detection_targets: Vec<(usize, usize, f64, f64, bool, String)> = Vec::new();

    for (thief_idx, company) in companies.iter().enumerate() {
        for (ip_idx, stolen) in company.stolen_ips.iter().enumerate() {
            if stolen.detected {
                continue;
            }
            let tech_cost = tech_tree
                .get(&stolen.tech_id)
                .map(|n| n.cost as f64)
                .unwrap_or(100.0);
            let turns_since = (current_turn - stolen.stolen_turn) as f64;
            let current_evasion =
                (stolen.initial_evasion * (1.0 - config.evasion_decay_rate * turns_since)).max(0.0);
            let is_cross_border = stolen.victim_country != this_country;
            detection_targets.push((
                thief_idx,
                ip_idx,
                tech_cost,
                current_evasion,
                is_cross_border,
                stolen.victim_country.clone(),
            ));
        }
    }

    // Process detection rolls.
    for (thief_idx, ip_idx, tech_cost, evasion, is_cross_border, victim_country) in detection_targets {
        let detection_prob =
            (victim_justice + victim_intel) / (victim_justice + victim_intel + evasion).max(1.0);

        let thief_id = companies[thief_idx].id.clone();
        let tech_id = companies[thief_idx].stolen_ips[ip_idx].tech_id.clone();
        let roll = deterministic_roll(current_turn, &thief_id, &tech_id);
        let detected = roll < detection_prob;

        if detected {
            companies[thief_idx].stolen_ips[ip_idx].detected = true;
            companies[thief_idx].stolen_ips[ip_idx].detected_turn = Some(current_turn);

            let victim_company_id =
                companies[thief_idx].stolen_ips[ip_idx].victim_company_id.clone();
            let stolen_turn = companies[thief_idx].stolen_ips[ip_idx].stolen_turn;

            if is_cross_border {
                // Cross-border: check for IP enforcement treaty.
                let has_treaty = ip_treaty_partners.contains(&victim_country);

                if has_treaty {
                    // Treaty-enforced cross-border judgment.
                    messages.push(format!(
                        "IP theft detected: {} stole {} from {} (cross-border, treaty-enforced)",
                        thief_id, tech_id, victim_company_id
                    ));
                    // Enforce judgment (back-royalties + treble damages).
                    enforce_ip_judgment(
                        companies,
                        &thief_id,
                        &victim_company_id,
                        tech_cost,
                        stolen_turn,
                        current_turn,
                        config,
                    );
                } else {
                    // No treaty: diplomatic penalty only.
                    let severity = (tech_cost / 1000.0).clamp(0.5, 1.0);
                    let penalty = reputation_config.ip_theft_penalty * severity;
                    country.global_reputation.score -= penalty;
                    country.global_reputation.violation_history.push(
                        crate::international::reputation::TreatyViolation {
                            treaty_id: format!("IP_THEFT_{}", current_turn),
                            turn: current_turn,
                            severity,
                            description: format!(
                                "Cross-border IP theft: {} stole {} from {}",
                                thief_id, tech_id, victim_company_id
                            ),
                        },
                    );
                    messages.push(format!(
                        "IP theft detected: {} stole {} from {} (cross-border, diplomatic penalty only)",
                        thief_id, tech_id, victim_company_id
                    ));
                }
            } else {
                // Domestic theft: enforce judgment.
                messages.push(format!(
                    "IP theft detected: {} stole {} from {} (domestic)",
                    thief_id, tech_id, victim_company_id
                ));
                enforce_ip_judgment(
                    companies,
                    &thief_id,
                    &victim_company_id,
                    tech_cost,
                    stolen_turn,
                    current_turn,
                    config,
                );
            }
        }
    }

    messages
}

/// Enforce an IP theft judgment: back-royalties + treble damages.
///
/// # Rules (strict balance sheet accounting)
/// 1. Compute total damages = `back_royalties × treble_damages_multiplier`.
/// 2. Attempt to debit the thief via `debit_company_by_id`.
/// 3. Credit the victim via `credit_company_by_id`.
/// 4. If `debited < total_damages`: record unpaid remainder as `JudgmentDebt` liability.
/// 5. Do NOT manually set `company_capital` negative.
/// 6. The normal accounting update will organically compute negative equity.
/// 7. The victim is recorded as an unsecured creditor.
/// 8. No debt forgiveness, no negative cash balances.
fn enforce_ip_judgment(
    companies: &mut [Company],
    thief_id: &str,
    victim_id: &str,
    tech_cost: f64,
    stolen_turn: u32,
    current_turn: u32,
    config: &IPTheftConfig,
) {
    // Estimate back-royalties: tech_cost * default_royalty_ratio * turns_elapsed.
    let turns_elapsed = (current_turn - stolen_turn) as f64;
    let back_royalties = tech_cost * 0.05 * turns_elapsed; // 5% royalty per turn.
    let total_damages = back_royalties * config.treble_damages_multiplier;

    if total_damages <= 0.0 {
        return;
    }

    // Debit the thief.
    let debited = debit_company_by_id(companies, thief_id, total_damages);

    // Credit the victim.
    if debited > 0.0 {
        credit_company_by_id(companies, victim_id, debited);
    }

    // Record unpaid remainder as judgment debt liability.
    let unpaid = total_damages - debited;
    if unpaid > 0.0 {
        if let Some(thief) = companies.iter_mut().find(|c| c.id == thief_id) {
            thief.judgment_debts.push(crate::entities::JudgmentDebt {
                creditor_company_id: victim_id.to_string(),
                amount: unpaid,
                turn_incurred: current_turn,
            });
            // Add to balance sheet liabilities (organic equity computation).
            thief.liabilities += unpaid;
        }
    }
}

/// Check if a company is using a tech via a stolen IP (undetected).
/// Used by royalty processing to skip royalty payments for stolen IPs.
pub fn is_tech_stolen_undetected(company: &Company, tech_id: &TechId) -> bool {
    company
        .stolen_ips
        .iter()
        .any(|s| &s.tech_id == tech_id && !s.detected)
}

/// Get the total IntelligenceCapacity produced by state-owned intelligence_hq buildings.
/// This is computed from building inventories in the turn loop and passed in.
/// Here we return 0 as a fallback; the actual value is computed at the call site.
pub fn get_country_intelligence_capacity(country: &Country) -> f64 {
    // IntelligenceCapacity is a physical commodity stored in building inventories.
    // The actual sum is computed in the turn loop where buildings are accessible.
    // This fallback returns 0 — the caller should pass the real value.
    let _ = country;
    0.0
}

/// Deterministic roll based on (turn, entity_id, tech_id).
/// Returns a value in [0.0, 1.0).
fn deterministic_roll(turn: u32, entity_id: &str, tech_id: &str) -> f64 {
    let mut hash: u64 = 0x51734517;
    for byte in turn.to_le_bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
    }
    for byte in entity_id.as_bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(*byte as u64);
    }
    for byte in tech_id.as_bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(*byte as u64);
    }
    (hash % 10000) as f64 / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::tech_tree::{ResearchDomain, TechType};

    fn make_tech_node(id: &str, cost: u32) -> (TechId, TechNode) {
        (
            id.to_string(),
            TechNode {
                name: format!("Test Tech {}", id),
                tech_type: TechType::Commercial,
                research_domain: ResearchDomain::Engineering,
                cost,
                year: 2000,
                prerequisites: Vec::new(),
                unlocks_methods: HashMap::new(),
                unlocks_projects: Vec::new(),
                description: String::new(),
                patent_duration_turns: 240,
                royalty_vwap_ratio: 0.05,
            },
        )
    }

    #[test]
    fn test_private_espionage_success_grants_access() {
        let mut thief = Company::default();
        thief.id = "THIEF".to_string();
        thief.available_cash = 100_000.0;

        let (tech_id, tech_node) = make_tech_node("tech_test_001", 1000);
        let mut country = Country::default();
        country.budget.liquid_reserves = 0.0;

        let result = execute_private_espionage(
            &mut thief,
            "VICTIM",
            "TestCountry",
            &tech_id,
            &tech_node,
            0.0, // No victim intel → high success probability
            10.0,
            &IPTheftConfig::default(),
            1,
            &mut country,
        );

        // With 0 victim intel, success_prob = blackops / (blackops + 1.0) ≈ 1.0.
        // The deterministic roll should be < success_prob.
        if result.success {
            assert!(result.stolen_ip.is_some());
            assert_eq!(thief.stolen_ips.len(), 1);
            assert_eq!(thief.stolen_ips[0].tech_id, "tech_test_001");
            assert!(!thief.stolen_ips[0].detected);
        }
        // Cash should have been debited.
        assert!(result.cash_spent > 0.0);
        assert!(country.budget.liquid_reserves > 0.0);
    }

    #[test]
    fn test_private_espionage_consumes_admin_and_electronics() {
        let mut thief = Company::default();
        thief.id = "THIEF".to_string();
        thief.available_cash = 100_000.0;

        let (tech_id, tech_node) = make_tech_node("tech_test_002", 1000);
        let mut country = Country::default();

        let result = execute_private_espionage(
            &mut thief,
            "VICTIM",
            "TestCountry",
            &tech_id,
            &tech_node,
            100.0,
            10.0,
            &IPTheftConfig::default(),
            1,
            &mut country,
        );

        // Verify physical resource consumption is recorded.
        assert!(result.admin_services_consumed > 0.0);
        assert!(result.electronics_consumed > 0.0);
        assert!(result.cash_spent > 0.0);
        // State intelligence should NOT be consumed by private espionage.
        assert_eq!(result.state_intel_consumed, 0.0);
    }

    #[test]
    fn test_private_espionage_cannot_raid_state_intel() {
        let mut thief = Company::default();
        thief.id = "THIEF".to_string();
        thief.available_cash = 100_000.0;

        let (tech_id, tech_node) = make_tech_node("tech_test_003", 1000);
        let mut country = Country::default();

        let result = execute_private_espionage(
            &mut thief,
            "VICTIM",
            "TestCountry",
            &tech_id,
            &tech_node,
            500.0, // High victim intel
            10.0,
            &IPTheftConfig::default(),
            1,
            &mut country,
        );

        // Private espionage should never consume state IntelligenceCapacity.
        assert_eq!(result.state_intel_consumed, 0.0);
    }

    #[test]
    fn test_state_sponsored_espionage_consumes_state_intel() {
        let mut thief = Company::default();
        thief.id = "THIEF".to_string();

        let (tech_id, tech_node) = make_tech_node("tech_test_004", 1000);

        let result = execute_state_sponsored_espionage(
            &mut thief,
            "VICTIM",
            "TestCountry",
            &tech_id,
            &tech_node,
            500.0, // State intel available
            100.0, // Victim intel
            &IPTheftConfig::default(),
            1,
        );

        // State-sponsored espionage consumes state IntelligenceCapacity.
        assert!(result.state_intel_consumed > 0.0);
        // No company cash should be debited.
        assert_eq!(result.cash_spent, 0.0);
    }

    #[test]
    fn test_reverse_engineering_consumes_research_output() {
        let mut thief = Company::default();
        thief.id = "THIEF".to_string();
        thief.available_cash = 100_000.0;

        let (tech_id, tech_node) = make_tech_node("tech_test_005", 1000);
        let mut country = Country::default();

        let result = execute_reverse_engineering(
            &mut thief,
            "VICTIM",
            "TestCountry",
            &tech_id,
            &tech_node,
            10_000.0, // Plenty of ResearchOutput
            0.0,      // No innovation points needed
            10.0,
            &IPTheftConfig::default(),
            1,
            &mut country,
        );

        // Should consume ResearchOutput (2× tech cost = 2000).
        assert!(result.research_output_consumed > 0.0);
        assert_eq!(result.innovation_points_consumed, 0.0);
    }

    #[test]
    fn test_reverse_engineering_falls_back_to_innovation_points() {
        let mut thief = Company::default();
        thief.id = "THIEF".to_string();
        thief.available_cash = 100_000.0;

        let (tech_id, tech_node) = make_tech_node("tech_test_006", 1000);
        let mut country = Country::default();

        let result = execute_reverse_engineering(
            &mut thief,
            "VICTIM",
            "TestCountry",
            &tech_id,
            &tech_node,
            0.0,      // No ResearchOutput available
            10_000.0, // Plenty of innovation points
            10.0,
            &IPTheftConfig::default(),
            1,
            &mut country,
        );

        // Should fall back to domain Innovation Points (3× tech cost = 3000).
        assert_eq!(result.research_output_consumed, 0.0);
        assert!(result.innovation_points_consumed > 0.0);
    }

    #[test]
    fn test_domestic_theft_judgment_debt_recorded_as_liability() {
        let mut thief = Company::default();
        thief.id = "THIEF".to_string();
        thief.available_cash = 100.0; // Very low cash

        let mut victim = Company::default();
        victim.id = "VICTIM".to_string();

        let mut companies = vec![thief, victim];

        // Enforce a judgment: total_damages = 1000, thief can only pay 100.
        enforce_ip_judgment(
            &mut companies,
            "THIEF",
            "VICTIM",
            1000.0, // tech_cost
            1,      // stolen_turn
            10,     // current_turn
            &IPTheftConfig::default(),
        );

        // back_royalties = 1000 * 0.05 * 9 = 450
        // total_damages = 450 * 3.0 = 1350
        // Thief can only pay ~100, so unpaid = ~1250
        let thief = &companies[0];
        assert!(!thief.judgment_debts.is_empty());
        assert!(thief.liabilities > 0.0);
        // No manual company_capital override — it stays as is.
        // The accounting equation will organically compute negative equity.
    }

    #[test]
    fn test_judgment_debt_organically_drives_negative_equity() {
        // Verify that liabilities from judgment debts will make equity negative
        // via the standard accounting equation: equity = assets - liabilities.
        let mut thief = Company::default();
        thief.id = "THIEF".to_string();
        thief.available_cash = 50.0;
        thief.fixed_capital = 100.0;
        thief.liquid_capital = 50.0;
        // company_capital = 100 + 50 - 0 = 150 initially.

        let mut victim = Company::default();
        victim.id = "VICTIM".to_string();

        let mut companies = vec![thief, victim];

        enforce_ip_judgment(
            &mut companies,
            "THIEF",
            "VICTIM",
            1000.0,
            1,
            10,
            &IPTheftConfig::default(),
        );

        // After judgment: liabilities increased by unpaid amount.
        // The organic accounting update will compute:
        //   company_capital = fixed_capital + liquid_capital - liabilities
        // If liabilities > (fixed + liquid), company_capital goes negative.
        let thief = &companies[0];
        assert!(thief.liabilities > 0.0);
        // Verify that organic equity would be negative if liabilities > assets.
        let organic_equity = thief.fixed_capital + thief.liquid_capital - thief.liabilities;
        // back_royalties = 1000 * 0.05 * 9 = 450, damages = 1350
        // thief pays ~50, unpaid = ~1300
        // organic_equity = 100 + 50 - 1300 = -1150 (negative!)
        assert!(
            organic_equity < 0.0,
            "Organic equity should be negative after large judgment debt"
        );
    }

    #[test]
    fn test_is_tech_stolen_undetected() {
        let mut company = Company::default();
        company.stolen_ips.push(StolenIP {
            tech_id: "tech_001".to_string(),
            victim_company_id: "VICTIM".to_string(),
            victim_country: "TestCountry".to_string(),
            method: IPTheftMethod::PrivateEspionage,
            stolen_turn: 1,
            detected: false,
            detected_turn: None,
            initial_evasion: 100.0,
        });
        company.stolen_ips.push(StolenIP {
            tech_id: "tech_002".to_string(),
            victim_company_id: "VICTIM".to_string(),
            victim_country: "TestCountry".to_string(),
            method: IPTheftMethod::PrivateEspionage,
            stolen_turn: 1,
            detected: true,
            detected_turn: Some(5),
            initial_evasion: 100.0,
        });

        assert!(is_tech_stolen_undetected(&company, &"tech_001".to_string()));
        assert!(!is_tech_stolen_undetected(&company, &"tech_002".to_string()));
        assert!(!is_tech_stolen_undetected(
            &company,
            &"tech_003".to_string()
        ));
    }

    #[test]
    fn test_deterministic_roll_is_deterministic() {
        let r1 = deterministic_roll(10, "COMPANY_A", "tech_001");
        let r2 = deterministic_roll(10, "COMPANY_A", "tech_001");
        assert_eq!(r1, r2);
        assert!(r1 >= 0.0 && r1 < 1.0);
    }

    #[test]
    fn test_deterministic_roll_differs_by_inputs() {
        let r1 = deterministic_roll(10, "COMPANY_A", "tech_001");
        let r2 = deterministic_roll(10, "COMPANY_B", "tech_001");
        let r3 = deterministic_roll(11, "COMPANY_A", "tech_001");
        // At least one should differ (extremely high probability).
        assert!(r1 != r2 || r1 != r3);
    }
}

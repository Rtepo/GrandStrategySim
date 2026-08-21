//! Phase 70.7: Hybrid War Declaration System.
//!
//! Implements two paths to war:
//!
//! 1. **Direct Declaration** — A country directly declares war on another
//!    via `DiplomaticAction::DeclareWar`. This is the explicit, immediate path.
//!
//! 2. **Tension Escalation** — Bilateral tension accumulates through border
//!    provocations, proxy war funding, and diplomatic incidents. When tension
//!    exceeds the `war_tension_threshold`, war auto-declares.
//!
//! # War State
//!
//! When war is declared (either path), both countries' `at_war_with` lists
//! are updated. The war continues until a peace treaty is signed via
//! `DiplomaticAction::SueForPeace`.
//!
//! # Peace Terms
//!
//! Peace settlements can include:
//! - `StatusQuoAnte` — return to pre-war borders
//! - `TerritorialCession` — transfer regions from loser to winner
//! - `Reparations` — financial compensation (double-entry: loser pays winner)
//! - `UnconditionalSurrender` — winner dictates all terms
//! - `RepatriationOfPows` — all POWs returned to their home countries

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// WAR REASON
// ============================================================================

/// The stated reason for declaring war.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum WarReason {
    /// War to conquer territory from the target.
    TerritorialConquest,
    /// War to change the target's government or regime.
    RegimeChange,
    /// War to control strategic resources in the target's territory.
    ResourceControl,
    /// Preemptive strike against a growing threat.
    PreemptiveStrike,
    /// War triggered by tension escalation (no explicit reason).
    TensionEscalation,
}

// ============================================================================
// PEACE TERMS
// ============================================================================

/// Terms for a peace settlement.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum PeaceTerms {
    /// Return to pre-war borders and status. No territorial or financial changes.
    StatusQuoAnte,
    /// Transfer specific regions from the losing side to the winning side.
    TerritorialCession {
        /// Region IDs to transfer from loser to winner.
        regions: Vec<String>,
    },
    /// Financial reparations paid by the loser to the winner.
    /// Double-entry: loser's treasury debited, winner's treasury credited.
    Reparations {
        /// Total reparations amount.
        amount: f64,
        /// Whether POW repatriation is included as a condition.
        pow_repatriation: bool,
    },
    /// Unconditional surrender — the winner dictates all terms.
    /// Implemented as territorial cession + reparations + POW repatriation.
    UnconditionalSurrender {
        /// Regions to transfer.
        regions: Vec<String>,
        /// Reparations amount.
        reparations: f64,
    },
    /// Repatriation of POWs only (no territorial or financial changes).
    RepatriationOfPows,
}

// ============================================================================
// WAR STATE
// ============================================================================

/// Tracks the state of a war between two countries.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WarState {
    /// Country that initiated the war.
    pub aggressor: String,
    /// Country that was attacked.
    pub defender: String,
    /// Turn when the war started.
    pub start_turn: u32,
    /// Reason for the war.
    pub reason: WarReason,
    /// Whether peace has been requested (and by whom).
    pub peace_requested_by: Option<String>,
    /// Proposed peace terms (if any).
    pub proposed_terms: Option<PeaceTerms>,
}

impl WarState {
    /// Creates a new war state.
    pub fn new(aggressor: String, defender: String, start_turn: u32, reason: WarReason) -> Self {
        Self {
            aggressor,
            defender,
            start_turn,
            reason,
            peace_requested_by: None,
            proposed_terms: None,
        }
    }
}

// ============================================================================
// BILATERAL TENSION
// ============================================================================

/// Bilateral tension between two countries (0.0–100.0).
///
/// Tension accumulates through:
/// - Border provocations (each provocation adds tension)
/// - Proxy war funding in the other country's autonomous republics
/// - Diplomatic incidents
///
/// When tension exceeds `war_tension_threshold`, war auto-declares.
/// Tension decays at `tension_decay_rate` per turn when no provocations occur.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct BilateralTension {
    /// Current tension level (0.0–100.0).
    pub level: f64,
    /// Total tension ever accumulated (for historical tracking).
    pub total_ever_accumulated: f64,
    /// Number of provocations that have occurred.
    pub provocation_count: u32,
}

impl BilateralTension {
    /// Creates a new bilateral tension tracker at zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds tension from a provocation.
    ///
    /// # Arguments
    /// * `intensity` - Provocation intensity (0.0–1.0).
    /// * `tension_per_provocation` - Config value: tension added per unit intensity.
    pub fn add_provocation(&mut self, intensity: f64, tension_per_provocation: f64) {
        let tension_gain = intensity * tension_per_provocation;
        self.level = (self.level + tension_gain).min(100.0);
        self.total_ever_accumulated += tension_gain;
        self.provocation_count += 1;
    }

    /// Decays tension by the decay rate.
    ///
    /// # Arguments
    /// * `decay_rate` - Fraction of tension to remove per turn.
    pub fn decay(&mut self, decay_rate: f64) {
        self.level = (self.level - self.level * decay_rate).max(0.0);
    }

    /// Returns true if tension has exceeded the war threshold.
    pub fn exceeds_threshold(&self, threshold: f64) -> bool {
        self.level >= threshold
    }
}

// ============================================================================
// WAR DECLARATION CONFIG
// ============================================================================

/// Configuration for the hybrid war declaration system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WarDeclarationConfig {
    /// Tension level at which war auto-declares (e.g., 80.0).
    pub war_tension_threshold: f64,
    /// Tension decay rate per turn (fraction, e.g., 0.05 = 5% decay).
    pub tension_decay_rate: f64,
    /// Tension added per unit of provocation intensity (e.g., 15.0).
    pub tension_per_provocation: f64,
    /// Tension added when funding proxy wars in the other country (e.g., 10.0).
    pub tension_per_proxy_funding: f64,
}

impl Default for WarDeclarationConfig {
    fn default() -> Self {
        Self {
            war_tension_threshold: 80.0,
            tension_decay_rate: 0.05,
            tension_per_provocation: 15.0,
            tension_per_proxy_funding: 10.0,
        }
    }
}

// ============================================================================
// WAR DECLARATION ACTIONS
// ============================================================================

/// Result of a war declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct WarDeclarationResult {
    /// Whether war was successfully declared.
    pub declared: bool,
    /// The war state (if war was declared).
    pub war_state: Option<WarState>,
    /// Log messages.
    pub messages: Vec<String>,
}

/// Declares war between two countries.
///
/// This is the direct declaration path. Updates both countries' `at_war_with`
/// lists and creates a `WarState` entry.
///
/// # Arguments
/// * `aggressor` - Name of the country declaring war.
/// * `defender` - Name of the country being attacked.
/// * `turn` - Current game turn.
/// * `reason` - Reason for the war.
/// * `at_war_with` - Mutable map of country → at_war_with lists (will be updated).
///
/// # Returns
/// `WarDeclarationResult` with the war state.
pub fn declare_war(
    aggressor: &str,
    defender: &str,
    turn: u32,
    reason: WarReason,
    at_war_with: &mut HashMap<String, Vec<String>>,
) -> WarDeclarationResult {
    let mut messages = Vec::new();

    // Check if already at war
    let aggressor_wars = at_war_with.get(aggressor).cloned().unwrap_or_default();
    if aggressor_wars.contains(&defender.to_string()) {
        messages.push(format!("[WAR] {} is already at war with {}", aggressor, defender));
        return WarDeclarationResult {
            declared: false,
            war_state: None,
            messages,
        };
    }

    // Add to aggressor's war list
    at_war_with.entry(aggressor.to_string()).or_default().push(defender.to_string());
    // Add to defender's war list
    at_war_with.entry(defender.to_string()).or_default().push(aggressor.to_string());

    let war_state = WarState::new(
        aggressor.to_string(),
        defender.to_string(),
        turn,
        reason,
    );

    messages.push(format!(
        "[WAR] {} declares war on {} (reason: {:?})",
        aggressor, defender, war_state.reason
    ));

    WarDeclarationResult {
        declared: true,
        war_state: Some(war_state),
        messages,
    }
}

/// Checks all bilateral tensions and auto-declares war for any that exceed
/// the threshold.
///
/// This is the tension escalation path. Called once per turn after tension
/// decay.
///
/// # Arguments
/// * `tensions` - All bilateral tensions (key = "country_a|country_b" sorted).
/// * `at_war_with` - Mutable map of country → at_war_with lists.
/// * `turn` - Current game turn.
/// * `config` - War declaration configuration.
///
/// # Returns
/// Vector of `WarDeclarationResult` for each war declared.
pub fn check_tension_escalations(
    tensions: &mut HashMap<String, BilateralTension>,
    at_war_with: &mut HashMap<String, Vec<String>>,
    turn: u32,
    config: &WarDeclarationConfig,
) -> Vec<WarDeclarationResult> {
    let mut results = Vec::new();

    // Find tensions that exceed the threshold
    let escalated: Vec<(String, String)> = tensions.iter()
        .filter(|(_, t)| t.exceeds_threshold(config.war_tension_threshold))
        .filter_map(|(key, _)| {
            // Parse the key "country_a|country_b"
            let parts: Vec<&str> = key.split('|').collect();
            if parts.len() == 2 {
                Some((parts[0].to_string(), parts[1].to_string()))
            } else {
                None
            }
        })
        .collect();

    for (country_a, country_b) in escalated {
        // Check if already at war
        let already_at_war = at_war_with.get(&country_a)
            .map(|wars| wars.contains(&country_b))
            .unwrap_or(false);

        if already_at_war {
            continue;
        }

        // Determine aggressor: the country with higher tension contribution
        // (more provocations). For simplicity, country_a is the aggressor.
        let result = declare_war(
            &country_a,
            &country_b,
            turn,
            WarReason::TensionEscalation,
            at_war_with,
        );

        if result.declared {
            // Reset tension after war declaration
            let key = format!("{}|{}", country_a, country_b);
            if let Some(tension) = tensions.get_mut(&key) {
                tension.level = 0.0;
            }
            results.push(result);
        }
    }

    results
}

/// Decays all bilateral tensions by the configured rate.
///
/// # Arguments
/// * `tensions` - All bilateral tensions (will be mutated).
/// * `config` - War declaration configuration.
pub fn decay_all_tensions(
    tensions: &mut HashMap<String, BilateralTension>,
    config: &WarDeclarationConfig,
) {
    for tension in tensions.values_mut() {
        tension.decay(config.tension_decay_rate);
    }
}

/// Generates a bilateral tension key (sorted pair of country names).
pub fn tension_key(country_a: &str, country_b: &str) -> String {
    if country_a <= country_b {
        format!("{}|{}", country_a, country_b)
    } else {
        format!("{}|{}", country_b, country_a)
    }
}

// ============================================================================
// PEACE SETTLEMENT
// ============================================================================

/// Result of a peace settlement.
#[derive(Debug, Clone, PartialEq)]
pub struct PeaceSettlementResult {
    /// Whether peace was successfully established.
    pub peace_established: bool,
    /// The terms that were agreed.
    pub terms: PeaceTerms,
    /// Financial transfer amount (if reparations).
    pub financial_transfer: Option<f64>,
    /// Regions transferred (if territorial cession).
    pub regions_transferred: Vec<String>,
    /// Whether POWs were repatriated.
    pub pows_repatriated: bool,
    /// Log messages.
    pub messages: Vec<String>,
}

/// Processes a peace settlement between two countries.
///
/// This function:
/// 1. Removes both countries from each other's `at_war_with` lists.
/// 2. Applies the peace terms:
///    - `StatusQuoAnte` — no changes.
///    - `TerritorialCession` — transfers regions.
///    - `Reparations` — financial transfer (double-entry).
///    - `UnconditionalSurrender` — all of the above.
///    - `RepatriationOfPows` — triggers POW repatriation.
///
/// # Arguments
/// * `aggressor` - The aggressor country name.
/// * `defender` - The defender country name.
/// * `terms` - The peace terms to apply.
/// * `at_war_with` - Mutable war lists (will be updated).
///
/// # Returns
/// `PeaceSettlementResult` with details of the settlement.
pub fn settle_peace(
    aggressor: &str,
    defender: &str,
    terms: &PeaceTerms,
    at_war_with: &mut HashMap<String, Vec<String>>,
) -> PeaceSettlementResult {
    let mut messages = Vec::new();
    let mut financial_transfer = None;
    let mut regions_transferred = Vec::new();
    let mut pows_repatriated = false;

    // Remove from war lists
    if let Some(wars) = at_war_with.get_mut(aggressor) {
        wars.retain(|w| w != defender);
    }
    if let Some(wars) = at_war_with.get_mut(defender) {
        wars.retain(|w| w != aggressor);
    }

    match terms {
        PeaceTerms::StatusQuoAnte => {
            messages.push(format!(
                "[PEACE] {} and {} sign peace: Status Quo Ante (return to pre-war borders)",
                aggressor, defender
            ));
        }
        PeaceTerms::TerritorialCession { regions } => {
            regions_transferred = regions.clone();
            messages.push(format!(
                "[PEACE] {} cedes {} regions to {}",
                defender, regions.len(), aggressor
            ));
        }
        PeaceTerms::Reparations { amount, pow_repatriation } => {
            financial_transfer = Some(*amount);
            pows_repatriated = *pow_repatriation;
            messages.push(format!(
                "[PEACE] {} pays {} reparations to {}{}",
                defender, amount, aggressor,
                if *pow_repatriation { " (with POW repatriation)" } else { "" }
            ));
        }
        PeaceTerms::UnconditionalSurrender { regions, reparations } => {
            regions_transferred = regions.clone();
            financial_transfer = Some(*reparations);
            pows_repatriated = true;
            messages.push(format!(
                "[PEACE] {} unconditionally surrenders to {}: {} regions, {} reparations, POW repatriation",
                defender, aggressor, regions.len(), reparations
            ));
        }
        PeaceTerms::RepatriationOfPows => {
            pows_repatriated = true;
            messages.push(format!(
                "[PEACE] {} and {} agree to POW repatriation",
                aggressor, defender
            ));
        }
    }

    PeaceSettlementResult {
        peace_established: true,
        terms: terms.clone(),
        financial_transfer,
        regions_transferred,
        pows_repatriated,
        messages,
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_declare_war_direct() {
        let mut at_war_with = HashMap::new();
        let result = declare_war(
            "Aggressor",
            "Defender",
            10,
            WarReason::TerritorialConquest,
            &mut at_war_with,
        );

        assert!(result.declared);
        assert!(result.war_state.is_some());
        assert!(at_war_with.get("Aggressor").unwrap().contains(&"Defender".to_string()));
        assert!(at_war_with.get("Defender").unwrap().contains(&"Aggressor".to_string()));
    }

    #[test]
    fn test_declare_war_already_at_war() {
        let mut at_war_with = HashMap::new();
        at_war_with.insert("A".to_string(), vec!["B".to_string()]);
        at_war_with.insert("B".to_string(), vec!["A".to_string()]);

        let result = declare_war("A", "B", 5, WarReason::ResourceControl, &mut at_war_with);

        assert!(!result.declared, "Should not declare war if already at war");
    }

    #[test]
    fn test_bilateral_tension_accumulation() {
        let mut tension = BilateralTension::new();
        tension.add_provocation(0.5, 15.0); // +7.5

        assert!((tension.level - 7.5).abs() < 0.001);
        assert_eq!(tension.provocation_count, 1);
    }

    #[test]
    fn test_bilateral_tension_cap_at_100() {
        let mut tension = BilateralTension::new();
        tension.add_provocation(1.0, 200.0); // Would add 200, capped at 100

        assert_eq!(tension.level, 100.0);
    }

    #[test]
    fn test_bilateral_tension_decay() {
        let mut tension = BilateralTension::new();
        tension.add_provocation(1.0, 50.0); // level = 50
        tension.decay(0.1); // 10% decay → 45

        assert!((tension.level - 45.0).abs() < 0.001);
    }

    #[test]
    fn test_tension_exceeds_threshold() {
        let mut tension = BilateralTension::new();
        tension.add_provocation(1.0, 90.0); // level = 90

        assert!(tension.exceeds_threshold(80.0));
        assert!(!tension.exceeds_threshold(95.0));
    }

    #[test]
    fn test_tension_escalation_auto_declares_war() {
        let mut tensions = HashMap::new();
        let key = tension_key("CountryA", "CountryB");
        let mut tension = BilateralTension::new();
        tension.add_provocation(1.0, 90.0); // level = 90 > threshold 80
        tensions.insert(key, tension);

        let mut at_war_with = HashMap::new();
        let config = WarDeclarationConfig::default();

        let results = check_tension_escalations(&mut tensions, &mut at_war_with, 5, &config);

        assert_eq!(results.len(), 1);
        assert!(results[0].declared);
        assert!(at_war_with.get("CountryA").unwrap().contains(&"CountryB".to_string()));
    }

    #[test]
    fn test_tension_escalation_skips_already_at_war() {
        let mut tensions = HashMap::new();
        let key = tension_key("A", "B");
        let mut tension = BilateralTension::new();
        tension.add_provocation(1.0, 90.0);
        tensions.insert(key, tension);

        let mut at_war_with = HashMap::new();
        at_war_with.insert("A".to_string(), vec!["B".to_string()]);
        at_war_with.insert("B".to_string(), vec!["A".to_string()]);

        let config = WarDeclarationConfig::default();
        let results = check_tension_escalations(&mut tensions, &mut at_war_with, 5, &config);

        assert_eq!(results.len(), 0, "Should not declare war if already at war");
    }

    #[test]
    fn test_tension_escalation_resets_tension() {
        let mut tensions = HashMap::new();
        let key = tension_key("A", "B");
        let mut tension = BilateralTension::new();
        tension.add_provocation(1.0, 90.0);
        tensions.insert(key.clone(), tension);

        let mut at_war_with = HashMap::new();
        let config = WarDeclarationConfig::default();

        let _ = check_tension_escalations(&mut tensions, &mut at_war_with, 5, &config);

        // Tension should be reset after war declaration
        assert_eq!(tensions.get(&key).unwrap().level, 0.0);
    }

    #[test]
    fn test_decay_all_tensions() {
        let mut tensions = HashMap::new();
        let mut t1 = BilateralTension::new();
        t1.add_provocation(1.0, 50.0); // level = 50
        tensions.insert("A|B".to_string(), t1);

        let mut t2 = BilateralTension::new();
        t2.add_provocation(1.0, 30.0); // level = 30
        tensions.insert("C|D".to_string(), t2);

        let config = WarDeclarationConfig::default();
        decay_all_tensions(&mut tensions, &config);

        // 5% decay: 50 → 47.5, 30 → 28.5
        assert!((tensions.get("A|B").unwrap().level - 47.5).abs() < 0.001);
        assert!((tensions.get("C|D").unwrap().level - 28.5).abs() < 0.001);
    }

    #[test]
    fn test_tension_key_sorted() {
        assert_eq!(tension_key("A", "B"), "A|B");
        assert_eq!(tension_key("B", "A"), "A|B"); // Always sorted
    }

    #[test]
    fn test_peace_settlement_status_quo() {
        let mut at_war_with = HashMap::new();
        at_war_with.insert("A".to_string(), vec!["B".to_string()]);
        at_war_with.insert("B".to_string(), vec!["A".to_string()]);

        let result = settle_peace("A", "B", &PeaceTerms::StatusQuoAnte, &mut at_war_with);

        assert!(result.peace_established);
        assert!(!at_war_with.get("A").unwrap().contains(&"B".to_string()));
        assert!(!at_war_with.get("B").unwrap().contains(&"A".to_string()));
        assert!(result.financial_transfer.is_none());
        assert!(result.regions_transferred.is_empty());
    }

    #[test]
    fn test_peace_settlement_reparations() {
        let mut at_war_with = HashMap::new();
        at_war_with.insert("A".to_string(), vec!["B".to_string()]);
        at_war_with.insert("B".to_string(), vec!["A".to_string()]);

        let result = settle_peace(
            "A", "B",
            &PeaceTerms::Reparations { amount: 1000.0, pow_repatriation: true },
            &mut at_war_with,
        );

        assert!(result.peace_established);
        assert_eq!(result.financial_transfer, Some(1000.0));
        assert!(result.pows_repatriated);
    }

    #[test]
    fn test_peace_settlement_territorial_cession() {
        let mut at_war_with = HashMap::new();
        at_war_with.insert("A".to_string(), vec!["B".to_string()]);
        at_war_with.insert("B".to_string(), vec!["A".to_string()]);

        let result = settle_peace(
            "A", "B",
            &PeaceTerms::TerritorialCession { regions: vec!["region1".to_string(), "region2".to_string()] },
            &mut at_war_with,
        );

        assert!(result.peace_established);
        assert_eq!(result.regions_transferred.len(), 2);
    }

    #[test]
    fn test_peace_settlement_unconditional_surrender() {
        let mut at_war_with = HashMap::new();
        at_war_with.insert("A".to_string(), vec!["B".to_string()]);
        at_war_with.insert("B".to_string(), vec!["A".to_string()]);

        let result = settle_peace(
            "A", "B",
            &PeaceTerms::UnconditionalSurrender {
                regions: vec!["r1".to_string()],
                reparations: 5000.0,
            },
            &mut at_war_with,
        );

        assert!(result.peace_established);
        assert_eq!(result.financial_transfer, Some(5000.0));
        assert!(!result.regions_transferred.is_empty());
        assert!(result.pows_repatriated);
    }

    #[test]
    fn test_peace_settlement_repatriation_only() {
        let mut at_war_with = HashMap::new();
        at_war_with.insert("A".to_string(), vec!["B".to_string()]);
        at_war_with.insert("B".to_string(), vec!["A".to_string()]);

        let result = settle_peace("A", "B", &PeaceTerms::RepatriationOfPows, &mut at_war_with);

        assert!(result.peace_established);
        assert!(result.pows_repatriated);
        assert!(result.financial_transfer.is_none());
        assert!(result.regions_transferred.is_empty());
    }

    #[test]
    fn test_war_state_creation() {
        let war = WarState::new(
            "Aggressor".to_string(),
            "Defender".to_string(),
            10,
            WarReason::TerritorialConquest,
        );

        assert_eq!(war.aggressor, "Aggressor");
        assert_eq!(war.defender, "Defender");
        assert_eq!(war.start_turn, 10);
        assert!(war.peace_requested_by.is_none());
        assert!(war.proposed_terms.is_none());
    }
}

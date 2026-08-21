//! Phase 67: Modular multi-party treaties with clauses, negotiation timers,
//! diplomatic capacity costs, and deep engine integration.
//!
//! Treaty clauses have real economic effects:
//! - `CustomsUnion`: merges market demand/supply between participants
//! - `SchengenFreeMovement`: zeros border enforcement, boosts migration
//! - `FinancialMarketIntegration`: bypasses foreign ownership caps
//! - `MutualDefense`: military coordination trigger
//! - `TradePreference`: tariff reduction between participants
//! - `ResourceAccess`: grants access to a specific commodity

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A clause within a treaty, each with distinct economic effects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TreatyClause {
    /// Merge market demand/supply between participants (deep integration).
    CustomsUnion,
    /// Zero border enforcement, boost migration attractiveness between participants.
    SchengenFreeMovement,
    /// Bypass foreign ownership caps for cross-border stock/land purchases.
    FinancialMarketIntegration,
    /// Military coordination — mutual defense pact trigger.
    MutualDefense,
    /// Tariff reduction between participants (parameterized).
    TradePreference,
    /// Grants access to a specific commodity market.
    ResourceAccess { /// The commodity this treaty grants access to.
        commodity: String },
}

impl TreatyClause {
    /// Human-readable label for UI display.
    pub fn as_str(&self) -> &'static str {
        match self {
            TreatyClause::CustomsUnion => "Customs Union",
            TreatyClause::SchengenFreeMovement => "Schengen Free Movement",
            TreatyClause::FinancialMarketIntegration => "Financial Market Integration",
            TreatyClause::MutualDefense => "Mutual Defense",
            TreatyClause::TradePreference => "Trade Preference",
            TreatyClause::ResourceAccess { .. } => "Resource Access",
        }
    }

    /// Returns the base diplomatic capacity cost for this clause type.
    pub fn base_capacity_cost(&self) -> u32 {
        match self {
            TreatyClause::CustomsUnion => 3,
            TreatyClause::SchengenFreeMovement => 4,
            TreatyClause::FinancialMarketIntegration => 3,
            TreatyClause::MutualDefense => 2,
            TreatyClause::TradePreference => 1,
            TreatyClause::ResourceAccess { .. } => 2,
        }
    }
}

/// The lifecycle status of a treaty.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum TreatyStatus {
    /// Treaty has been proposed but not yet accepted by all parties.
    #[default]
    Proposed,
    /// Negotiations are ongoing; progress increases each turn.
    Negotiating,
    /// Treaty is in force; clauses are actively applied.
    Active,
    /// Treaty is temporarily suspended (e.g., due to diplomatic freeze).
    Suspended,
    /// Treaty has been unilaterally abrogated by one party.
    Abrogated,
    /// Treaty has expired naturally after its duration.
    Expired,
}

impl TreatyStatus {
    /// Returns a human-readable label for UI display.
    pub fn as_str(&self) -> &'static str {
        match self {
            TreatyStatus::Proposed => "Proposed",
            TreatyStatus::Negotiating => "Negotiating",
            TreatyStatus::Active => "Active",
            TreatyStatus::Suspended => "Suspended",
            TreatyStatus::Abrogated => "Abrogated",
            TreatyStatus::Expired => "Expired",
        }
    }
}

/// A multi-party treaty with clauses, negotiation progress, and lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Treaty {
    /// Unique treaty ID (e.g., "TREATY-000001").
    #[serde(default)]
    pub id: String,
    /// Human-readable treaty name.
    #[serde(default)]
    pub name: String,
    /// All participant country names.
    #[serde(default)]
    pub participants: Vec<String>,
    /// Active clauses in this treaty.
    #[serde(default)]
    pub clauses: Vec<TreatyClause>,
    /// Current lifecycle status.
    #[serde(default)]
    pub status: TreatyStatus,
    /// Negotiation progress (0.0 to 1.0). Reaches 1.0 when ready to sign.
    #[serde(default)]
    pub negotiation_progress: f64,
    /// Diplomatic capacity cost (determined by clauses + reputation).
    #[serde(default)]
    pub diplomatic_capacity_cost: u32,
    /// Turn when the treaty was proposed.
    #[serde(default)]
    pub initiated_turn: u32,
    /// Turn when the treaty was signed (if Active/Expired/Abrogated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_turn: Option<u32>,
    /// Duration in turns after which the treaty expires naturally.
    #[serde(default)]
    pub duration_turns: u32,
    /// Country that initiated the treaty proposal.
    #[serde(default)]
    pub initiator: String,
}

impl Treaty {
    /// Creates a new proposed treaty with computed capacity cost.
    pub fn new(
        id: String,
        name: String,
        participants: Vec<String>,
        clauses: Vec<TreatyClause>,
        initiated_turn: u32,
        duration_turns: u32,
        initiator: String,
    ) -> Self {
        let base_cost: u32 = clauses.iter().map(|c| c.base_capacity_cost()).sum();
        Self {
            id,
            name,
            participants,
            clauses,
            status: TreatyStatus::Proposed,
            negotiation_progress: 0.0,
            diplomatic_capacity_cost: base_cost,
            initiated_turn,
            signed_turn: None,
            duration_turns,
            initiator,
        }
    }

    /// Returns true if both countries are participants in this treaty.
    pub fn has_participants(&self, a: &str, b: &str) -> bool {
        self.participants.contains(&a.to_string()) && self.participants.contains(&b.to_string())
    }

    /// Returns true if this treaty has the given clause.
    pub fn has_clause(&self, clause: &TreatyClause) -> bool {
        self.clauses.contains(clause)
    }

    /// Returns true if the treaty is currently active.
    pub fn is_active(&self) -> bool {
        self.status == TreatyStatus::Active
    }

    /// Returns the number of turns since signing (if signed).
    pub fn turns_since_signing(&self, current_turn: u32) -> Option<u32> {
        self.signed_turn.map(|t| current_turn.saturating_sub(t))
    }

    /// Returns true if the treaty has expired naturally.
    pub fn is_expired(&self, current_turn: u32) -> bool {
        if let Some(signed) = self.signed_turn {
            current_turn >= signed + self.duration_turns
        } else {
            false
        }
    }
}

/// Configuration for the treaty system. No magic numbers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TreatyConfig {
    /// Base negotiation speed per turn (progress increment before modifiers).
    pub negotiation_speed_base: f64,
    /// Bonus to negotiation speed per ambassador assigned to any participant.
    pub ambassador_negotiation_bonus: f64,
    /// Bonus to negotiation speed when relations are positive (>50).
    pub good_relations_bonus: f64,
    /// Penalty to negotiation speed when relations are negative (<-50).
    pub bad_relations_penalty: f64,
    /// Default treaty duration in turns.
    pub default_duration_turns: u32,
    /// Minimum relations score required to propose a treaty.
    pub min_relations_to_propose: i64,
}

impl Default for TreatyConfig {
    fn default() -> Self {
        Self {
            negotiation_speed_base: 0.10,
            ambassador_negotiation_bonus: 0.05,
            good_relations_bonus: 0.05,
            bad_relations_penalty: 0.05,
            default_duration_turns: 100,
            min_relations_to_propose: -20,
        }
    }
}

/// Registry tracking all treaties and the next ID counter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TreatyRegistry {
    /// All treaties (active, pending, expired, abrogated).
    #[serde(default)]
    pub treaties: Vec<Treaty>,
    /// Next auto-increment ID counter.
    #[serde(default)]
    pub next_id: u64,
}

impl TreatyRegistry {
    /// Generates the next treaty ID.
    pub fn next_treaty_id(&mut self) -> String {
        self.next_id += 1;
        format!("TREATY-{:06}", self.next_id)
    }

    /// Returns all active treaties.
    pub fn active_treaties(&self) -> Vec<&Treaty> {
        self.treaties.iter().filter(|t| t.is_active()).collect()
    }

    /// Returns all treaties involving a specific country.
    pub fn treaties_for_country(&self, country: &str) -> Vec<&Treaty> {
        self.treaties.iter().filter(|t| t.participants.contains(&country.to_string())).collect()
    }

    /// Returns active treaties shared between two countries.
    pub fn active_treaties_between(&self, a: &str, b: &str) -> Vec<&Treaty> {
        self.treaties.iter()
            .filter(|t| t.is_active() && t.has_participants(a, b))
            .collect()
    }

    /// Checks if two countries share an active treaty with the given clause.
    pub fn has_active_clause_between(&self, a: &str, b: &str, clause: &TreatyClause) -> bool {
        self.active_treaties_between(a, b).iter().any(|t| t.has_clause(clause))
    }

    /// Advances negotiation progress for all pending treaties.
    pub fn advance_negotiations(
        &mut self,
        current_turn: u32,
        config: &TreatyConfig,
        diplomacy: &std::collections::HashMap<String, std::collections::HashMap<String, crate::international::DiplomaticRelation>>,
        ambassador_counts: &BTreeMap<String, BTreeMap<String, u32>>,
    ) {
        for treaty in &mut self.treaties {
            if treaty.status != TreatyStatus::Proposed && treaty.status != TreatyStatus::Negotiating {
                continue;
            }

            treaty.status = TreatyStatus::Negotiating;

            let mut speed = config.negotiation_speed_base;

            // Ambassador bonus: count ambassadors posted to any participant
            for participant in &treaty.participants {
                if let Some(hosts) = ambassador_counts.get(participant) {
                    for (_, count) in hosts {
                        speed += config.ambassador_negotiation_bonus * (*count as f64);
                    }
                }
            }

            // Relations bonus/penalty: average relations between all participant pairs
            let mut avg_relations: f64 = 0.0;
            let mut pair_count: f64 = 0.0;
            for i in 0..treaty.participants.len() {
                for j in (i + 1)..treaty.participants.len() {
                    let rel = diplomacy
                        .get(&treaty.participants[i])
                        .and_then(|p| p.get(&treaty.participants[j]))
                        .map(|r| r.relations as f64)
                        .unwrap_or(0.0);
                    avg_relations += rel;
                    pair_count += 1.0;
                }
            }
            if pair_count > 0.0 {
                avg_relations /= pair_count;
                if avg_relations > 50.0 {
                    speed += config.good_relations_bonus;
                } else if avg_relations < -50.0 {
                    speed -= config.bad_relations_penalty;
                }
            }

            treaty.negotiation_progress = (treaty.negotiation_progress + speed).min(1.0);

            // Auto-sign when progress reaches 1.0
            if treaty.negotiation_progress >= 1.0 {
                treaty.status = TreatyStatus::Active;
                treaty.signed_turn = Some(current_turn);
                if treaty.duration_turns == 0 {
                    treaty.duration_turns = config.default_duration_turns;
                }
            }
        }
    }

    /// Signs a treaty immediately (bypassing negotiation progress).
    pub fn sign_treaty(&mut self, treaty_id: &str, current_turn: u32, config: &TreatyConfig) -> bool {
        if let Some(treaty) = self.treaties.iter_mut().find(|t| t.id == treaty_id) {
            if treaty.status == TreatyStatus::Proposed || treaty.status == TreatyStatus::Negotiating {
                treaty.status = TreatyStatus::Active;
                treaty.signed_turn = Some(current_turn);
                treaty.negotiation_progress = 1.0;
                if treaty.duration_turns == 0 {
                    treaty.duration_turns = config.default_duration_turns;
                }
                return true;
            }
        }
        false
    }

    /// Abrogates a treaty unilaterally — triggers reputation penalty.
    /// Returns the treaty that was abrogated (for reputation processing).
    pub fn abrogate_treaty(&mut self, treaty_id: &str) -> Option<Treaty> {
        if let Some(treaty) = self.treaties.iter_mut().find(|t| t.id == treaty_id) {
            if treaty.status == TreatyStatus::Active {
                treaty.status = TreatyStatus::Abrogated;
                return Some(treaty.clone());
            }
        }
        None
    }

    /// Expires treaties that have reached their duration.
    pub fn expire_finished_treaties(&mut self, current_turn: u32) {
        for treaty in &mut self.treaties {
            if treaty.is_active() && treaty.is_expired(current_turn) {
                treaty.status = TreatyStatus::Expired;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_treaty_clause_capacity_costs() {
        assert!(TreatyClause::CustomsUnion.base_capacity_cost() > 0);
        assert!(TreatyClause::SchengenFreeMovement.base_capacity_cost() > 0);
        assert_eq!(TreatyClause::TradePreference.base_capacity_cost(), 1);
    }

    #[test]
    fn test_treaty_new() {
        let treaty = Treaty::new(
            "TREATY-000001".to_string(),
            "Test Treaty".to_string(),
            vec!["CountryA".to_string(), "CountryB".to_string()],
            vec![TreatyClause::CustomsUnion, TreatyClause::TradePreference],
            10,
            100,
            "CountryA".to_string(),
        );
        assert_eq!(treaty.status, TreatyStatus::Proposed);
        assert_eq!(treaty.negotiation_progress, 0.0);
        assert_eq!(treaty.diplomatic_capacity_cost, 4); // 3 + 1
        assert!(treaty.has_participants("CountryA", "CountryB"));
        assert!(!treaty.has_participants("CountryA", "CountryC"));
        assert!(treaty.has_clause(&TreatyClause::CustomsUnion));
        assert!(!treaty.has_clause(&TreatyClause::MutualDefense));
    }

    #[test]
    fn test_treaty_registry_active_treaties_between() {
        let mut registry = TreatyRegistry::default();
        let mut t1 = Treaty::new(
            "TREATY-000001".to_string(),
            "Customs Pact".to_string(),
            vec!["A".to_string(), "B".to_string()],
            vec![TreatyClause::CustomsUnion],
            1, 100, "A".to_string(),
        );
        t1.status = TreatyStatus::Active;
        t1.signed_turn = Some(5);
        registry.treaties.push(t1);

        let mut t2 = Treaty::new(
            "TREATY-000002".to_string(),
            "Defense Pact".to_string(),
            vec!["B".to_string(), "C".to_string()],
            vec![TreatyClause::MutualDefense],
            2, 100, "B".to_string(),
        );
        t2.status = TreatyStatus::Proposed;
        registry.treaties.push(t2);

        let active_ab = registry.active_treaties_between("A", "B");
        assert_eq!(active_ab.len(), 1);
        assert!(registry.has_active_clause_between("A", "B", &TreatyClause::CustomsUnion));
        assert!(!registry.has_active_clause_between("B", "C", &TreatyClause::MutualDefense));
    }

    #[test]
    fn test_treaty_sign_and_abrogate() {
        let mut registry = TreatyRegistry::default();
        let config = TreatyConfig::default();
        registry.treaties.push(Treaty::new(
            "TREATY-000001".to_string(),
            "Test".to_string(),
            vec!["A".to_string(), "B".to_string()],
            vec![TreatyClause::TradePreference],
            1, 100, "A".to_string(),
        ));

        // Sign
        assert!(registry.sign_treaty("TREATY-000001", 5, &config));
        assert_eq!(registry.treaties[0].status, TreatyStatus::Active);
        assert_eq!(registry.treaties[0].signed_turn, Some(5));

        // Abrogate
        let abrogated = registry.abrogate_treaty("TREATY-000001");
        assert!(abrogated.is_some());
        assert_eq!(registry.treaties[0].status, TreatyStatus::Abrogated);
    }

    #[test]
    fn test_treaty_expiry() {
        let mut registry = TreatyRegistry::default();
        let mut t = Treaty::new(
            "TREATY-000001".to_string(),
            "Short Treaty".to_string(),
            vec!["A".to_string(), "B".to_string()],
            vec![TreatyClause::TradePreference],
            1, 10, "A".to_string(),
        );
        t.status = TreatyStatus::Active;
        t.signed_turn = Some(5);
        t.duration_turns = 10;
        registry.treaties.push(t);

        // Not expired at turn 14
        registry.expire_finished_treaties(14);
        assert_eq!(registry.treaties[0].status, TreatyStatus::Active);

        // Expired at turn 15
        registry.expire_finished_treaties(15);
        assert_eq!(registry.treaties[0].status, TreatyStatus::Expired);
    }

    #[test]
    fn test_advance_negotiations() {
        let mut registry = TreatyRegistry::default();
        let config = TreatyConfig::default();
        registry.treaties.push(Treaty::new(
            "TREATY-000001".to_string(),
            "Test".to_string(),
            vec!["A".to_string(), "B".to_string()],
            vec![TreatyClause::TradePreference],
            1, 100, "A".to_string(),
        ));

        let diplomacy = std::collections::HashMap::new();
        let ambassadors = BTreeMap::new();

        // Advance a few turns
        for turn in 2..20 {
            registry.advance_negotiations(turn, &config, &diplomacy, &ambassadors);
            if registry.treaties[0].status == TreatyStatus::Active {
                break;
            }
        }
        // Should eventually become active
        assert_eq!(registry.treaties[0].status, TreatyStatus::Active);
    }
}

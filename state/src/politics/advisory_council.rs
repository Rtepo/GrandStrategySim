//! Phase 48: Advisory councils for authoritarian/royal regimes.
//!
//! Advisory councils represent factions and interest groups in non-democratic
//! regimes. They issue *opinions* on decrees rather than voting on legislation.
//! When a monarch or dictator ignores council advice, faction loyalty drops.
//! Very low loyalty triggers faction-organized revolts (military defection,
//! mass strikes, religious uprisings, aristocratic conspiracies).

use serde::{Deserialize, Serialize};

use crate::politics::ideology::IdeologyCompass;
use crate::politics::vip_registry::VipRegistry;

// ============================================================================
// COUNCIL TYPES
// ============================================================================

/// Type of advisory council, determined by government form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum CouncilType {
    #[default]
    /// Royal council for monarchies — nobles, clergy, military.
    RoyalCouncil,
    /// Party politburo for one-party states — party factions.
    PartyPolitburo,
    /// Military junta for military dictatorships — top generals.
    MilitaryJunta,
    /// Religious synod for theocracies — religious leaders.
    ReligiousSynod,
}

// ============================================================================
// COUNCIL MEMBER
// ============================================================================

/// A faction representative on an authoritarian/royal advisory council.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CouncilMember {
    /// VIP ID of the council member.
    #[serde(default)]
    pub vip_id: String,
    /// Faction/interest group they represent.
    #[serde(default)]
    pub faction: String,
    /// Current loyalty to the ruler (0.0–1.0).
    #[serde(default = "default_loyalty")]
    pub loyalty: f64,
    /// Influence within the council (0–100).
    #[serde(default)]
    pub influence: f64,
    /// Their opinion on the current decree (-1.0 to 1.0).
    #[serde(default)]
    pub current_opinion: f64,
    /// Faction type — determines revolt behavior when loyalty is very low.
    #[serde(default)]
    pub faction_type: FactionType,
}

fn default_loyalty() -> f64 {
    0.7
}

/// Type of faction a council member represents — determines revolt behavior.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum FactionType {
    #[default]
    /// Military faction — military defection on low loyalty.
    Military,
    /// Union/labor faction — mass strikes on low loyalty.
    Labor,
    /// Religious faction — religious uprising on low loyalty.
    Religious,
    /// Nobility/elite faction — aristocratic conspiracy on low loyalty.
    Nobility,
    /// Party faction — political intrigue on low loyalty.
    Party,
    /// Bureaucratic faction — administrative sabotage on low loyalty.
    Bureaucratic,
}

// ============================================================================
// ADVISORY COUNCIL
// ============================================================================

/// Advisory council for authoritarian/royal regimes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdvisoryCouncil {
    /// Council members (one per major faction/interest group).
    #[serde(default)]
    pub members: Vec<CouncilMember>,
    /// Council type.
    #[serde(default)]
    pub council_type: CouncilType,
    /// Current aggregate loyalty (0.0–1.0).
    #[serde(default = "default_loyalty")]
    pub aggregate_loyalty: f64,
    /// Coup risk threshold (when loyalty drops below this, coup risk rises).
    #[serde(default = "default_coup_threshold")]
    pub coup_risk_threshold: f64,
    /// Cooldown: turn until which no new coup/revolt can trigger.
    /// Set to current_turn + 24 after a successful coup/revolt.
    #[serde(default)]
    pub coup_cooldown_until_turn: u32,
}

impl Default for AdvisoryCouncil {
    fn default() -> Self {
        AdvisoryCouncil {
            members: Vec::new(),
            council_type: CouncilType::default(),
            aggregate_loyalty: default_loyalty(),
            coup_risk_threshold: default_coup_threshold(),
            coup_cooldown_until_turn: 0,
        }
    }
}

fn default_coup_threshold() -> f64 {
    0.3
}

impl AdvisoryCouncil {
    /// Create a new advisory council of the given type.
    pub fn new(council_type: CouncilType) -> Self {
        AdvisoryCouncil {
            members: Vec::new(),
            council_type,
            aggregate_loyalty: 0.7,
            coup_risk_threshold: 0.3,
            coup_cooldown_until_turn: 0,
        }
    }

    /// Recalculate aggregate loyalty from member loyalties.
    pub fn recalculate_loyalty(&mut self) {
        if self.members.is_empty() {
            self.aggregate_loyalty = 0.0;
            return;
        }
        let total_influence: f64 = self.members.iter().map(|m| m.influence).sum();
        if total_influence <= 0.0 {
            self.aggregate_loyalty =
                self.members.iter().map(|m| m.loyalty).sum::<f64>() / self.members.len() as f64;
        } else {
            self.aggregate_loyalty = self
                .members
                .iter()
                .map(|m| m.loyalty * (m.influence / total_influence))
                .sum();
        }
    }

    /// Check if the council is in coup cooldown.
    pub fn in_coup_cooldown(&self, current_turn: u32) -> bool {
        current_turn < self.coup_cooldown_until_turn
    }

    /// Check if coup risk is active (loyalty below threshold and not in cooldown).
    pub fn coup_risk_active(&self, current_turn: u32) -> bool {
        self.aggregate_loyalty < self.coup_risk_threshold && !self.in_coup_cooldown(current_turn)
    }

    /// Get members with loyalty below the revolt threshold (0.2).
    pub fn disloyal_members(&self) -> Vec<&CouncilMember> {
        self.members.iter().filter(|m| m.loyalty < 0.2).collect()
    }
}

// ============================================================================
// PHASE 86: COUNCIL INFLUENCE MODIFIERS
// ============================================================================

/// Phase 86: Modifiers derived from council composition that affect existing
/// political and macro variables. All targets are existing `Country` fields —
/// no hallucinated hooks to non-existent engines.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CouncilInfluenceModifier {
    /// Committee delay adjustment (-0.5 to +0.5 turns).
    /// High loyalty → faster decree processing; low loyalty → slower.
    pub decree_speed_modifier: f64,
    /// Executive sign probability adjustment (-0.2 to +0.2).
    /// Low loyalty → higher veto probability.
    pub veto_probability_modifier: f64,
    /// Per-turn delta applied to `country.macro_indicators.social_unrest`.
    /// Positive = increases unrest (military/bureaucratic discontent).
    /// Negative = decreases unrest (religious stabilization).
    pub social_unrest_delta: f64,
    /// Per-turn delta on regional `autonomy_level` (-0.1 to +0.1).
    /// Negative = stabilizes autonomy (loyal nobility).
    /// Positive = destabilizes autonomy (aristocratic intrigue).
    pub autonomy_stabilization: f64,
}

impl AdvisoryCouncil {
    /// Phase 86: Calculate influence modifiers from current council composition.
    ///
    /// Modifiers are derived from per-faction loyalty levels:
    /// - Military faction loyalty < 0.4 → social_unrest_delta increases
    /// - Bureaucratic faction loyalty < 0.4 → social_unrest_delta increases
    /// - Religious faction loyalty > 0.6 → social_unrest_delta decreases (theocracies)
    /// - Nobility faction loyalty < 0.4 → autonomy_stabilization increases (intrigue)
    /// - Aggregate loyalty > 0.7 → decree_speed_modifier positive (faster)
    /// - Aggregate loyalty < 0.4 → veto_probability_modifier positive (more vetoes)
    pub fn calculate_influence_modifiers(&self) -> CouncilInfluenceModifier {
        // Accumulator pattern: fields are built up in the loop below, so we
        // use Default + field reassignment rather than a single initializer.
        #[allow(clippy::field_reassign_with_default)]
        let mut modifier = CouncilInfluenceModifier::default();

        // Decree speed: high aggregate loyalty → faster processing.
        modifier.decree_speed_modifier = ((self.aggregate_loyalty - 0.5) * 1.0).clamp(-0.5, 0.5);

        // Veto probability: low aggregate loyalty → higher veto chance.
        if self.aggregate_loyalty < 0.5 {
            modifier.veto_probability_modifier =
                ((0.5 - self.aggregate_loyalty) * 0.4).clamp(0.0, 0.2);
        }

        // Per-faction effects on social unrest and autonomy.
        for member in &self.members {
            match member.faction_type {
                FactionType::Military => {
                    if member.loyalty < 0.4 {
                        // Military discontent spills into society.
                        modifier.social_unrest_delta += (0.4 - member.loyalty) * 2.0;
                    }
                }
                FactionType::Bureaucratic => {
                    if member.loyalty < 0.4 {
                        // Administrative dysfunction increases unrest.
                        modifier.social_unrest_delta += (0.4 - member.loyalty) * 1.5;
                    }
                }
                FactionType::Religious => {
                    if member.loyalty > 0.6 {
                        // Religious stability reduces unrest.
                        modifier.social_unrest_delta -= (member.loyalty - 0.6) * 1.0;
                    }
                }
                FactionType::Nobility => {
                    if member.loyalty < 0.4 {
                        // Aristocratic intrigue destabilizes regional autonomy.
                        modifier.autonomy_stabilization += (0.4 - member.loyalty) * 0.5;
                    } else if member.loyalty > 0.7 {
                        // Loyal nobility stabilizes regions.
                        modifier.autonomy_stabilization -= (member.loyalty - 0.7) * 0.3;
                    }
                }
                FactionType::Labor | FactionType::Party => {
                    // Labor and party factions affect unrest through existing mechanics.
                    if member.loyalty < 0.3 {
                        modifier.social_unrest_delta += (0.3 - member.loyalty) * 1.0;
                    }
                }
            }
        }

        // Clamp all modifiers to their valid ranges.
        modifier.social_unrest_delta = modifier.social_unrest_delta.clamp(-2.0, 5.0);
        modifier.autonomy_stabilization = modifier.autonomy_stabilization.clamp(-0.1, 0.1);
        modifier
    }

    /// Phase 86: Apply per-turn loyalty drift based on macro variables.
    ///
    /// Loyalty drifts based on:
    /// - GDP growth (positive → loyalty boost)
    /// - Inflation (high → loyalty drain)
    /// - Social unrest (high → loyalty drain)
    /// - Military spending ratio (high → military faction loyalty boost)
    pub fn apply_loyalty_drift(
        &mut self,
        gdp_growth_rate: f64,
        inflation_rate: f64,
        social_unrest: f64,
        military_spending_ratio: f64,
    ) {
        // Normalize macro variables to per-member deltas.
        // GDP growth: +2% → +0.02 loyalty; -2% → -0.02 loyalty.
        let economic_bonus = (gdp_growth_rate / 100.0).clamp(-0.05, 0.05);
        // Inflation: +10% → -0.03 loyalty. Penalty is a positive value subtracted from delta.
        let inflation_penalty = (inflation_rate / 100.0 * 0.3).clamp(0.0, 0.05);
        // Unrest: 50/100 → -0.025 loyalty. Penalty is a positive value subtracted from delta.
        let unrest_penalty = (social_unrest / 100.0 * 0.05).clamp(0.0, 0.05);
        // Military spending: 5% of GDP → +0.02 military faction loyalty.
        let military_bonus = (military_spending_ratio * 0.4).clamp(0.0, 0.03);

        for member in &mut self.members {
            let mut delta = economic_bonus - inflation_penalty - unrest_penalty;
            // Military faction gets additional boost from military spending.
            if member.faction_type == FactionType::Military {
                delta += military_bonus;
            }
            member.loyalty = (member.loyalty + delta).clamp(0.0, 1.0);
        }

        self.recalculate_loyalty();
    }
}

// ============================================================================
// COUNCIL OPINION
// ============================================================================

/// Council opinion on a proposed decree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CouncilOpinion {
    /// Decree description.
    #[serde(default)]
    pub decree: String,
    /// Members who support it (VIP IDs).
    #[serde(default)]
    pub supporters: Vec<String>,
    /// Members who oppose it (VIP IDs).
    #[serde(default)]
    pub opponents: Vec<String>,
    /// Net opinion score (-1.0 to 1.0).
    #[serde(default)]
    pub net_opinion: f64,
    /// Loyalty damage if decree is passed against council opinion.
    #[serde(default)]
    pub loyalty_damage: f64,
}

/// Calculate council opinion on a proposed decree.
///
/// Each member's opinion is based on the ideological alignment between
/// the decree's ideological vector and the member's faction ideology.
pub fn calculate_council_opinion(
    council: &AdvisoryCouncil,
    decree_ideology: &IdeologyCompass,
    vip_registry: &VipRegistry,
) -> CouncilOpinion {
    let mut supporters = Vec::new();
    let mut opponents = Vec::new();
    let mut total_opinion = 0.0;

    for member in &council.members {
        // Get the VIP's ideology compass from their party or faction.
        // For now, use a simplified alignment based on the member's VIP traits.
        let vip = vip_registry.get(&member.vip_id);
        let member_ideology = vip
            .map(|v| ideology_from_string(&v.ideology))
            .unwrap_or_default();

        // Calculate alignment as 1.0 - normalized_manhattan_distance.
        let distance = (decree_ideology.economy - member_ideology.economy).abs()
            + (decree_ideology.liberty - member_ideology.liberty).abs()
            + (decree_ideology.tradition - member_ideology.tradition).abs();
        let alignment = 1.0 - (distance / 2.0);

        if alignment >= 0.5 {
            supporters.push(member.vip_id.clone());
        } else {
            opponents.push(member.vip_id.clone());
        }
        total_opinion += alignment * (member.influence / 100.0);
    }

    let net = if council.members.is_empty() {
        0.0
    } else {
        total_opinion / council.members.len() as f64
    };

    // Loyalty damage proportional to how negative the opinion is.
    let loyalty_damage = if net < 0.0 {
        (-net * 0.15).min(0.15)
    } else {
        0.0
    };

    CouncilOpinion {
        decree: String::new(),
        supporters,
        opponents,
        net_opinion: net,
        loyalty_damage,
    }
}

/// Apply loyalty damage when a decree is passed against council opinion.
///
/// Each opposing member's loyalty drops by the calculated damage.
/// Aggregate loyalty is recalculated.
pub fn apply_decree_against_council(
    council: &mut AdvisoryCouncil,
    opinion: &CouncilOpinion,
) -> Vec<String> {
    let mut messages = Vec::new();

    for member in &mut council.members {
        if opinion.opponents.contains(&member.vip_id) {
            member.loyalty = (member.loyalty - opinion.loyalty_damage).max(0.0);
        }
    }

    council.recalculate_loyalty();

    messages.push(format!(
        "[COUNCIL] Decree passed against opinion (net={:.2}). Loyalty damage: {:.3}. New aggregate: {:.3}",
        opinion.net_opinion, opinion.loyalty_damage, council.aggregate_loyalty
    ));

    messages
}

/// Convert an ideology string to an IdeologyCompass.
/// Simplified mapping for council opinion calculation.
fn ideology_from_string(ideology: &str) -> IdeologyCompass {
    match ideology {
        "OrthodoxMarxism" | "MarxismLeninism" | "Maoism" => IdeologyCompass {
            economy: -0.8,
            liberty: -0.3,
            tradition: -0.5,
        },
        "SocialDemocracy" | "GreenPolitics" => IdeologyCompass {
            economy: -0.4,
            liberty: 0.5,
            tradition: 0.0,
        },
        "ClassicalLiberalism" | "SocialLiberalism" => IdeologyCompass {
            economy: 0.3,
            liberty: 0.7,
            tradition: -0.2,
        },
        "Agrarianism" | "ChristianDemocracy" => IdeologyCompass {
            economy: 0.0,
            liberty: 0.2,
            tradition: 0.5,
        },
        "SocialConservatism" | "Neoconservatism" => IdeologyCompass {
            economy: 0.4,
            liberty: -0.2,
            tradition: 0.6,
        },
        "Neoliberalism" | "NationalConservatism" => IdeologyCompass {
            economy: 0.6,
            liberty: 0.0,
            tradition: 0.4,
        },
        "AnarchoCapitalism" => IdeologyCompass {
            economy: 0.9,
            liberty: 0.8,
            tradition: -0.3,
        },
        "Fascism" => IdeologyCompass {
            economy: 0.2,
            liberty: -0.8,
            tradition: 0.7,
        },
        _ => IdeologyCompass {
            economy: 0.0,
            liberty: 0.0,
            tradition: 0.0,
        },
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::Vip;

    #[test]
    fn test_advisory_council_default() {
        let council = AdvisoryCouncil::default();
        assert_eq!(council.council_type, CouncilType::RoyalCouncil);
        assert!((council.aggregate_loyalty - 0.7).abs() < 1e-6);
        assert!((council.coup_risk_threshold - 0.3).abs() < 1e-6);
    }

    #[test]
    fn test_recalculate_loyalty_empty() {
        let mut council = AdvisoryCouncil::new(CouncilType::MilitaryJunta);
        council.recalculate_loyalty();
        assert_eq!(council.aggregate_loyalty, 0.0);
    }

    #[test]
    fn test_recalculate_loyalty_weighted() {
        let mut council = AdvisoryCouncil::new(CouncilType::RoyalCouncil);
        council.members.push(CouncilMember {
            vip_id: "VIP-001".to_string(),
            loyalty: 0.8,
            influence: 50.0,
            ..Default::default()
        });
        council.members.push(CouncilMember {
            vip_id: "VIP-002".to_string(),
            loyalty: 0.4,
            influence: 50.0,
            ..Default::default()
        });
        council.recalculate_loyalty();
        assert!(
            (council.aggregate_loyalty - 0.6).abs() < 1e-6,
            "Weighted average should be 0.6"
        );
    }

    #[test]
    fn test_coup_risk_active_below_threshold() {
        let mut council = AdvisoryCouncil::new(CouncilType::MilitaryJunta);
        council.aggregate_loyalty = 0.2;
        assert!(council.coup_risk_active(10));
    }

    #[test]
    fn test_coup_risk_inactive_above_threshold() {
        let mut council = AdvisoryCouncil::new(CouncilType::MilitaryJunta);
        council.aggregate_loyalty = 0.5;
        assert!(!council.coup_risk_active(10));
    }

    #[test]
    fn test_coup_cooldown_blocks_risk() {
        let mut council = AdvisoryCouncil::new(CouncilType::MilitaryJunta);
        council.aggregate_loyalty = 0.1; // Very low
        council.coup_cooldown_until_turn = 30;
        assert!(
            !council.coup_risk_active(10),
            "Cooldown should block coup risk"
        );
        assert!(
            council.coup_risk_active(35),
            "After cooldown, coup risk active"
        );
    }

    #[test]
    fn test_disloyal_members() {
        let mut council = AdvisoryCouncil::new(CouncilType::RoyalCouncil);
        council.members.push(CouncilMember {
            vip_id: "VIP-001".to_string(),
            loyalty: 0.1,
            ..Default::default()
        });
        council.members.push(CouncilMember {
            vip_id: "VIP-002".to_string(),
            loyalty: 0.5,
            ..Default::default()
        });
        council.members.push(CouncilMember {
            vip_id: "VIP-003".to_string(),
            loyalty: 0.15,
            ..Default::default()
        });
        let disloyal = council.disloyal_members();
        assert_eq!(disloyal.len(), 2);
    }

    #[test]
    fn test_apply_decree_against_council_damages_loyalty() {
        let mut council = AdvisoryCouncil::new(CouncilType::RoyalCouncil);
        council.members.push(CouncilMember {
            vip_id: "VIP-001".to_string(),
            loyalty: 0.8,
            influence: 50.0,
            ..Default::default()
        });
        council.members.push(CouncilMember {
            vip_id: "VIP-002".to_string(),
            loyalty: 0.6,
            influence: 50.0,
            ..Default::default()
        });

        let opinion = CouncilOpinion {
            decree: "Raise taxes".to_string(),
            supporters: vec!["VIP-001".to_string()],
            opponents: vec!["VIP-002".to_string()],
            net_opinion: -0.3,
            loyalty_damage: 0.10,
        };

        let msgs = apply_decree_against_council(&mut council, &opinion);
        assert!(!msgs.is_empty());

        // VIP-001 (supporter) should not lose loyalty.
        // VIP-002 (opponent) should lose 0.10 loyalty.
        assert!(
            (council.members[0].loyalty - 0.8).abs() < 1e-6,
            "Supporter keeps loyalty"
        );
        assert!(
            (council.members[1].loyalty - 0.5).abs() < 1e-6,
            "Opponent loses 0.10 loyalty"
        );
    }

    #[test]
    fn test_calculate_council_opinion() {
        let mut council = AdvisoryCouncil::new(CouncilType::RoyalCouncil);
        council.members.push(CouncilMember {
            vip_id: "VIP-001".to_string(),
            loyalty: 0.8,
            influence: 50.0,
            ..Default::default()
        });

        let mut registry = VipRegistry::new();
        registry.register_new(Vip {
            id: "VIP-001".to_string(),
            full_name: "Test Noble".to_string(),
            ideology: "SocialConservatism".to_string(),
            ..Default::default()
        });

        let decree_ideology = IdeologyCompass {
            economy: 0.4,
            liberty: -0.2,
            tradition: 0.6,
        };

        let opinion = calculate_council_opinion(&council, &decree_ideology, &registry);
        // Decree ideology matches SocialConservatism → high alignment → supporter.
        assert!(opinion.supporters.contains(&"VIP-001".to_string()));
        assert!(opinion.net_opinion > 0.0);
    }

    #[test]
    fn test_ideology_from_string_marxism() {
        let compass = ideology_from_string("MarxismLeninism");
        assert!(compass.economy < 0.0, "Marxism should be left-wing economy");
    }

    #[test]
    fn test_ideology_from_string_fascism() {
        let compass = ideology_from_string("Fascism");
        assert!(compass.liberty < 0.0, "Fascism should be authoritarian");
        assert!(compass.tradition > 0.0, "Fascism should be traditionalist");
    }

    #[test]
    fn test_ideology_from_string_unknown() {
        let compass = ideology_from_string("UnknownIdeology");
        assert_eq!(compass.economy, 0.0);
        assert_eq!(compass.liberty, 0.0);
        assert_eq!(compass.tradition, 0.0);
    }
}

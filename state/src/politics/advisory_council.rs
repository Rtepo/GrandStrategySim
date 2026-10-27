//! Phase 48: Advisory councils for authoritarian/royal regimes.
//!
//! Advisory councils represent factions and interest groups in non-democratic
//! regimes. They issue *opinions* on decrees rather than voting on legislation.
//! When a monarch or dictator ignores council advice, faction loyalty drops.
//! Very low loyalty triggers faction-organized revolts (military defection,
//! mass strikes, religious uprisings, aristocratic conspiracies).

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::politics::ideology::IdeologyCoordinates;
use crate::politics::system::GovernmentForm;
use crate::politics::vip_registry::VipRegistry;
use crate::state::Country;

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
// v4.23.0 WS2: DECREES & PETITIONS — the autocratic legislative record
// ============================================================================

/// v4.23.0 WS2: A decree issued by the ruler of a non-elected regime.
/// For chamberless autocracies decrees ARE the legislation: each is laid
/// before the advisory council, which renders a real faction-weighted
/// `CouncilOpinion` from the decree's ideological signature; the ruler then
/// promulgates it — sometimes over the council's objection, which damages
/// faction loyalty — or withdraws it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CouncilDecree {
    /// Stable id (`DECREE-####`), unique within the council.
    #[serde(default)]
    pub id: String,
    /// Decree title (e.g. "Royal Decree on Labor Standards").
    #[serde(default)]
    pub title: String,
    /// Policy domain the decree legislates (`BillType` debug label).
    #[serde(default)]
    pub domain: String,
    /// Turn the decree was issued.
    #[serde(default)]
    pub issued_turn: u32,
    /// Council net opinion at issue, -1.0 (hostile) .. 1.0 (favorable) —
    /// the real `calculate_council_opinion` output for the bill signature.
    #[serde(default)]
    pub net_opinion: f64,
    /// VIP ids of council members who supported the decree.
    #[serde(default)]
    pub supporters: Vec<String>,
    /// VIP ids of council members who opposed the decree.
    #[serde(default)]
    pub opponents: Vec<String>,
    /// "Promulgated" | "Promulgated over Council Objection" | "Withdrawn".
    #[serde(default)]
    pub status: String,
}

/// v4.23.0 WS2: A petition carried to the ruler by a council member whose
/// faction loyalty has eroded — the autocratic counterpart of a democratic
/// opposition's floor motions. Ignored or rejected petitions push loyalty
/// toward the coup threshold; granted ones buy the regime time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CouncilPetition {
    /// Stable id (`PET-####`), unique within the council.
    #[serde(default)]
    pub id: String,
    /// Display label of the faction that filed it.
    #[serde(default)]
    pub faction: String,
    /// VIP id of the council member who carried the petition.
    #[serde(default)]
    pub petitioner_vip_id: String,
    /// Petition subject (faction-specific grievance).
    #[serde(default)]
    pub subject: String,
    /// Turn filed.
    #[serde(default)]
    pub filed_turn: u32,
    /// Loyalty deficit that motivated the petition (0.0–1.0).
    #[serde(default)]
    pub urgency: f64,
    /// "Pending" | "Granted" | "Rejected" | "Ignored".
    #[serde(default)]
    pub status: String,
    /// Turn the petition was resolved (None while pending).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_turn: Option<u32>,
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
    /// v4.23.0 WS2: Decrees the ruler has issued — for chamberless
    /// autocracies decrees ARE the legislation; the council renders a
    /// faction-weighted opinion on each (newest first, capped at 24).
    #[serde(default)]
    pub decrees: Vec<CouncilDecree>,
    /// v4.23.0 WS2: Petitions factions carry to the ruler when their loyalty
    /// erodes (newest first, capped at 24).
    #[serde(default)]
    pub petitions: Vec<CouncilPetition>,
    /// v4.23.0 WS2: Monotone decree id counter (`DECREE-####`).
    #[serde(default)]
    pub next_decree_id: u32,
    /// v4.23.0 WS2: Monotone petition id counter (`PET-####`).
    #[serde(default)]
    pub next_petition_id: u32,
}

impl Default for AdvisoryCouncil {
    fn default() -> Self {
        AdvisoryCouncil {
            members: Vec::new(),
            council_type: CouncilType::default(),
            aggregate_loyalty: default_loyalty(),
            coup_risk_threshold: default_coup_threshold(),
            coup_cooldown_until_turn: 0,
            decrees: Vec::new(),
            petitions: Vec::new(),
            next_decree_id: 1,
            next_petition_id: 1,
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
            decrees: Vec::new(),
            petitions: Vec::new(),
            next_decree_id: 1,
            next_petition_id: 1,
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
    decree_ideology: &IdeologyCoordinates,
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

/// Convert an ideology string to an IdeologyCoordinates.
/// Simplified mapping for council opinion calculation.
/// v4.23.0 WS2: whitespace-insensitive — VIP records carry both enum-debug
/// ("NationalConservatism") and display ("National Conservatism") spellings.
fn ideology_from_string(ideology: &str) -> IdeologyCoordinates {
    let key: String = ideology.chars().filter(|c| !c.is_whitespace()).collect();
    match key.as_str() {
        "OrthodoxMarxism" | "MarxismLeninism" | "Maoism" => IdeologyCoordinates {
            economy: -0.8,
            liberty: -0.3,
            tradition: -0.5,
        },
        "SocialDemocracy" | "GreenPolitics" => IdeologyCoordinates {
            economy: -0.4,
            liberty: 0.5,
            tradition: 0.0,
        },
        "ClassicalLiberalism" | "SocialLiberalism" => IdeologyCoordinates {
            economy: 0.3,
            liberty: 0.7,
            tradition: -0.2,
        },
        "Agrarianism" | "ChristianDemocracy" => IdeologyCoordinates {
            economy: 0.0,
            liberty: 0.2,
            tradition: 0.5,
        },
        "SocialConservatism" | "Neoconservatism" => IdeologyCoordinates {
            economy: 0.4,
            liberty: -0.2,
            tradition: 0.6,
        },
        "Neoliberalism" | "NationalConservatism" => IdeologyCoordinates {
            economy: 0.6,
            liberty: 0.0,
            tradition: 0.4,
        },
        "AnarchoCapitalism" => IdeologyCoordinates {
            economy: 0.9,
            liberty: 0.8,
            tradition: -0.3,
        },
        "Fascism" => IdeologyCoordinates {
            economy: 0.2,
            liberty: -0.8,
            tradition: 0.7,
        },
        _ => IdeologyCoordinates {
            economy: 0.0,
            liberty: 0.0,
            tradition: 0.0,
        },
    }
}

// ============================================================================
// v4.23.0 WS2: COUNCIL SEEDING, DECREES & PETITIONS
// ============================================================================
//
// The autocratic counterpart of the democratic floor machinery: instead of
// elections → seats → floor votes, non-elected regimes get factions → named
// councilors → decrees (the legislation) + petitions (the grievance channel).
// Everything below runs on real state — councilor VIPs are registered in the
// country's `VipRegistry`, decree opinions come from the existing
// `calculate_council_opinion` machinery, petition pressure comes from the
// real per-member loyalty that `apply_loyalty_drift` maintains.

/// Map a government form to the advisory-council type that serves it.
/// Returns `None` for regimes that legislate exclusively through elected
/// chambers — monarchies with a rubber-stamp chamber still keep their
/// royal council, so dualist and elective monarchies map to `RoyalCouncil`.
pub fn council_type_for(form: GovernmentForm) -> Option<CouncilType> {
    use GovernmentForm::*;
    match form {
        AbsoluteMonarchy | DualistMonarchy | ElectiveMonarchy => {
            Some(CouncilType::RoyalCouncil)
        }
        MilitaryDictatorship => Some(CouncilType::MilitaryJunta),
        Theocracy => Some(CouncilType::ReligiousSynod),
        OnePartyState => Some(CouncilType::PartyPolitburo),
        _ => None,
    }
}

/// Display name of a council type — log lines and the UI both use it.
pub fn council_type_name(council_type: &CouncilType) -> &'static str {
    match council_type {
        CouncilType::RoyalCouncil => "Royal Council",
        CouncilType::PartyPolitburo => "Politburo",
        CouncilType::MilitaryJunta => "Junta",
        CouncilType::ReligiousSynod => "Supreme Religious Council",
    }
}

/// Honorific prefix for decree titles per council type.
fn decree_prefix(council_type: &CouncilType) -> &'static str {
    match council_type {
        CouncilType::RoyalCouncil => "Royal Decree",
        CouncilType::PartyPolitburo => "Politburo Resolution",
        CouncilType::MilitaryJunta => "Junta Directive",
        CouncilType::ReligiousSynod => "Holy Ordinance",
    }
}

/// Faction roster per council type: `(faction_type, display label, seats)`.
/// Rosters are fixed per regime so faction composition is stable across
/// turns once seeded.
fn faction_roster(council_type: &CouncilType) -> &'static [(FactionType, &'static str, u32)] {
    match council_type {
        CouncilType::RoyalCouncil => &[
            (FactionType::Nobility, "Nobility", 2),
            (FactionType::Military, "Army", 1),
            (FactionType::Religious, "Clergy", 1),
            (FactionType::Bureaucratic, "Civil Service", 1),
        ],
        CouncilType::MilitaryJunta => &[
            (FactionType::Military, "Army", 2),
            (FactionType::Military, "Navy & Air Force", 1),
            (FactionType::Bureaucratic, "Civil Service", 1),
            (FactionType::Party, "Government Party", 1),
        ],
        CouncilType::ReligiousSynod => &[
            (FactionType::Religious, "High Clergy", 2),
            (FactionType::Religious, "Monastic Orders", 1),
            (FactionType::Bureaucratic, "Temple Administration", 1),
        ],
        CouncilType::PartyPolitburo => &[
            (FactionType::Party, "Party Old Guard", 2),
            (FactionType::Party, "Reformist Wing", 1),
            (FactionType::Bureaucratic, "State Apparatus", 1),
            (FactionType::Labor, "Trade Unions", 1),
        ],
    }
}

/// Faction grievance — what the faction petitions the ruler about.
fn petition_subject(faction_type: &FactionType) -> &'static str {
    match faction_type {
        FactionType::Military => "Increased defense appropriations",
        FactionType::Labor => "Wage and bread relief for the workers",
        FactionType::Religious => "Strengthened clerical authority",
        FactionType::Nobility => "Restoration of estate privileges",
        FactionType::Party => "Expanded party cadre appointments",
        FactionType::Bureaucratic => "Civil service funding and appointments",
    }
}

/// Ideology label stamped on a seeded councilor's VIP record — the same
/// debug-form strings `ideology_from_string` understands, so decree opinion
/// alignment is meaningful per faction.
fn faction_ideology(faction_type: &FactionType) -> String {
    match faction_type {
        FactionType::Military => "NationalConservatism".to_string(),
        FactionType::Labor => "SocialDemocracy".to_string(),
        FactionType::Religious => "SocialConservatism".to_string(),
        FactionType::Nobility => "Neoconservatism".to_string(),
        FactionType::Party => "NationalConservatism".to_string(),
        FactionType::Bureaucratic => "ChristianDemocracy".to_string(),
    }
}

/// Registry role assigned to a seeded councilor.
fn faction_role(
    faction_type: &FactionType,
) -> crate::politics::vip_registry::VipRoleExtended {
    use crate::politics::vip_registry::VipRoleExtended as Role;
    match faction_type {
        FactionType::Military => Role::MilitaryCommander,
        FactionType::Labor => Role::UnionBoss,
        FactionType::Religious => Role::ReligiousLeader,
        _ => Role::Minister,
    }
}

/// v4.23.0 WS2: Lazily ensure the country's advisory council exists and is
/// populated. Called at the top of `process_advisory_council_turn`, so
/// councils populate on saves that predate council bootstrap or where the
/// bootstrap seeding produced an empty member list. No-op for regimes that
/// have no council type.
pub fn ensure_advisory_council(
    country: &mut Country,
    current_turn: u32,
    messages: &mut Vec<String>,
) {
    if country
        .politics
        .advisory_council
        .as_ref()
        .map(|c| !c.members.is_empty())
        .unwrap_or(false)
    {
        return; // Populated already.
    }
    let Some(council_type) = council_type_for(country.politics.government_form) else {
        return;
    };
    seed_council_members(country, council_type, current_turn, messages);
}

/// Seed a full council roster — one real registered VIP per roster seat.
fn seed_council_members(
    country: &mut Country,
    council_type: CouncilType,
    current_turn: u32,
    messages: &mut Vec<String>,
) {
    use crate::politics::vip_registry::{assign_core_traits, Vip};

    let cultural_group = if country.macro_indicators.cultural_group.is_empty() {
        "western".to_string()
    } else {
        country.macro_indicators.cultural_group.clone()
    };
    let country_name = country.name.clone();

    // Councillors are registered VIPs — the registry must exist.
    if country.politics.vip_registry.is_none() {
        country.politics.vip_registry = Some(VipRegistry::new());
    }

    let politics = &mut country.politics;
    let council = {
        let registry = politics.vip_registry.as_mut().unwrap();
        // Seed the dedup set with every name already taken so councilors
        // never collide with monarchs, ministers or party leaders.
        let mut used_names: HashSet<String> = registry
            .vips
            .values()
            .map(|v| v.full_name.clone())
            .chain(registry.deceased.iter().map(|v| v.full_name.clone()))
            .collect();
        let mut rng = crate::engine::seeded_rng::thread_rng();

        let mut members = Vec::new();
        for (faction_type, label, count) in faction_roster(&council_type) {
            for _ in 0..*count {
                let name = crate::politics::names::generate_key_vip(
                    &cultural_group,
                    &mut rng,
                    &mut used_names,
                );
                let (traits, main_trait) = assign_core_traits(&mut rng);
                let age = 40 + rng.gen_range(0..28);
                let influence = 30.0 + rng.gen_range(0..45) as f64;
                let vip = Vip {
                    full_name: name.full_name,
                    gender: name.gender,
                    age,
                    traits,
                    main_trait,
                    ideology: faction_ideology(faction_type),
                    nationality: country_name.clone(),
                    roles: vec![faction_role(faction_type)],
                    base_influence: influence as u32,
                    ..Default::default()
                };
                let vip_id = registry.register_new(vip);
                members.push(CouncilMember {
                    vip_id,
                    faction: (*label).to_string(),
                    // New councilors start loyal-ish; drift does the rest.
                    loyalty: 0.55 + rng.gen_range(0..=25) as f64 / 100.0,
                    influence,
                    current_opinion: 0.0,
                    faction_type: faction_type.clone(),
                });
            }
        }

        let mut council = politics
            .advisory_council
            .take()
            .unwrap_or_else(|| AdvisoryCouncil::new(council_type));
        council.members = members;
        council.recalculate_loyalty();
        council
    };
    politics.advisory_council = Some(council);
    if let Some(council) = politics.advisory_council.as_ref() {
        messages.push(format!(
            "[COUNCIL] {} convened with {} councilors (T{}).",
            council_type_name(&council.council_type),
            council.members.len(),
            current_turn
        ));
    }
}

/// v4.23.0 WS2: Per-turn council legislature — the chamberless counterpart
/// of `process_bill_stage`. Runs three phases on real state:
/// 1. Resolve pending petitions (grant / reject / age-out to Ignored).
/// 2. Disloyal councilors file new petitions (prob ∝ loyalty deficit).
/// 3. On cadence, the ruler issues a decree the council opines on through
///    the existing `calculate_council_opinion` machinery.
pub fn process_council_legislation(
    country: &mut Country,
    current_turn: u32,
    messages: &mut Vec<String>,
) {
    // The decree agenda is an immutable read of country macro state — take
    // it before the mutable council borrow below.
    let agenda = crate::politics::bill_lifecycle::policy_agenda_for_country(country);
    let politics = &mut country.politics;
    let Some(council) = politics.advisory_council.as_mut() else {
        return;
    };
    let Some(registry) = politics.vip_registry.as_ref() else {
        return;
    };

    resolve_petitions(council, current_turn, messages);
    spawn_petitions(council, current_turn, messages);
    maybe_issue_decree(council, registry, &agenda, current_turn, messages);
}

/// Phase 1: resolve pending petitions — grant (loyalty +), reject
/// (loyalty −), or let them age out after 6 turns (Ignored, loyalty −).
fn resolve_petitions(
    council: &mut AdvisoryCouncil,
    current_turn: u32,
    messages: &mut Vec<String>,
) {
    let coup_active = council.coup_risk_active(current_turn);
    // Member loyalty effects are applied after the petition scan so the
    // two field borrows never overlap.
    let mut loyalty_effects: Vec<(String, f64)> = Vec::new();
    for petition in &mut council.petitions {
        if petition.status != "Pending" {
            continue;
        }
        // Filed this turn — stays pending at least one turn.
        if petition.filed_turn >= current_turn {
            continue;
        }
        if current_turn.saturating_sub(petition.filed_turn) > 6 {
            petition.status = "Ignored".to_string();
            petition.resolved_turn = Some(current_turn);
            loyalty_effects.push((petition.petitioner_vip_id.clone(), -0.03));
            continue;
        }
        let petitioner_influence = council
            .members
            .iter()
            .find(|m| m.vip_id == petition.petitioner_vip_id)
            .map(|m| m.influence)
            .unwrap_or(50.0);
        // Influential petitioners and petitioners holding a knife at the
        // throne (coup risk active) get heard more often.
        let grant_prob = (0.25
            + petitioner_influence / 100.0 * 0.35
            + if coup_active { 0.25 } else { 0.0 })
        .clamp(0.05, 0.9);
        let seed = format!("petition:{}:{}:resolve", petition.id, current_turn);
        if crate::politics::bill_lifecycle::deterministic_roll(&seed, grant_prob) {
            petition.status = "Granted".to_string();
            petition.resolved_turn = Some(current_turn);
            loyalty_effects.push((petition.petitioner_vip_id.clone(), 0.10));
            messages.push(format!(
                "[COUNCIL] Petition {} ({}) GRANTED.",
                petition.id, petition.faction
            ));
        } else {
            petition.status = "Rejected".to_string();
            petition.resolved_turn = Some(current_turn);
            loyalty_effects.push((petition.petitioner_vip_id.clone(), -0.05));
            messages.push(format!(
                "[COUNCIL] Petition {} ({}) REJECTED.",
                petition.id, petition.faction
            ));
        }
    }
    for (vip_id, delta) in loyalty_effects {
        if let Some(member) = council.members.iter_mut().find(|m| m.vip_id == vip_id) {
            member.loyalty = (member.loyalty + delta).clamp(0.0, 1.0);
        }
    }
    if !council.members.is_empty() {
        council.recalculate_loyalty();
    }
}

/// Phase 2: disloyal councilors (loyalty < 0.45) who don't already have a
/// pending petition file one — probability proportional to the deficit.
fn spawn_petitions(
    council: &mut AdvisoryCouncil,
    current_turn: u32,
    messages: &mut Vec<String>,
) {
    let candidates: Vec<(String, String, f64, FactionType)> = council
        .members
        .iter()
        .filter(|m| m.loyalty < 0.45)
        .filter(|m| {
            !council
                .petitions
                .iter()
                .any(|p| p.status == "Pending" && p.petitioner_vip_id == m.vip_id)
        })
        .filter(|m| {
            let prob = (0.45 - m.loyalty) * 0.9;
            crate::politics::bill_lifecycle::deterministic_roll(
                &format!("petition:{}:{}:file", m.vip_id, current_turn),
                prob,
            )
        })
        .map(|m| {
            (
                m.vip_id.clone(),
                m.faction.clone(),
                m.loyalty,
                m.faction_type.clone(),
            )
        })
        .collect();

    for (vip_id, faction, loyalty, faction_type) in candidates {
        let subject = petition_subject(&faction_type).to_string();
        let id = format!("PET-{:04}", council.next_petition_id);
        council.next_petition_id += 1;
        council.petitions.insert(
            0,
            CouncilPetition {
                id: id.clone(),
                faction: faction.clone(),
                petitioner_vip_id: vip_id,
                subject: subject.clone(),
                filed_turn: current_turn,
                urgency: (0.45 - loyalty).clamp(0.0, 1.0),
                status: "Pending".to_string(),
                resolved_turn: None,
            },
        );
        messages.push(format!(
            "[COUNCIL] Petition {} filed by the {} — {}.",
            id, faction, subject
        ));
    }
    council.petitions.truncate(24);
}

/// Phase 3: on cadence (~every 6 turns) the ruler issues a decree drawn
/// from the same macro-weighted policy agenda a parliament would face —
/// the council renders a real faction-weighted opinion, and promulgation
/// over objection damages loyalty through `apply_decree_against_council`.
fn maybe_issue_decree(
    council: &mut AdvisoryCouncil,
    registry: &VipRegistry,
    agenda: &[(
        crate::politics::legislation::BillType,
        &'static str,
        IdeologyCoordinates,
        f64,
    )],
    current_turn: u32,
    messages: &mut Vec<String>,
) {
    // Decree cadence: once every six turns (≈quarterly on the 24-turn year).
    if current_turn % 6 != 2 {
        return;
    }
    if council.members.is_empty() || agenda.is_empty() {
        return;
    }

    // Weighted draw over the agenda by macro urgency — deterministic hash,
    // no RNG state, so replays reproduce the same decree sequence.
    let total: f64 = agenda.iter().map(|d| d.3.max(0.01)).sum();
    let roll = crate::politics::bill_lifecycle::deterministic_uniform(&format!(
        "decree:{}:{}:domain",
        current_turn, council.next_decree_id
    )) * total;
    let mut picked = &agenda[0];
    let mut acc = 0.0;
    for domain in agenda {
        acc += domain.3.max(0.01);
        if roll <= acc {
            picked = domain;
            break;
        }
    }

    let mut opinion = calculate_council_opinion(council, &picked.2, registry);
    opinion.decree = picked.1.to_string();

    let status = if opinion.net_opinion < -0.05 {
        // A hostile council is overridden three times out of four — the
        // override path runs the real loyalty-damage machinery.
        let seed = format!("decree:{}:{}:override", current_turn, council.next_decree_id);
        if crate::politics::bill_lifecycle::deterministic_roll(&seed, 0.75) {
            messages.extend(apply_decree_against_council(council, &opinion));
            "Promulgated over Council Objection"
        } else {
            "Withdrawn"
        }
    } else {
        "Promulgated"
    };

    let id = format!("DECREE-{:04}", council.next_decree_id);
    council.next_decree_id += 1;
    let title = format!(
        "{} on {}",
        decree_prefix(&council.council_type),
        picked.1.trim_end_matches(" Act")
    );
    messages.push(format!(
        "[COUNCIL] {} issued — {} (net opinion {:+.2}).",
        id, title, opinion.net_opinion
    ));
    council.decrees.insert(
        0,
        CouncilDecree {
            id,
            title,
            domain: format!("{:?}", picked.0),
            issued_turn: current_turn,
            net_opinion: opinion.net_opinion,
            supporters: opinion.supporters,
            opponents: opinion.opponents,
            status: status.to_string(),
        },
    );
    council.decrees.truncate(24);
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

        let decree_ideology = IdeologyCoordinates {
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

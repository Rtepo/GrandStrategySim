//! Phase 68: Evolving International Organizations.
//!
//! Meta-state organizations with budgets, parliaments, evolving integration
//! tiers, and directives. The World Forum is spawned at world generation as
//! a neutral platform. All other organizations form dynamically via treaties.
//!
//! Organizations can enact economic sanctions against bad-faith actors:
//! - TradeEmbargo: blocks target from GlobalMarket
//! - AssetFreeze: freezes target's foreign BrokerageAccounts
//! - FinancialIsolation: blocks aid and investment flows
//! - FullEmbargo: all three combined

use crate::state::Treasury;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// Integration level of an international organization.
/// Organizations evolve from loose trade areas to political unions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum IntegrationLevel {
    /// Loose free trade area — tariff reduction only.
    #[default]
    FreeTradeArea,
    /// Customs union — common external tariffs.
    CustomsUnion,
    /// Common market — free movement of goods, labor, capital.
    CommonMarket,
    /// Economic union — harmonized regulations and fiscal policy.
    EconomicUnion,
    /// Political union — pooled sovereignty, common foreign policy.
    PoliticalUnion,
}

impl IntegrationLevel {
    /// Returns a human-readable label.
    pub fn as_str(&self) -> &'static str {
        match self {
            IntegrationLevel::FreeTradeArea => "Free Trade Area",
            IntegrationLevel::CustomsUnion => "Customs Union",
            IntegrationLevel::CommonMarket => "Common Market",
            IntegrationLevel::EconomicUnion => "Economic Union",
            IntegrationLevel::PoliticalUnion => "Political Union",
        }
    }

    /// Returns the ordinal for comparison (higher = more integrated).
    pub fn ordinal(&self) -> u8 {
        match self {
            IntegrationLevel::FreeTradeArea => 0,
            IntegrationLevel::CustomsUnion => 1,
            IntegrationLevel::CommonMarket => 2,
            IntegrationLevel::EconomicUnion => 3,
            IntegrationLevel::PoliticalUnion => 4,
        }
    }

    /// Advances to the next integration level, if possible.
    pub fn advance(&self) -> Option<IntegrationLevel> {
        match self {
            IntegrationLevel::FreeTradeArea => Some(IntegrationLevel::CustomsUnion),
            IntegrationLevel::CustomsUnion => Some(IntegrationLevel::CommonMarket),
            IntegrationLevel::CommonMarket => Some(IntegrationLevel::EconomicUnion),
            IntegrationLevel::EconomicUnion => Some(IntegrationLevel::PoliticalUnion),
            IntegrationLevel::PoliticalUnion => None,
        }
    }
}

/// Voting mechanism for organization decisions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum VotingMechanism {
    /// Every member must agree (used by World Forum).
    #[default]
    Unanimity,
    /// Qualified majority — requires threshold fraction of votes.
    QualifiedMajority {
        /// Fraction of votes required (0.0 to 1.0).
        threshold: f64,
    },
    /// Simple majority — more than 50% of votes.
    SimpleMajority,
}

impl VotingMechanism {
    /// Returns a human-readable label.
    pub fn as_str(&self) -> &'static str {
        match self {
            VotingMechanism::Unanimity => "Unanimity",
            VotingMechanism::QualifiedMajority { .. } => "Qualified Majority",
            VotingMechanism::SimpleMajority => "Simple Majority",
        }
    }

    /// Checks if a vote passes given the fraction of yes votes.
    pub fn passes(&self, yes_fraction: f64) -> bool {
        match self {
            VotingMechanism::Unanimity => yes_fraction >= 1.0,
            VotingMechanism::QualifiedMajority { threshold } => yes_fraction >= *threshold,
            VotingMechanism::SimpleMajority => yes_fraction > 0.5,
        }
    }
}

/// Council member — one representative per member state with veto power.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CouncilMember {
    /// Country name this representative belongs to.
    pub country: String,
    /// Representative VIP name (optional — may not be assigned yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub representative: Option<String>,
    /// Whether this member has veto power (typically all members do in Unanimity).
    pub has_veto: bool,
}

/// The council of an international organization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OrgCouncil {
    /// One member per member state.
    pub members: Vec<CouncilMember>,
}

impl OrgCouncil {
    /// Creates a council with all members having veto power.
    pub fn from_members(countries: &[String]) -> Self {
        Self {
            members: countries
                .iter()
                .map(|c| CouncilMember {
                    country: c.clone(),
                    representative: None,
                    has_veto: true,
                })
                .collect(),
        }
    }

    /// Adds a member state to the council.
    pub fn add_member(&mut self, country: &str) {
        if !self.members.iter().any(|m| m.country == country) {
            self.members.push(CouncilMember {
                country: country.to_string(),
                representative: None,
                has_veto: true,
            });
        }
    }

    /// Removes a member state from the council.
    pub fn remove_member(&mut self, country: &str) {
        self.members.retain(|m| m.country != country);
    }
}

/// Parliament seat allocation for an international organization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OrgParliament {
    /// Seats allocated per country (country -> seat count).
    pub seats: BTreeMap<String, u32>,
    /// Currently proposed directives awaiting vote.
    pub proposed_directives: Vec<Directive>,
    /// Passed directives.
    pub passed_directives: Vec<Directive>,
}

impl OrgParliament {
    /// Allocates seats proportional to population (1 seat per `seats_per_million` million people, min 1).
    pub fn allocate_seats(&mut self, populations: &BTreeMap<String, u64>, seats_per_million: f64) {
        self.seats.clear();
        for (country, pop) in populations {
            let seats = ((*pop as f64) / 1_000_000.0 * seats_per_million).ceil() as u32;
            self.seats.insert(country.clone(), seats.max(1));
        }
    }

    /// Returns total seat count.
    pub fn total_seats(&self) -> u32 {
        self.seats.values().sum()
    }
}

/// A directive issued by an organization's parliament.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Directive {
    /// Unique directive ID.
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Mandate type (funded or unfunded).
    pub mandate_type: MandateType,
    /// Turn by which member states must comply.
    pub compliance_deadline: u32,
    /// Fine for non-compliance (debited from member treasury, credited to org).
    pub fine_for_noncompliance: f64,
    /// Target law type that members must enact (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_law: Option<String>,
    /// Turn the directive was enacted.
    pub enacted_turn: u32,
    /// v4.22.1 WS5.2: per-country vote record (member name → "For"/"Against"/
    /// "Abstain") captured when the resolution was debated — powers the
    /// International Assembly panel's roll call.
    #[serde(default)]
    pub votes: std::collections::BTreeMap<String, String>,
    /// v4.22.1 WS5.2: outcome — "Enacted" | "Rejected". Legacy saves default
    /// to Enacted (all pre-existing directives were live).
    #[serde(default = "default_directive_status")]
    pub status: String,
    /// v4.25.0 WS7: per-member compliance record — each member starts
    /// "Pending" inside the compliance window; `enforce_directives` marks
    /// holdouts "Fined — non-compliance" and For-voters "Compliant —
    /// ratified" (the roll call is the ratification record).
    #[serde(default)]
    pub member_compliance: std::collections::BTreeMap<String, String>,
    /// v4.25.0 WS7: assembly debate log — tabling, roll-call summary, and
    /// the outcome with its statutory terms.
    #[serde(default)]
    pub debate_log: Vec<String>,
    /// v4.25.0 WS7: turn enforcement ran (fines issued / compliance
    /// confirmed). None while still inside the compliance window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enforcement_turn: Option<u32>,
    /// v4.27.0 (TASK-008): full-text description of the resolution —
    /// what it is and why the assembly tabled it.
    #[serde(default)]
    pub description: String,
    /// v4.27.0 (TASK-008): statutory rules the resolution imposes on
    /// member states.
    #[serde(default)]
    pub statutory_rules: String,
    /// v4.27.0 (TASK-008): expected economic effects on members.
    #[serde(default)]
    pub economic_effects: String,
    /// v4.27.0 (TASK-008): how the resolution's programmes are funded.
    #[serde(default)]
    pub funding_source: String,
}

fn default_directive_status() -> String {
    "Enacted".to_string()
}

/// Type of mandate for a directive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum MandateType {
    /// Unfunded mandate — member states bear the cost.
    #[default]
    UnfundedMandate,
    /// Funded mandate — organization provides budget allocation.
    FundedMandate {
        /// Budget allocation from the organization's treasury.
        budget_allocation: f64,
    },
}

impl MandateType {
    /// Returns a human-readable label.
    pub fn as_str(&self) -> &'static str {
        match self {
            MandateType::UnfundedMandate => "Unfunded Mandate",
            MandateType::FundedMandate { .. } => "Funded Mandate",
        }
    }
}

/// An international organization (meta-state).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InternationalOrganization {
    /// Unique organization ID (e.g., "ORG-000001").
    pub id: String,
    /// Human-readable name (e.g., "World Forum").
    pub name: String,
    /// Member state country names.
    pub member_states: Vec<String>,
    /// Organization treasury (for funded mandates, fines collected).
    pub budget: Treasury,
    /// Council with member-state representatives.
    pub council: OrgCouncil,
    /// Parliament with proportional seat allocation.
    pub parliament: OrgParliament,
    /// Current integration level.
    pub integration_level: IntegrationLevel,
    /// Voting mechanism for decisions.
    pub voting_mechanism: VotingMechanism,
    /// Active directives.
    pub directives: Vec<Directive>,
    /// Turn the organization was founded.
    pub founded_turn: u32,
}

impl InternationalOrganization {
    /// Creates a new World Forum — neutral, all countries as members, Unanimity voting.
    pub fn new_world_forum(countries: &[String], founded_turn: u32) -> Self {
        Self {
            id: "ORG-WORLDFORUM".to_string(),
            name: "World Forum".to_string(),
            member_states: countries.to_vec(),
            budget: Treasury::default(),
            council: OrgCouncil::from_members(countries),
            parliament: OrgParliament::default(),
            integration_level: IntegrationLevel::FreeTradeArea,
            voting_mechanism: VotingMechanism::Unanimity,
            directives: Vec::new(),
            founded_turn,
        }
    }

    /// Creates a new organization with the given parameters.
    pub fn new(
        id: String,
        name: String,
        member_states: Vec<String>,
        integration_level: IntegrationLevel,
        voting_mechanism: VotingMechanism,
        founded_turn: u32,
    ) -> Self {
        Self {
            id,
            name,
            council: OrgCouncil::from_members(&member_states),
            member_states,
            budget: Treasury::default(),
            parliament: OrgParliament::default(),
            integration_level,
            voting_mechanism,
            directives: Vec::new(),
            founded_turn,
        }
    }

    /// Returns true if the given country is a member.
    pub fn is_member(&self, country: &str) -> bool {
        self.member_states.contains(&country.to_string())
    }

    /// Adds a member state.
    pub fn add_member(&mut self, country: &str) {
        if !self.is_member(country) {
            self.member_states.push(country.to_string());
            self.council.add_member(country);
        }
    }

    /// Removes a member state.
    pub fn remove_member(&mut self, country: &str) {
        self.member_states.retain(|c| c != country);
        self.council.remove_member(country);
        self.parliament.seats.remove(country);
    }

    /// Checks if a vote passes given the number of yes votes and total votes.
    pub fn vote_passes(&self, yes_votes: u32, total_votes: u32) -> bool {
        if total_votes == 0 {
            return false;
        }
        let yes_fraction = yes_votes as f64 / total_votes as f64;
        self.voting_mechanism.passes(yes_fraction)
    }
}

/// Configuration for international organizations. No magic numbers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrgConfig {
    /// Minimum turns as member before integration can advance.
    pub min_turns_for_integration: u32,
    /// Minimum trade volume between members for integration advancement.
    pub min_trade_volume_for_integration: f64,
    /// Seats per million people in parliament.
    pub seats_per_million: f64,
    /// Default fine for non-compliance with directives.
    pub default_noncompliance_fine: f64,
    /// Default compliance deadline (turns after enactment).
    pub default_compliance_deadline_turns: u32,
    /// Integration level at which voting evolves from Unanimity to Qualified Majority.
    pub qmv_integration_threshold: IntegrationLevel,
    /// QMV threshold fraction.
    pub qmv_threshold: f64,
    /// v4.22.1 WS5.2: turns between International Assembly resolutions.
    pub resolution_interval_turns: u32,
}

impl Default for OrgConfig {
    fn default() -> Self {
        Self {
            min_turns_for_integration: 50,
            min_trade_volume_for_integration: 1_000_000_000.0,
            seats_per_million: 5.0,
            default_noncompliance_fine: 10_000_000.0,
            default_compliance_deadline_turns: 20,
            qmv_integration_threshold: IntegrationLevel::CommonMarket,
            qmv_threshold: 0.65,
            resolution_interval_turns: 48,
        }
    }
}

/// Registry tracking all international organizations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OrganizationRegistry {
    /// All organizations.
    pub organizations: Vec<InternationalOrganization>,
    /// Next auto-increment ID counter.
    pub next_id: u64,
}

impl OrganizationRegistry {
    /// Generates the next organization ID.
    pub fn next_org_id(&mut self) -> String {
        self.next_id += 1;
        format!("ORG-{:06}", self.next_id)
    }

    /// Returns all organizations a country belongs to.
    pub fn orgs_for_country(&self, country: &str) -> Vec<&InternationalOrganization> {
        self.organizations
            .iter()
            .filter(|o| o.is_member(country))
            .collect()
    }

    /// Returns the World Forum (if it exists).
    pub fn world_forum(&self) -> Option<&InternationalOrganization> {
        self.organizations.iter().find(|o| o.id == "ORG-WORLDFORUM")
    }

    /// Returns a mutable reference to the World Forum (if it exists).
    pub fn world_forum_mut(&mut self) -> Option<&mut InternationalOrganization> {
        self.organizations
            .iter_mut()
            .find(|o| o.id == "ORG-WORLDFORUM")
    }

    /// Checks if a country is sanctioned by any organization.
    pub fn is_country_sanctioned(
        &self,
        country: &str,
        sanctions: &[crate::international::sanctions::Sanction],
    ) -> bool {
        sanctions
            .iter()
            .any(|s| s.target_country == country && s.is_active())
    }

    /// Processes a turn for all organizations — integration progression, voting evolution.
    pub fn process_turn(
        &mut self,
        current_turn: u32,
        config: &OrgConfig,
        populations: &BTreeMap<String, u64>,
    ) {
        for org in &mut self.organizations {
            // Reallocate parliament seats
            let member_pops: BTreeMap<String, u64> = populations
                .iter()
                .filter(|(k, _)| org.is_member(k))
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            org.parliament
                .allocate_seats(&member_pops, config.seats_per_million);

            // Check if integration can advance
            let turns_as_org = current_turn.saturating_sub(org.founded_turn);
            if turns_as_org >= config.min_turns_for_integration {
                if let Some(next_level) = org.integration_level.advance() {
                    org.integration_level = next_level;
                }
            }

            // Evolve voting mechanism at QMV threshold
            if org.integration_level.ordinal() >= config.qmv_integration_threshold.ordinal()
                && org.voting_mechanism == VotingMechanism::Unanimity
            {
                org.voting_mechanism = VotingMechanism::QualifiedMajority {
                    threshold: config.qmv_threshold,
                };
            }
        }

        // v4.22.1 WS5.2: International Assembly deliberation — every
        // `resolution_interval_turns` each org debates and votes a
        // resolution, recording per-member roll calls for the IA panel.
        if current_turn > 0
            && current_turn.is_multiple_of(config.resolution_interval_turns)
        {
            self.deliberate_resolutions(current_turn, config);
        }
    }

    /// v4.22.1 WS5.2: Assembly deliberation — each org with ≥2 members votes
    /// on a resolution. Member ballots come from the seeded RNG in canonical
    /// (sorted) member order, so a given seed reproduces the same assembly
    /// record. Assent propensity rises with integration depth — a Free Trade
    /// Area debates more contested measures than a Political Union.
    pub fn deliberate_resolutions(&mut self, current_turn: u32, config: &OrgConfig) {
        let mut rng = crate::engine::seeded_rng::thread_rng();
        let cycle = (current_turn / config.resolution_interval_turns) as usize;
        for org in &mut self.organizations {
            if org.member_states.len() < 2 {
                continue;
            }
            // v4.27.0 (TASK-008): each organisation debates from ITS OWN
            // era-appropriate catalogue — universal themes for the World
            // Forum, regional-cooperation themes for "… Compact" blocs,
            // offset by the bloc name so two blocs never clone rows.
            let catalog = resolution_catalog(&org.name);
            let name_offset: usize = org
                .name
                .bytes()
                .map(|b| b as usize)
                .sum::<usize>()
                % catalog.len();
            let idx = (cycle + org.member_states.len() + name_offset) % catalog.len();
            let (theme, description, statutory_rules, economic_effects, funding_source) =
                catalog[idx];
            // Canonical draw order — member_states insertion order can differ
            // across loads; sorting keeps the RNG stream deterministic.
            let mut members = org.member_states.clone();
            members.sort();
            let assent = 0.55 + 0.10 * f64::from(org.integration_level.ordinal());
            let mut votes = BTreeMap::new();
            let mut yes: u32 = 0;
            for member in &members {
                let roll: f64 = rng.gen();
                let vote = if roll < assent {
                    yes += 1;
                    "For"
                } else if roll < assent + 0.10 {
                    "Abstain"
                } else {
                    "Against"
                };
                votes.insert(member.clone(), vote.to_string());
            }
            let total = votes.len() as u32;
            let passed = org.vote_passes(yes, total);
            // v4.25.0 WS7: the tabling member is the largest delegation
            // (seat count); ties break by sorted member order — the same
            // canonical order the ballots were drawn in.
            let proposer = members
                .iter()
                .max_by_key(|m| {
                    org.parliament.seats.get(*m).copied().unwrap_or(0)
                })
                .cloned()
                .unwrap_or_else(|| members[0].clone());
            let deadline = current_turn + config.default_compliance_deadline_turns;
            let against = votes.values().filter(|v| v.as_str() == "Against").count();
            let abstain = votes.values().filter(|v| v.as_str() == "Abstain").count();
            let mechanism = org.voting_mechanism.as_str();
            let mut debate_log = vec![
                format!(
                    "T{current_turn}: '{theme}' tabled by {proposer}; {total} members voting under {mechanism}"
                ),
                format!(
                    "Roll call — {yes} For · {against} Against · {abstain} Abstain"
                ),
            ];
            let mut member_compliance = std::collections::BTreeMap::new();
            if passed {
                debate_log.push(format!(
                    "Enacted — compliance deadline T{deadline}; non-compliance fine {:.0}",
                    config.default_noncompliance_fine
                ));
                for m in &members {
                    member_compliance
                        .insert(m.clone(), "Pending — within compliance window".to_string());
                }
            } else {
                debate_log.push(format!(
                    "Rejected — motion failed under {mechanism} voting"
                ));
                for m in &members {
                    member_compliance.insert(m.clone(), "Not enacted".to_string());
                }
            }
            org.directives.push(Directive {
                id: format!("RES-{}-{:04}", org.id, org.directives.len() + 1),
                title: theme.to_string(),
                mandate_type: MandateType::UnfundedMandate,
                compliance_deadline: deadline,
                fine_for_noncompliance: config.default_noncompliance_fine,
                target_law: None,
                enacted_turn: current_turn,
                votes,
                status: if passed {
                    "Enacted".to_string()
                } else {
                    "Rejected".to_string()
                },
                member_compliance,
                debate_log,
                enforcement_turn: None,
                description: description.to_string(),
                statutory_rules: statutory_rules.to_string(),
                economic_effects: economic_effects.to_string(),
                funding_source: funding_source.to_string(),
            });
        }
    }


    /// Enforces directives — checks compliance and applies fines.
    /// Fines are returned as (country, amount) pairs for sequential double-entry processing.
    ///
    /// v4.25.0 WS7: per-member compliance is now recorded on the
    /// directive (`member_compliance`, `enforcement_turn`). Members that
    /// voted For are deemed compliant — the roll call doubles as the
    /// ratification record; holdouts (Against/Abstain, and legacy
    /// directives with no recorded vote) are fined. Enforcement runs once
    /// per directive, the first turn past the compliance deadline.
    pub fn enforce_directives(&mut self, current_turn: u32) -> Vec<(String, f64, String)> {
        let mut fines = Vec::new();
        for org in &mut self.organizations {
            for directive in &mut org.directives {
                if directive.status == "Rejected"
                    || current_turn <= directive.compliance_deadline
                    || directive.enforcement_turn.is_some()
                {
                    continue;
                }
                directive.enforcement_turn = Some(current_turn);
                for member in &org.member_states {
                    let ratified = directive
                        .votes
                        .get(member)
                        .map(|v| v == "For")
                        .unwrap_or(false);
                    if ratified {
                        directive.member_compliance.insert(
                            member.clone(),
                            "Compliant — ratified".to_string(),
                        );
                    } else {
                        directive.member_compliance.insert(
                            member.clone(),
                            format!("Fined T{current_turn} — non-compliance"),
                        );
                        fines.push((
                            member.clone(),
                            directive.fine_for_noncompliance,
                            format!("Non-compliance fine for directive '{}'", directive.title),
                        ));
                    }
                }
            }
        }
        fines
    }
}

use std::collections::BTreeMap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integration_level_advancement() {
        let level = IntegrationLevel::FreeTradeArea;
        let next = level.advance().unwrap();
        assert_eq!(next, IntegrationLevel::CustomsUnion);

        let top = IntegrationLevel::PoliticalUnion;
        assert!(
            top.advance().is_none(),
            "Political Union cannot advance further"
        );
    }

    #[test]
    fn test_voting_mechanism_unanimity() {
        let vm = VotingMechanism::Unanimity;
        assert!(vm.passes(1.0), "100% yes should pass unanimity");
        assert!(!vm.passes(0.99), "99% yes should fail unanimity");
    }

    #[test]
    fn test_voting_mechanism_qualified_majority() {
        let vm = VotingMechanism::QualifiedMajority { threshold: 0.65 };
        assert!(vm.passes(0.65), "65% yes should pass QMV 65%");
        assert!(!vm.passes(0.64), "64% yes should fail QMV 65%");
    }

    #[test]
    fn test_voting_mechanism_simple_majority() {
        let vm = VotingMechanism::SimpleMajority;
        assert!(vm.passes(0.51), "51% yes should pass simple majority");
        assert!(!vm.passes(0.50), "50% yes should fail simple majority");
    }

    #[test]
    fn test_world_forum_creation() {
        let countries = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let forum = InternationalOrganization::new_world_forum(&countries, 1);
        assert_eq!(forum.name, "World Forum");
        assert_eq!(forum.member_states.len(), 3);
        assert_eq!(forum.voting_mechanism, VotingMechanism::Unanimity);
        assert_eq!(forum.integration_level, IntegrationLevel::FreeTradeArea);
        assert!(forum.is_member("A"));
        assert!(!forum.is_member("D"));
    }

    #[test]
    fn test_org_member_add_remove() {
        let mut org = InternationalOrganization::new(
            "ORG-000001".to_string(),
            "Test Org".to_string(),
            vec!["A".to_string(), "B".to_string()],
            IntegrationLevel::CustomsUnion,
            VotingMechanism::SimpleMajority,
            1,
        );
        assert_eq!(org.member_states.len(), 2);

        org.add_member("C");
        assert_eq!(org.member_states.len(), 3);
        assert!(org.council.members.iter().any(|m| m.country == "C"));

        org.remove_member("A");
        assert_eq!(org.member_states.len(), 2);
        assert!(!org.council.members.iter().any(|m| m.country == "A"));
    }

    #[test]
    fn test_org_vote_passes() {
        let org = InternationalOrganization::new(
            "ORG-000001".to_string(),
            "Test".to_string(),
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
            IntegrationLevel::FreeTradeArea,
            VotingMechanism::SimpleMajority,
            1,
        );
        assert!(org.vote_passes(2, 3), "2/3 should pass simple majority");
        assert!(!org.vote_passes(1, 3), "1/3 should fail simple majority");
    }

    #[test]
    fn test_parliament_seat_allocation() {
        let mut parliament = OrgParliament::default();
        let mut pops = BTreeMap::new();
        pops.insert("A".to_string(), 10_000_000); // 10M → 50 seats at 5/M
        pops.insert("B".to_string(), 500_000); // 0.5M → 3 seats (ceil)
        pops.insert("C".to_string(), 100_000); // 0.1M → 1 seat (min)

        parliament.allocate_seats(&pops, 5.0);
        assert!(parliament.seats["A"] >= 50);
        assert!(parliament.seats["B"] >= 1);
        assert!(parliament.seats["C"] >= 1);
        assert!(parliament.total_seats() > 0);
    }

    #[test]
    fn test_org_registry_world_forum() {
        let mut registry = OrganizationRegistry::default();
        registry
            .organizations
            .push(InternationalOrganization::new_world_forum(
                &["A".to_string(), "B".to_string()],
                1,
            ));
        assert!(registry.world_forum().is_some());
        assert_eq!(registry.world_forum().unwrap().name, "World Forum");
    }

    #[test]
    fn test_org_registry_orgs_for_country() {
        let mut registry = OrganizationRegistry::default();
        registry
            .organizations
            .push(InternationalOrganization::new_world_forum(
                &["A".to_string(), "B".to_string()],
                1,
            ));
        registry.organizations.push(InternationalOrganization::new(
            "ORG-000001".to_string(),
            "Pacific Pact".to_string(),
            vec!["B".to_string(), "C".to_string()],
            IntegrationLevel::CustomsUnion,
            VotingMechanism::QualifiedMajority { threshold: 0.6 },
            5,
        ));

        let a_orgs = registry.orgs_for_country("A");
        assert_eq!(a_orgs.len(), 1, "A should be in 1 org (World Forum)");

        let b_orgs = registry.orgs_for_country("B");
        assert_eq!(b_orgs.len(), 2, "B should be in 2 orgs");
    }

    #[test]
    fn test_org_process_turn_integration_advancement() {
        let mut registry = OrganizationRegistry::default();
        registry
            .organizations
            .push(InternationalOrganization::new_world_forum(
                &["A".to_string()],
                1,
            ));
        let config = OrgConfig {
            min_turns_for_integration: 10,
            ..OrgConfig::default()
        };
        let pops = BTreeMap::new();

        // Before threshold — no advancement
        registry.process_turn(5, &config, &pops);
        assert_eq!(
            registry.organizations[0].integration_level,
            IntegrationLevel::FreeTradeArea
        );

        // After threshold — should advance
        registry.process_turn(51, &config, &pops);
        assert_ne!(
            registry.organizations[0].integration_level,
            IntegrationLevel::FreeTradeArea
        );
    }

    #[test]
    fn test_org_process_turn_voting_evolution() {
        let mut registry = OrganizationRegistry::default();
        let mut org = InternationalOrganization::new_world_forum(&["A".to_string()], 1);
        org.integration_level = IntegrationLevel::CommonMarket;
        registry.organizations.push(org);

        let config = OrgConfig::default();
        let pops = BTreeMap::new();

        registry.process_turn(100, &config, &pops);
        assert_ne!(
            registry.organizations[0].voting_mechanism,
            VotingMechanism::Unanimity,
            "Voting should evolve past Unanimity at CommonMarket level"
        );
    }

    #[test]
    fn test_enforce_directives_returns_fines() {
        let mut registry = OrganizationRegistry::default();
        let mut org =
            InternationalOrganization::new_world_forum(&["A".to_string(), "B".to_string()], 1);
        org.directives.push(Directive {
            id: "DIR-001".to_string(),
            title: "Emission Standards".to_string(),
            mandate_type: MandateType::UnfundedMandate,
            compliance_deadline: 10,
            fine_for_noncompliance: 5_000_000.0,
            target_law: None,
            enacted_turn: 1,
            votes: std::collections::BTreeMap::new(),
            status: "Passed".to_string(),
            member_compliance: std::collections::BTreeMap::new(),
            debate_log: Vec::new(),
            enforcement_turn: None,
            description: String::new(),
            statutory_rules: String::new(),
            economic_effects: String::new(),
            funding_source: String::new(),
        });
        registry.organizations.push(org);

        // Before deadline — no fines
        let fines_before = registry.enforce_directives(5);
        assert!(fines_before.is_empty());

        // After deadline — fines for all members
        let fines_after = registry.enforce_directives(15);
        assert_eq!(fines_after.len(), 2, "Both members should be fined");
        assert!(fines_after
            .iter()
            .all(|(_, amount, _)| *amount == 5_000_000.0));
    }

    #[test]
    fn test_directive_serialization() {
        let directive = Directive {
            id: "DIR-001".to_string(),
            title: "Test".to_string(),
            mandate_type: MandateType::FundedMandate {
                budget_allocation: 1_000_000.0,
            },
            compliance_deadline: 10,
            fine_for_noncompliance: 500_000.0,
            target_law: Some("TaxRateChange".to_string()),
            enacted_turn: 1,
            votes: std::collections::BTreeMap::new(),
            status: "Passed".to_string(),
            member_compliance: std::collections::BTreeMap::new(),
            debate_log: Vec::new(),
            enforcement_turn: None,
            description: String::new(),
            statutory_rules: String::new(),
            economic_effects: String::new(),
            funding_source: String::new(),
        };
        let json = serde_json::to_string(&directive).unwrap();
        let deserialized: Directive = serde_json::from_str(&json).unwrap();
        assert_eq!(directive, deserialized);
    }
}
// ============================================================================
// v4.27.0 (TASK-008): ERA-APPROPRIATE RESOLUTION CATALOGUES
// ============================================================================
//
// Each catalogue entry is (title, description, statutory_rules,
// economic_effects, funding_source). Universal bodies (the World Forum)
// debate planet-wide instruments; regional "… Compact" blocs debate
// intra-bloc cooperation. All themes are period-appropriate for the
// game's early-industrial era — no anachronisms (no emissions accords
// in 1900).

/// Catalogue row: (title, description, statutory rules, economic
/// effects, funding source).
type ResolutionTheme = (&'static str, &'static str, &'static str, &'static str, &'static str);

/// Universal-body resolutions (World Forum and any org that is not a
/// regional Compact).
const WORLD_FORUM_THEMES: &[ResolutionTheme] = &[
    (
        "Concert of Nations Peace Protocol",
        "A standing mediation protocol obliging signatories to submit \
         interstate disputes to a Forum-arbitrated conference before \
         resorting to arms.",
        "Members must offer good offices before any declaration of war; \
         a two-thirds quorum of members may convene an emergency \
         congress against an aggressor.",
        "Dampens bilateral tension growth among members; war-weary \
         states recover trade access faster after a mediated peace.",
        "Funded by proportional member dues levied on treasuries each \
         assembly cycle.",
    ),
    (
        "Universal Postal & Telegraphic Convention",
        "Standardises cross-border mail rates, telegraph routing, and \
         interconnection of national telegraph networks among all \
         member states.",
        "Members must honour foreign mail at domestic-equivalent rates \
         and maintain telegraph interconnection at designated border \
         exchanges.",
        "Improves commercial correspondence speed, marginally raising \
         trade throughput and B2B clearing velocity across borders.",
        "Funded by per-member communication tariffs collected at border \
         exchanges.",
    ),
    (
        "Open Straits Navigation Accord",
        "Guarantees free merchant passage through internationally \
         administered straits and coastal sea lanes in peace and war.",
        "No member may levy discriminatory tolls or close recognised \
         international straits to neutral shipping.",
        "Lowers effective freight costs on sea lanes, raising export \
         revenue for coastal member economies.",
        "Funded by tonnage dues remitted to the org treasury.",
    ),
    (
        "Gold Standard Arbitration Charter",
        "Establishes a standing monetary conference to arbitrate \
         exchange disputes between gold-standard member treasuries.",
        "Members pledge convertibility of their currency into gold at \
         declared parity and accept arbitration of parity disputes.",
        "Stabilises FX spreads, reducing treasury reserve volatility on \
         cross-border settlements.",
        "Funded by assessments scaled to members' declared gold \
         reserves.",
    ),
    (
        "International Railway Interconnection Charter",
        "Coordinates gauge standards and through-ticketing on \
         trans-national rail corridors crossing member frontiers.",
        "Members must permit through-running of foreign rolling stock \
         on designated international corridors at published tariffs.",
        "Cuts border-transfer delays on trade routes, boosting rail \
         freight share on international corridors.",
        "Funded by corridor access fees shared pro-rata among members.",
    ),
    (
        "Colonial Spheres Mediation Pact",
        "Creates a forum procedure for the peaceful delimitation of \
         colonial claims and sphere-of-influence disputes.",
        "Members must register new territorial claims with the Forum \
         and accept arbitration findings before annexation.",
        "Reduces the incidence of escalatory border incidents over \
         contested periphery regions.",
        "Funded by proportional member dues plus filing fees on \
         registered claims.",
    ),
];

/// Regional-bloc resolutions (any "… Compact" organisation). Distinct
/// from the universal catalogue so a bloc never debates a World Forum
/// instrument.
const BLOC_THEMES: &[ResolutionTheme] = &[
    (
        "Most-Favoured-Nation Tariff Schedule",
        "A reciprocal schedule granting every bloc member the lowest \
         tariff rate extended to any member — consolidating the \
         bloc's free-trade area.",
        "Members must apply the published common schedule to intra-bloc \
         trade and may not raise preferential barriers against members.",
        "Raises intra-bloc trade volume and customs revenue stability; \
         narrows price spreads between member markets.",
        "Revenue-neutral — funded by the customs duties it harmonises.",
    ),
    (
        "Interstate Railway Gauge Convention",
        "Harmonises track gauge and signalling practice on railways \
         crossing the bloc's internal borders.",
        "Members must convert designated trunk lines to the common \
         gauge and accept mutual inspection of signalling standards.",
        "Cuts transshipment costs at borders, increasing rail freight \
         throughput inside the bloc.",
        "Funded by a bloc infrastructure levy scaled to each member's \
         rail mileage.",
    ),
    (
        "Joint Border Demarcation Survey",
        "A mixed-commission survey producing a legally binding common \
         map of the bloc's internal frontiers.",
        "Members must place survey commissioners and accept the \
         demarcated boundary line as final between them.",
        "Ends border-strip disputes that periodically raise bilateral \
         tension; secures cadastral registries along the frontier.",
        "Funded by equal shares from member treasury survey budgets.",
    ),
    (
        "Reciprocal Consular Immunity Compact",
        "Extends mutual consular immunity and expedited visa \
         processing to accredited diplomats of all bloc members.",
        "Members grant consular premises inviolability and honour \
         expedited travel papers issued under the compact.",
        "Improves the reliability of bilateral diplomacy missions and \
         reduces incident-related tension spikes.",
        "Funded by consular fees administered by each member's foreign \
         ministry.",
    ),
    (
        "Common Grain Reserve Protocol",
        "Establishes a pooled strategic grain reserve drawn from member \
         harvests to stabilise regional food prices in dearth years.",
        "Members must contribute their quota of grain to the reserve \
         and may draw only under certified harvest failure.",
        "Blaunts famine-driven price spikes and emigration waves inside \
         the bloc; smooths food-commodity volatility.",
        "Funded by in-kind member grain quotas warehoused at designated \
         depots.",
    ),
    (
        "Mutual Defense Consultation Pact",
        "A standing military consultative council coordinating bloc \
         members' defensive posture against external aggression.",
        "Members must convene the council within two turns of an \
         external war declaration touching any member.",
        "Deters external aggression; modest joint-planning bonuses to \
         readiness on bloc borders.",
        "Funded by a bloc military assessment scaled to each member's \
         standing manpower.",
    ),
];

/// Picks the era-appropriate resolution catalogue for an organisation.
/// The World Forum (and non-Compact bodies) use the universal
/// catalogue; any org named "… Compact" is a regional bloc and uses
/// the bloc catalogue — guaranteeing `world_forum_resolutions !=
/// regional_bloc_resolutions` at every cycle.
fn resolution_catalog(org_name: &str) -> &'static [ResolutionTheme] {
    if org_name.ends_with("Compact") {
        BLOC_THEMES
    } else {
        WORLD_FORUM_THEMES
    }
}


#[cfg(test)]
mod resolution_catalog_tests {
    use super::*;

    /// v4.27.0 (TASK-008) release gate: the World Forum and regional
    /// blocs debate from disjoint catalogues — no cloned rows and no
    /// anachronisms in the early-industrial era.
    #[test]
    fn test_v4270_world_forum_resolutions_differ_from_bloc_resolutions() {
        let forum_titles: std::collections::BTreeSet<_> =
            WORLD_FORUM_THEMES.iter().map(|t| t.0).collect();
        let bloc_titles: std::collections::BTreeSet<_> =
            BLOC_THEMES.iter().map(|t| t.0).collect();
        assert!(
            forum_titles.is_disjoint(&bloc_titles),
            "catalogues must be disjoint"
        );
        for (_, desc, rules, effects, funding) in
            WORLD_FORUM_THEMES.iter().chain(BLOC_THEMES.iter())
        {
            assert!(!desc.is_empty() && !rules.is_empty());
            assert!(!effects.is_empty() && !funding.is_empty());
        }
        assert_eq!(resolution_catalog("World Forum").first().unwrap().0,
            "Concert of Nations Peace Protocol");
        assert!(resolution_catalog("Slavic Compact")
            .iter().all(|t| t.0 != "Concert of Nations Peace Protocol"));
    }
}

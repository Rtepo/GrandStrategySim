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

use serde::{Deserialize, Serialize};
use crate::state::Treasury;

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VotingMechanism {
    /// Every member must agree (used by World Forum).
    Unanimity,
    /// Qualified majority — requires threshold fraction of votes.
    QualifiedMajority { /// Fraction of votes required (0.0 to 1.0).
        threshold: f64 },
    /// Simple majority — more than 50% of votes.
    SimpleMajority,
}

impl Default for VotingMechanism {
    fn default() -> Self {
        VotingMechanism::Unanimity
    }
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
            members: countries.iter().map(|c| CouncilMember {
                country: c.clone(),
                representative: None,
                has_veto: true,
            }).collect(),
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
}

/// Type of mandate for a directive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum MandateType {
    /// Unfunded mandate — member states bear the cost.
    #[default]
    UnfundedMandate,
    /// Funded mandate — organization provides budget allocation.
    FundedMandate { /// Budget allocation from the organization's treasury.
        budget_allocation: f64 },
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
        self.organizations.iter()
            .filter(|o| o.is_member(country))
            .collect()
    }

    /// Returns the World Forum (if it exists).
    pub fn world_forum(&self) -> Option<&InternationalOrganization> {
        self.organizations.iter().find(|o| o.id == "ORG-WORLDFORUM")
    }

    /// Returns a mutable reference to the World Forum (if it exists).
    pub fn world_forum_mut(&mut self) -> Option<&mut InternationalOrganization> {
        self.organizations.iter_mut().find(|o| o.id == "ORG-WORLDFORUM")
    }

    /// Checks if a country is sanctioned by any organization.
    pub fn is_country_sanctioned(&self, country: &str, sanctions: &[crate::international::sanctions::Sanction]) -> bool {
        sanctions.iter().any(|s| s.target_country == country && s.is_active())
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
            let member_pops: BTreeMap<String, u64> = populations.iter()
                .filter(|(k, _)| org.is_member(k))
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            org.parliament.allocate_seats(&member_pops, config.seats_per_million);

            // Check if integration can advance
            let turns_as_org = current_turn.saturating_sub(org.founded_turn);
            if turns_as_org >= config.min_turns_for_integration {
                if let Some(next_level) = org.integration_level.advance() {
                    org.integration_level = next_level;
                }
            }

            // Evolve voting mechanism at QMV threshold
            if org.integration_level.ordinal() >= config.qmv_integration_threshold.ordinal() {
                if org.voting_mechanism == VotingMechanism::Unanimity {
                    org.voting_mechanism = VotingMechanism::QualifiedMajority {
                        threshold: config.qmv_threshold,
                    };
                }
            }
        }
    }

    /// Enforces directives — checks compliance and applies fines.
    /// Fines are returned as (country, amount) pairs for sequential double-entry processing.
    pub fn enforce_directives(
        &self,
        current_turn: u32,
    ) -> Vec<(String, f64, String)> {
        let mut fines = Vec::new();
        for org in &self.organizations {
            for directive in &org.directives {
                if current_turn > directive.compliance_deadline {
                    // Non-compliant — fine each member that hasn't complied
                    // (Simplified: all members are checked; in full impl, we'd check law enactment)
                    for member in &org.member_states {
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
        assert!(top.advance().is_none(), "Political Union cannot advance further");
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
        pops.insert("B".to_string(), 500_000);    // 0.5M → 3 seats (ceil)
        pops.insert("C".to_string(), 100_000);    // 0.1M → 1 seat (min)

        parliament.allocate_seats(&pops, 5.0);
        assert!(parliament.seats["A"] >= 50);
        assert!(parliament.seats["B"] >= 1);
        assert!(parliament.seats["C"] >= 1);
        assert!(parliament.total_seats() > 0);
    }

    #[test]
    fn test_org_registry_world_forum() {
        let mut registry = OrganizationRegistry::default();
        registry.organizations.push(InternationalOrganization::new_world_forum(
            &["A".to_string(), "B".to_string()],
            1,
        ));
        assert!(registry.world_forum().is_some());
        assert_eq!(registry.world_forum().unwrap().name, "World Forum");
    }

    #[test]
    fn test_org_registry_orgs_for_country() {
        let mut registry = OrganizationRegistry::default();
        registry.organizations.push(InternationalOrganization::new_world_forum(
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
        registry.organizations.push(InternationalOrganization::new_world_forum(
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
        assert_eq!(registry.organizations[0].integration_level, IntegrationLevel::FreeTradeArea);

        // After threshold — should advance
        registry.process_turn(51, &config, &pops);
        assert_ne!(registry.organizations[0].integration_level, IntegrationLevel::FreeTradeArea);
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
        let mut org = InternationalOrganization::new_world_forum(&["A".to_string(), "B".to_string()], 1);
        org.directives.push(Directive {
            id: "DIR-001".to_string(),
            title: "Emission Standards".to_string(),
            mandate_type: MandateType::UnfundedMandate,
            compliance_deadline: 10,
            fine_for_noncompliance: 5_000_000.0,
            target_law: None,
            enacted_turn: 1,
        });
        registry.organizations.push(org);

        // Before deadline — no fines
        let fines_before = registry.enforce_directives(5);
        assert!(fines_before.is_empty());

        // After deadline — fines for all members
        let fines_after = registry.enforce_directives(15);
        assert_eq!(fines_after.len(), 2, "Both members should be fined");
        assert!(fines_after.iter().all(|(_, amount, _)| *amount == 5_000_000.0));
    }

    #[test]
    fn test_directive_serialization() {
        let directive = Directive {
            id: "DIR-001".to_string(),
            title: "Test".to_string(),
            mandate_type: MandateType::FundedMandate { budget_allocation: 1_000_000.0 },
            compliance_deadline: 10,
            fine_for_noncompliance: 500_000.0,
            target_law: Some("TaxRateChange".to_string()),
            enacted_turn: 1,
        };
        let json = serde_json::to_string(&directive).unwrap();
        let deserialized: Directive = serde_json::from_str(&json).unwrap();
        assert_eq!(directive, deserialized);
    }
}

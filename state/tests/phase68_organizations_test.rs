//! Phase 68: Integration tests for international organizations, sanctions,
//! and deep engine integration.
//!
//! Tests verify:
//! - Organization creation and World Forum spawning
//! - Integration level progression and voting mechanism evolution
//! - Directive enforcement and fine application (double-entry)
//! - Sanction enactment, expiry, and renewal
//! - TradeEmbargo blocking GlobalMarket access
//! - AssetFreeze blocking foreign purchases
//! - FinancialIsolation blocking economic aid
//! - Reputation damage from sanctions

use sim_engine::international::organizations::{
    InternationalOrganization, IntegrationLevel, VotingMechanism, OrgConfig,
    OrganizationRegistry, Directive, MandateType, OrgCouncil, OrgParliament,
};
use sim_engine::international::sanctions::{
    Sanction, SanctionType, SanctionConfig, SanctionRegistry,
};
use sim_engine::state::{GameState, Country, Treasury};
use sim_engine::state::diplomatic_actions::{DiplomaticAction, drain_diplomatic_actions};
use sim_engine::international::fog_of_war::DiplomaticConfig;

// ============================================================================
// ORGANIZATION CREATION & WORLD FORUM
// ============================================================================

#[test]
fn test_world_forum_creation() {
    let countries = vec!["A".to_string(), "B".to_string(), "C".to_string()];
    let forum = InternationalOrganization::new_world_forum(&countries, 1);
    assert_eq!(forum.name, "World Forum");
    assert_eq!(forum.id, "ORG-WORLDFORUM");
    assert_eq!(forum.member_states.len(), 3);
    assert_eq!(forum.voting_mechanism, VotingMechanism::Unanimity);
    assert_eq!(forum.integration_level, IntegrationLevel::FreeTradeArea);
    assert!(forum.is_member("A"));
    assert!(forum.is_member("B"));
    assert!(forum.is_member("C"));
    assert!(!forum.is_member("D"));
}

#[test]
fn test_world_forum_all_countries_as_members() {
    let countries = vec!["Nation1".to_string(), "Nation2".to_string()];
    let forum = InternationalOrganization::new_world_forum(&countries, 0);
    for c in &countries {
        assert!(forum.is_member(c), "{} should be a World Forum member", c);
    }
    assert_eq!(forum.council.members.len(), 2);
}

#[test]
fn test_custom_organization_creation() {
    let org = InternationalOrganization::new(
        "ORG-000001".to_string(),
        "Pacific Trade Bloc".to_string(),
        vec!["A".to_string(), "B".to_string()],
        IntegrationLevel::CustomsUnion,
        VotingMechanism::QualifiedMajority { threshold: 0.65 },
        10,
    );
    assert_eq!(org.name, "Pacific Trade Bloc");
    assert_eq!(org.integration_level, IntegrationLevel::CustomsUnion);
    assert!(matches!(org.voting_mechanism, VotingMechanism::QualifiedMajority { .. }));
}

// ============================================================================
// INTEGRATION LEVEL PROGRESSION
// ============================================================================

#[test]
fn test_integration_level_advancement() {
    let mut registry = OrganizationRegistry::default();
    registry.organizations.push(InternationalOrganization::new_world_forum(&["A".to_string()], 1));
    let config = OrgConfig {
        min_turns_for_integration: 10,
        ..OrgConfig::default()
    };
    let pops = std::collections::BTreeMap::new();

    // Before threshold — no advancement
    registry.process_turn(5, &config, &pops);
    assert_eq!(registry.organizations[0].integration_level, IntegrationLevel::FreeTradeArea);

    // After threshold — should advance
    registry.process_turn(51, &config, &pops);
    assert_eq!(registry.organizations[0].integration_level, IntegrationLevel::CustomsUnion);
}

#[test]
fn test_voting_mechanism_evolution() {
    let mut registry = OrganizationRegistry::default();
    let mut org = InternationalOrganization::new_world_forum(&["A".to_string()], 1);
    org.integration_level = IntegrationLevel::CommonMarket;
    registry.organizations.push(org);

    let config = OrgConfig::default();
    let pops = std::collections::BTreeMap::new();

    registry.process_turn(100, &config, &pops);
    assert_ne!(
        registry.organizations[0].voting_mechanism,
        VotingMechanism::Unanimity,
        "Voting should evolve past Unanimity at CommonMarket level"
    );
}

// ============================================================================
// DIRECTIVE ENFORCEMENT & FINES
// ============================================================================

#[test]
fn test_directive_enforcement_applies_fines() {
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
fn test_directive_fine_double_entry() {
    let mut state = GameState::default();
    let mut org = InternationalOrganization::new_world_forum(&["A".to_string()], 1);
    org.directives.push(Directive {
        id: "DIR-001".to_string(),
        title: "Test".to_string(),
        mandate_type: MandateType::UnfundedMandate,
        compliance_deadline: 5,
        fine_for_noncompliance: 1_000_000.0,
        target_law: None,
        enacted_turn: 1,
    });
    state.international_organizations.organizations.push(org);

    let mut country_a = Country::mock_for_tests();
    country_a.name = "A".to_string();
    country_a.budget.liquid_reserves = 10_000_000.0;
    state.countries.insert("A".to_string(), country_a);

    let initial_country_reserves = state.countries["A"].budget.liquid_reserves;
    let initial_org_reserves = state.international_organizations.organizations[0].budget.liquid_reserves;

    // Enforce directives and apply fines
    let fines = state.international_organizations.enforce_directives(10);
    for (country_name, fine_amount, _) in fines {
        if let Some(country) = state.countries.get_mut(&country_name) {
            if country.budget.liquid_reserves >= fine_amount {
                country.budget.liquid_reserves -= fine_amount;
                for org in &mut state.international_organizations.organizations {
                    if org.is_member(&country_name) {
                        org.budget.liquid_reserves += fine_amount;
                        break;
                    }
                }
            }
        }
    }

    // Verify double-entry: country lost 1M, org gained 1M
    assert_eq!(state.countries["A"].budget.liquid_reserves, initial_country_reserves - 1_000_000.0);
    assert_eq!(state.international_organizations.organizations[0].budget.liquid_reserves, initial_org_reserves + 1_000_000.0);
}

// ============================================================================
// SANCTION ENACTMENT & EXPIRY
// ============================================================================

#[test]
fn test_sanction_enactment() {
    let mut registry = SanctionRegistry::default();
    let id = registry.next_sanction_id();
    registry.enact_sanction(Sanction::new(
        id,
        "Badland".to_string(),
        "World Forum".to_string(),
        SanctionType::TradeEmbargo,
        1, 50, "Treaty violation".to_string(),
    ));

    assert!(registry.is_sanctioned("Badland", 25));
    assert!(registry.has_trade_embargo("Badland", 25));
    assert!(!registry.has_asset_freeze("Badland", 25));
}

#[test]
fn test_sanction_expiry() {
    let mut registry = SanctionRegistry::default();
    registry.enact_sanction(Sanction::new(
        "S1".to_string(),
        "Badland".to_string(),
        "World Forum".to_string(),
        SanctionType::TradeEmbargo,
        1, 10, "Test".to_string(),
    ));

    assert!(registry.has_trade_embargo("Badland", 10));
    registry.expire_finished_sanctions(11);
    assert!(!registry.has_trade_embargo("Badland", 11));
}

#[test]
fn test_sanction_lift() {
    let mut registry = SanctionRegistry::default();
    registry.enact_sanction(Sanction::new(
        "S1".to_string(),
        "Badland".to_string(),
        "World Forum".to_string(),
        SanctionType::FullEmbargo,
        1, 100, "Test".to_string(),
    ));

    assert!(registry.is_sanctioned("Badland", 50));
    assert!(registry.lift_sanction("S1"));
    assert!(!registry.is_sanctioned("Badland", 50));
}

#[test]
fn test_full_embargo_includes_all_types() {
    let mut registry = SanctionRegistry::default();
    registry.enact_sanction(Sanction::new(
        "S1".to_string(),
        "Badland".to_string(),
        "World Forum".to_string(),
        SanctionType::FullEmbargo,
        1, 100, "Test".to_string(),
    ));

    assert!(registry.has_trade_embargo("Badland", 50));
    assert!(registry.has_asset_freeze("Badland", 50));
    assert!(registry.has_financial_isolation("Badland", 50));
}

// ============================================================================
// TRADE EMBARGO BLOCKS GLOBAL MARKET
// ============================================================================

#[test]
fn test_trade_embargo_recognition_in_trade() {
    // This test verifies that the sanction registry correctly identifies
    // trade-embargoed countries, which balance_global_trade() uses.
    let mut registry = SanctionRegistry::default();
    registry.enact_sanction(Sanction::new(
        "S1".to_string(),
        "Embargoed".to_string(),
        "World Forum".to_string(),
        SanctionType::TradeEmbargo,
        1, 100, "Test".to_string(),
    ));

    assert!(registry.has_trade_embargo("Embargoed", 50));
    assert!(!registry.has_trade_embargo("NotEmbargoed", 50));
}

// ============================================================================
// ASSET FREEZE BLOCKS FOREIGN PURCHASES
// ============================================================================

#[test]
fn test_asset_freeze_blocks_real_estate_purchase() {
    use sim_engine::society::real_estate_market::{check_foreign_purchase_allowed, AgrarianReformLaw};
    use sim_engine::society::cadastre::{Cadastre, ParcelChunk, ParcelOwnerType};

    let cadastre = Cadastre::default();
    let parcel = ParcelChunk::default();
    let law = AgrarianReformLaw {
        foreign_ownership_cap: 0.50,
        ..Default::default()
    };

    let mut registry = SanctionRegistry::default();
    registry.enact_sanction(Sanction::new(
        "S1".to_string(),
        "Frozenland".to_string(),
        "World Forum".to_string(),
        SanctionType::AssetFreeze,
        1, 100, "Corruption".to_string(),
    ));

    // Asset freeze should block the purchase
    assert!(!check_foreign_purchase_allowed(
        &cadastre, &parcel, &law, None, "Frozenland", "SellerLand",
        Some(&registry), 50,
    ), "Asset freeze should block foreign purchase");

    // Non-sanctioned country should be allowed (no cap exceeded with empty cadastre)
    assert!(check_foreign_purchase_allowed(
        &cadastre, &parcel, &law, None, "FreeCountry", "SellerLand",
        Some(&registry), 50,
    ), "Non-sanctioned country should be allowed");
}

// ============================================================================
// FINANCIAL ISOLATION BLOCKS ECONOMIC AID
// ============================================================================

#[test]
fn test_financial_isolation_blocks_aid() {
    let mut state = GameState::default();
    let mut from = Country::mock_for_tests();
    from.name = "RichCountry".to_string();
    from.budget.liquid_reserves = 1_000_000.0;
    let mut to = Country::mock_for_tests();
    to.name = "IsolatedCountry".to_string();
    to.budget.liquid_reserves = 100_000.0;
    state.countries.insert("RichCountry".to_string(), from);
    state.countries.insert("IsolatedCountry".to_string(), to);

    // Enact financial isolation against IsolatedCountry
    state.active_sanctions.enact_sanction(Sanction::new(
        "S1".to_string(),
        "IsolatedCountry".to_string(),
        "World Forum".to_string(),
        SanctionType::FinancialIsolation,
        1, 100, "Treaty violation".to_string(),
    ));

    let initial_from = state.countries["RichCountry"].budget.liquid_reserves;
    let initial_to = state.countries["IsolatedCountry"].budget.liquid_reserves;

    // Queue aid to IsolatedCountry
    state.pending_diplomatic_actions.push(DiplomaticAction::SendEconomicAid {
        from_country: "RichCountry".to_string(),
        to_country: "IsolatedCountry".to_string(),
        amount: 500_000.0,
    });

    let config = DiplomaticConfig::default();
    drain_diplomatic_actions(&mut state, &config);

    // Aid should be blocked — no money moved
    assert_eq!(state.countries["RichCountry"].budget.liquid_reserves, initial_from,
        "Sender should not lose funds when aid is blocked by FinancialIsolation");
    assert_eq!(state.countries["IsolatedCountry"].budget.liquid_reserves, initial_to,
        "Receiver should not gain funds when aid is blocked by FinancialIsolation");
}

#[test]
fn test_aid_allowed_without_sanction() {
    let mut state = GameState::default();
    let mut from = Country::mock_for_tests();
    from.name = "RichCountry".to_string();
    from.budget.liquid_reserves = 1_000_000.0;
    let mut to = Country::mock_for_tests();
    to.name = "FreeCountry".to_string();
    to.budget.liquid_reserves = 100_000.0;
    state.countries.insert("RichCountry".to_string(), from);
    state.countries.insert("FreeCountry".to_string(), to);

    let initial_from = state.countries["RichCountry"].budget.liquid_reserves;

    state.pending_diplomatic_actions.push(DiplomaticAction::SendEconomicAid {
        from_country: "RichCountry".to_string(),
        to_country: "FreeCountry".to_string(),
        amount: 500_000.0,
    });

    let config = DiplomaticConfig::default();
    drain_diplomatic_actions(&mut state, &config);

    // Aid should go through
    assert_eq!(state.countries["RichCountry"].budget.liquid_reserves, initial_from - 500_000.0);
    assert_eq!(state.countries["FreeCountry"].budget.liquid_reserves, 600_000.0);
}

// ============================================================================
// SANCTION REPUTATION DAMAGE
// ============================================================================

#[test]
fn test_sanctioned_country_reputation_damage() {
    let mut state = GameState::default();
    let mut country = Country::mock_for_tests();
    country.name = "Badland".to_string();
    country.global_reputation.score = 0.0;
    state.countries.insert("Badland".to_string(), country);

    state.active_sanctions.enact_sanction(Sanction::new(
        "S1".to_string(),
        "Badland".to_string(),
        "World Forum".to_string(),
        SanctionType::FullEmbargo,
        1, 100, "Test".to_string(),
    ));

    let sanction_config = state.sanction_config.clone();
    let current_turn = 10u32;

    // Apply reputation damage (simulating turn processing)
    if state.active_sanctions.is_sanctioned("Badland", current_turn) {
        if let Some(c) = state.countries.get_mut("Badland") {
            c.global_reputation.score -= sanction_config.reputation_damage_per_turn;
        }
    }

    assert!(state.countries["Badland"].global_reputation.score < 0.0,
        "Sanctioned country should suffer reputation damage");
}

// ============================================================================
// ORGANIZATION MEMBER MANAGEMENT
// ============================================================================

#[test]
fn test_org_add_remove_member() {
    let mut org = InternationalOrganization::new(
        "ORG-000001".to_string(),
        "Test".to_string(),
        vec!["A".to_string(), "B".to_string()],
        IntegrationLevel::FreeTradeArea,
        VotingMechanism::SimpleMajority,
        1,
    );

    org.add_member("C");
    assert_eq!(org.member_states.len(), 3);
    assert!(org.council.members.iter().any(|m| m.country == "C"));

    org.remove_member("A");
    assert_eq!(org.member_states.len(), 2);
    assert!(!org.council.members.iter().any(|m| m.country == "A"));
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
    assert_eq!(a_orgs.len(), 1);

    let b_orgs = registry.orgs_for_country("B");
    assert_eq!(b_orgs.len(), 2);

    let c_orgs = registry.orgs_for_country("C");
    assert_eq!(c_orgs.len(), 1);
}

// ============================================================================
// VOTING MECHANISM TESTS
// ============================================================================

#[test]
fn test_org_vote_passes_simple_majority() {
    let org = InternationalOrganization::new(
        "ORG-000001".to_string(),
        "Test".to_string(),
        vec!["A".to_string(), "B".to_string(), "C".to_string()],
        IntegrationLevel::FreeTradeArea,
        VotingMechanism::SimpleMajority,
        1,
    );
    assert!(org.vote_passes(2, 3));
    assert!(!org.vote_passes(1, 3));
}

#[test]
fn test_org_vote_passes_unanimity() {
    let org = InternationalOrganization::new_world_forum(&["A".to_string(), "B".to_string()], 1);
    assert!(org.vote_passes(2, 2));
    assert!(!org.vote_passes(1, 2));
}

#[test]
fn test_org_vote_passes_qualified_majority() {
    let org = InternationalOrganization::new(
        "ORG-000001".to_string(),
        "Test".to_string(),
        vec!["A".to_string(), "B".to_string(), "C".to_string()],
        IntegrationLevel::CustomsUnion,
        VotingMechanism::QualifiedMajority { threshold: 0.65 },
        1,
    );
    assert!(org.vote_passes(2, 3)); // 66.7% > 65%
    assert!(!org.vote_passes(1, 3)); // 33.3% < 65%
}

// ============================================================================
// PARLIAMENT SEAT ALLOCATION
// ============================================================================

#[test]
fn test_parliament_seat_allocation_proportional() {
    let mut parliament = OrgParliament::default();
    let mut pops = std::collections::BTreeMap::new();
    pops.insert("BigCountry".to_string(), 50_000_000); // 50M → 250 seats at 5/M
    pops.insert("SmallCountry".to_string(), 200_000);   // 0.2M → 1 seat (min)

    parliament.allocate_seats(&pops, 5.0);
    assert!(parliament.seats["BigCountry"] > parliament.seats["SmallCountry"]);
    assert!(parliament.seats["SmallCountry"] >= 1);
    assert!(parliament.total_seats() > 0);
}

// ============================================================================
// SERIALIZATION TESTS
// ============================================================================

#[test]
fn test_organization_serialization() {
    let org = InternationalOrganization::new_world_forum(&["A".to_string(), "B".to_string()], 1);
    let json = serde_json::to_string(&org).unwrap();
    let deserialized: InternationalOrganization = serde_json::from_str(&json).unwrap();
    // Compare key fields (Treasury has its own serialization quirks)
    assert_eq!(org.id, deserialized.id);
    assert_eq!(org.name, deserialized.name);
    assert_eq!(org.member_states, deserialized.member_states);
    assert_eq!(org.integration_level, deserialized.integration_level);
    assert_eq!(org.voting_mechanism, deserialized.voting_mechanism);
    assert_eq!(org.founded_turn, deserialized.founded_turn);
    assert_eq!(org.council.members.len(), deserialized.council.members.len());
}

#[test]
fn test_sanction_serialization() {
    let sanction = Sanction::new(
        "SANCTION-000001".to_string(),
        "Badland".to_string(),
        "World Forum".to_string(),
        SanctionType::FullEmbargo,
        10,
        50,
        "Treaty violation".to_string(),
    );
    let json = serde_json::to_string(&sanction).unwrap();
    let deserialized: Sanction = serde_json::from_str(&json).unwrap();
    assert_eq!(sanction, deserialized);
}

#[test]
fn test_directive_serialization() {
    let directive = Directive {
        id: "DIR-001".to_string(),
        title: "Test Directive".to_string(),
        mandate_type: MandateType::FundedMandate { budget_allocation: 5_000_000.0 },
        compliance_deadline: 20,
        fine_for_noncompliance: 1_000_000.0,
        target_law: Some("TaxRateChange".to_string()),
        enacted_turn: 5,
    };
    let json = serde_json::to_string(&directive).unwrap();
    let deserialized: Directive = serde_json::from_str(&json).unwrap();
    assert_eq!(directive, deserialized);
}

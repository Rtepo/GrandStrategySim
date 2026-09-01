//! Unit tests for Stage 4 political enhancements
//!
//! Tests for councilor traits, voting logic, espionage, and coalition building

use sim_engine::politics::{
    build_coalition_with_concessions, calculate_vote_probability, ConcessionClause, Councilor,
    CouncilorTrait, EspionageState, EspionageType, Faction,
};
use std::collections::HashMap;

#[test]
fn test_councilor_trait_loyist_vote() {
    let councilor = Councilor {
        id: "test_loyalist".to_string(),
        name: "Jan Kowalski".to_string(),
        represented_class: "Burghers".to_string(),
        faction: Faction::Moderates,
        years_in_office: 5,
        political_influence: 75.0,
        hidden_trait: CouncilorTrait::Loyalist,
        trait_revealed: false,
        blackmail_material: None,
        party: "Party A".to_string(),
        corruption_risk: 0.0,
    };

    // Neutral party context (no discipline, no war chest) isolates the trait.
    let probability = calculate_vote_probability(&councilor, false, 0.5, false, false, 0.0, 0.0);

    // Loyalist should always vote party line (0.9+ probability)
    assert!(
        probability >= 0.9,
        "Loyalist vote probability should be >= 0.9, got {}",
        probability
    );
}

#[test]
fn test_councilor_trait_undecided_vote() {
    let councilor = Councilor {
        id: "test_undecided".to_string(),
        name: "Anna Nowak".to_string(),
        represented_class: "Peasants".to_string(),
        faction: Faction::Populares,
        years_in_office: 2,
        political_influence: 40.0,
        hidden_trait: CouncilorTrait::Undecided,
        trait_revealed: false,
        blackmail_material: None,
        party: "Party B".to_string(),
        corruption_risk: 0.0,
    };

    // Base 50% with no concessions.
    // Alignment is 0.0, not 0.5: the formula adds `alignment * 0.2`, so 0.0 is
    // the neutral value. Passing 0.5 here would silently add +0.1 and make the
    // "base" case measure something other than the documented baseline.
    let base_prob = calculate_vote_probability(&councilor, false, 0.0, false, false, 0.0, 0.0);
    assert!(
        (base_prob - 0.5).abs() < 0.1,
        "Undecided base probability should be ~0.5, got {}",
        base_prob
    );

    // With concession, should increase by ~30%
    let concession_prob = calculate_vote_probability(&councilor, true, 0.0, false, false, 0.0, 0.0);
    assert!(
        concession_prob > base_prob,
        "Concession should increase vote probability"
    );
    assert!(
        (concession_prob - 0.8).abs() < 0.1,
        "Undecided with concession should be ~0.8, got {}",
        concession_prob
    );

    // With ideological alignment, should increase by ~20%
    let alignment_prob = calculate_vote_probability(&councilor, false, 1.0, false, false, 0.0, 0.0);
    assert!(
        alignment_prob > base_prob,
        "Ideological alignment should increase vote probability"
    );

    // Party discipline is the new lever: it must also pull an Undecided councilor
    // toward the party line.
    let disciplined_prob =
        calculate_vote_probability(&councilor, false, 0.0, false, false, 1.0, 0.0);
    assert!(
        disciplined_prob > base_prob,
        "Party discipline should increase vote probability"
    );
}

#[test]
fn test_councilor_trait_corrupt_vote() {
    let councilor = Councilor {
        id: "test_corrupt".to_string(),
        name: "Piotr Wiśniewski".to_string(),
        represented_class: "Aristocracy".to_string(),
        faction: Faction::Optimates,
        years_in_office: 10,
        political_influence: 90.0,
        hidden_trait: CouncilorTrait::Corrupt,
        trait_revealed: false,
        blackmail_material: Some("Skandal finansowy".to_string()),
        party: "Party C".to_string(),
        corruption_risk: 0.8,
    };

    // A penniless party (wealth 0.0) cannot buy discipline, so these cases
    // isolate the trait's own base rates.
    // Base 40% for corrupt
    let base_prob = calculate_vote_probability(&councilor, false, 0.5, false, false, 0.0, 0.0);
    assert!(
        (base_prob - 0.4).abs() < 0.1,
        "Corrupt base probability should be ~0.4, got {}",
        base_prob
    );

    // With bribe, should increase by ~40%
    let bribed_prob = calculate_vote_probability(&councilor, false, 0.5, true, false, 0.0, 0.0);
    assert!(
        bribed_prob > base_prob,
        "Bribe should increase vote probability"
    );
    assert!(
        (bribed_prob - 0.8).abs() < 0.1,
        "Corrupt with bribe should be ~0.8, got {}",
        bribed_prob
    );

    // With blackmail, should increase by ~20%
    let blackmailed_prob =
        calculate_vote_probability(&councilor, false, 0.5, false, true, 0.0, 0.0);
    assert!(
        blackmailed_prob > base_prob,
        "Blackmail should increase vote probability"
    );

    // Corrupt councilors only respond to discipline when the party is rich
    // enough to make it worth their while (wealth > 10_000).
    let poor_party = calculate_vote_probability(&councilor, false, 0.5, false, false, 1.0, 5_000.0);
    assert!(
        (poor_party - 0.4).abs() < 0.1,
        "Poor party discipline should not sway a corrupt councilor, got {}",
        poor_party
    );

    let rich_party =
        calculate_vote_probability(&councilor, false, 0.5, false, false, 1.0, 50_000.0);
    assert!(
        rich_party > poor_party,
        "A wealthy party's discipline should sway a corrupt councilor"
    );
}

#[test]
fn test_councilor_trait_maverick_vote() {
    let councilor = Councilor {
        id: "test_maverick".to_string(),
        name: "Marek Zieliński".to_string(),
        represented_class: "Burghers".to_string(),
        faction: Faction::Moderates,
        years_in_office: 3,
        political_influence: 55.0,
        hidden_trait: CouncilorTrait::Maverick,
        trait_revealed: false,
        blackmail_material: None,
        party: "Party A".to_string(),
        corruption_risk: 0.1,
    };

    // Maverick votes based on ideological alignment with randomness.
    // Mavericks ignore party discipline and wealth entirely by design.
    let high_alignment =
        calculate_vote_probability(&councilor, false, 1.0, false, false, 1.0, 100_000.0);
    let low_alignment =
        calculate_vote_probability(&councilor, false, 0.0, false, false, 1.0, 100_000.0);

    // High alignment should generally give higher probability
    assert!(
        high_alignment > low_alignment || (high_alignment - low_alignment).abs() < 0.5,
        "High ideological alignment should generally increase vote probability"
    );
}

#[test]
fn test_espionage_operation_creation() {
    let operation = EspionageState::create_operation(
        "OP-001".to_string(),
        "COUNCILOR-001".to_string(),
        30.0,
        EspionageType::Surveillance,
        10,
        0.5,
    );

    assert_eq!(operation.id, "OP-001");
    assert_eq!(operation.target_councilor_id, "COUNCILOR-001");
    assert_eq!(operation.budget, 30.0);
    assert_eq!(operation.operation_type, EspionageType::Surveillance);
    assert!(operation.completion_turn >= 11 && operation.completion_turn <= 12);
    // Success probability = budget/100 + corruption_level = 0.3 + 0.5 = 0.8
    assert!((operation.success_probability - 0.8).abs() < 0.01);
}

#[test]
fn test_espionage_bribery_operation() {
    let operation = EspionageState::create_operation(
        "OP-002".to_string(),
        "COUNCILOR-002".to_string(),
        50.0,
        EspionageType::Bribery,
        10,
        0.3,
    );

    assert_eq!(operation.completion_turn, 10); // Bribery is immediate
                                               // Success probability = budget/150 + corruption_level = 0.33 + 0.3 = 0.63
    assert!((operation.success_probability - 0.63).abs() < 0.01);
}

#[test]
fn test_espionage_blackmail_operation() {
    let operation = EspionageState::create_operation(
        "OP-003".to_string(),
        "COUNCILOR-003".to_string(),
        0.0,
        EspionageType::Blackmail,
        10,
        0.0,
    );

    assert_eq!(operation.completion_turn, 10); // Blackmail is immediate
    assert_eq!(operation.success_probability, 0.8); // Fixed high success if material exists
}

#[test]
fn test_coalition_with_concessions() {
    let mut parliament = HashMap::new();
    parliament.insert("Party A".to_string(), 45);
    parliament.insert("Party B".to_string(), 30);
    parliament.insert("Party C".to_string(), 25);

    let mut parties = HashMap::new();
    parties.insert(
        "Party A".to_string(),
        sim_engine::politics::Party {
            ideology: "Socjaldemokracja".to_string(),
            support: 45.0,
            profile: "Lewica".to_string(),
            economic_school: "Keynesizm".to_string(),
            base: vec!["Robotnicy".to_string()],
            id: "[PRT-001]".to_string(),
            leader: sim_engine::politics::Leader::default(),
            // Party finances (brokerage, loans, black money, donations, campaign
            // spending, organization) play no part in coalition arithmetic, which
            // is driven purely by seat counts and ideological distance.
            ..Default::default()
        },
    );
    parties.insert(
        "Party B".to_string(),
        sim_engine::politics::Party {
            ideology: "Liberalizm".to_string(),
            support: 30.0,
            profile: "Centrum".to_string(),
            economic_school: "Monetarystyczna".to_string(),
            base: vec!["Burżuazja".to_string()],
            id: "[PRT-002]".to_string(),
            leader: sim_engine::politics::Leader::default(),
            ..Default::default()
        },
    );
    parties.insert(
        "Party C".to_string(),
        sim_engine::politics::Party {
            ideology: "Konserwatyzm".to_string(),
            support: 25.0,
            profile: "Prawica".to_string(),
            economic_school: "Neoliberalizm".to_string(),
            base: vec!["Arystokracja".to_string()],
            id: "[PRT-003]".to_string(),
            leader: sim_engine::politics::Leader::default(),
            ..Default::default()
        },
    );

    let concessions = vec![ConcessionClause {
        description: "Increase hospital funding".to_string(),
        target: "Party B".to_string(),
        cost: 5.0,
        distance_reduction: 0.2,
    }];

    let (ruling, coalition, minority, _id, cost) =
        build_coalition_with_concessions(&parliament, &parties, &concessions);

    assert_eq!(ruling, "Party A");
    assert_eq!(cost, 5.0);
    // With concessions, should be able to form coalition more easily
    assert!(!coalition.is_empty() || minority);
}

#[test]
fn test_coalition_without_concessions() {
    let mut parliament = HashMap::new();
    parliament.insert("Party A".to_string(), 45);
    parliament.insert("Party B".to_string(), 30);
    parliament.insert("Party C".to_string(), 25);

    let mut parties = HashMap::new();
    parties.insert(
        "Party A".to_string(),
        sim_engine::politics::Party {
            ideology: "Socjaldemokracja".to_string(),
            support: 45.0,
            profile: "Lewica".to_string(),
            economic_school: "Keynesizm".to_string(),
            base: vec!["Robotnicy".to_string()],
            id: "[PRT-001]".to_string(),
            leader: sim_engine::politics::Leader::default(),
            ..Default::default()
        },
    );
    parties.insert(
        "Party B".to_string(),
        sim_engine::politics::Party {
            ideology: "Liberalizm".to_string(),
            support: 30.0,
            profile: "Centrum".to_string(),
            economic_school: "Monetarystyczna".to_string(),
            base: vec!["Burżuazja".to_string()],
            id: "[PRT-002]".to_string(),
            leader: sim_engine::politics::Leader::default(),
            ..Default::default()
        },
    );
    parties.insert(
        "Party C".to_string(),
        sim_engine::politics::Party {
            ideology: "Konserwatyzm".to_string(),
            support: 25.0,
            profile: "Prawica".to_string(),
            economic_school: "Neoliberalizm".to_string(),
            base: vec!["Arystokracja".to_string()],
            id: "[PRT-003]".to_string(),
            leader: sim_engine::politics::Leader::default(),
            ..Default::default()
        },
    );

    let (ruling, _coalition, _minority, _id, cost) =
        build_coalition_with_concessions(&parliament, &parties, &[]);

    assert_eq!(ruling, "Party A");
    assert_eq!(cost, 0.0); // No concessions = no cost
}

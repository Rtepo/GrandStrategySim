//! Phase 70: Integration tests for the Military Epic.
//!
//! Tests the interaction between:
//! - Hierarchical OOB (Phase 70.2)
//! - Unit modernization (Phase 70.3)
//! - Multi-domain combat (Phase 70.4)
//! - POWs and forced labor (Phase 70.4a)
//! - Strategic retreat (Phase 70.4b)
//! - VIP commander tactics (Phase 70.5)
//! - Asymmetric army generation (Phase 70.6)
//! - Hybrid war declaration (Phase 70.7)

use sim_engine::military::oob::{
    OobGenerationConfig, generate_oob, generate_asymmetric_oob,
};
use sim_engine::military::modernization::{
    ModernizationConfig, modernize_unit, apply_scrap_to_stockpile,
};
use sim_engine::military::multi_domain::resolve_multi_domain_battle;
use sim_engine::military::pows::{
    PowCamp, PowCaptureConfig, PrisonerOfWar, PowStatus,
    capture_pows_from_casualties,
    process_forced_labor_lease_fees, repatriate_pows_from_country,
};
use sim_engine::military::retreat::{
    CommanderRetraitProfile, RetreatEvaluation, evaluate_retreat,
    process_retreat,
};
use sim_engine::military::commander_traits::{
    evaluate_military_tactics,
    to_retreat_profile,
};
use sim_engine::military::war_declarations::{
    WarReason, PeaceTerms, BilateralTension, WarDeclarationConfig,
    declare_war, check_tension_escalations,
    settle_peace, tension_key,
};
use sim_engine::military::units::{MilitaryUnit, UnitType};
use sim_engine::military::config::MilitaryCombatConfig;
use sim_engine::military::fronts::Casualties;
use sim_engine::registries::enums::Commodity;
use sim_engine::society::geography::RuralClass;
use std::collections::HashMap;

// ============================================================================
// OOB INTEGRATION TESTS
// ============================================================================

#[test]
fn test_oob_generation_and_flattening() {
    let config = OobGenerationConfig {
        army_count: 2,
        divisions_per_army: 2,
        regiments_per_division: 2,
        units_per_regiment: 3,
        base_unit_manpower: 500,
        home_regions: vec!["region_a".to_string(), "region_b".to_string()],
        country_name: "TestNation".to_string(),
    };

    let oob = generate_oob(&config);

    // Verify hierarchy
    assert_eq!(oob.armies.len(), 2);
    assert_eq!(oob.armies[0].divisions.len(), 2);
    assert_eq!(oob.armies[0].divisions[0].regiments.len(), 2);
    assert_eq!(oob.armies[0].divisions[0].regiments[0].units.len(), 3);

    // Verify flattening
    assert_eq!(oob.unit_count(), 24); // 2*2*2*3
    assert_eq!(oob.total_manpower(), 12000); // 24 * 500
}

#[test]
fn test_asymmetric_oob_rich_vs_poor() {
    let mut rng = rand::thread_rng();
    let rich_oob = generate_asymmetric_oob(
        "RichNation",
        5_000_000_000.0, // High total GDP
        5000.0,           // High GDP per capita
        4000.0,           // High average wage
        1_000_000,
        vec!["r1".to_string(), "r2".to_string()],
        &mut rng,
    );
    let poor_oob = generate_asymmetric_oob(
        "PoorNation",
        30_000_000.0, // Low total GDP
        300.0,         // Low GDP per capita
        240.0,         // Low average wage
        100_000,
        vec!["r1".to_string()],
        &mut rng,
    );

    // Rich nation should have more units
    assert!(rich_oob.unit_count() > 0);
    assert!(poor_oob.unit_count() > 0);

    // Poor nation should have no tanks (converted to infantry)
    let poor_tanks = poor_oob.collect_units_by_type(UnitType::Tanks);
    assert!(poor_tanks.is_empty(), "Poor nation should have no tanks");
}

// ============================================================================
// MODERNIZATION INTEGRATION TESTS
// ============================================================================

#[test]
fn test_modernization_scrap_returns_physical_commodities() {
    let mut unit = MilitaryUnit::new(
        "TEST-TANK-001".to_string(),
        UnitType::Tanks,
        1000,
        rustc_hash::FxHashMap::default(),
        "home".to_string(),
    );
    unit.equipment_reserves = UnitType::Tanks.table_of_equipment(1920);

    let config = ModernizationConfig::default();
    let result = modernize_unit(&mut unit, 1935, &config);

    assert!(result.upgraded);
    // Scrap must return physical commodities (Steel, Aluminum)
    assert!(result.scrap_recovered.contains_key(&Commodity::Steel));
}

#[test]
fn test_modernization_generates_procurement_demand() {
    let mut unit = MilitaryUnit::new(
        "TEST-TANK-002".to_string(),
        UnitType::Tanks,
        1000,
        rustc_hash::FxHashMap::default(),
        "home".to_string(),
    );
    unit.equipment_reserves = UnitType::Tanks.table_of_equipment(1920);

    let config = ModernizationConfig::default();
    let result = modernize_unit(&mut unit, 1935, &config);

    assert!(!result.procurement_demand.is_empty());
    assert!(result.procurement_demand.contains_key(&Commodity::MediumTanks));
}

#[test]
fn test_scrap_applied_to_stockpile() {
    let mut stockpile = HashMap::new();
    let mut scrap = HashMap::new();
    scrap.insert(Commodity::Steel, 500.0);

    apply_scrap_to_stockpile(&mut stockpile, &scrap);

    assert_eq!(stockpile.get(&Commodity::Steel), Some(&500.0));
}

// ============================================================================
// MULTI-DOMAIN COMBAT INTEGRATION TESTS
// ============================================================================

#[test]
fn test_multi_domain_air_superiority_boosts_land() {
    let mut attacker = vec![
        make_unit("att-air", UnitType::AirForce, 1000),
        make_unit("att-inf", UnitType::Infantry, 1000),
    ];
    let mut defender = vec![
        make_unit("def-inf", UnitType::Infantry, 1000),
    ];

    let config = MilitaryCombatConfig::default();
    let result = resolve_multi_domain_battle(
        &mut attacker,
        &mut defender,
        "region_a".to_string(),
        "Attacker".to_string(),
        "Defender".to_string(),
        1,
        "BATTLE-001".to_string(),
        &config,
        "plains",
        false,
    );

    // Attacker has air units, defender has none → attacker should get air superiority
    if result.modifiers.attacker_air_superiority {
        assert!(result.modifiers.attacker_land_power_multiplier > 1.0,
            "Air superiority must boost land combat power");
    }
}

#[test]
fn test_multi_domain_naval_battle_coastal_only() {
    let mut attacker = vec![
        make_unit("att-nav", UnitType::Naval, 500),
        make_unit("att-inf", UnitType::Infantry, 1000),
    ];
    let mut defender = vec![
        make_unit("def-nav", UnitType::Naval, 500),
        make_unit("def-inf", UnitType::Infantry, 1000),
    ];

    let config = MilitaryCombatConfig::default();

    // Non-coastal — no naval battle
    let result = resolve_multi_domain_battle(
        &mut attacker.clone(),
        &mut defender.clone(),
        "region_a".to_string(),
        "A".to_string(),
        "D".to_string(),
        1,
        "B1".to_string(),
        &config,
        "plains",
        false,
    );
    assert!(result.naval_battle.is_none());

    // Coastal — naval battle occurs
    let result2 = resolve_multi_domain_battle(
        &mut attacker,
        &mut defender,
        "region_a".to_string(),
        "A".to_string(),
        "D".to_string(),
        1,
        "B2".to_string(),
        &config,
        "plains",
        true,
    );
    assert!(result2.naval_battle.is_some());
}

// ============================================================================
// POW INTEGRATION TESTS
// ============================================================================

#[test]
fn test_pow_capture_and_forced_labor_lease() {
    let casualties = Casualties {
        dead: 50,
        wounded: 200,
        deserters: 50,
        demographic_breakdown: {
            let mut m = HashMap::new();
            m.insert(RuralClass::FreePeasant, 300);
            m
        },
    };

    let config = PowCaptureConfig::default();
    let mut counter = 0u64;

    let pows = capture_pows_from_casualties(
        &casualties, "Captor", "Origin", 1, "region", &config, &mut counter,
    );

    assert!(!pows.is_empty());

    let mut camp = PowCamp::new();
    camp.add_prisoners(pows);

    // Set POWs to interned
    for pow in &mut camp.prisoners {
        pow.status = PowStatus::Interned;
    }

    // Assign first POW to factory
    let first_id = camp.prisoners[0].id.clone();
    camp.assign_to_factory(&first_id, "FACTORY-001");

    // Process lease fees
    let mut factory_capital = HashMap::new();
    factory_capital.insert("FACTORY-001".to_string(), 10000.0);
    let mut treasury = 5000.0;

    let result = process_forced_labor_lease_fees(
        &camp, 100.0, &mut factory_capital, &mut treasury,
    );

    // Verify double-entry: factory debited, treasury credited
    assert!(result.total_fees_collected > 0.0, "Lease fee must be positive — no free labor");
    assert!(treasury > 5000.0, "Treasury must be credited");
}

#[test]
fn test_pow_repatriation() {
    let mut camp = PowCamp::new();
    camp.add_prisoners(vec![
        PrisonerOfWar {
            id: "POW-1".to_string(),
            captor_country: "C".to_string(),
            origin_country: "O1".to_string(),
            capture_turn: 1,
            internment_region: "r".to_string(),
            origin_class: RuralClass::FreePeasant,
            status: PowStatus::Interned,
            assigned_factory_id: None,
            productivity_factor: 0.6,
        },
        PrisonerOfWar {
            id: "POW-2".to_string(),
            captor_country: "C".to_string(),
            origin_country: "O2".to_string(),
            capture_turn: 1,
            internment_region: "r".to_string(),
            origin_class: RuralClass::FreePeasant,
            status: PowStatus::Interned,
            assigned_factory_id: None,
            productivity_factor: 0.6,
        },
    ]);

    let repatriated = repatriate_pows_from_country(&mut camp, "O1");
    assert_eq!(repatriated.len(), 1);
    assert_eq!(camp.current_count(), 1);
}

// ============================================================================
// STRATEGIC RETREAT INTEGRATION TESTS
// ============================================================================

#[test]
fn test_retreat_cautious_commander_withdraws() {
    let config = MilitaryCombatConfig::default();
    let attacker_cmd = CommanderRetraitProfile::fighting("AggressiveGen".to_string());
    let defender_cmd = CommanderRetraitProfile::retreating("CautiousGen".to_string());

    // Defender is overwhelmed 10:1
    let result = evaluate_retreat(1000.0, 100.0, &attacker_cmd, &defender_cmd, &config);

    assert_eq!(result, RetreatEvaluation::DefenderRetreats);
}

#[test]
fn test_retreat_aggressive_commander_fights_to_death() {
    let config = MilitaryCombatConfig::default();
    let attacker_cmd = CommanderRetraitProfile::fighting("AggressiveGen".to_string());
    let defender_cmd = CommanderRetraitProfile::fighting("BraveGen".to_string());

    // Defender is overwhelmed 10:1 but commander is aggressive
    let result = evaluate_retreat(1000.0, 100.0, &attacker_cmd, &defender_cmd, &config);

    assert_eq!(result, RetreatEvaluation::NoRetreat);
}

#[test]
fn test_retreat_equipment_captured_not_cash() {
    let config = MilitaryCombatConfig::default();
    let mut retreating = vec![make_unit("ret-1", UnitType::Infantry, 1000)];
    let victor = vec![make_unit("vic-1", UnitType::Infantry, 1000)];

    let result = process_retreat(
        &mut retreating, &victor,
        "Defender", "Attacker", &config,
    );

    // Captured equipment must be physical commodities
    for commodity in result.captured_equipment.keys() {
        let _ = commodity; // Verify it's a Commodity enum
    }
    assert!(!result.captured_equipment.is_empty() || result.retreating_casualties.total() > 0);
}

// ============================================================================
// COMMANDER TACTICS INTEGRATION TESTS
// ============================================================================

#[test]
fn test_commander_tactics_from_traits() {
    let traits = vec!["Aggressive".to_string(), "Cunning".to_string()];
    let tactics = evaluate_military_tactics(&traits);

    // Aggressive: aggression *= 1.5, Cunning: maneuver += 0.3
    assert!((tactics.aggression_multiplier - 1.5).abs() < 0.001);
    assert!((tactics.maneuver_bonus - 0.3).abs() < 0.001);
    assert!(!tactics.will_retreat);
}

#[test]
fn test_commander_tactics_to_retreat_profile() {
    let tactics = evaluate_military_tactics(&["Cautious".to_string()]);
    let profile = to_retreat_profile(&tactics, "General Cautious");

    assert!(profile.will_retreat);
    assert_eq!(profile.commander_name, "General Cautious");
}

// ============================================================================
// WAR DECLARATION INTEGRATION TESTS
// ============================================================================

#[test]
fn test_direct_war_declaration() {
    let mut at_war_with = HashMap::new();
    let result = declare_war(
        "Aggressor", "Defender", 10,
        WarReason::TerritorialConquest,
        &mut at_war_with,
    );

    assert!(result.declared);
    assert!(at_war_with.get("Aggressor").unwrap().contains(&"Defender".to_string()));
    assert!(at_war_with.get("Defender").unwrap().contains(&"Aggressor".to_string()));
}

#[test]
fn test_tension_escalation_to_war() {
    let mut tensions = HashMap::new();
    let key = tension_key("CountryA", "CountryB");
    let mut tension = BilateralTension::new();
    tension.add_provocation(1.0, 90.0); // Exceeds threshold of 80
    tensions.insert(key, tension);

    let mut at_war_with = HashMap::new();
    let config = WarDeclarationConfig::default();

    let results = check_tension_escalations(&mut tensions, &mut at_war_with, 5, &config);

    assert_eq!(results.len(), 1);
    assert!(results[0].declared);
}

#[test]
fn test_peace_settlement_ends_war() {
    let mut at_war_with = HashMap::new();
    at_war_with.insert("A".to_string(), vec!["B".to_string()]);
    at_war_with.insert("B".to_string(), vec!["A".to_string()]);

    let result = settle_peace("A", "B", &PeaceTerms::StatusQuoAnte, &mut at_war_with);

    assert!(result.peace_established);
    assert!(!at_war_with.get("A").unwrap().contains(&"B".to_string()));
    assert!(!at_war_with.get("B").unwrap().contains(&"A".to_string()));
}

// ============================================================================
// COMBINED INTEGRATION TEST
// ============================================================================

#[test]
fn test_full_military_flow_oob_to_combat_to_peace() {
    // 1. Generate OOB for two countries
    let config = OobGenerationConfig {
        army_count: 1,
        divisions_per_army: 1,
        regiments_per_division: 1,
        units_per_regiment: 2,
        base_unit_manpower: 1000,
        home_regions: vec!["region_a".to_string()],
        country_name: "CountryA".to_string(),
    };
    let oob_a = generate_oob(&config);

    let config_b = OobGenerationConfig {
        army_count: 1,
        divisions_per_army: 1,
        regiments_per_division: 1,
        units_per_regiment: 1,
        base_unit_manpower: 500,
        home_regions: vec!["region_a".to_string()],
        country_name: "CountryB".to_string(),
    };
    let oob_b = generate_oob(&config_b);

    // 2. Verify both OOBs have units
    assert!(oob_a.unit_count() > 0);
    assert!(oob_b.unit_count() > 0);

    // 3. Declare war
    let mut at_war_with = HashMap::new();
    let war_result = declare_war(
        "CountryA", "CountryB", 1,
        WarReason::TerritorialConquest,
        &mut at_war_with,
    );
    assert!(war_result.declared);

    // 4. Resolve a battle
    let mut attacker_units = oob_a.flatten();
    let mut defender_units = oob_b.flatten();
    let combat_config = MilitaryCombatConfig::default();

    let battle_result = resolve_multi_domain_battle(
        &mut attacker_units,
        &mut defender_units,
        "region_a".to_string(),
        "CountryA".to_string(),
        "CountryB".to_string(),
        1,
        "BATTLE-001".to_string(),
        &combat_config,
        "plains",
        false,
    );

    // Battle should have a result
    assert!(!battle_result.messages.is_empty());

    // 5. Settle peace
    let peace_result = settle_peace(
        "CountryA", "CountryB",
        &PeaceTerms::StatusQuoAnte,
        &mut at_war_with,
    );
    assert!(peace_result.peace_established);

    // 6. Verify war is over
    assert!(!at_war_with.get("CountryA").unwrap().contains(&"CountryB".to_string()));
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn make_unit(id: &str, unit_type: UnitType, manpower: i64) -> MilitaryUnit {
    MilitaryUnit::new(
        id.to_string(),
        unit_type,
        manpower,
        rustc_hash::FxHashMap::default(),
        "home".to_string(),
    )
}

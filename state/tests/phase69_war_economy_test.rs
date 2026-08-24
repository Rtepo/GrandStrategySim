//! Phase 69: Integration tests for the war economy system.
//!
//! Tests verify:
//! - Production decree swaps building active_method to military method
//! - Production decree restores original method when lifted
//! - Production decree with distinct physical inputs (Rule 3)
//! - Conscription drains population from demographics (Rule 1)
//! - Conscription creates units with manpower_origin tracking
//! - Conscription applies labor participation penalty
//! - War bond issuance debits savings and credits treasury (Rule 1)
//! - War bond respects GDP cap
//! - Peacetime conscription drafts zero recruits
//! - Expired decrees are automatically cleaned up

use sim_engine::entities::{Building, ActiveProductionMethod};
use sim_engine::military::war_economy::{
    WarEconomyState, WarEconomyConfig, ConscriptionLevel,
    apply_production_decree, lift_production_decree, process_expired_decrees,
    execute_conscription, issue_war_bonds,
};
use sim_engine::military::oob::OrderOfBattle;
use sim_engine::military::units::UnitType;
use sim_engine::registries::enums::{Commodity, Sector};
use sim_engine::society::geography::{
    Region, RegionalClassDemographics, ClassDemographics,
};
use sim_engine::state::Country;
use std::collections::BTreeMap;

// ============================================================================
// HELPERS
// ============================================================================

fn make_test_region(id: &str, population: i64) -> Region {
    let mut region = Region::default();
    region.id = id.to_string();
    region.display_name = id.to_string();
    region.population = population;
    region.gdp = 1_000_000.0;

    let mut rural_classes = BTreeMap::new();
    rural_classes.insert(
        "FreePeasant".to_string(),
        ClassDemographics {
            population: population / 2,
            savings: 50_000.0,
            labor_participation: 0.8,
            ..Default::default()
        },
    );
    rural_classes.insert(
        "LandlessLaborer".to_string(),
        ClassDemographics {
            population: population / 2,
            savings: 20_000.0,
            labor_participation: 0.9,
            ..Default::default()
        },
    );

    let mut urban_classes = BTreeMap::new();
    urban_classes.insert(
        "Bourgeoisie".to_string(),
        ClassDemographics {
            population: population / 4,
            savings: 100_000.0,
            labor_participation: 0.7,
            ..Default::default()
        },
    );

    region.class_demographics = RegionalClassDemographics {
        rural_classes,
        urban_classes,
    };

    region
}

fn make_military_building(id: &str, sector: Sector) -> Building {
    Building {
        id: id.to_string(),
        name: format!("Factory {}", id),
        sector,
        current_employment: 100,
        active_method: ActiveProductionMethod {
            year: 1930,
            efficiency: 1.0,
            inputs: BTreeMap::from([(Commodity::Steel, 50.0)]),
            outputs: BTreeMap::from([(Commodity::Steel, 100.0)]),
            ..Default::default()
        },
        ..Default::default()
    }
}

// ============================================================================
// PRODUCTION DECREE TESTS
// ============================================================================

#[test]
fn test_decree_swaps_to_military_method_with_distinct_inputs() {
    let mut buildings = vec![make_military_building("b1", Sector::HeavyIndustry)];

    let military_method = ActiveProductionMethod {
        year: 1935,
        efficiency: 0.9,
        inputs: BTreeMap::from([
            (Commodity::Steel, 30.0),
            (Commodity::Aluminum, 20.0),
        ]),
        outputs: BTreeMap::from([(Commodity::MediumTanks, 10.0)]),
        ..Default::default()
    };

    let decree = apply_production_decree(
        &mut buildings,
        Sector::HeavyIndustry,
        &military_method,
        "tank_production",
        10,
        Some(20),
        0.15,
    );

    assert!(decree.is_some());
    // Verify: military method has DIFFERENT physical inputs than original (Rule 3)
    assert!(buildings[0].active_method.inputs.contains_key(&Commodity::Aluminum));
    assert!(!buildings[0].active_method.inputs.contains_key(&Commodity::Steel)
        || buildings[0].active_method.inputs.get(&Commodity::Steel) != Some(&50.0));
    // Verify: output is now military
    assert!(buildings[0].active_method.outputs.contains_key(&Commodity::MediumTanks));
    // Verify: civilian output is gone
    assert!(!buildings[0].active_method.outputs.contains_key(&Commodity::Steel)
        || buildings[0].active_method.outputs.get(&Commodity::Steel) == Some(&0.0)
        || !buildings[0].active_method.outputs.contains_key(&Commodity::Steel));
}

#[test]
fn test_decree_restores_original_method() {
    let mut buildings = vec![make_military_building("b1", Sector::HeavyIndustry)];
    let original_outputs = buildings[0].active_method.outputs.clone();

    let military_method = ActiveProductionMethod {
        year: 1935,
        efficiency: 0.9,
        outputs: BTreeMap::from([(Commodity::MediumTanks, 10.0)]),
        ..Default::default()
    };

    let decree = apply_production_decree(
        &mut buildings,
        Sector::HeavyIndustry,
        &military_method,
        "tank_production",
        10,
        None,
        0.15,
    ).unwrap();

    lift_production_decree(&mut buildings, &decree);

    assert_eq!(buildings[0].active_method.outputs, original_outputs);
}

#[test]
fn test_decree_skips_unemployed_buildings() {
    let mut buildings = vec![
        Building {
            id: "b1".to_string(),
            sector: Sector::HeavyIndustry,
            current_employment: 100,
            ..Default::default()
        },
        Building {
            id: "b2".to_string(),
            sector: Sector::HeavyIndustry,
            current_employment: 0, // Unemployed — should be skipped
            ..Default::default()
        },
    ];

    let military_method = ActiveProductionMethod::default();
    let decree = apply_production_decree(
        &mut buildings,
        Sector::HeavyIndustry,
        &military_method,
        "test",
        10,
        None,
        0.0,
    ).unwrap();

    assert_eq!(decree.affected_building_ids.len(), 1);
    assert_eq!(decree.affected_building_ids[0], "b1");
}

#[test]
fn test_decree_retooling_penalty_reduces_efficiency() {
    let mut buildings = vec![make_military_building("b1", Sector::HeavyIndustry)];

    let military_method = ActiveProductionMethod {
        efficiency: 1.0,
        ..Default::default()
    };

    let _ = apply_production_decree(
        &mut buildings,
        Sector::HeavyIndustry,
        &military_method,
        "test",
        10,
        None,
        0.20, // 20% retooling penalty
    );

    assert!((buildings[0].active_method.efficiency - 0.80).abs() < 0.001);
}

// ============================================================================
// CONSCRIPTION TESTS
// ============================================================================

#[test]
fn test_conscription_drains_population_from_demographics() {
    let mut regions = vec![make_test_region("r1", 10_000)];
    let mut units = OrderOfBattle::default();
    let mut war_economy = WarEconomyState {
        conscription_level: ConscriptionLevel::Selective,
        ..Default::default()
    };
    let config = WarEconomyConfig::default();

    let original_total_pop: i64 = regions[0].class_demographics.rural_classes.values()
        .chain(regions[0].class_demographics.urban_classes.values())
        .map(|d| d.population)
        .sum();

    let result = execute_conscription(
        &mut regions,
        &mut units,
        &mut war_economy,
        &config,
        "TestCountry",
        1,
    );

    assert!(result.recruits_drafted > 0);
    assert_eq!(units.unit_count(), 1);

    let new_total_pop: i64 = regions[0].class_demographics.rural_classes.values()
        .chain(regions[0].class_demographics.urban_classes.values())
        .map(|d| d.population)
        .sum();

    // Population must have decreased by exactly the number of recruits drafted (Rule 1)
    assert_eq!(original_total_pop - new_total_pop, result.recruits_drafted);
}

#[test]
fn test_conscription_creates_unit_with_manpower_origin() {
    let mut regions = vec![make_test_region("r1", 10_000)];
    let mut units = OrderOfBattle::default();
    let mut war_economy = WarEconomyState {
        conscription_level: ConscriptionLevel::UniversalDraft,
        ..Default::default()
    };
    let config = WarEconomyConfig::default();

    let result = execute_conscription(
        &mut regions,
        &mut units,
        &mut war_economy,
        &config,
        "TestCountry",
        1,
    );

    assert!(!result.manpower_origin.is_empty());
    assert_eq!(units.unit_count(), 1);
    let all_units = units.collect_all_units();
    assert_eq!(all_units[0].unit_type, UnitType::Infantry);
    assert_eq!(all_units[0].manpower, result.recruits_drafted);

    // Verify manpower_origin matches the result
    let total_origin: i64 = all_units[0].manpower_origin.values().sum();
    assert_eq!(total_origin, result.recruits_drafted);
}

#[test]
fn test_conscription_applies_labor_penalty() {
    let mut regions = vec![make_test_region("r1", 10_000)];
    let mut units = OrderOfBattle::default();
    let mut war_economy = WarEconomyState {
        conscription_level: ConscriptionLevel::TotalMobilization,
        ..Default::default()
    };
    let config = WarEconomyConfig::default();

    let original_labor: f64 = regions[0].class_demographics.rural_classes
        .get("FreePeasant")
        .map(|d| d.labor_participation)
        .unwrap_or(0.0);

    let result = execute_conscription(
        &mut regions,
        &mut units,
        &mut war_economy,
        &config,
        "TestCountry",
        1,
    );

    assert!(result.labor_penalty_applied > 0.0);

    let new_labor: f64 = regions[0].class_demographics.rural_classes
        .get("FreePeasant")
        .map(|d| d.labor_participation)
        .unwrap_or(0.0);

    assert!(new_labor < original_labor);
}

#[test]
fn test_peacetime_conscription_drafts_zero() {
    let mut regions = vec![make_test_region("r1", 10_000)];
    let mut units = OrderOfBattle::default();
    let mut war_economy = WarEconomyState {
        conscription_level: ConscriptionLevel::Peacetime,
        ..Default::default()
    };
    let config = WarEconomyConfig::default();

    let result = execute_conscription(
        &mut regions,
        &mut units,
        &mut war_economy,
        &config,
        "TestCountry",
        1,
    );

    assert_eq!(result.recruits_drafted, 0);
    assert_eq!(units.unit_count(), 0);
}

#[test]
fn test_conscription_tracks_total_drafted() {
    let mut regions = vec![make_test_region("r1", 10_000)];
    let mut units = OrderOfBattle::default();
    let mut war_economy = WarEconomyState {
        conscription_level: ConscriptionLevel::Selective,
        ..Default::default()
    };
    let config = WarEconomyConfig::default();

    let result = execute_conscription(
        &mut regions,
        &mut units,
        &mut war_economy,
        &config,
        "TestCountry",
        1,
    );

    assert_eq!(war_economy.total_conscripts_drafted, result.recruits_drafted);
}

// ============================================================================
// WAR BOND TESTS
// ============================================================================

#[test]
fn test_war_bond_credits_treasury_and_debits_savings() {
    let mut country = Country::default();
    country.name = "TestCountry".to_string();
    country.regions = vec![make_test_region("r1", 10_000)];
    country.war_economy.conscription_level = ConscriptionLevel::Selective;

    let original_treasury = country.budget.liquid_reserves;
    let original_savings: f64 = country.regions.iter()
        .flat_map(|r| r.class_demographics.rural_classes.values()
            .chain(r.class_demographics.urban_classes.values()))
        .map(|d| d.savings)
        .sum();

    let config = WarEconomyConfig::default();
    let amount = 10_000.0;

    let raised = issue_war_bonds(&mut country, amount, &config, 1, 1000.0);

    assert!(raised > 0.0);
    assert!(raised <= amount);

    // Treasury must have increased
    assert!(country.budget.liquid_reserves > original_treasury);
    assert!((country.budget.liquid_reserves - original_treasury - raised).abs() < 0.01);

    // Savings must have decreased (at least partially — retail portion)
    let new_savings: f64 = country.regions.iter()
        .flat_map(|r| r.class_demographics.rural_classes.values()
            .chain(r.class_demographics.urban_classes.values()))
        .map(|d| d.savings)
        .sum();
    assert!(new_savings < original_savings);

    // War economy state must track the bond
    assert!(country.war_economy.war_bonds_issued > 0.0);
    assert_eq!(country.war_economy.outstanding_war_bond_ids.len(), 1);
}

#[test]
fn test_war_bond_respects_gdp_cap() {
    let mut country = Country::default();
    country.name = "TestCountry".to_string();
    country.regions = vec![make_test_region("r1", 10_000)];
    // GDP = 1_000_000.0, max_war_bond_gdp_fraction = 0.25 → max = 250_000
    // But savings are only ~170_000, so actual issuance is capped by subscription capacity

    let config = WarEconomyConfig::default();
    let requested = 1_000_000.0; // Way more than GDP cap

    let raised = issue_war_bonds(&mut country, requested, &config, 1, 1000.0);

    let gdp: f64 = country.regions.iter().map(|r| r.gdp).sum();
    let max_issuance = gdp * config.max_war_bond_gdp_fraction;

    assert!(raised <= max_issuance + 0.01, "War bond issuance {} exceeds GDP cap {}", raised, max_issuance);
}

#[test]
fn test_war_bond_zero_amount_returns_zero() {
    let mut country = Country::default();
    country.name = "TestCountry".to_string();
    let config = WarEconomyConfig::default();

    let raised = issue_war_bonds(&mut country, 0.0, &config, 1, 1000.0);
    assert_eq!(raised, 0.0);
    assert_eq!(country.war_economy.war_bonds_issued, 0.0);
}

#[test]
fn test_war_bond_adds_security_to_debt_market() {
    let mut country = Country::default();
    country.name = "TestCountry".to_string();
    country.regions = vec![make_test_region("r1", 10_000)];

    let config = WarEconomyConfig::default();
    let original_count = country.debt_market.outstanding_securities.len();

    let _raised = issue_war_bonds(&mut country, 5_000.0, &config, 1, 1000.0);

    assert_eq!(country.debt_market.outstanding_securities.len(), original_count + 1);
}

// ============================================================================
// EXPIRED DECREE CLEANUP TESTS
// ============================================================================

#[test]
fn test_expired_decree_auto_cleanup() {
    let mut buildings = vec![make_military_building("b1", Sector::HeavyIndustry)];
    let original_outputs = buildings[0].active_method.outputs.clone();

    let military_method = ActiveProductionMethod {
        outputs: BTreeMap::from([(Commodity::MediumTanks, 10.0)]),
        ..Default::default()
    };

    let decree = apply_production_decree(
        &mut buildings,
        Sector::HeavyIndustry,
        &military_method,
        "tank_production",
        10,
        Some(15), // Expires at turn 15
        0.15,
    ).unwrap();

    let mut war_economy = WarEconomyState {
        active_decrees: vec![decree],
        ..Default::default()
    };

    // Turn 14: not expired
    process_expired_decrees(&mut buildings, &mut war_economy, 14);
    assert_eq!(war_economy.active_decrees.len(), 1);

    // Turn 15: expired — should be cleaned up
    process_expired_decrees(&mut buildings, &mut war_economy, 15);
    assert_eq!(war_economy.active_decrees.len(), 0);
    assert_eq!(buildings[0].active_method.outputs, original_outputs);
}

// ============================================================================
// FULL WAR ECONOMY FLOW TEST
// ============================================================================

#[test]
fn test_full_war_economy_flow() {
    // 1. Set up country at war
    let mut country = Country::default();
    country.name = "WarNation".to_string();
    country.regions = vec![make_test_region("r1", 20_000)];
    country.at_war_with = vec!["EnemyNation".to_string()];
    country.war_economy.conscription_level = ConscriptionLevel::UniversalDraft;

    // 2. Execute conscription
    let mut units = OrderOfBattle::default();
    let config = WarEconomyConfig::default();

    let conscription_result = execute_conscription(
        &mut country.regions,
        &mut units,
        &mut country.war_economy,
        &config,
        &country.name,
        1,
    );

    assert!(conscription_result.recruits_drafted > 0);
    assert_eq!(units.unit_count(), 1);
    assert!(country.war_economy.total_conscripts_drafted > 0);

    // 3. Issue war bonds to finance the war
    let original_treasury = country.budget.liquid_reserves;
    let bond_amount = 50_000.0;
    let raised = issue_war_bonds(&mut country, bond_amount, &config, 1, 1000.0);

    assert!(raised > 0.0);
    assert!(country.budget.liquid_reserves > original_treasury);

    // 4. Apply production decree to convert industry to military
    let mut buildings = vec![make_military_building("b1", Sector::HeavyIndustry)];
    let military_method = ActiveProductionMethod {
        year: 1935,
        efficiency: 0.9,
        inputs: BTreeMap::from([
            (Commodity::Steel, 30.0),
            (Commodity::Aluminum, 20.0),
        ]),
        outputs: BTreeMap::from([(Commodity::MediumTanks, 10.0)]),
        ..Default::default()
    };

    let decree = apply_production_decree(
        &mut buildings,
        Sector::HeavyIndustry,
        &military_method,
        "tank_production",
        1,
        Some(10),
        0.15,
    );

    assert!(decree.is_some());
    assert!(buildings[0].active_method.outputs.contains_key(&Commodity::MediumTanks));

    // 5. Verify the full cycle: conscription + war bonds + production decree
    assert!(country.war_economy.total_conscripts_drafted > 0);
    assert!(country.war_economy.war_bonds_issued > 0.0);
    assert!(buildings[0].active_method.outputs.contains_key(&Commodity::MediumTanks));
}

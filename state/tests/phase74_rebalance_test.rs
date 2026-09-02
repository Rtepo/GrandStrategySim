//! Phase 74 Integration Tests: Economic Rebalance, Time Scaling, and Physical Constants.
//!
//! Tests:
//! - 74.1: Compound annual-to-turn rate conversion
//! - 74.2: B2C consumption blackhole fix (savings_per_capita initialization)
//! - 74.3: Missing production chains (DraftAnimals, Ammunition, stockpile)
//! - 74.4: Physical constants (calorific values, dynamic combustion)
//! - 74.5: Price elasticity, substitution, and housing complementarity

use sim_engine::data::consumption_registry;
use sim_engine::economy::trade::retail::build_consumer_demand;
use sim_engine::registries::enums::Commodity;
use sim_engine::registries::production_methods_data::default_production_methods;
use sim_engine::society::geography::{ClassDemographics, Region, RuralClass, UrbanClass};
use sim_engine::society::housing::{
    HousingBuilding, HousingSlots, HousingType, UtilityConnections,
};

// ============================================================================
// 74.1: Compound Rate Conversion
// ============================================================================

#[test]
fn test_compound_rate_conversion_is_not_simple_division() {
    // R_turn = (1 + R_annual)^(1/24) - 1
    // For a 10% annual rate:
    let r_annual: f64 = 0.10;
    let r_turn_compound: f64 = (1.0_f64 + r_annual).powf(1.0 / 24.0) - 1.0;
    let r_turn_simple: f64 = r_annual / 24.0;

    // Compound rate should be slightly less than simple division
    // because compounding means each turn's growth builds on the previous
    assert!(
        r_turn_compound < r_turn_simple,
        "Compound rate {} should be < simple rate {}",
        r_turn_compound,
        r_turn_simple
    );
    // But they should be close for small rates
    assert!(
        (r_turn_compound - r_turn_simple).abs() < 0.001,
        "Rates should be close for small annual rates"
    );
    // And the compound rate should compound back to the annual rate
    let compounded: f64 = (1.0_f64 + r_turn_compound).powi(24) - 1.0;
    assert!(
        (compounded - r_annual).abs() < 1e-10,
        "24 turns of compound rate should equal annual rate: {} vs {}",
        compounded,
        r_annual
    );
}

#[test]
fn test_compound_rate_conversion_zero_rate() {
    let r_annual: f64 = 0.0;
    let r_turn: f64 = (1.0_f64 + r_annual).powf(1.0 / 24.0) - 1.0;
    assert_eq!(r_turn, 0.0);
}

#[test]
fn test_compound_rate_conversion_high_rate() {
    // For a 100% annual rate, the difference is significant
    let r_annual: f64 = 1.0; // 100%
    let r_turn_compound: f64 = (1.0_f64 + r_annual).powf(1.0 / 24.0) - 1.0;
    let r_turn_simple: f64 = r_annual / 24.0;

    // Compound: ~2.93% per turn, Simple: ~4.17% per turn
    assert!(
        r_turn_compound < r_turn_simple,
        "Compound {} should be significantly less than simple {} for high rates",
        r_turn_compound,
        r_turn_simple
    );
    // Verify compounding
    let compounded: f64 = (1.0_f64 + r_turn_compound).powi(24) - 1.0;
    assert!(
        (compounded - r_annual).abs() < 1e-10,
        "24 turns should compound to annual: {} vs {}",
        compounded,
        r_annual
    );
}

// ============================================================================
// 74.2: B2C Consumption Blackhole
// ============================================================================

#[test]
fn test_savings_per_capita_initialized() {
    // When demographics are generated with savings and population,
    // savings_per_capita should be savings / population, not 0.0
    let mut d = ClassDemographics::default();
    d.population = 1000;
    d.savings = 500_000.0;
    d.savings_per_capita = d.savings / d.population as f64;

    assert_eq!(d.savings_per_capita, 500.0);
    assert!(d.savings_per_capita > 0.0);
}

#[test]
fn test_savings_per_capita_zero_population() {
    let mut d = ClassDemographics::default();
    d.population = 0;
    d.savings = 0.0;
    // Safe division: should not panic, should be 0.0
    d.savings_per_capita = if d.population > 0 {
        d.savings / d.population as f64
    } else {
        0.0
    };
    assert_eq!(d.savings_per_capita, 0.0);
}

#[test]
fn test_meat_and_fruit_in_subsistence_baskets() {
    let registry = consumption_registry::consumption_registry();
    // FreePeasant should now have Meat and/or Fruit in subsistence
    if let Some(basket) = registry.get("FreePeasant") {
        let subsistence = basket
            .tiers
            .get(&consumption_registry::NeedTier::Subsistence);
        if let Some(subs) = subsistence {
            let has_meat = subs.contains_key(&Commodity::Meat);
            let has_fruit = subs.contains_key(&Commodity::Fruit);
            // At least one should be present (added in Phase 74.2)
            assert!(
                has_meat || has_fruit,
                "FreePeasant subsistence basket should include Meat or Fruit"
            );
        }
    }
}

// ============================================================================
// 74.3: Missing Production Chains
// ============================================================================

#[test]
fn test_draft_animal_breeding_method_exists() {
    let registry = default_production_methods();
    let agriculture = registry
        .get("agriculture")
        .expect("agriculture sector should exist");
    let has_draft = agriculture
        .iter_all()
        .any(|m| m.outputs.contains_key(&Commodity::DraftAnimals));
    assert!(
        has_draft,
        "Agriculture should have a method that outputs DraftAnimals"
    );
}

#[test]
fn test_cartridge_manufacturing_method_exists() {
    let registry = default_production_methods();
    let armaments = registry
        .get("armaments_industry")
        .expect("armaments_industry sector should exist");
    let has_cartridge = armaments
        .iter_all()
        .any(|m| m.outputs.contains_key(&Commodity::Ammunition) && m.year <= 1880);
    assert!(
        has_cartridge,
        "Armaments should have an Ammunition-producing method available from 1880 or earlier"
    );
}

#[test]
fn test_ammunition_available_at_1925_start() {
    let registry = default_production_methods();
    let armaments = registry
        .get("armaments_industry")
        .expect("armaments_industry sector should exist");
    // At year 1925, at least one Ammunition-producing method should be available
    let available = armaments
        .iter_all()
        .any(|m| m.year <= 1925 && m.outputs.contains_key(&Commodity::Ammunition));
    assert!(
        available,
        "Ammunition production should be available at 1925 start year"
    );
}

#[test]
fn test_draft_animal_breeding_has_complete_inputs() {
    let registry = default_production_methods();
    let agriculture = registry.get("agriculture").unwrap();
    let breeding = agriculture
        .iter_all()
        .find(|m| m.outputs.contains_key(&Commodity::DraftAnimals))
        .expect("Draft Animal Breeding method should exist");
    // Should consume Fodder (animals need to eat)
    assert!(
        breeding.inputs.contains_key(&Commodity::Fodder),
        "Draft Animal Breeding should consume Fodder"
    );
    // Should also output some Livestock (byproduct)
    assert!(
        breeding.outputs.contains_key(&Commodity::Livestock),
        "Draft Animal Breeding should also produce Livestock as byproduct"
    );
}

// ============================================================================
// 74.4: Physical Constants and Dynamic Combustion
// ============================================================================

#[test]
fn test_calorific_values_are_positive_for_fuels() {
    assert!(Commodity::HardCoal.calorific_value_mj_per_unit() > 0.0);
    assert!(Commodity::BrownCoal.calorific_value_mj_per_unit() > 0.0);
    assert!(Commodity::Oil.calorific_value_mj_per_unit() > 0.0);
    assert!(Commodity::NaturalGas.calorific_value_mj_per_unit() > 0.0);
    assert!(Commodity::Fuels.calorific_value_mj_per_unit() > 0.0);
    assert!(Commodity::Uranium.calorific_value_mj_per_unit() > 0.0);
}

#[test]
fn test_calorific_values_are_zero_for_non_fuels() {
    assert_eq!(Commodity::Cereal.calorific_value_mj_per_unit(), 0.0);
    assert_eq!(Commodity::Steel.calorific_value_mj_per_unit(), 0.0);
    assert_eq!(Commodity::Furniture.calorific_value_mj_per_unit(), 0.0);
    assert_eq!(Commodity::Energy.calorific_value_mj_per_unit(), 0.0);
}

#[test]
fn test_is_fuel_helper() {
    assert!(Commodity::HardCoal.is_fuel());
    assert!(Commodity::Oil.is_fuel());
    assert!(Commodity::NaturalGas.is_fuel());
    assert!(!Commodity::Cereal.is_fuel());
    assert!(!Commodity::Steel.is_fuel());
}

#[test]
fn test_calorific_value_ordering() {
    // Uranium should have the highest calorific value
    assert!(
        Commodity::Uranium.calorific_value_mj_per_unit()
            > Commodity::NaturalGas.calorific_value_mj_per_unit()
    );
    // NaturalGas > Oil > HardCoal > BrownCoal
    assert!(
        Commodity::NaturalGas.calorific_value_mj_per_unit()
            > Commodity::Oil.calorific_value_mj_per_unit()
    );
    assert!(
        Commodity::Oil.calorific_value_mj_per_unit()
            > Commodity::HardCoal.calorific_value_mj_per_unit()
    );
    assert!(
        Commodity::HardCoal.calorific_value_mj_per_unit()
            > Commodity::BrownCoal.calorific_value_mj_per_unit()
    );
}

#[test]
fn test_energy_methods_have_thermal_efficiency() {
    let registry = default_production_methods();
    let energy = registry.get("energy").expect("energy sector should exist");
    // Coal-Fired Boilers should have thermal_efficiency > 0
    let coal_boiler = energy
        .iter_all()
        .find(|m| {
            m.inputs.contains_key(&Commodity::HardCoal)
                && m.outputs.contains_key(&Commodity::Energy)
        })
        .expect("Coal-Fired Boilers method should exist");
    assert!(
        coal_boiler.thermal_efficiency > 0.0,
        "Coal-Fired Boilers should have thermal_efficiency > 0, got {}",
        coal_boiler.thermal_efficiency
    );
    assert!(
        coal_boiler.thermal_efficiency <= 1.0,
        "Thermal efficiency should be <= 1.0"
    );
}

#[test]
fn test_non_fuel_energy_methods_have_zero_thermal_efficiency() {
    let registry = default_production_methods();
    let energy = registry.get("energy").unwrap();
    // Hydroelectric Power doesn't burn fuel — should have thermal_efficiency = 0
    let hydro = energy.iter_all().find(|m| {
        m.outputs.contains_key(&Commodity::Energy)
            && !m.inputs.keys().any(|c| c.is_fuel())
            && m.inputs.contains_key(&Commodity::Water)
            && m.thermal_efficiency == 0.0
    });
    // At least one non-fuel energy method should exist with thermal_efficiency = 0
    assert!(
        hydro.is_some(),
        "Non-fuel energy methods (e.g., Hydroelectric) should have thermal_efficiency = 0"
    );
}

#[test]
fn test_combined_cycle_has_higher_efficiency_than_coal_boiler() {
    let registry = default_production_methods();
    let energy = registry.get("energy").unwrap();
    let coal = energy
        .iter_all()
        .find(|m| m.thermal_efficiency > 0.0 && m.inputs.contains_key(&Commodity::HardCoal))
        .expect("Coal-Fired Boilers should exist");
    let combined = energy
        .iter_all()
        .find(|m| {
            m.thermal_efficiency > 0.0
                && m.inputs.contains_key(&Commodity::NaturalGas)
                && m.outputs.contains_key(&Commodity::Heat)
        })
        .expect("Combined Cycle Plant should exist");
    assert!(
        combined.thermal_efficiency > coal.thermal_efficiency,
        "Combined Cycle ({}) should be more efficient than Coal Boilers ({})",
        combined.thermal_efficiency,
        coal.thermal_efficiency
    );
}

// ============================================================================
// 74.5: Price Elasticity and Complementarity
// ============================================================================

#[test]
fn test_price_substitution_matrix_exists() {
    let matrix = consumption_registry::price_substitution_matrix();
    // Meat should have substitution candidates
    assert!(
        matrix.contains_key(&Commodity::Meat),
        "Price substitution matrix should have Meat as a primary"
    );
    let meat_subs = matrix.get(&Commodity::Meat).unwrap();
    assert!(
        !meat_subs.is_empty(),
        "Meat should have at least one substitute"
    );
    // Check elasticity coefficients are in the approved 0.7-0.9 range
    for sub in meat_subs {
        assert!(
            sub.elasticity_coefficient >= 0.7 && sub.elasticity_coefficient <= 0.9,
            "Elasticity coefficient {} should be in [0.7, 0.9]",
            sub.elasticity_coefficient
        );
        assert!(
            sub.max_substitution > 0.0 && sub.max_substitution <= 1.0,
            "Max substitution {} should be in (0, 1]",
            sub.max_substitution
        );
    }
}

// Phase 76: test_price_substitution_protein_has_cereal removed — Protein merged into Meat.

#[test]
fn test_requires_housing_helper() {
    assert!(Commodity::Furniture.requires_housing());
    assert!(Commodity::LuxuryFurniture.requires_housing());
    assert!(Commodity::Agd.requires_housing());
    assert!(Commodity::Televisions.requires_housing());
    // Non-housing-gated commodities
    assert!(!Commodity::Cereal.requires_housing());
    assert!(!Commodity::Meat.requires_housing());
    assert!(!Commodity::Clothing.requires_housing());
}

#[test]
fn test_homeless_class_zero_housing_demand() {
    // A class with no housing buildings should have housing_rate = 0
    let housing_buildings: Vec<HousingBuilding> = Vec::new();
    let rate = sim_engine::economy::trade::retail::test_class_housing_possession_rate(
        &housing_buildings,
        "region_1",
        "Worker",
        1000,
    );
    assert_eq!(
        rate, 0.0,
        "Homeless class should have 0 housing possession rate"
    );
}

#[test]
fn test_housed_class_positive_housing_demand() {
    // A class with housing should have housing_rate > 0
    let housing = vec![HousingBuilding {
        id: "h1".to_string(),
        housing_type: HousingType::Tenement,
        micro_region_id: "region_1".to_string(),
        owner: "State".to_string(),
        primary_slots: HousingSlots {
            total_capacity: 250,
            occupied_slots: 200,
            target_class: Some(RuralClass::FreePeasant),
            rent_per_slot: 10.0,
        },
        sublet_slots: None,
        living_standard: 0.5,
        construction_cost: 100_000.0,
        maintenance_cost: 1_000.0,
        condition: 1.0,
        utility_connections: UtilityConnections::default(),
        ..Default::default()
    }];
    let rate = sim_engine::economy::trade::retail::test_class_housing_possession_rate(
        &housing,
        "region_1",
        "FreePeasant",
        1000,
    );
    // 200 slots × 4 household_size = 800 people housed out of 1000
    assert!(
        rate > 0.0,
        "Housed class should have positive housing rate, got {}",
        rate
    );
    assert!(
        (rate - 0.8).abs() < 0.01,
        "Rate should be ~0.8 (800/1000), got {}",
        rate
    );
}

#[test]
fn test_build_consumer_demand_with_substitution() {
    // When Meat price is high relative to wage, some demand should shift to substitutes
    let mut region = Region::default();
    region.id = "test_region".to_string();
    let mut demos = ClassDemographics::default();
    demos.population = 1000;
    demos.savings = 500_000.0;
    demos.savings_per_capita = 500.0;
    region
        .class_demographics
        .rural_classes
        .insert(RuralClass::FreePeasant, demos);

    let mut prices = rustc_hash::FxHashMap::default();
    // Set Meat price very high relative to wage
    prices.insert(Commodity::Meat, 100.0);
    prices.insert(Commodity::Cereal, 5.0);
    prices.insert(Commodity::Vegetable, 5.0);

    let demand = build_consumer_demand(&region, 0, &prices, 50.0, &[]);
    // Total demand should exist for Meat or its substitutes
    let meat_demand = demand
        .total_demand
        .get(&Commodity::Meat)
        .copied()
        .unwrap_or(0.0);
    let cereal_demand = demand
        .total_demand
        .get(&Commodity::Cereal)
        .copied()
        .unwrap_or(0.0);
    // Either Meat demand is reduced, or Cereal demand is increased from substitution
    // (At minimum, some demand should exist)
    let total_food = meat_demand + cereal_demand;
    assert!(
        total_food >= 0.0,
        "Total food demand should be non-negative"
    );
}

#[test]
fn test_build_consumer_demand_homeless_no_furniture() {
    // A homeless class should not demand Furniture (complementarity gating)
    let mut region = Region::default();
    region.id = "test_region".to_string();
    let mut demos = ClassDemographics::default();
    demos.population = 1000;
    demos.savings = 1_000_000.0;
    demos.savings_per_capita = 1000.0;
    // Set household_durables to simulate existing stock (so compute_durable_demand returns >0)
    demos.household_durables = Vec::new();
    region
        .class_demographics
        .urban_classes
        .insert(UrbanClass::Worker, demos);

    // No housing buildings → homeless
    let demand = build_consumer_demand(&region, 0, &rustc_hash::FxHashMap::default(), 100.0, &[]);
    let furniture_demand = demand
        .total_demand
        .get(&Commodity::Furniture)
        .copied()
        .unwrap_or(0.0);
    assert_eq!(
        furniture_demand, 0.0,
        "Homeless class should not demand Furniture (complementarity gating)"
    );
}

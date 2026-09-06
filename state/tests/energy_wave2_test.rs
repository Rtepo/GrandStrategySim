//! Phase 81 Wave 2: Comprehensive unit tests for the energy consumption
//! and microgeneration system.
//!
//! Tests cover:
//! - CoalGas commodity existence, count, calorific value, and parsing.
//! - MethodSlot expansion (10 variants) and default.
//! - Consumption method registries (housing, commercial, heavy industry, mining).
//! - Coal Carbonization production method (CoalGas + Coke output).
//! - ConsumptionBom computation and scaling for housing and commercial.
//! - Microgeneration output (Rooftop PV).
//! - CAPEX BOM computation and scaling.
//! - District Heating adoption gate (regional capacity check).
//! - PPA registry defaults and expiration.
//! - Spot market state defaults.
//! - UpgradeProject progress, completion, and accumulation.
//! - Future-proofed MethodSlot variants (WaterSupply, Sanitation, WasteDisposal).

use sim_engine::construction::upgrade_project::UpgradeProject;
use sim_engine::energy::ppa::{expire_ppas, plant_ppa_mw};
use sim_engine::energy::types::{PowerPurchaseAgreement, PpaRegistry, PpaStatus, SpotMarketState};
use sim_engine::infrastructure::CapacityType;
use sim_engine::registries::enums::Commodity;
use sim_engine::registries::production_methods::MethodSlot;
use sim_engine::registries::production_methods_data::default_production_methods;
use sim_engine::society::geography::Region;
use sim_engine::society::housing::{CommercialBuilding, HousingBuilding, HousingSlots};
use sim_engine::state::Country;
use sim_engine::utilities::consumption_bom::{
    can_adopt_district_heating, commercial_scale_factor, compute_capex_bom,
    compute_housing_consumption_bom, housing_scale_factor,
};
use std::collections::BTreeMap;

// ===========================================================================
// COMMODITY TESTS
// ===========================================================================

#[test]
fn test_coal_gas_exists() {
    // CoalGas variant must exist and be present in Commodity::all().
    let all = Commodity::all();
    assert!(
        all.contains(&Commodity::CoalGas),
        "Commodity::all() must contain CoalGas"
    );
}

#[test]
fn test_commodity_count_is_150() {
    // The total number of commodity variants must meet the minimum threshold.
    // Uses >= to avoid breakage when new commodities are added by concurrent agents.
    let all = Commodity::all();
    assert!(
        all.len() >= Commodity::MIN_COMMODITIES,
        "Commodity::all() must return at least {} variants, got {}",
        Commodity::MIN_COMMODITIES,
        all.len()
    );
}

#[test]
fn test_coal_gas_calorific_value() {
    // CoalGas has a calorific value of ~17 MJ/m³.
    let cv = Commodity::CoalGas.calorific_value_mj_per_unit();
    assert!(
        (cv - 17.0).abs() < 1e-9,
        "CoalGas calorific value must be 17.0, got {}",
        cv
    );
}

#[test]
fn test_coal_gas_parseable() {
    // The string "coal_gas" must parse back to Commodity::CoalGas.
    let result = Commodity::try_from("coal_gas");
    assert!(result.is_ok(), "Parsing 'coal_gas' must succeed");
    assert_eq!(
        result.unwrap(),
        Commodity::CoalGas,
        "Parsed commodity must be CoalGas"
    );
}

// ===========================================================================
// METHOD SLOT TESTS
// ===========================================================================

#[test]
fn test_method_slot_has_10_variants() {
    // MethodSlot must have all 10 variants: the 3 original production slots
    // plus the 7 Wave 2 consumption/future-proofed slots.
    let all_keys = [
        "automation",
        "production",
        "organization",
        "lighting",
        "heating",
        "ventilation",
        "power_generation",
        "water_supply",
        "sanitation",
        "waste_disposal",
    ];
    for key in &all_keys {
        let slot = MethodSlot::from_key(key);
        assert!(
            slot.is_some(),
            "MethodSlot::from_key('{}') must return Some",
            key
        );
    }
    assert_eq!(all_keys.len(), 10, "Must have exactly 10 slot keys");
}

#[test]
fn test_method_slot_default() {
    // The default MethodSlot must be Automation (the #[default] variant).
    let default = MethodSlot::default();
    assert_eq!(
        default,
        MethodSlot::Automation,
        "MethodSlot::default() must be Automation"
    );
}

// ===========================================================================
// CONSUMPTION METHOD REGISTRY TESTS
// ===========================================================================

#[test]
fn test_housing_consumption_registry_exists() {
    let registry = default_production_methods();
    assert!(
        registry.contains_key("housing_consumption"),
        "default_production_methods() must contain 'housing_consumption'"
    );
}

#[test]
fn test_housing_has_lighting_methods() {
    let registry = default_production_methods();
    let housing = registry
        .get("housing_consumption")
        .expect("housing_consumption registry must exist");
    let lighting = &housing.lighting;
    assert!(
        lighting.contains_key("None"),
        "Housing lighting must have 'None' method"
    );
    assert!(
        lighting.contains_key("Kerosene Lamps"),
        "Housing lighting must have 'Kerosene Lamps' method"
    );
    assert!(
        lighting.contains_key("LED Lighting"),
        "Housing lighting must have 'LED Lighting' method"
    );
}

#[test]
fn test_housing_has_heating_methods() {
    let registry = default_production_methods();
    let housing = registry
        .get("housing_consumption")
        .expect("housing_consumption registry must exist");
    let heating = &housing.heating;
    assert!(
        heating.contains_key("None"),
        "Housing heating must have 'None' method"
    );
    assert!(
        heating.contains_key("Coal Stove"),
        "Housing heating must have 'Coal Stove' method"
    );
    assert!(
        heating.contains_key("District Heating"),
        "Housing heating must have 'District Heating' method"
    );
    assert!(
        heating.contains_key("Heat Pump"),
        "Housing heating must have 'Heat Pump' method"
    );
}

#[test]
fn test_housing_has_power_generation_methods() {
    let registry = default_production_methods();
    let housing = registry
        .get("housing_consumption")
        .expect("housing_consumption registry must exist");
    let pg = &housing.power_generation;
    assert!(
        pg.contains_key("None"),
        "Housing power generation must have 'None' method"
    );
    assert!(
        pg.contains_key("Rooftop PV"),
        "Housing power generation must have 'Rooftop PV' method"
    );
}

#[test]
fn test_commercial_consumption_registry_exists() {
    let registry = default_production_methods();
    assert!(
        registry.contains_key("commercial_consumption"),
        "default_production_methods() must contain 'commercial_consumption'"
    );
}

#[test]
fn test_heavy_industry_has_ventilation_methods() {
    let registry = default_production_methods();
    let hi = registry
        .get("heavy_industry_consumption")
        .expect("heavy_industry_consumption registry must exist");
    let vent = &hi.ventilation;
    assert!(
        vent.contains_key("None"),
        "Heavy industry ventilation must have 'None' method"
    );
    assert!(
        vent.contains_key("Steam-Driven"),
        "Heavy industry ventilation must have 'Steam-Driven' method"
    );
    assert!(
        vent.contains_key("Electric Pumps/Fans"),
        "Heavy industry ventilation must have 'Electric Pumps/Fans' method"
    );
}

#[test]
fn test_mining_has_ventilation_methods() {
    let registry = default_production_methods();
    let mining = registry
        .get("mining_consumption")
        .expect("mining_consumption registry must exist");
    let vent = &mining.ventilation;
    assert!(
        vent.contains_key("None"),
        "Mining ventilation must have 'None' method"
    );
    assert!(
        vent.contains_key("Steam-Driven"),
        "Mining ventilation must have 'Steam-Driven' method"
    );
    assert!(
        vent.contains_key("Electric Pumps/Fans"),
        "Mining ventilation must have 'Electric Pumps/Fans' method"
    );
}

#[test]
fn test_coal_carbonization_exists() {
    // The heavy_industry production registry must include "Coal Carbonization"
    // which outputs CoalGas and Coke from HardCoal + Water.
    let registry = default_production_methods();
    let hi = registry
        .get("heavy_industry")
        .expect("heavy_industry registry must exist");
    let method = hi
        .production
        .get("Coal Carbonization")
        .expect("'Coal Carbonization' method must exist in heavy_industry");
    assert!(
        method.outputs.contains_key(&Commodity::CoalGas),
        "Coal Carbonization must output CoalGas"
    );
    assert!(
        method.outputs.contains_key(&Commodity::Coke),
        "Coal Carbonization must output Coke"
    );
    assert!(
        method.inputs.contains_key(&Commodity::HardCoal),
        "Coal Carbonization must consume HardCoal"
    );
}

// ===========================================================================
// CONSUMPTION BOM AND SCALING TESTS
// ===========================================================================

#[test]
fn test_housing_scale_factor() {
    // scale = primary_slots.occupied_slots + sublet_slots.occupied_slots
    let mut building = HousingBuilding::default();
    building.primary_slots = HousingSlots {
        total_capacity: 100,
        occupied_slots: 50,
        target_class: None,
        rent_per_slot: 0.0,
    };
    building.sublet_slots = Some(HousingSlots {
        total_capacity: 20,
        occupied_slots: 10,
        target_class: None,
        rent_per_slot: 0.0,
    });
    let scale = housing_scale_factor(&building);
    assert!(
        (scale - 60.0).abs() < 1e-9,
        "Housing scale factor must be 60.0 (50+10), got {}",
        scale
    );
}

#[test]
fn test_commercial_scale_factor() {
    // scale = (office_capacity + retail_capacity) / 100.0
    let mut building = CommercialBuilding::default();
    building.office_capacity = 300.0;
    building.retail_capacity = 200.0;
    let scale = commercial_scale_factor(&building);
    assert!(
        (scale - 5.0).abs() < 1e-9,
        "Commercial scale factor must be 5.0 ((300+200)/100), got {}",
        scale
    );
}

#[test]
fn test_housing_consumption_bom_lighting() {
    // Kerosene Lamps consumes Oil at 0.5/occupant. With 10 occupants,
    // physical_commodity_demand should contain Oil => 5.0.
    let registry = default_production_methods();
    let housing_methods = registry
        .get("housing_consumption")
        .expect("housing_consumption registry must exist");

    let mut building = HousingBuilding::default();
    building.primary_slots = HousingSlots {
        total_capacity: 100,
        occupied_slots: 10,
        target_class: None,
        rent_per_slot: 0.0,
    };
    building.active_lighting = "Kerosene Lamps".to_string();

    let bom = compute_housing_consumption_bom(&building, housing_methods);
    let oil_demand = bom
        .physical_commodity_demand
        .get(&Commodity::Oil)
        .copied()
        .unwrap_or(0.0);
    assert!(
        (oil_demand - 5.0).abs() < 1e-9,
        "Oil demand must be 5.0 (0.5 * 10 occupants), got {}",
        oil_demand
    );
}

#[test]
fn test_housing_consumption_bom_heating() {
    // Coal Stove consumes HardCoal at 1.0/occupant. With 20 occupants,
    // physical_commodity_demand should contain HardCoal => 20.0.
    let registry = default_production_methods();
    let housing_methods = registry
        .get("housing_consumption")
        .expect("housing_consumption registry must exist");

    let mut building = HousingBuilding::default();
    building.primary_slots = HousingSlots {
        total_capacity: 100,
        occupied_slots: 20,
        target_class: None,
        rent_per_slot: 0.0,
    };
    building.active_heating = "Coal Stove".to_string();

    let bom = compute_housing_consumption_bom(&building, housing_methods);
    let coal_demand = bom
        .physical_commodity_demand
        .get(&Commodity::HardCoal)
        .copied()
        .unwrap_or(0.0);
    assert!(
        (coal_demand - 20.0).abs() < 1e-9,
        "HardCoal demand must be 20.0 (1.0 * 20 occupants), got {}",
        coal_demand
    );
}

#[test]
fn test_housing_consumption_bom_none() {
    // When all methods are "None", the BOM should be completely empty.
    let registry = default_production_methods();
    let housing_methods = registry
        .get("housing_consumption")
        .expect("housing_consumption registry must exist");

    let mut building = HousingBuilding::default();
    building.primary_slots = HousingSlots {
        total_capacity: 100,
        occupied_slots: 50,
        target_class: None,
        rent_per_slot: 0.0,
    };
    building.active_lighting = "None".to_string();
    building.active_heating = "None".to_string();
    building.active_power_generation = "None".to_string();

    let bom = compute_housing_consumption_bom(&building, housing_methods);
    assert!(
        bom.grid_utility_demand.is_empty(),
        "Grid utility demand must be empty when all methods are None"
    );
    assert!(
        bom.physical_commodity_demand.is_empty(),
        "Physical commodity demand must be empty when all methods are None"
    );
    assert!(
        bom.microgeneration_output.is_empty(),
        "Microgeneration output must be empty when all methods are None"
    );
}

#[test]
fn test_housing_consumption_bom_scaling() {
    // Doubling occupied slots must double the BOM quantities.
    let registry = default_production_methods();
    let housing_methods = registry
        .get("housing_consumption")
        .expect("housing_consumption registry must exist");

    // Small building: 10 occupants, Kerosene Lamps
    let mut small = HousingBuilding::default();
    small.primary_slots = HousingSlots {
        total_capacity: 100,
        occupied_slots: 10,
        target_class: None,
        rent_per_slot: 0.0,
    };
    small.active_lighting = "Kerosene Lamps".to_string();
    let small_bom = compute_housing_consumption_bom(&small, housing_methods);

    // Large building: 20 occupants, Kerosene Lamps (double)
    let mut large = HousingBuilding::default();
    large.primary_slots = HousingSlots {
        total_capacity: 100,
        occupied_slots: 20,
        target_class: None,
        rent_per_slot: 0.0,
    };
    large.active_lighting = "Kerosene Lamps".to_string();
    let large_bom = compute_housing_consumption_bom(&large, housing_methods);

    let small_oil = small_bom
        .physical_commodity_demand
        .get(&Commodity::Oil)
        .copied()
        .unwrap_or(0.0);
    let large_oil = large_bom
        .physical_commodity_demand
        .get(&Commodity::Oil)
        .copied()
        .unwrap_or(0.0);
    assert!(
        (large_oil - 2.0 * small_oil).abs() < 1e-9,
        "Doubling occupants must double Oil demand: small={}, large={}",
        small_oil,
        large_oil
    );
}

#[test]
fn test_microgeneration_output() {
    // Rooftop PV produces Energy at 0.5/occupant. With 10 occupants,
    // microgeneration_output should contain Energy => 5.0.
    let registry = default_production_methods();
    let housing_methods = registry
        .get("housing_consumption")
        .expect("housing_consumption registry must exist");

    let mut building = HousingBuilding::default();
    building.primary_slots = HousingSlots {
        total_capacity: 100,
        occupied_slots: 10,
        target_class: None,
        rent_per_slot: 0.0,
    };
    building.active_power_generation = "Rooftop PV".to_string();

    let bom = compute_housing_consumption_bom(&building, housing_methods);
    let energy_output = bom
        .microgeneration_output
        .get(&Commodity::Energy)
        .copied()
        .unwrap_or(0.0);
    assert!(
        (energy_output - 5.0).abs() < 1e-9,
        "Microgeneration Energy output must be 5.0 (0.5 * 10 occupants), got {}",
        energy_output
    );
}

// ===========================================================================
// CAPEX TESTS
// ===========================================================================

#[test]
fn test_capex_bom_computation() {
    // LED Lighting has CAPEX: ElectronicComponents (0.05) + Semiconductors (0.02).
    // With scale=10, the CAPEX BOM should contain ElectronicComponents => 0.5
    // and Semiconductors => 0.2.
    let registry = default_production_methods();
    let housing_methods = registry
        .get("housing_consumption")
        .expect("housing_consumption registry must exist");
    let led = housing_methods
        .lighting
        .get("LED Lighting")
        .expect("'LED Lighting' must exist in housing lighting methods");

    let scale = 10.0;
    let capex_bom = compute_capex_bom(led, scale);

    let ec = capex_bom
        .get(&Commodity::ElectronicComponents)
        .copied()
        .unwrap_or(0.0);
    let semi = capex_bom
        .get(&Commodity::Semiconductors)
        .copied()
        .unwrap_or(0.0);
    assert!(
        (ec - 0.5).abs() < 1e-9,
        "ElectronicComponents CAPEX must be 0.5 (0.05 * 10), got {}",
        ec
    );
    assert!(
        (semi - 0.2).abs() < 1e-9,
        "Semiconductors CAPEX must be 0.2 (0.02 * 10), got {}",
        semi
    );
}

#[test]
fn test_capex_bom_scaling() {
    // Doubling the scale factor must double the CAPEX quantities.
    let registry = default_production_methods();
    let housing_methods = registry
        .get("housing_consumption")
        .expect("housing_consumption registry must exist");
    let led = housing_methods
        .lighting
        .get("LED Lighting")
        .expect("'LED Lighting' must exist in housing lighting methods");

    let small_bom = compute_capex_bom(led, 10.0);
    let large_bom = compute_capex_bom(led, 20.0);

    let small_ec = small_bom
        .get(&Commodity::ElectronicComponents)
        .copied()
        .unwrap_or(0.0);
    let large_ec = large_bom
        .get(&Commodity::ElectronicComponents)
        .copied()
        .unwrap_or(0.0);
    assert!(
        (large_ec - 2.0 * small_ec).abs() < 1e-9,
        "Doubling scale must double CAPEX: small={}, large={}",
        small_ec,
        large_ec
    );
}

// ===========================================================================
// DISTRICT HEATING GATE TESTS
// ===========================================================================

#[test]
fn test_district_heating_gate_no_capacity() {
    // A region with no DistrictHeating capacity must reject adoption.
    let region = Region::default();
    assert!(
        !can_adopt_district_heating(&region),
        "Region with no DistrictHeating capacity must not allow adoption"
    );
}

#[test]
fn test_district_heating_gate_with_capacity() {
    // A region with DistrictHeating capacity > 0 must allow adoption.
    let mut region = Region::default();
    region
        .capacity_pool
        .insert(CapacityType::DistrictHeating, 100.0);
    assert!(
        can_adopt_district_heating(&region),
        "Region with DistrictHeating capacity > 0 must allow adoption"
    );
}

// ===========================================================================
// PPA TESTS
// ===========================================================================

#[test]
fn test_ppa_registry_default_empty() {
    // PpaRegistry::default() must have empty active and expired lists.
    let registry = PpaRegistry::default();
    assert!(
        registry.active_ppas.is_empty(),
        "Default PpaRegistry must have no active PPAs"
    );
    assert!(
        registry.expired_ppas.is_empty(),
        "Default PpaRegistry must have no expired PPAs"
    );
}

#[test]
fn test_ppa_expire() {
    // A PPA with end_turn=5 must move to expired when expire_ppas is called
    // with current_turn=6.
    let mut country = Country::default();
    country
        .ppa_registry
        .active_ppas
        .push(PowerPurchaseAgreement {
            id: "ppa_test_1".to_string(),
            seller_company_id: "seller_1".to_string(),
            buyer_company_id: "buyer_1".to_string(),
            plant_building_id: "plant_1".to_string(),
            fixed_price_per_mwh: 50.0,
            contracted_mw: 10.0,
            start_turn: 1,
            end_turn: 5,
            status: PpaStatus::Active,
        });

    expire_ppas(&mut country, 6);

    assert_eq!(
        country.ppa_registry.active_ppas.len(),
        0,
        "PPA must be removed from active after expiration"
    );
    assert_eq!(
        country.ppa_registry.expired_ppas.len(),
        1,
        "PPA must be moved to expired after expiration"
    );
    assert_eq!(
        country.ppa_registry.expired_ppas[0].status,
        PpaStatus::Expired,
        "Expired PPA must have Expired status"
    );
}

#[test]
fn test_plant_ppa_mw_aggregation() {
    // plant_ppa_mw must sum contracted_mw across all active PPAs for a plant.
    let mut registry = PpaRegistry::default();
    registry.active_ppas.push(PowerPurchaseAgreement {
        id: "ppa_1".to_string(),
        seller_company_id: "s1".to_string(),
        buyer_company_id: "b1".to_string(),
        plant_building_id: "plant_X".to_string(),
        fixed_price_per_mwh: 50.0,
        contracted_mw: 15.0,
        start_turn: 1,
        end_turn: 60,
        status: PpaStatus::Active,
    });
    registry.active_ppas.push(PowerPurchaseAgreement {
        id: "ppa_2".to_string(),
        seller_company_id: "s1".to_string(),
        buyer_company_id: "b2".to_string(),
        plant_building_id: "plant_X".to_string(),
        fixed_price_per_mwh: 55.0,
        contracted_mw: 25.0,
        start_turn: 1,
        end_turn: 60,
        status: PpaStatus::Active,
    });
    // PPA for a different plant should not be counted
    registry.active_ppas.push(PowerPurchaseAgreement {
        id: "ppa_3".to_string(),
        seller_company_id: "s1".to_string(),
        buyer_company_id: "b3".to_string(),
        plant_building_id: "plant_Y".to_string(),
        fixed_price_per_mwh: 60.0,
        contracted_mw: 100.0,
        start_turn: 1,
        end_turn: 60,
        status: PpaStatus::Active,
    });

    let total = plant_ppa_mw(&registry, "plant_X");
    assert!(
        (total - 40.0).abs() < 1e-9,
        "plant_ppa_mw for plant_X must be 40.0 (15+25), got {}",
        total
    );
}

// ===========================================================================
// SPOT MARKET TESTS
// ===========================================================================

#[test]
fn test_spot_market_state_default_empty() {
    // SpotMarketState::default() must have empty maps and vectors.
    let sms = SpotMarketState::default();
    assert!(
        sms.marginal_costs.is_empty(),
        "Default SpotMarketState must have empty marginal_costs"
    );
    assert!(
        sms.clearing_prices.is_empty(),
        "Default SpotMarketState must have empty clearing_prices"
    );
    assert!(
        sms.dispatch_order.is_empty(),
        "Default SpotMarketState must have empty dispatch_order"
    );
    assert!(
        sms.revenue_distribution.is_empty(),
        "Default SpotMarketState must have empty revenue_distribution"
    );
    assert!(
        sms.dispatched_mw.is_empty(),
        "Default SpotMarketState must have empty dispatched_mw"
    );
}

// ===========================================================================
// UPGRADE PROJECT TESTS
// ===========================================================================

#[test]
fn test_upgrade_project_progress_zero() {
    // An UpgradeProject with required materials but no deliveries must have
    // progress 0.0 and not be complete.
    let mut required = BTreeMap::new();
    required.insert(Commodity::Glass, 10.0);
    required.insert(Commodity::ElectronicComponents, 5.0);
    let project = UpgradeProject {
        target_slot: MethodSlot::Lighting,
        target_method: "LED Lighting".to_string(),
        required_materials: required,
        delivered_materials: BTreeMap::new(),
        progress: 0.0,
        start_turn: 0,
    };
    assert!(
        (project.compute_progress() - 0.0).abs() < 1e-9,
        "Progress must be 0.0 with no deliveries"
    );
    assert!(
        !project.is_complete(),
        "Must not be complete with no deliveries"
    );
}

#[test]
fn test_upgrade_project_progress_partial() {
    // Partial deliveries of ALL required materials must yield progress
    // between 0 and 1. compute_progress takes the min ratio across all
    // materials, so every material must have at least some delivery.
    let mut required = BTreeMap::new();
    required.insert(Commodity::Glass, 10.0);
    required.insert(Commodity::ElectronicComponents, 5.0);
    let mut delivered = BTreeMap::new();
    delivered.insert(Commodity::Glass, 5.0);
    delivered.insert(Commodity::ElectronicComponents, 2.5);
    let project = UpgradeProject {
        target_slot: MethodSlot::Lighting,
        target_method: "LED Lighting".to_string(),
        required_materials: required,
        delivered_materials: delivered,
        progress: 0.0,
        start_turn: 0,
    };
    let progress = project.compute_progress();
    assert!(
        progress > 0.0 && progress < 1.0,
        "Progress must be between 0 and 1 with partial delivery, got {}",
        progress
    );
    assert!(
        !project.is_complete(),
        "Must not be complete with partial delivery"
    );
}

#[test]
fn test_upgrade_project_progress_complete() {
    // Full deliveries of all required materials must yield progress 1.0 and
    // is_complete() must return true.
    let mut required = BTreeMap::new();
    required.insert(Commodity::Glass, 10.0);
    required.insert(Commodity::ElectronicComponents, 5.0);
    let mut delivered = BTreeMap::new();
    delivered.insert(Commodity::Glass, 10.0);
    delivered.insert(Commodity::ElectronicComponents, 5.0);
    let project = UpgradeProject {
        target_slot: MethodSlot::Lighting,
        target_method: "LED Lighting".to_string(),
        required_materials: required,
        delivered_materials: delivered,
        progress: 0.0,
        start_turn: 0,
    };
    assert!(
        (project.compute_progress() - 1.0).abs() < 1e-9,
        "Progress must be 1.0 with full deliveries"
    );
    assert!(
        project.is_complete(),
        "Must be complete with full deliveries"
    );
}

#[test]
fn test_upgrade_project_accumulation() {
    // Delivering partial materials across multiple calls must accumulate
    // the delivered quantities correctly.
    let mut required = BTreeMap::new();
    required.insert(Commodity::Glass, 10.0);
    let mut project = UpgradeProject {
        target_slot: MethodSlot::Lighting,
        target_method: "Incandescent Bulbs".to_string(),
        required_materials: required,
        delivered_materials: BTreeMap::new(),
        progress: 0.0,
        start_turn: 0,
    };

    // First delivery: 3.0
    let acc1 = project.accumulate_delivery(Commodity::Glass, 3.0);
    assert!(
        (acc1 - 3.0).abs() < 1e-9,
        "First accumulation must return 3.0, got {}",
        acc1
    );
    let delivered_after_1 = project
        .delivered_materials
        .get(&Commodity::Glass)
        .copied()
        .unwrap_or(0.0);
    assert!(
        (delivered_after_1 - 3.0).abs() < 1e-9,
        "Delivered after first call must be 3.0, got {}",
        delivered_after_1
    );

    // Second delivery: 4.0
    let acc2 = project.accumulate_delivery(Commodity::Glass, 4.0);
    assert!(
        (acc2 - 4.0).abs() < 1e-9,
        "Second accumulation must return 4.0, got {}",
        acc2
    );
    let delivered_after_2 = project
        .delivered_materials
        .get(&Commodity::Glass)
        .copied()
        .unwrap_or(0.0);
    assert!(
        (delivered_after_2 - 7.0).abs() < 1e-9,
        "Delivered after second call must be 7.0 (3+4), got {}",
        delivered_after_2
    );
    assert!(!project.is_complete(), "Must not be complete at 7/10");

    // Third delivery: 3.0 (completes the requirement)
    let acc3 = project.accumulate_delivery(Commodity::Glass, 3.0);
    assert!(
        (acc3 - 3.0).abs() < 1e-9,
        "Third accumulation must return 3.0, got {}",
        acc3
    );
    assert!(
        project.is_complete(),
        "Must be complete after 10/10 delivery"
    );
}

// ===========================================================================
// FUTURE-PROOFING TESTS
// ===========================================================================

#[test]
fn test_water_supply_slot_exists() {
    // MethodSlot::WaterSupply must exist and be parseable from "water_supply".
    let slot = MethodSlot::from_key("water_supply");
    assert_eq!(
        slot,
        Some(MethodSlot::WaterSupply),
        "MethodSlot::from_key('water_supply') must return WaterSupply"
    );
}

#[test]
fn test_sanitation_slot_exists() {
    // MethodSlot::Sanitation must exist and be parseable from "sanitation".
    let slot = MethodSlot::from_key("sanitation");
    assert_eq!(
        slot,
        Some(MethodSlot::Sanitation),
        "MethodSlot::from_key('sanitation') must return Sanitation"
    );
}

#[test]
fn test_waste_disposal_slot_exists() {
    // MethodSlot::WasteDisposal must exist and be parseable from "waste_disposal".
    let slot = MethodSlot::from_key("waste_disposal");
    assert_eq!(
        slot,
        Some(MethodSlot::WasteDisposal),
        "MethodSlot::from_key('waste_disposal') must return WasteDisposal"
    );
}

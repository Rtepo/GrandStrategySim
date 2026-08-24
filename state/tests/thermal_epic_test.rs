//! Phase 82: The Thermal Epic — comprehensive integration tests.
//!
//! Tests cover:
//! - Thermal grid physics (radial loss, degradation, connectable buildings)
//! - Heating plant registries (7 distinct types)
//! - Parallel heating consumption tracks (standalone vs district heating)
//! - Emission control registries (8 methods, reduction factors)
//! - Industrial emission factors (heavy industry methods)
//! - CHP cogeneration (auxiliary boiler mode)
//! - Smog concentration computation (mass → concentration → decay)
//! - Municipal AI decision tree (pipe/supply/cost-benefit gates)
//! - Regulated heat pricing (cost-plus with amortization + smoothing)
//! - Tech tree integration (thermo_020 through thermo_025)
//! - Snapshot role-gating

use sim_engine::energy::heating_types::{ChpRetrofitMetadata, HeatingPlantType};
use sim_engine::energy::municipal_heating_ai::{
    run_municipal_heating_ai, HeatSalesHistory, PlantTypeCostData,
};
use sim_engine::energy::thermal_grid::{
    compute_regulated_heat_price, ThermalGridState,
};
use sim_engine::environment::smog::{
    compute_smog_for_region, smog_mortality_multiplier,
    smog_year_round_mortality, LocalPollutionState,
};
use sim_engine::registries::production_methods_data::default_production_methods;
use sim_engine::utilities::consumption_bom::is_district_heating_method;

// ============================================================================
// THERMAL GRID PHYSICS
// ============================================================================

#[test]
fn test_thermal_grid_default_has_pristine_pipes() {
    let grid = ThermalGridState::default();
    assert_eq!(grid.pipe_condition, 1.0);
    assert_eq!(grid.loss_per_km, 0.02);
    assert_eq!(grid.pipe_network_km, 0.0);
}

#[test]
fn test_no_pipe_network_means_total_loss() {
    let grid = ThermalGridState::default();
    assert_eq!(grid.transmission_loss(1), 1.0);
    assert_eq!(grid.transmission_loss(5), 1.0);
}

#[test]
fn test_radial_delivery_distance_formula() {
    let grid = ThermalGridState {
        pipe_network_km: 500.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    // 500/5 = 100, sqrt(100) = 10, * 1.5 = 15.0
    let dist = grid.average_delivery_distance_km(5);
    assert!((dist - 15.0).abs() < 0.01);
}

#[test]
fn test_radial_distance_with_one_plant() {
    let grid = ThermalGridState {
        pipe_network_km: 100.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    // sqrt(100) * 1.5 = 15.0
    let dist = grid.average_delivery_distance_km(1);
    assert!((dist - 15.0).abs() < 0.01);
}

#[test]
fn test_transmission_loss_increases_with_distance() {
    let grid = ThermalGridState {
        pipe_network_km: 100.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    let loss_1_plant = grid.transmission_loss(1);
    let loss_4_plants = grid.transmission_loss(4);
    // More plants = shorter avg distance = less loss
    assert!(loss_1_plant > loss_4_plants);
}

#[test]
fn test_pipe_condition_reduces_effective_supply() {
    let grid_pristine = ThermalGridState {
        pipe_network_km: 50.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    let grid_degraded = ThermalGridState {
        pipe_network_km: 50.0,
        pipe_condition: 0.5,
        loss_per_km: 0.02,
    };
    let eff_pristine = grid_pristine.effective_heat_supply(100.0, 1);
    let eff_degraded = grid_degraded.effective_heat_supply(100.0, 1);
    assert!(eff_pristine > eff_degraded * 1.5);
}

#[test]
fn test_pipe_degradation_basal_rate() {
    let mut grid = ThermalGridState {
        pipe_network_km: 10.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    grid.degrade(0.0);
    assert!((grid.pipe_condition - 0.998).abs() < 1e-9);
}

#[test]
fn test_pipe_degradation_winter_accelerated() {
    let mut grid = ThermalGridState {
        pipe_network_km: 10.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    grid.degrade(2.0); // Harsh winter
    // 0.002 * (1 + 2.0) = 0.006
    assert!((grid.pipe_condition - 0.994).abs() < 1e-9);
}

#[test]
fn test_pipe_degradation_floor_at_zero() {
    let mut grid = ThermalGridState {
        pipe_network_km: 10.0,
        pipe_condition: 0.001,
        loss_per_km: 0.02,
    };
    grid.degrade(10.0);
    assert_eq!(grid.pipe_condition, 0.0);
}

#[test]
fn test_max_connectable_buildings_scales_with_development() {
    let grid = ThermalGridState {
        pipe_network_km: 10.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    let rural = grid.max_connectable_buildings(0.1);
    let urban = grid.max_connectable_buildings(0.8);
    assert!(urban > rural * 2);
}

// ============================================================================
// HEATING PLANT REGISTRIES (7 DISTINCT TYPES)
// ============================================================================

#[test]
fn test_all_seven_heating_plant_registries_exist() {
    let registry = default_production_methods();
    let keys = [
        "wood_boiler_plant",
        "coal_heat_plant",
        "lignite_heat_plant",
        "coke_oven_gas_heat_plant",
        "oil_heat_plant",
        "natural_gas_heat_plant",
        "geothermal_heat_plant",
    ];
    for key in &keys {
        assert!(
            registry.contains_key(*key),
            "Missing heating plant registry: {}",
            key
        );
    }
}

#[test]
fn test_no_generic_heating_plant_registry() {
    let registry = default_production_methods();
    assert!(
        !registry.contains_key("heating_plant"),
        "Generic heating_plant registry should not exist"
    );
}

#[test]
fn test_heating_plant_registry_keys_match_enum() {
    let types = [
        HeatingPlantType::WoodBoiler,
        HeatingPlantType::CoalHeatPlant,
        HeatingPlantType::LigniteHeatPlant,
        HeatingPlantType::CokeOvenGasHeatPlant,
        HeatingPlantType::OilHeatPlant,
        HeatingPlantType::NaturalGasHeatPlant,
        HeatingPlantType::GeothermalHeatPlant,
    ];
    let registry = default_production_methods();
    for pt in &types {
        let key = pt.registry_key();
        assert!(
            registry.contains_key(key),
            "Registry key {} for {:?} not found",
            key,
            pt
        );
    }
}

#[test]
fn test_each_heating_plant_has_production_methods() {
    let registry = default_production_methods();
    let types = [
        HeatingPlantType::WoodBoiler,
        HeatingPlantType::CoalHeatPlant,
        HeatingPlantType::LigniteHeatPlant,
        HeatingPlantType::CokeOvenGasHeatPlant,
        HeatingPlantType::OilHeatPlant,
        HeatingPlantType::NaturalGasHeatPlant,
        HeatingPlantType::GeothermalHeatPlant,
    ];
    for pt in &types {
        let methods = registry.get(pt.registry_key()).unwrap();
        assert!(
            methods.production.len() >= 2,
            "{:?} should have at least 2 production methods",
            pt
        );
    }
}

#[test]
fn test_heating_plant_methods_have_emission_factors() {
    let registry = default_production_methods();
    let coal = registry.get("coal_heat_plant").unwrap();
    for method in coal.production.values() {
        assert!(
            method.emission_factor > 0.0,
            "Coal heat plant method should have positive emission factor"
        );
    }
}

#[test]
fn test_geothermal_has_zero_emissions() {
    let registry = default_production_methods();
    let geo = registry.get("geothermal_heat_plant").unwrap();
    for method in geo.production.values() {
        assert_eq!(
            method.emission_factor, 0.0,
            "Geothermal should have zero emissions"
        );
    }
}

#[test]
fn test_geothermal_requires_geological_trait() {
    assert!(HeatingPlantType::GeothermalHeatPlant.requires_geological_trait());
    assert!(!HeatingPlantType::CoalHeatPlant.requires_geological_trait());
    assert!(!HeatingPlantType::WoodBoiler.requires_geological_trait());
}

#[test]
fn test_shared_heating_automation_registry_exists() {
    let registry = default_production_methods();
    assert!(registry.contains_key("heating_automation"));
    assert!(registry.contains_key("heating_organization"));
}

// ============================================================================
// PARALLEL HEATING CONSUMPTION TRACKS
// ============================================================================

#[test]
fn test_is_district_heating_method_identifies_dh_track() {
    assert!(is_district_heating_method("Unmetered Radiators"));
    assert!(is_district_heating_method("Thermostatic Valves"));
    assert!(is_district_heating_method("Smart Substations"));
}

#[test]
fn test_is_district_heating_method_rejects_standalone() {
    assert!(!is_district_heating_method("Coal Stove"));
    assert!(!is_district_heating_method("Oil Boiler"));
    assert!(!is_district_heating_method("Heat Pump"));
    assert!(!is_district_heating_method("None"));
    assert!(!is_district_heating_method("Primitive Fireplace"));
}

#[test]
fn test_housing_consumption_has_both_tracks() {
    let registry = default_production_methods();
    let housing = registry.get("housing_consumption").unwrap();
    // Standalone track
    assert!(housing.heating.contains_key("Coal Stove"));
    assert!(housing.heating.contains_key("Oil Boiler"));
    assert!(housing.heating.contains_key("Heat Pump"));
    // District heating track
    assert!(housing.heating.contains_key("Unmetered Radiators"));
    assert!(housing.heating.contains_key("Thermostatic Valves"));
    assert!(housing.heating.contains_key("Smart Substations"));
}

#[test]
fn test_standalone_methods_have_emission_factors() {
    let registry = default_production_methods();
    let housing = registry.get("housing_consumption").unwrap();
    let coal_stove = &housing.heating["Coal Stove"];
    assert!(coal_stove.emission_factor > 0.0);
    let fireplace = &housing.heating["Primitive Fireplace"];
    assert!(fireplace.emission_factor > 0.0);
}

#[test]
fn test_district_heating_methods_consume_heat_commodity() {
    let registry = default_production_methods();
    let housing = registry.get("housing_consumption").unwrap();
    let radiators = &housing.heating["Unmetered Radiators"];
    assert!(radiators.inputs.iter().any(|(c, _)| *c == sim_engine::registries::enums::Commodity::Heat));
}

#[test]
fn test_standalone_methods_consume_physical_fuel() {
    let registry = default_production_methods();
    let housing = registry.get("housing_consumption").unwrap();
    let coal_stove = &housing.heating["Coal Stove"];
    assert!(coal_stove.inputs.iter().any(|(c, _)| *c == sim_engine::registries::enums::Commodity::HardCoal));
}

#[test]
fn test_old_district_heating_method_removed() {
    let registry = default_production_methods();
    let housing = registry.get("housing_consumption").unwrap();
    assert!(
        !housing.heating.contains_key("District Heating"),
        "Old generic 'District Heating' method should be replaced by track-specific methods"
    );
}

// ============================================================================
// EMISSION CONTROL REGISTRIES
// ============================================================================

#[test]
fn test_emission_control_registries_exist() {
    let registry = default_production_methods();
    assert!(registry.contains_key("heavy_industry_emission_control"));
    assert!(registry.contains_key("heating_plant_emission_control"));
    assert!(registry.contains_key("power_plant_emission_control"));
}

#[test]
fn test_heavy_industry_emission_control_has_8_methods() {
    let registry = default_production_methods();
    let controls = registry.get("heavy_industry_emission_control").unwrap();
    assert!(controls.emission_control.contains_key("None"));
    assert!(controls.emission_control.contains_key("Basic Settling Chamber"));
    assert!(controls.emission_control.contains_key("Cyclone Separator"));
    assert!(controls.emission_control.contains_key("Wet Scrubber"));
    assert!(controls.emission_control.contains_key("Baghouse Filter"));
    assert!(controls.emission_control.contains_key("Flue-Gas Desulfurization"));
    assert!(controls.emission_control.contains_key("Electrostatic Precipitator"));
    assert!(controls.emission_control.contains_key("Selective Catalytic Reduction"));
}

#[test]
fn test_emission_control_reduction_progression() {
    let registry = default_production_methods();
    let controls = registry.get("heavy_industry_emission_control").unwrap();
    let none = &controls.emission_control["None"];
    let scr = &controls.emission_control["Selective Catalytic Reduction"];
    // None = 1.0 (no reduction), SCR = 0.005 (99.5% reduction)
    assert_eq!(none.efficiency, 1.0);
    assert!(scr.efficiency < 0.01);
    assert!(none.efficiency > scr.efficiency);
}

#[test]
fn test_emission_controls_have_capex() {
    let registry = default_production_methods();
    let controls = registry.get("heavy_industry_emission_control").unwrap();
    let fgd = &controls.emission_control["Flue-Gas Desulfurization"];
    assert!(!fgd.capex.is_empty(), "FGD should have CAPEX requirements");
}

#[test]
fn test_emission_controls_have_recurring_opex() {
    let registry = default_production_methods();
    let controls = registry.get("heavy_industry_emission_control").unwrap();
    let wet_scrubber = &controls.emission_control["Wet Scrubber"];
    assert!(!wet_scrubber.inputs.is_empty(), "Wet scrubber should have recurring OPEX inputs");
}

// ============================================================================
// INDUSTRIAL EMISSION FACTORS
// ============================================================================

#[test]
fn test_cement_production_has_high_emission_factor() {
    let registry = default_production_methods();
    let heavy = registry.get("heavy_industry").unwrap();
    let cement = &heavy.production["Cement Production"];
    assert!(cement.emission_factor >= 10.0, "Cement should have very high emission factor");
}

#[test]
fn test_coke_production_has_high_emission_factor() {
    let registry = default_production_methods();
    let heavy = registry.get("heavy_industry").unwrap();
    let coke = &heavy.production["Coke Production"];
    assert!(coke.emission_factor >= 9.0, "Coke production should have high emission factor");
}

#[test]
fn test_software_has_zero_emissions() {
    let registry = default_production_methods();
    let heavy = registry.get("heavy_industry").unwrap();
    let software = &heavy.production["Software Development"];
    assert_eq!(software.emission_factor, 0.0);
}

#[test]
fn test_steel_methods_have_progressive_emission_reduction() {
    let registry = default_production_methods();
    let heavy = registry.get("heavy_industry").unwrap();
    let bessemer = &heavy.production["Bessemer Converters"];
    let mini_mill = &heavy.production["Mini-Mill Production"];
    // Older tech = higher emissions
    assert!(bessemer.emission_factor > mini_mill.emission_factor);
}

// ============================================================================
// CHP COGENERATION
// ============================================================================

#[test]
fn test_chp_eligible_thermal_plants() {
    assert!(ChpRetrofitMetadata::is_eligible_for_chp("coal_fired_plant"));
    assert!(ChpRetrofitMetadata::is_eligible_for_chp("nuclear_plant"));
    assert!(ChpRetrofitMetadata::is_eligible_for_chp("biomass_plant"));
}

#[test]
fn test_chp_ineligible_renewables() {
    assert!(!ChpRetrofitMetadata::is_eligible_for_chp("solar_plant"));
    assert!(!ChpRetrofitMetadata::is_eligible_for_chp("wind_farm"));
    assert!(!ChpRetrofitMetadata::is_eligible_for_chp("hydro_plant"));
}

#[test]
fn test_chp_heat_from_electrical() {
    let chp = ChpRetrofitMetadata {
        heat_to_power_ratio: 1.0,
        electrical_efficiency_penalty: 0.08,
        is_active: true,
        auxiliary_efficiency_factor: 0.85,
    };
    let heat = chp.heat_from_electrical(100.0, 0.35);
    assert!((heat - 35.0).abs() < 1e-9);
}

#[test]
fn test_chp_auxiliary_boiler_capped_by_demand() {
    let chp = ChpRetrofitMetadata {
        heat_to_power_ratio: 1.0,
        electrical_efficiency_penalty: 0.08,
        is_active: true,
        auxiliary_efficiency_factor: 0.85,
    };
    let heat = chp.auxiliary_heat(100.0, 24.0, 0.35, 100.0);
    // potential = 100 * 24 * 0.35 * 0.85 = 714, demand = 100 → capped at 100
    assert!((heat - 100.0).abs() < 1e-9);
}

#[test]
fn test_chp_auxiliary_zero_when_no_demand() {
    let chp = ChpRetrofitMetadata {
        heat_to_power_ratio: 1.0,
        electrical_efficiency_penalty: 0.08,
        is_active: true,
        auxiliary_efficiency_factor: 0.85,
    };
    assert_eq!(chp.auxiliary_heat(100.0, 24.0, 0.35, 0.0), 0.0);
}

#[test]
fn test_chp_compute_heat_output_no_curtailment() {
    let chp = ChpRetrofitMetadata {
        heat_to_power_ratio: 1.0,
        electrical_efficiency_penalty: 0.08,
        is_active: true,
        auxiliary_efficiency_factor: 0.85,
    };
    let heat = sim_engine::energy::chp::compute_chp_heat_output(
        &chp, 100.0, 100.0, 0.35, 50.0, 24.0, 200.0,
    );
    // No curtailment → heat_from_chp = 100 * 1.0 * 0.35 = 35.0
    assert!((heat - 35.0).abs() < 1e-9);
}

#[test]
fn test_chp_compute_heat_output_with_curtailment() {
    let chp = ChpRetrofitMetadata {
        heat_to_power_ratio: 1.0,
        electrical_efficiency_penalty: 0.08,
        is_active: true,
        auxiliary_efficiency_factor: 0.85,
    };
    // Curtailed to 50 MW, demand = 200 GJ
    let heat = sim_engine::energy::chp::compute_chp_heat_output(
        &chp, 50.0, 100.0, 0.35, 50.0, 24.0, 200.0,
    );
    // heat_from_chp = 50 * 1.0 * 0.35 = 17.5
    // remaining = 200 - 17.5 = 182.5
    // auxiliary = min(50*24*0.35*0.85, 182.5) = min(357, 182.5) = 182.5
    // total = 17.5 + 182.5 = 200.0
    assert!((heat - 200.0).abs() < 0.5);
}

#[test]
fn test_chp_inactive_produces_no_heat() {
    let chp = ChpRetrofitMetadata {
        is_active: false,
        ..Default::default()
    };
    let heat = sim_engine::energy::chp::compute_chp_heat_output(
        &chp, 100.0, 100.0, 0.35, 50.0, 24.0, 200.0,
    );
    assert_eq!(heat, 0.0);
}

// ============================================================================
// SMOG CONCENTRATION COMPUTATION
// ============================================================================

#[test]
fn test_smog_concentration_rural_vs_urban() {
    let mut rural = LocalPollutionState::default();
    let mut urban = LocalPollutionState::default();
    // Same emissions, different area
    compute_smog_for_region(&mut rural, 500.0, 300.0, 200.0, 50000.0);
    compute_smog_for_region(&mut urban, 500.0, 300.0, 200.0, 50.0);
    assert!(urban.smog_level > rural.smog_level * 100.0);
}

#[test]
fn test_smog_accumulation_with_decay() {
    let mut p = LocalPollutionState::default();
    compute_smog_for_region(&mut p, 500.0, 0.0, 0.0, 100.0);
    let first = p.smog_level;
    compute_smog_for_region(&mut p, 500.0, 0.0, 0.0, 100.0);
    let second = p.smog_level;
    // Second turn should have more smog (accumulation)
    assert!(second > first);
}

#[test]
fn test_smog_clamped_at_100() {
    let mut p = LocalPollutionState {
        smog_level: 99.0,
        ..Default::default()
    };
    compute_smog_for_region(&mut p, 100000.0, 0.0, 0.0, 1.0);
    assert_eq!(p.smog_level, 100.0);
}

#[test]
fn test_smog_decay_with_zero_emissions() {
    let mut p = LocalPollutionState {
        smog_level: 50.0,
        ..Default::default()
    };
    compute_smog_for_region(&mut p, 0.0, 0.0, 0.0, 100.0);
    // (50 + 0) * 0.95 = 47.5
    assert!((p.smog_level - 47.5).abs() < 0.01);
}

#[test]
fn test_smog_mortality_at_zero() {
    assert_eq!(smog_mortality_multiplier(0.0), 1.0);
}

#[test]
fn test_smog_mortality_at_max() {
    assert!((smog_mortality_multiplier(100.0) - 1.5).abs() < 1e-9);
}

#[test]
fn test_smog_year_round_mortality() {
    assert_eq!(smog_year_round_mortality(0.0), 0.0);
    assert!((smog_year_round_mortality(100.0) - 0.1).abs() < 1e-9);
}

#[test]
fn test_smog_emission_breakdown_stored() {
    let mut p = LocalPollutionState::default();
    compute_smog_for_region(&mut p, 100.0, 200.0, 300.0, 1000.0);
    assert_eq!(p.standalone_emissions, 100.0);
    assert_eq!(p.centralized_emissions, 200.0);
    assert_eq!(p.industrial_emissions, 300.0);
}

// ============================================================================
// MUNICIPAL AI DECISION TREE
// ============================================================================

#[test]
fn test_municipal_ai_no_investment_when_sufficient() {
    let grid = ThermalGridState {
        pipe_network_km: 100.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    let plan = run_municipal_heating_ai(
        &grid, 100, 500.0, 400.0, &[], 10.0, 10000.0, 0.0, true,
    );
    assert_eq!(plan.pipe_expansion_km, 0.0);
    assert_eq!(plan.new_plant_type, None);
}

#[test]
fn test_municipal_ai_pipe_expansion_when_capacity_insufficient() {
    let grid = ThermalGridState {
        pipe_network_km: 5.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    let plan = run_municipal_heating_ai(
        &grid, 500, 100.0, 100.0, &[], 10.0, 10000.0, 0.0, true,
    );
    assert_eq!(plan.pipe_expansion_km, 5.0);
}

#[test]
fn test_municipal_ai_selects_lowest_opex_plant() {
    let grid = ThermalGridState {
        pipe_network_km: 100.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    let costs = vec![
        PlantTypeCostData {
            plant_type: HeatingPlantType::WoodBoiler,
            fuel_opex_per_gj: 3.0,
            maintenance_opex_per_gj: 0.5,
            capex_per_gj: 10.0,
            tech_unlocked: true,
            geologically_eligible: true,
        },
        PlantTypeCostData {
            plant_type: HeatingPlantType::CoalHeatPlant,
            fuel_opex_per_gj: 2.0,
            maintenance_opex_per_gj: 0.5,
            capex_per_gj: 15.0,
            tech_unlocked: true,
            geologically_eligible: true,
        },
    ];
    let plan = run_municipal_heating_ai(
        &grid, 100, 100.0, 500.0, &costs, 10.0, 100000.0, 10.0, true,
    );
    assert_eq!(plan.new_plant_type, Some(HeatingPlantType::CoalHeatPlant));
}

#[test]
fn test_municipal_ai_excludes_geologically_ineligible() {
    let grid = ThermalGridState {
        pipe_network_km: 100.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    let costs = vec![
        PlantTypeCostData {
            plant_type: HeatingPlantType::GeothermalHeatPlant,
            fuel_opex_per_gj: 0.1,
            maintenance_opex_per_gj: 0.3,
            capex_per_gj: 50.0,
            tech_unlocked: true,
            geologically_eligible: false,
        },
        PlantTypeCostData {
            plant_type: HeatingPlantType::CoalHeatPlant,
            fuel_opex_per_gj: 2.0,
            maintenance_opex_per_gj: 0.5,
            capex_per_gj: 15.0,
            tech_unlocked: true,
            geologically_eligible: true,
        },
    ];
    let plan = run_municipal_heating_ai(
        &grid, 100, 100.0, 500.0, &costs, 10.0, 100000.0, 10.0, true,
    );
    assert_ne!(plan.new_plant_type, Some(HeatingPlantType::GeothermalHeatPlant));
}

#[test]
fn test_municipal_ai_cost_benefit_gate_rejects_low_value() {
    let grid = ThermalGridState {
        pipe_network_km: 5.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    let plan = run_municipal_heating_ai(
        &grid, 500, 100.0, 500.0, &[], 10.0, 100.0, 1.0, true,
    );
    assert!(!plan.passes_cost_benefit_gate);
}

#[test]
fn test_municipal_ai_rejects_without_financing() {
    let grid = ThermalGridState {
        pipe_network_km: 5.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    let plan = run_municipal_heating_ai(
        &grid, 500, 100.0, 500.0, &[], 10.0, 1000000.0, 100.0, false,
    );
    assert!(!plan.passes_cost_benefit_gate);
}

#[test]
fn test_municipal_ai_produces_rationale() {
    let grid = ThermalGridState {
        pipe_network_km: 5.0,
        pipe_condition: 1.0,
        loss_per_km: 0.02,
    };
    let plan = run_municipal_heating_ai(
        &grid, 500, 100.0, 500.0, &[], 10.0, 100000.0, 10.0, true,
    );
    assert!(!plan.rationale.is_empty());
}

// ============================================================================
// REGULATED HEAT PRICING
// ============================================================================

#[test]
fn test_regulated_price_includes_amortized_capex() {
    let price_with_capex = compute_regulated_heat_price(
        1000.0, 200.0, 300.0, 50000.0, 160.0, 50.0, 1.10, 10.0,
    );
    let price_no_capex = compute_regulated_heat_price(
        1000.0, 200.0, 300.0, 0.0, 160.0, 50.0, 1.10, 10.0,
    );
    assert!(price_with_capex > price_no_capex);
}

#[test]
fn test_regulated_price_uses_smoothed_sales() {
    let price_high_sales = compute_regulated_heat_price(
        1000.0, 200.0, 300.0, 50000.0, 160.0, 100.0, 1.10, 10.0,
    );
    let price_low_sales = compute_regulated_heat_price(
        1000.0, 200.0, 300.0, 50000.0, 160.0, 10.0, 1.10, 10.0,
    );
    // Lower sales = higher price per unit (cost recovery)
    assert!(price_low_sales > price_high_sales);
}

#[test]
fn test_regulated_price_fallback_when_no_sales() {
    let price = compute_regulated_heat_price(
        1000.0, 200.0, 300.0, 50000.0, 160.0, 0.0, 1.10, 10.0,
    );
    // Fallback: average_wage * 0.5
    assert!((price - 5.0).abs() < 1e-9);
}

#[test]
fn test_regulated_price_margin_applied() {
    let price = compute_regulated_heat_price(
        1000.0, 0.0, 0.0, 0.0, 160.0, 100.0, 1.10, 10.0,
    );
    // (1000 + 0 + 0 + 0) / 100 * 1.10 = 11.0
    assert!((price - 11.0).abs() < 0.01);
}

// ============================================================================
// HEAT SALES HISTORY (SMOOTHED PRICING DENOMINATOR)
// ============================================================================

#[test]
fn test_heat_sales_history_records_and_averages() {
    let mut h = HeatSalesHistory::default();
    h.record(100.0);
    h.record(200.0);
    h.record(300.0);
    assert!((h.rolling_average() - 200.0).abs() < 1e-9);
}

#[test]
fn test_heat_sales_history_window_limit() {
    let mut h = HeatSalesHistory {
        window_size: 3,
        ..Default::default()
    };
    h.record(100.0);
    h.record(200.0);
    h.record(300.0);
    h.record(400.0);
    assert_eq!(h.sales_history.len(), 3);
    assert!((h.rolling_average() - 300.0).abs() < 1e-9);
}

#[test]
fn test_heat_sales_history_empty_average_is_zero() {
    let h = HeatSalesHistory::default();
    assert_eq!(h.rolling_average(), 0.0);
}

// ============================================================================
// TECH TREE INTEGRATION
// ============================================================================

#[test]
fn test_thermo_020_through_025_exist() {
    let tech_tree = sim_engine::registries::tech_tree_data::default_tech_tree();
    let ids = ["thermo_020", "thermo_021", "thermo_022", "thermo_023", "thermo_024", "thermo_025"];
    for id in &ids {
        assert!(
            tech_tree.contains_key(*id),
            "Tech tree node {} should exist",
            id
        );
    }
}

#[test]
fn test_thermo_020_unlocks_basic_heating() {
    let tech_tree = sim_engine::registries::tech_tree_data::default_tech_tree();
    let thermo_020 = &tech_tree["thermo_020"];
    // Should unlock heating methods in housing_consumption
    assert!(!thermo_020.unlocks_methods.is_empty());
}

#[test]
fn test_thermo_025_depends_on_thermo_024() {
    let tech_tree = sim_engine::registries::tech_tree_data::default_tech_tree();
    let thermo_025 = &tech_tree["thermo_025"];
    assert!(thermo_025.prerequisites.contains(&"thermo_024".to_string()));
}

// ============================================================================
// CONSTRUCTION PROJECT TYPES
// ============================================================================

#[test]
fn test_thermal_construction_project_types_exist() {
    use sim_engine::construction::projects::ConstructionProjectType;
    // Just verify the variants exist by constructing them
    let _pipe = ConstructionProjectType::ThermalGridPipe;
    let _plant = ConstructionProjectType::ThermalHeatingPlant;
    let _chp = ConstructionProjectType::CHPRetrofit;
}

// ============================================================================
// SNAPSHOT STRUCTURES
// ============================================================================

#[test]
fn test_thermal_grid_snapshot_default() {
    use sim_engine::ui::snapshot::ThermalGridSnapshot;
    let snap = ThermalGridSnapshot::default();
    assert_eq!(snap.pipe_network_km, 0.0);
    assert_eq!(snap.pipe_condition, 0.0);
}

#[test]
fn test_smog_snapshot_default() {
    use sim_engine::ui::snapshot::SmogSnapshot;
    let snap = SmogSnapshot::default();
    assert_eq!(snap.smog_level, 0.0);
    assert_eq!(snap.mortality_multiplier, 0.0);
}

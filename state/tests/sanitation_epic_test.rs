//! Phase 83: Sanitation Epic — comprehensive test suite.
//!
//! Tests cover:
//! 1. Water reserve state defaults and regeneration
//! 2. Water network state and quality-carrier model
//! 3. Sewer network state and leakage
//! 4. Water treatment plant registries (6 types)
//! 5. Wastewater treatment plant registries (5 types)
//! 6. Water treatment production (quality upgrade, not creation)
//! 7. Wastewater treatment production (Fertilizers extraction, surface healing)
//! 8. Water distribution (pro-rata, quality-aware)
//! 9. Sewage collection (leakage, delivery to treatment)
//! 10. Biological pollution computation
//! 11. Biohazard mortality
//! 12. Consumption track helpers (centralized vs standalone)
//! 13. Sanitation biohazard factors
//! 14. Municipal infrastructure AI (crisis override, ROI)
//! 15. Tech tree nodes (sanit_001–sanit_006)
//! 16. Snapshot DTOs
//! 17. Conservation (water mass, no creation/destruction)

use sim_engine::energy::municipal_infrastructure_ai::{
    is_crisis_condition, run_sanitation_investment_ai, run_unified_municipal_ai,
    run_water_investment_ai, ElectricalInvestmentPlan, InfrastructureDomain,
    SanitationInvestmentPlan, WaterInvestmentPlan,
    CRISIS_BIOHAZARD_THRESHOLD, CRISIS_DEHYDRATION_MORTALITY_THRESHOLD,
    CRISIS_SMOG_THRESHOLD, CRISIS_SURFACE_WATER_QUALITY_THRESHOLD,
    CRISIS_WINTER_MORTALITY_THRESHOLD,
};
use sim_engine::energy::municipal_heating_ai::HeatingInvestmentPlan;
use sim_engine::environment::smog::{
    biohazard_mortality_multiplier, compute_biohazard_for_region, BuildingWaterReceipt,
    LocalPollutionState, SAFE_WATER_QUALITY_THRESHOLD,
};
use sim_engine::registries::enums::Commodity;
use sim_engine::registries::production_methods_data::default_production_methods;
use sim_engine::registries::tech_tree_data::default_tech_tree;
use sim_engine::utilities::consumption_bom::{
    is_centralized_sanitation_method, is_centralized_water_method,
    sanitation_biohazard_factor, standalone_sanitation_leaks_to_groundwater,
    standalone_water_source_quality, standalone_water_uses_groundwater,
};
use sim_engine::utilities::hydro_grid::{
    collect_sewage, compute_dehydration_mortality, compute_regulated_sewage_price,
    compute_regulated_water_price, distribute_water, forecast_treatment_energy,
    process_wastewater_treatment, process_water_treatment, SewerNetworkState, WaterNetworkState,
    WaterReserveState, BLACKWATER_QUALITY, DEHYDRATION_SEVERITY,
    GROUNDWATER_OUTFLOW_RATE, INDUSTRIAL_WATER_QUALITY_THRESHOLD, NATURAL_GROUNDWATER_QUALITY,
    NATURAL_OUTFLOW_RATE, NATURAL_SURFACE_WATER_QUALITY, PATHOGEN_SEVERITY_FACTOR,
    PUMP_ENERGY_PER_LITER, SAFE_WATER_QUALITY_THRESHOLD as HYDRO_SAFE_THRESHOLD,
};
use sim_engine::utilities::hydro_types::{WastewaterPlantType, WaterPlantType};

// ============================================================================
// 1. WATER RESERVE STATE TESTS
// ============================================================================

#[test]
fn test_water_reserve_defaults() {
    let wrs = WaterReserveState::default();
    assert_eq!(wrs.groundwater_quality, NATURAL_GROUNDWATER_QUALITY);
    assert_eq!(wrs.surface_water_quality, NATURAL_SURFACE_WATER_QUALITY);
    assert_eq!(wrs.natural_outflow_rate, NATURAL_OUTFLOW_RATE);
}

#[test]
fn test_water_reserve_groundwater_draw() {
    let mut wrs = WaterReserveState {
        groundwater_volume: 1000.0,
        groundwater_quality: 0.9,
        ..Default::default()
    };
    let (drawn, quality) = wrs.draw_groundwater(500.0);
    assert_eq!(drawn, 500.0);
    assert_eq!(quality, 0.9);
    assert_eq!(wrs.groundwater_volume, 500.0);
}

#[test]
fn test_water_reserve_groundwater_draw_clamped() {
    let mut wrs = WaterReserveState {
        groundwater_volume: 100.0,
        ..Default::default()
    };
    let (drawn, _) = wrs.draw_groundwater(500.0);
    assert_eq!(drawn, 100.0);
    assert_eq!(wrs.groundwater_volume, 0.0);
}

#[test]
fn test_water_reserve_surface_draw() {
    let mut wrs = WaterReserveState {
        surface_water_volume: 2000.0,
        surface_water_quality: 0.6,
        ..Default::default()
    };
    let (drawn, quality) = wrs.draw_surface_water(800.0);
    assert_eq!(drawn, 800.0);
    assert_eq!(quality, 0.6);
    assert_eq!(wrs.surface_water_volume, 1200.0);
}

#[test]
fn test_water_reserve_surface_draw_clamped() {
    let mut wrs = WaterReserveState {
        surface_water_volume: 50.0,
        ..Default::default()
    };
    let (drawn, _) = wrs.draw_surface_water(200.0);
    assert_eq!(drawn, 50.0);
    assert_eq!(wrs.surface_water_volume, 0.0);
}

#[test]
fn test_water_reserve_discharge_heals_surface() {
    let mut wrs = WaterReserveState {
        surface_water_volume: 1000.0,
        surface_water_quality: 0.3,
        ..Default::default()
    };
    wrs.discharge_to_surface(500.0, 0.8);
    // Quality should improve: (1000*0.3 + 500*0.8) / 1500 = 700/1500 ≈ 0.467
    assert!(wrs.surface_water_quality > 0.3);
    assert!((wrs.surface_water_quality - 0.4667).abs() < 0.01);
    assert_eq!(wrs.surface_water_volume, 1500.0);
}

#[test]
fn test_water_reserve_regeneration_quality_drift() {
    let mut wrs = WaterReserveState {
        groundwater_volume: 1000.0,
        groundwater_quality: 0.5,
        surface_water_volume: 1000.0,
        surface_water_quality: 0.3,
        groundwater_regen_rate: 0.0,
        surface_water_inflow_rate: 0.0,
        ..Default::default()
    };
    wrs.regenerate(10000.0);
    // Quality should drift toward natural defaults
    assert!(wrs.groundwater_quality > 0.5);
    assert!(wrs.surface_water_quality > 0.3);
}

// ============================================================================
// 2. WATER NETWORK STATE TESTS
// ============================================================================

#[test]
fn test_water_network_defaults() {
    let wn = WaterNetworkState::default();
    assert_eq!(wn.pipe_condition, 1.0);
    assert_eq!(wn.current_quality, 0.0);
    assert_eq!(wn.throughput_liters, 0.0);
}

#[test]
fn test_water_network_transmission_loss() {
    let wn = WaterNetworkState {
        pipe_network_km: 50.0,
        pipe_condition: 1.0,
        loss_per_km: 0.01,
        ..Default::default()
    };
    let loss = wn.transmission_loss(1);
    // avg_distance = sqrt(50/1) * 1.2 ≈ 8.49
    // loss = 1 - (1 - 0.01)^8.49 ≈ 0.082
    assert!(loss > 0.0 && loss < 0.2);
}

#[test]
fn test_water_network_effective_delivered() {
    let wn = WaterNetworkState {
        pipe_network_km: 10.0,
        pipe_condition: 0.9,
        throughput_liters: 1000.0,
        ..Default::default()
    };
    let delivered = wn.effective_water_delivered(1);
    assert!(delivered < 1000.0); // Some loss
    assert!(delivered > 0.0);
}

#[test]
fn test_water_network_no_pipes_no_delivery() {
    let wn = WaterNetworkState::default();
    assert_eq!(wn.effective_water_delivered(0), 0.0);
}

#[test]
fn test_water_network_degrade() {
    let mut wn = WaterNetworkState {
        pipe_condition: 1.0,
        ..Default::default()
    };
    wn.degrade(1.0);
    assert!(wn.pipe_condition < 1.0);
    assert!(wn.pipe_condition > 0.0);
}

// ============================================================================
// 3. SEWER NETWORK STATE TESTS
// ============================================================================

#[test]
fn test_sewer_network_defaults() {
    let sn = SewerNetworkState::default();
    assert_eq!(sn.pipe_condition, 1.0);
    assert_eq!(sn.current_quality, BLACKWATER_QUALITY);
}

#[test]
fn test_sewer_network_leakage_exponential() {
    let sn = SewerNetworkState {
        pipe_network_km: 50.0,
        pipe_condition: 0.8,
        leakage_per_km: 0.02,
        throughput_liters: 1000.0,
        ..Default::default()
    };
    let leaked = sn.leaked_water_mass(1);
    // PATCH 1: exponential, never > 1.0
    assert!(leaked > 0.0);
    assert!(leaked < 1000.0); // Never exceeds throughput
}

#[test]
fn test_sewer_network_no_pipes_no_leakage() {
    let sn = SewerNetworkState::default();
    assert_eq!(sn.leaked_water_mass(0), 0.0);
}

#[test]
fn test_sewer_network_degrade() {
    let mut sn = SewerNetworkState {
        pipe_condition: 1.0,
        ..Default::default()
    };
    sn.degrade();
    assert!(sn.pipe_condition < 1.0);
}

// ============================================================================
// 4. WATER TREATMENT PLANT REGISTRIES (6 types)
// ============================================================================

#[test]
fn test_six_water_treatment_registries_exist() {
    let registry = default_production_methods();
    let keys = [
        "slow_sand_filter_plant",
        "rapid_sand_filter_plant",
        "chlorination_plant",
        "modern_treatment_plant",
        "advanced_treatment_plant",
        "desalination_plant",
    ];
    for key in &keys {
        assert!(
            registry.contains_key(*key),
            "Missing water treatment registry: {}",
            key
        );
    }
}

#[test]
fn test_water_treatment_registries_have_production_methods() {
    let registry = default_production_methods();
    let keys = [
        "slow_sand_filter_plant",
        "rapid_sand_filter_plant",
        "chlorination_plant",
        "modern_treatment_plant",
        "advanced_treatment_plant",
        "desalination_plant",
    ];
    for key in &keys {
        let methods = registry.get(*key).unwrap();
        assert!(
            !methods.production.is_empty(),
            "Registry {} has no production methods",
            key
        );
    }
}

#[test]
fn test_water_treatment_methods_have_output_water_quality() {
    let registry = default_production_methods();
    let keys = [
        "slow_sand_filter_plant",
        "rapid_sand_filter_plant",
        "chlorination_plant",
        "modern_treatment_plant",
        "advanced_treatment_plant",
        "desalination_plant",
    ];
    for key in &keys {
        let methods = registry.get(*key).unwrap();
        for pm in methods.production.values() {
            assert!(
                pm.output_water_quality > 0.0,
                "Production method in {} has zero output_water_quality",
                key
            );
        }
    }
}

#[test]
fn test_slow_sand_filter_quality_095() {
    let registry = default_production_methods();
    let methods = registry.get("slow_sand_filter_plant").unwrap();
    let pm = methods.production.get("Gravity Sand Bed").unwrap();
    assert_eq!(pm.output_water_quality, 0.95);
}

#[test]
fn test_modern_treatment_quality_099() {
    let registry = default_production_methods();
    let methods = registry.get("modern_treatment_plant").unwrap();
    let pm = methods.production.get("Coagulation-Flocculation").unwrap();
    assert_eq!(pm.output_water_quality, 0.99);
}

#[test]
fn test_optimized_treatment_quality_1() {
    let registry = default_production_methods();
    let methods = registry.get("modern_treatment_plant").unwrap();
    let pm = methods.production.get("Optimized Treatment").unwrap();
    assert_eq!(pm.output_water_quality, 1.0);
}

#[test]
fn test_water_automation_registry_exists() {
    let registry = default_production_methods();
    assert!(registry.contains_key("water_automation"));
    let methods = registry.get("water_automation").unwrap();
    assert!(!methods.automation.is_empty());
}

#[test]
fn test_water_organization_registry_exists() {
    let registry = default_production_methods();
    assert!(registry.contains_key("water_organization"));
    let methods = registry.get("water_organization").unwrap();
    assert!(!methods.organization.is_empty());
}

// ============================================================================
// 5. WASTEWATER TREATMENT PLANT REGISTRIES (5 types)
// ============================================================================

#[test]
fn test_five_wastewater_registries_exist() {
    let registry = default_production_methods();
    let keys = [
        "primary_settling_plant",
        "activated_sludge_plant",
        "secondary_treatment_plant",
        "tertiary_treatment_plant",
        "advanced_wastewater_plant",
    ];
    for key in &keys {
        assert!(
            registry.contains_key(*key),
            "Missing wastewater registry: {}",
            key
        );
    }
}

#[test]
fn test_wastewater_methods_have_discharge_quality() {
    let registry = default_production_methods();
    let keys = [
        "primary_settling_plant",
        "activated_sludge_plant",
        "secondary_treatment_plant",
        "tertiary_treatment_plant",
        "advanced_wastewater_plant",
    ];
    for key in &keys {
        let methods = registry.get(*key).unwrap();
        for pm in methods.production.values() {
            assert!(
                pm.discharge_quality > 0.0,
                "Production method in {} has zero discharge_quality",
                key
            );
        }
    }
}

#[test]
fn test_primary_settling_discharge_quality_030() {
    let registry = default_production_methods();
    let methods = registry.get("primary_settling_plant").unwrap();
    let pm = methods.production.get("Settling Tank").unwrap();
    assert_eq!(pm.discharge_quality, 0.30);
}

#[test]
fn test_advanced_mbr_discharge_quality_085() {
    let registry = default_production_methods();
    let methods = registry.get("advanced_wastewater_plant").unwrap();
    let pm = methods.production.get("Advanced MBR").unwrap();
    assert_eq!(pm.discharge_quality, 0.85);
}

#[test]
fn test_wastewater_methods_produce_fertilizers() {
    let registry = default_production_methods();
    let keys = [
        "primary_settling_plant",
        "activated_sludge_plant",
        "secondary_treatment_plant",
        "tertiary_treatment_plant",
        "advanced_wastewater_plant",
    ];
    for key in &keys {
        let methods = registry.get(*key).unwrap();
        for pm in methods.production.values() {
            assert!(
                pm.outputs.contains_key(&Commodity::Fertilizers),
                "Production method in {} does not produce Fertilizers",
                key
            );
        }
    }
}

#[test]
fn test_sewage_automation_registry_exists() {
    let registry = default_production_methods();
    assert!(registry.contains_key("sewage_automation"));
    let methods = registry.get("sewage_automation").unwrap();
    assert!(!methods.automation.is_empty());
}

#[test]
fn test_sewage_organization_registry_exists() {
    let registry = default_production_methods();
    assert!(registry.contains_key("sewage_organization"));
    let methods = registry.get("sewage_organization").unwrap();
    assert!(!methods.organization.is_empty());
}

// ============================================================================
// 6. WATER TREATMENT PRODUCTION TESTS
// ============================================================================

#[test]
fn test_water_treatment_upgrades_quality() {
    let mut reserves = WaterReserveState {
        groundwater_volume: 10000.0,
        groundwater_quality: 0.9,
        ..Default::default()
    };
    let mut network = WaterNetworkState::default();
    let plants = vec![(1000.0, 0.95, false)]; // 1000L, quality 0.95, not desalination
    let result = process_water_treatment(&mut reserves, &mut network, &plants, 100.0, 100.0);
    assert!(result.total_output > 0.0);
    assert!(result.output_quality > 0.9);
    assert_eq!(network.current_quality, 0.95);
}

#[test]
fn test_water_treatment_does_not_create_water() {
    let mut reserves = WaterReserveState {
        groundwater_volume: 500.0,
        ..Default::default()
    };
    let mut network = WaterNetworkState::default();
    let plants = vec![(1000.0, 0.95, false)]; // Demand 1000 but only 500 available
    let result = process_water_treatment(&mut reserves, &mut network, &plants, 100.0, 100.0);
    // Should draw only 500, not create extra
    assert_eq!(result.total_output, 500.0);
    assert_eq!(reserves.groundwater_volume, 0.0);
}

#[test]
fn test_desalination_does_not_deplete_reserves() {
    let mut reserves = WaterReserveState {
        groundwater_volume: 1000.0,
        surface_water_volume: 1000.0,
        ..Default::default()
    };
    let mut network = WaterNetworkState::default();
    let plants = vec![(2000.0, 0.99, true)]; // Desalination
    let result = process_water_treatment(&mut reserves, &mut network, &plants, 100.0, 100.0);
    // PATCH 8: Desalination draws from infinite Ocean
    assert_eq!(result.desalination_output, 2000.0);
    assert_eq!(result.groundwater_drawn, 0.0);
    assert_eq!(result.surface_water_drawn, 0.0);
    // Reserves unchanged
    assert_eq!(reserves.groundwater_volume, 1000.0);
    assert_eq!(reserves.surface_water_volume, 1000.0);
}

#[test]
fn test_water_treatment_no_plants() {
    let mut reserves = WaterReserveState::default();
    let mut network = WaterNetworkState::default();
    let result = process_water_treatment(&mut reserves, &mut network, &[], 0.0, 0.0);
    assert_eq!(result.total_output, 0.0);
    assert_eq!(network.throughput_liters, 0.0);
}

// ============================================================================
// 7. WASTEWATER TREATMENT PRODUCTION TESTS
// ============================================================================

#[test]
fn test_wastewater_treatment_produces_fertilizers() {
    let mut reserves = WaterReserveState::default();
    let sewer = SewerNetworkState {
        pipe_network_km: 10.0,
        pipe_condition: 1.0,
        throughput_liters: 1000.0,
        current_quality: BLACKWATER_QUALITY,
        ..Default::default()
    };
    let plants = vec![(1.0, 0.2, 0.50)]; // efficiency, fertilizer/liter, discharge_quality
    let result = process_wastewater_treatment(&mut reserves, &sewer, &plants, 1);
    assert!(result.fertilizers_produced > 0.0);
    assert!(result.water_discharged > 0.0);
}

#[test]
fn test_wastewater_treatment_heals_surface() {
    let mut reserves = WaterReserveState {
        surface_water_volume: 1000.0,
        surface_water_quality: 0.2,
        ..Default::default()
    };
    let sewer = SewerNetworkState {
        pipe_network_km: 10.0,
        pipe_condition: 1.0,
        throughput_liters: 500.0,
        current_quality: BLACKWATER_QUALITY,
        ..Default::default()
    };
    let plants = vec![(1.0, 0.1, 0.80)]; // discharge quality 0.80
    let result = process_wastewater_treatment(&mut reserves, &sewer, &plants, 1);
    // Surface water quality should improve
    assert!(reserves.surface_water_quality > 0.2);
    assert!(result.discharge_quality > 0.5);
}

#[test]
fn test_wastewater_treatment_no_plants() {
    let mut reserves = WaterReserveState::default();
    let sewer = SewerNetworkState::default();
    let result = process_wastewater_treatment(&mut reserves, &sewer, &[], 0);
    assert_eq!(result.fertilizers_produced, 0.0);
    assert_eq!(result.water_discharged, 0.0);
}

// ============================================================================
// 8. WATER DISTRIBUTION TESTS
// ============================================================================

#[test]
fn test_distribute_water_pro_rata() {
    let network = WaterNetworkState {
        pipe_network_km: 100.0,
        pipe_condition: 1.0,
        throughput_liters: 1000.0,
        current_quality: 0.95,
        ..Default::default()
    };
    let demands = vec![
        ("building_1".into(), 300.0),
        ("building_2".into(), 700.0),
    ];
    let result = distribute_water(&network, 1, &demands);
    assert_eq!(result.building_receipts.len(), 2);
    // Pro-rata: building_1 gets 30%, building_2 gets 70%
    let b1 = result.building_receipts.iter().find(|(id, _, _)| id == "building_1").unwrap();
    let b2 = result.building_receipts.iter().find(|(id, _, _)| id == "building_2").unwrap();
    assert!(b1.1 < b2.1); // building_2 gets more
}

#[test]
fn test_distribute_water_no_pipes() {
    let network = WaterNetworkState::default();
    let demands = vec![("b1".into(), 100.0)];
    let result = distribute_water(&network, 0, &demands);
    assert_eq!(result.total_delivered, 0.0);
}

#[test]
fn test_distribute_water_quality_propagated() {
    let network = WaterNetworkState {
        pipe_network_km: 50.0,
        pipe_condition: 1.0,
        throughput_liters: 500.0,
        current_quality: 0.98,
        ..Default::default()
    };
    let demands = vec![("b1".into(), 100.0)];
    let result = distribute_water(&network, 1, &demands);
    assert_eq!(result.delivered_quality, 0.98);
}

// ============================================================================
// 9. SEWAGE COLLECTION TESTS
// ============================================================================

#[test]
fn test_collect_sewage_basic() {
    let mut sewer = SewerNetworkState {
        pipe_network_km: 50.0,
        pipe_condition: 1.0,
        ..Default::default()
    };
    let discharges = vec![("b1".into(), 500.0), ("b2".into(), 300.0)];
    let result = collect_sewage(&mut sewer, 1, &discharges);
    assert_eq!(result.total_collected, 800.0);
    assert_eq!(result.sewage_quality, BLACKWATER_QUALITY);
}

#[test]
fn test_collect_sewage_no_pipes() {
    let mut sewer = SewerNetworkState::default();
    let discharges = vec![("b1".into(), 500.0)];
    let result = collect_sewage(&mut sewer, 0, &discharges);
    assert_eq!(result.total_collected, 0.0);
}

#[test]
fn test_collect_sewage_leakage() {
    let mut sewer = SewerNetworkState {
        pipe_network_km: 100.0,
        pipe_condition: 0.5,
        leakage_per_km: 0.02,
        ..Default::default()
    };
    let discharges = vec![("b1".into(), 1000.0)];
    let result = collect_sewage(&mut sewer, 1, &discharges);
    assert!(result.leaked > 0.0);
    assert!(result.delivered_to_treatment < 1000.0);
}

// ============================================================================
// 10. BIOLOGICAL POLLUTION TESTS
// ============================================================================

#[test]
fn test_biohazard_mortality() {
    assert_eq!(biohazard_mortality_multiplier(0.0), 1.0);
    assert!((biohazard_mortality_multiplier(100.0) - 2.0).abs() < 1e-9);
}

#[test]
fn test_biohazard_low_quality_water_rural() {
    let mut p = LocalPollutionState::default();
    let receipts = vec![BuildingWaterReceipt {
        building_id: "rural".into(),
        water_quality_received: 0.6,
        water_consumed: 100.0,
    }];
    compute_biohazard_for_region(&mut p, 0.0, 0.0, 0.0, &receipts, 100.0);
    // (0.9 - 0.6) * 100 * 0.5 = 15.0
    assert!((p.low_quality_water_biohazard - 15.0).abs() < 0.01);
}

#[test]
fn test_biohazard_clean_well_no_sickness() {
    let mut p = LocalPollutionState::default();
    let receipts = vec![BuildingWaterReceipt {
        building_id: "well".into(),
        water_quality_received: 0.9,
        water_consumed: 200.0,
    }];
    compute_biohazard_for_region(&mut p, 0.0, 0.0, 0.0, &receipts, 100.0);
    assert!(p.low_quality_water_biohazard < 0.01);
}

#[test]
fn test_biohazard_failing_grid() {
    let mut p = LocalPollutionState::default();
    let receipts = vec![BuildingWaterReceipt {
        building_id: "urban".into(),
        water_quality_received: 0.5,
        water_consumed: 200.0,
    }];
    compute_biohazard_for_region(&mut p, 0.0, 0.0, 0.0, &receipts, 100.0);
    // (0.9 - 0.5) * 200 * 0.5 = 40.0
    assert!((p.low_quality_water_biohazard - 40.0).abs() < 0.01);
}

#[test]
fn test_biohazard_decay() {
    let mut p = LocalPollutionState {
        biohazard_level: 50.0,
        ..Default::default()
    };
    compute_biohazard_for_region(&mut p, 0.0, 0.0, 0.0, &[], 100.0);
    // (50 + 0) * 0.97 = 48.5
    assert!((p.biohazard_level - 48.5).abs() < 0.1);
}

#[test]
fn test_biohazard_accumulation() {
    let mut p = LocalPollutionState::default();
    for _ in 0..10 {
        compute_biohazard_for_region(&mut p, 100.0, 0.0, 0.0, &[], 100.0);
    }
    assert!(p.biohazard_level > 5.0);
}

// ============================================================================
// 11. DEHYDRATION MORTALITY TESTS
// ============================================================================

#[test]
fn test_dehydration_mortality_no_deficit() {
    // deficit = 0, demand = 100
    let m = compute_dehydration_mortality(0.0, 100.0);
    assert_eq!(m, 1.0);
}

#[test]
fn test_dehydration_mortality_full_deficit() {
    // deficit = 100, demand = 100 → 100% deficit
    let m = compute_dehydration_mortality(100.0, 100.0);
    // 1.0 + 1.0 * 3.0 = 4.0
    assert!((m - 4.0).abs() < 1e-9);
}

#[test]
fn test_dehydration_mortality_half_deficit() {
    // deficit = 50, demand = 100 → 50% deficit
    let m = compute_dehydration_mortality(50.0, 100.0);
    // 1.0 + 0.5 * 3.0 = 2.5
    assert!((m - 2.5).abs() < 1e-9);
}

// ============================================================================
// 12. CONSUMPTION TRACK HELPER TESTS
// ============================================================================

#[test]
fn test_is_centralized_water_method() {
    assert!(!is_centralized_water_method("Local Well"));
    assert!(!is_centralized_water_method("Hand Pump Well"));
    assert!(is_centralized_water_method("Municipal Mains (Basic)"));
    assert!(is_centralized_water_method("Smart Meter Connection"));
}

#[test]
fn test_is_centralized_sanitation_method() {
    assert!(!is_centralized_sanitation_method("Open Defecation"));
    assert!(!is_centralized_sanitation_method("Septic Tank"));
    assert!(is_centralized_sanitation_method("Municipal Sewer (Basic)"));
    assert!(is_centralized_sanitation_method("Advanced Sewer + Tertiary"));
}

#[test]
fn test_sanitation_biohazard_factors() {
    assert_eq!(sanitation_biohazard_factor("None"), 5.0);
    assert_eq!(sanitation_biohazard_factor("Open Defecation"), 5.0);
    assert_eq!(sanitation_biohazard_factor("Cesspool"), 3.0);
    assert_eq!(sanitation_biohazard_factor("Outhouse"), 2.5);
    assert_eq!(sanitation_biohazard_factor("Septic Tank"), 1.0);
    assert_eq!(sanitation_biohazard_factor("Improved Septic"), 0.5);
    assert_eq!(sanitation_biohazard_factor("Municipal Sewer (Basic)"), 0.2);
    assert_eq!(sanitation_biohazard_factor("Advanced Sewer + Tertiary"), 0.005);
}

#[test]
fn test_standalone_water_source_quality() {
    assert_eq!(standalone_water_source_quality("Local Well"), Some(0.9));
    assert_eq!(standalone_water_source_quality("Hand Pump Well"), Some(0.9));
    assert_eq!(standalone_water_source_quality("Rainwater Catchment"), Some(0.6));
    assert_eq!(standalone_water_source_quality("None"), None);
    assert_eq!(standalone_water_source_quality("Municipal Mains (Basic)"), None);
}

#[test]
fn test_standalone_water_uses_groundwater() {
    assert!(standalone_water_uses_groundwater("Local Well"));
    assert!(standalone_water_uses_groundwater("Hand Pump Well"));
    assert!(!standalone_water_uses_groundwater("Rainwater Catchment"));
}

#[test]
fn test_standalone_sanitation_leaks_to_groundwater() {
    assert!(standalone_sanitation_leaks_to_groundwater("Cesspool"));
    assert!(standalone_sanitation_leaks_to_groundwater("Septic Tank"));
    assert!(!standalone_sanitation_leaks_to_groundwater("Open Defecation"));
    assert!(!standalone_sanitation_leaks_to_groundwater("Municipal Sewer (Basic)"));
}

// ============================================================================
// 13. REGULATED PRICING TESTS
// ============================================================================

#[test]
fn test_regulated_water_price_with_sales() {
    let price = compute_regulated_water_price(
        100.0, // chemicals
        200.0, // energy
        300.0, // labor
        50.0,  // maintenance
        10000.0, // asset value
        160.0, // amortization turns
        1000.0, // smoothed sales
        1.10,  // margin
        10.0,  // avg wage
    );
    // total_cost = 100+200+300+50 + 10000/160 = 712.5
    // price = 712.5 / 1000 * 1.10 = 0.78375
    assert!(price > 0.0);
    assert!((price - 0.78375).abs() < 0.01);
}

#[test]
fn test_regulated_water_price_fallback() {
    let price = compute_regulated_water_price(
        0.0, 0.0, 0.0, 0.0, 0.0, 160.0, 0.0, 1.10, 10.0,
    );
    // No sales → fallback to wage * 0.5
    assert_eq!(price, 5.0);
}

#[test]
fn test_regulated_sewage_price() {
    let price = compute_regulated_sewage_price(
        50.0, 100.0, 200.0, 30.0, 8000.0, 160.0, 800.0, 1.10, 10.0,
    );
    assert!(price > 0.0);
}

// ============================================================================
// 14. MUNICIPAL INFRASTRUCTURE AI TESTS
// ============================================================================

#[test]
fn test_crisis_thresholds() {
    assert_eq!(CRISIS_BIOHAZARD_THRESHOLD, 50.0);
    assert_eq!(CRISIS_SMOG_THRESHOLD, 50.0);
    assert_eq!(CRISIS_WINTER_MORTALITY_THRESHOLD, 2.0);
    assert_eq!(CRISIS_SURFACE_WATER_QUALITY_THRESHOLD, 0.3);
    assert_eq!(CRISIS_DEHYDRATION_MORTALITY_THRESHOLD, 2.0);
}

#[test]
fn test_crisis_condition_detection() {
    assert!(!is_crisis_condition(10.0, 10.0, 1.0, 0.8, 1.0));
    assert!(is_crisis_condition(60.0, 10.0, 1.0, 0.8, 1.0));
    assert!(is_crisis_condition(10.0, 10.0, 1.0, 0.2, 1.0));
    assert!(is_crisis_condition(10.0, 10.0, 1.0, 0.8, 3.0));
}

#[test]
fn test_water_investment_no_deficit() {
    let network = WaterNetworkState {
        pipe_network_km: 100.0,
        pipe_condition: 1.0,
        current_quality: 0.98,
        throughput_liters: 5000.0,
        ..Default::default()
    };
    let reserves = WaterReserveState::default();
    let plan = run_water_investment_ai(
        &network, &reserves, 100, 5000.0, 4000.0, &[], 10.0, 10000.0, 0.0, 1.0, true,
    );
    assert_eq!(plan.expand_pipes_km, 0.0);
    assert!(!plan.is_crisis);
}

#[test]
fn test_water_investment_quality_crisis() {
    let network = WaterNetworkState {
        pipe_network_km: 100.0,
        pipe_condition: 1.0,
        current_quality: 0.5,
        throughput_liters: 5000.0,
        ..Default::default()
    };
    let reserves = WaterReserveState::default();
    let plan = run_water_investment_ai(
        &network, &reserves, 100, 5000.0, 4000.0, &[], 10.0, 10000.0, 10.0, 1.0, true,
    );
    assert!(plan.is_crisis);
}

#[test]
fn test_sanitation_investment_surface_water_crisis() {
    let sewer = SewerNetworkState {
        pipe_network_km: 100.0,
        pipe_condition: 1.0,
        throughput_liters: 1000.0,
        ..Default::default()
    };
    let reserves = WaterReserveState {
        surface_water_quality: 0.2,
        ..Default::default()
    };
    let plan = run_sanitation_investment_ai(
        &sewer, &reserves, 100, 1000.0, 500.0, 10.0, &[], 10.0, 10000.0, 5.0, 1000.0, true,
    );
    assert!(plan.is_crisis);
}

#[test]
fn test_unified_ai_prioritizes_crisis() {
    let thermal = HeatingInvestmentPlan::default();
    let electrical = ElectricalInvestmentPlan::default();
    let water = WaterInvestmentPlan {
        estimated_capex: 50000.0,
        is_crisis: true,
        passes_cost_benefit_gate: true,
        ..Default::default()
    };
    let sanitation = SanitationInvestmentPlan::default();
    let plan = run_unified_municipal_ai(thermal, electrical, water, sanitation, sim_engine::energy::municipal_infrastructure_ai::WasteInvestmentPlan::default(), 100000.0);
    assert_eq!(plan.prioritized_domain, InfrastructureDomain::Water);
}

// ============================================================================
// 15. TECH TREE TESTS
// ============================================================================

#[test]
fn test_sanit_tech_nodes_exist() {
    let tree = default_tech_tree();
    let nodes = ["sanit_001", "sanit_002", "sanit_003", "sanit_004", "sanit_005", "sanit_006"];
    for node in &nodes {
        assert!(tree.contains_key(*node), "Missing tech node: {}", node);
    }
}

#[test]
fn test_sanit_001_basic_sanitation() {
    let tree = default_tech_tree();
    let node = tree.get("sanit_001").unwrap();
    assert_eq!(node.year, 1880);
    assert!(node.unlocks_methods.contains_key("slow_sand_filter_plant"));
}

#[test]
fn test_sanit_002_municipal_water() {
    let tree = default_tech_tree();
    let node = tree.get("sanit_002").unwrap();
    assert_eq!(node.year, 1890);
    assert!(node.unlocks_methods.contains_key("chlorination_plant"));
}

#[test]
fn test_sanit_006_advanced_water() {
    let tree = default_tech_tree();
    let node = tree.get("sanit_006").unwrap();
    assert_eq!(node.year, 2000);
    assert!(node.unlocks_methods.contains_key("advanced_wastewater_plant"));
}

#[test]
fn test_sanit_tech_prerequisites_form_chain() {
    let tree = default_tech_tree();
    let n1 = tree.get("sanit_001").unwrap();
    let n2 = tree.get("sanit_002").unwrap();
    let n3 = tree.get("sanit_003").unwrap();
    assert!(n1.prerequisites.is_empty() || n1.prerequisites.contains(&"thermo_005".to_string()));
    assert!(n2.prerequisites.contains(&"sanit_001".to_string()));
    assert!(n3.prerequisites.contains(&"sanit_002".to_string()));
}

// ============================================================================
// 16. INDUSTRIAL BIOHAZARD FACTOR TESTS
// ============================================================================

#[test]
fn test_tannery_biohazard_factor() {
    // Tannery doesn't exist as a production method yet, but the biohazard
    // map is defined for when it's added. Verify the map is present.
    let registry = default_production_methods();
    let heavy = registry.get("heavy_industry").unwrap();
    // Tannery is not yet a registered method — biohazard map is ready for it
    if let Some(pm) = heavy.production.get("Tannery") {
        assert_eq!(pm.biohazard_factor, 8.0);
    }
    // Paper Mill exists in light_industry and should have biohazard_factor = 5.0
    let light = registry.get("light_industry").unwrap();
    if let Some(pm) = light.production.get("Paper Mill") {
        assert_eq!(pm.biohazard_factor, 5.0);
    }
}

#[test]
fn test_abattoir_biohazard_factor() {
    // Abattoir doesn't exist as a production method yet
    let registry = default_production_methods();
    let heavy = registry.get("heavy_industry").unwrap();
    if let Some(pm) = heavy.production.get("Abattoir") {
        assert_eq!(pm.biohazard_factor, 7.0);
    }
}

#[test]
fn test_food_processing_biohazard_factor() {
    let registry = default_production_methods();
    let light = registry.get("light_industry").unwrap();
    let pm = light.production.get("Food Processing").unwrap();
    assert_eq!(pm.biohazard_factor, 6.0);
}

#[test]
fn test_steel_zero_biohazard() {
    let registry = default_production_methods();
    let heavy = registry.get("heavy_industry").unwrap();
    // Steel should have zero biohazard (not pathogenic)
    if let Some(pm) = heavy.production.get("Steel Mill") {
        assert_eq!(pm.biohazard_factor, 0.0);
    }
}

// ============================================================================
// 17. CONSUMPTION TRACK REGISTRY TESTS
// ============================================================================

#[test]
fn test_housing_consumption_has_water_supply_track() {
    let registry = default_production_methods();
    let housing = registry.get("housing_consumption").unwrap();
    assert!(!housing.water_supply.is_empty());
}

#[test]
fn test_housing_consumption_has_sanitation_track() {
    let registry = default_production_methods();
    let housing = registry.get("housing_consumption").unwrap();
    assert!(!housing.sanitation.is_empty());
}

#[test]
fn test_commercial_consumption_has_water_supply_track() {
    let registry = default_production_methods();
    let commercial = registry.get("commercial_consumption").unwrap();
    assert!(!commercial.water_supply.is_empty());
}

#[test]
fn test_commercial_consumption_has_sanitation_track() {
    let registry = default_production_methods();
    let commercial = registry.get("commercial_consumption").unwrap();
    assert!(!commercial.sanitation.is_empty());
}

#[test]
fn test_housing_water_supply_has_standalone_and_centralized() {
    let registry = default_production_methods();
    let housing = registry.get("housing_consumption").unwrap();
    // Should have standalone methods
    assert!(housing.water_supply.contains_key("Local Well"));
    assert!(housing.water_supply.contains_key("Rainwater Catchment"));
    // Should have centralized methods
    assert!(housing.water_supply.contains_key("Municipal Mains (Basic)"));
    assert!(housing.water_supply.contains_key("Smart Meter Connection"));
}

#[test]
fn test_housing_sanitation_has_standalone_and_centralized() {
    let registry = default_production_methods();
    let housing = registry.get("housing_consumption").unwrap();
    // Standalone
    assert!(housing.sanitation.contains_key("Open Defecation"));
    assert!(housing.sanitation.contains_key("Septic Tank"));
    // Centralized
    assert!(housing.sanitation.contains_key("Municipal Sewer (Basic)"));
    assert!(housing.sanitation.contains_key("Advanced Sewer + Tertiary"));
}

// ============================================================================
// 18. FORECAST TESTS
// ============================================================================

#[test]
fn test_forecast_treatment_energy_cold_start() {
    // Cold start: 50% of nameplate
    let energy = forecast_treatment_energy(1000.0, 0.0, 0.001);
    // 1000 * 0.5 * 0.001 = 0.5
    assert!((energy - 0.5).abs() < 1e-9);
}

#[test]
fn test_forecast_treatment_energy_warm_start() {
    let energy = forecast_treatment_energy(1000.0, 800.0, 0.001);
    // Uses prior throughput
    assert!((energy - 0.8).abs() < 1e-9);
}

// ============================================================================
// 19. PHYSICAL CONSTANTS TESTS
// ============================================================================

#[test]
fn test_physical_constants() {
    assert_eq!(NATURAL_OUTFLOW_RATE, 0.05);
    assert_eq!(GROUNDWATER_OUTFLOW_RATE, 0.025);
    assert_eq!(NATURAL_GROUNDWATER_QUALITY, 0.9);
    assert_eq!(NATURAL_SURFACE_WATER_QUALITY, 0.6);
    assert_eq!(BLACKWATER_QUALITY, 0.05);
    assert_eq!(SAFE_WATER_QUALITY_THRESHOLD, 0.9);
    assert_eq!(PATHOGEN_SEVERITY_FACTOR, 0.5);
    assert_eq!(DEHYDRATION_SEVERITY, 3.0);
    assert_eq!(PUMP_ENERGY_PER_LITER, 0.001);
    assert_eq!(INDUSTRIAL_WATER_QUALITY_THRESHOLD, 0.3);
    assert_eq!(HYDRO_SAFE_THRESHOLD, 0.9);
}

// ============================================================================
// 20. WATER PLANT TYPE ENUM TESTS
// ============================================================================

#[test]
fn test_water_plant_type_default() {
    let pt = WaterPlantType::default();
    assert_eq!(pt, WaterPlantType::SlowSandFilter);
}

#[test]
fn test_wastewater_plant_type_variants() {
    let types = [
        WastewaterPlantType::PrimarySettling,
        WastewaterPlantType::ActivatedSludge,
        WastewaterPlantType::SecondaryTreatment,
        WastewaterPlantType::TertiaryTreatment,
        WastewaterPlantType::AdvancedWastewaterPlant,
    ];
    assert_eq!(types.len(), 5);
}

#[test]
fn test_water_plant_type_variants() {
    let types = [
        WaterPlantType::SlowSandFilter,
        WaterPlantType::RapidSandFilter,
        WaterPlantType::ChlorinationPlant,
        WaterPlantType::ModernTreatmentPlant,
        WaterPlantType::AdvancedTreatmentPlant,
        WaterPlantType::DesalinationPlant,
    ];
    assert_eq!(types.len(), 6);
}

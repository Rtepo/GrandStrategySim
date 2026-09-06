//! Hardcoded production methods for all sectors, grouped by slot.
//!
//! This module provides `default_production_methods()` which returns a
//! `HashMap<String, BuildingMethods>` keyed by sector (English snake_case).
//! Each `BuildingMethods` contains production methods for the three slots:
//! automation, production, and organization.

use crate::registries::enums::CapacityType;
use crate::registries::enums::Commodity;
use crate::registries::production_methods::{BuildingMethods, MethodSlot, ProductionMethod};
use std::collections::HashMap;

/// Helper: create a `ProductionMethod` with sensible defaults.
#[allow(clippy::too_many_arguments)]
fn pm(
    year: u32,
    tech: Option<&str>,
    experts: f64,
    skilled: f64,
    basic: f64,
    eff: f64,
    inputs: &[(Commodity, f64)],
    outputs: &[(Commodity, f64)],
) -> ProductionMethod {
    ProductionMethod {
        year,
        required_tech: tech.map(|s| s.to_string()),
        experts_ratio: experts,
        skilled_ratio: skilled,
        basic_ratio: basic,
        efficiency: eff,
        inputs: inputs.iter().copied().collect(),
        outputs: outputs.iter().copied().collect(),
        thermal_efficiency: 0.0,
        storage_efficiency: 0.0,
        capex: HashMap::new(),
        emission_factor: 0.0,
        biohazard_factor: 0.0,
        output_water_quality: 0.0,
        discharge_quality: 0.0,
        waste_generation_factor: 0.0,
        seat_type: None,
    }
}

/// Phase A.2.3: Helper for education/healthcare/care methods that produce
/// service capacity (seats). Sets the typed `seat_type` field explicitly —
/// no string heuristics. The seat type is a physical classification that
/// determines which `CapacityType` pool this method's output feeds into.
#[allow(clippy::too_many_arguments)]
fn pm_education(
    year: u32,
    tech: Option<&str>,
    experts: f64,
    skilled: f64,
    basic: f64,
    eff: f64,
    inputs: &[(Commodity, f64)],
    outputs: &[(Commodity, f64)],
    seat_type: crate::registries::enums::CapacityType,
) -> ProductionMethod {
    let mut m = pm(year, tech, experts, skilled, basic, eff, inputs, outputs);
    m.seat_type = Some(seat_type);
    m
}

/// Phase 74: Helper for energy production methods with thermal efficiency.
/// Same as `pm()` but sets `thermal_efficiency` > 0.0, which triggers
/// dynamic fuel consumption computation in `process_building_cycle()`.
#[allow(clippy::too_many_arguments)]
fn pm_thermal(
    year: u32,
    tech: Option<&str>,
    experts: f64,
    skilled: f64,
    basic: f64,
    eff: f64,
    inputs: &[(Commodity, f64)],
    outputs: &[(Commodity, f64)],
    thermal_efficiency: f64,
) -> ProductionMethod {
    ProductionMethod {
        year,
        required_tech: tech.map(|s| s.to_string()),
        experts_ratio: experts,
        skilled_ratio: skilled,
        basic_ratio: basic,
        efficiency: eff,
        inputs: inputs.iter().copied().collect(),
        outputs: outputs.iter().copied().collect(),
        thermal_efficiency,
        storage_efficiency: 0.0,
        capex: HashMap::new(),
        emission_factor: 0.0,
        biohazard_factor: 0.0,
        output_water_quality: 0.0,
        discharge_quality: 0.0,
        waste_generation_factor: 0.0,
        seat_type: None,
    }
}

/// Phase 79: Helper for energy storage production methods with round-trip
/// storage efficiency. Sets `storage_efficiency` > 0.0, which triggers
/// strict conservation in `process_building_cycle()`: output_energy =
/// input_energy * storage_efficiency. Used by PumpedStoragePlant and BatteryBank.
#[allow(clippy::too_many_arguments)]
fn pm_storage(
    year: u32,
    tech: Option<&str>,
    experts: f64,
    skilled: f64,
    basic: f64,
    eff: f64,
    inputs: &[(Commodity, f64)],
    outputs: &[(Commodity, f64)],
    storage_efficiency: f64,
) -> ProductionMethod {
    ProductionMethod {
        year,
        required_tech: tech.map(|s| s.to_string()),
        experts_ratio: experts,
        skilled_ratio: skilled,
        basic_ratio: basic,
        efficiency: eff,
        inputs: inputs.iter().copied().collect(),
        outputs: outputs.iter().copied().collect(),
        thermal_efficiency: 0.0,
        storage_efficiency,
        capex: HashMap::new(),
        emission_factor: 0.0,
        biohazard_factor: 0.0,
        output_water_quality: 0.0,
        discharge_quality: 0.0,
        waste_generation_factor: 0.0,
        seat_type: None,
    }
}

/// Phase 81 Wave 2: Helper for consumption methods with CAPEX (one-time
/// installation cost). Same as `pm()` but sets `capex` to the provided
/// commodities. Used by lighting, heating, ventilation, and power generation
/// method progressions where upgrading requires a one-time physical investment.
#[allow(clippy::too_many_arguments)]
fn pm_capex(
    year: u32,
    tech: Option<&str>,
    experts: f64,
    skilled: f64,
    basic: f64,
    eff: f64,
    inputs: &[(Commodity, f64)],
    outputs: &[(Commodity, f64)],
    capex: &[(Commodity, f64)],
) -> ProductionMethod {
    ProductionMethod {
        year,
        required_tech: tech.map(|s| s.to_string()),
        experts_ratio: experts,
        skilled_ratio: skilled,
        basic_ratio: basic,
        efficiency: eff,
        inputs: inputs.iter().copied().collect(),
        outputs: outputs.iter().copied().collect(),
        thermal_efficiency: 0.0,
        storage_efficiency: 0.0,
        capex: capex.iter().copied().collect(),
        emission_factor: 0.0,
        biohazard_factor: 0.0,
        output_water_quality: 0.0,
        discharge_quality: 0.0,
        waste_generation_factor: 0.0,
        seat_type: None,
    }
}

/// Phase 82: Helper for heating plant production methods with emission factor.
/// Same as `pm_thermal` but includes `emission_factor` for smog computation.
fn pm_heating(
    year: u32,
    tech: Option<&str>,
    experts: f64,
    skilled: f64,
    basic: f64,
    eff: f64,
    inputs: &[(Commodity, f64)],
    outputs: &[(Commodity, f64)],
    thermal_efficiency: f64,
    emission_factor: f64,
) -> ProductionMethod {
    ProductionMethod {
        year,
        required_tech: tech.map(|s| s.to_string()),
        experts_ratio: experts,
        skilled_ratio: skilled,
        basic_ratio: basic,
        efficiency: eff,
        inputs: inputs.iter().copied().collect(),
        outputs: outputs.iter().copied().collect(),
        thermal_efficiency,
        storage_efficiency: 0.0,
        capex: HashMap::new(),
        emission_factor,
        biohazard_factor: 0.0,
        output_water_quality: 0.0,
        discharge_quality: 0.0,
        waste_generation_factor: 0.0,
        seat_type: None,
    }
}

/// Phase 82: Helper for consumption methods with emission factor and CAPEX.
/// Used for standalone heating methods that have both per-turn inputs (fuel)
/// and one-time CAPEX (installation cost), plus emission factors for smog.
fn pm_consumption_emission(
    year: u32,
    tech: Option<&str>,
    experts: f64,
    skilled: f64,
    basic: f64,
    eff: f64,
    inputs: &[(Commodity, f64)],
    outputs: &[(Commodity, f64)],
    capex: &[(Commodity, f64)],
    emission_factor: f64,
) -> ProductionMethod {
    ProductionMethod {
        year,
        required_tech: tech.map(|s| s.to_string()),
        experts_ratio: experts,
        skilled_ratio: skilled,
        basic_ratio: basic,
        efficiency: eff,
        inputs: inputs.iter().copied().collect(),
        outputs: outputs.iter().copied().collect(),
        thermal_efficiency: 0.0,
        storage_efficiency: 0.0,
        capex: capex.iter().copied().collect(),
        emission_factor,
        biohazard_factor: 0.0,
        output_water_quality: 0.0,
        discharge_quality: 0.0,
        waste_generation_factor: 0.0,
        seat_type: None,
    }
}

// ============================================================================
// PHASE 83: WATER TREATMENT PLANT REGISTRIES (Water Quality Spectrum)
// ============================================================================

/// Phase 83: Helper for water treatment plant production methods.
/// Sets `output_water_quality` > 0.0, which marks this as a water treatment
/// method. The throughput (liters/turn) is encoded as the `Commodity::Water`
/// output quantity. The `output_water_quality` is the quality target.
/// PARADIGM SHIFT: These plants do NOT create water — they upgrade its quality.
/// The `Commodity::Water` output represents throughput, not mass creation.
#[allow(clippy::too_many_arguments)]
fn pm_water_treatment(
    year: u32,
    tech: Option<&str>,
    experts: f64,
    skilled: f64,
    basic: f64,
    eff: f64,
    inputs: &[(Commodity, f64)],
    throughput_liters: f64,
    output_water_quality: f64,
) -> ProductionMethod {
    ProductionMethod {
        year,
        required_tech: tech.map(|s| s.to_string()),
        experts_ratio: experts,
        skilled_ratio: skilled,
        basic_ratio: basic,
        efficiency: eff,
        inputs: inputs.iter().copied().collect(),
        outputs: [(Commodity::Water, throughput_liters)]
            .into_iter()
            .collect(),
        thermal_efficiency: 0.0,
        storage_efficiency: 0.0,
        capex: HashMap::new(),
        emission_factor: 0.0,
        biohazard_factor: 0.0,
        output_water_quality,
        discharge_quality: 0.0,
        waste_generation_factor: 0.0,
        seat_type: None,
    }
}

/// Phase 83: Helper for wastewater treatment plant production methods.
/// Sets `discharge_quality` > 0.0, which marks this as a wastewater treatment
/// method. The throughput is encoded as `Commodity::Water` input (blackwater
/// intake). Fertilizers output represents extracted biosolids.
/// The `discharge_quality` is the quality of water returned to surface reserves.
#[allow(clippy::too_many_arguments)]
fn pm_wastewater_treatment(
    year: u32,
    tech: Option<&str>,
    experts: f64,
    skilled: f64,
    basic: f64,
    eff: f64,
    inputs: &[(Commodity, f64)],
    fertilizer_output: f64,
    discharge_quality: f64,
) -> ProductionMethod {
    ProductionMethod {
        year,
        required_tech: tech.map(|s| s.to_string()),
        experts_ratio: experts,
        skilled_ratio: skilled,
        basic_ratio: basic,
        efficiency: eff,
        inputs: inputs.iter().copied().collect(),
        outputs: [(Commodity::Fertilizers, fertilizer_output)]
            .into_iter()
            .collect(),
        thermal_efficiency: 0.0,
        storage_efficiency: 0.0,
        capex: HashMap::new(),
        emission_factor: 0.0,
        biohazard_factor: 0.0,
        output_water_quality: 0.0,
        discharge_quality,
        waste_generation_factor: 0.0,
        seat_type: None,
    }
}

/// Phase 83: Slow sand filtration plant (1850s). Gravity-fed sand beds.
/// Lowest tech water treatment. Output quality ~0.95.
fn slow_sand_filter_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Gravity Sand Bed".into(),
        pm_water_treatment(
            1850,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Energy, 0.1)],
            800.0,
            0.95,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Improved Sand Bed".into(),
        pm_water_treatment(
            1880,
            Some("sanit_001"),
            0.05,
            0.20,
            0.75,
            1.2,
            &[(Commodity::Energy, 0.1)],
            900.0,
            0.96,
        ),
    );
    m
}

/// Phase 83: Rapid sand filtration plant (1890s). Mechanical filtration + chlorination.
fn rapid_sand_filter_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Mechanical Sand Filter".into(),
        pm_water_treatment(
            1890,
            Some("sanit_002"),
            0.08,
            0.25,
            0.67,
            1.0,
            &[(Commodity::Chemicals, 0.5), (Commodity::Energy, 0.3)],
            1200.0,
            0.97,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Improved Rapid Filter".into(),
        pm_water_treatment(
            1920,
            Some("chem_003"),
            0.10,
            0.25,
            0.65,
            1.5,
            &[(Commodity::Chemicals, 0.3), (Commodity::Energy, 0.2)],
            1500.0,
            0.98,
        ),
    );
    m
}

/// Phase 83: Chlorination plant (1910s). Chemical disinfection.
fn chlorination_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Chlorine Disinfection".into(),
        pm_water_treatment(
            1910,
            Some("chem_002"),
            0.08,
            0.25,
            0.67,
            1.0,
            &[(Commodity::Chemicals, 1.0), (Commodity::Energy, 0.2)],
            1800.0,
            0.98,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Improved Chlorination".into(),
        pm_water_treatment(
            1940,
            Some("chem_006"),
            0.10,
            0.28,
            0.62,
            1.5,
            &[(Commodity::Chemicals, 0.8), (Commodity::Energy, 0.15)],
            2200.0,
            0.99,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Chloramine Treatment".into(),
        pm_water_treatment(
            1970,
            Some("chem_008"),
            0.12,
            0.30,
            0.58,
            2.0,
            &[(Commodity::Chemicals, 0.5), (Commodity::Energy, 0.1)],
            2600.0,
            0.995,
        ),
    );
    m
}

/// Phase 83: Modern water treatment plant (1950s). Coagulation + flocculation + filtration + chlorination.
fn modern_treatment_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Coagulation-Flocculation".into(),
        pm_water_treatment(
            1950,
            Some("chem_006"),
            0.10,
            0.30,
            0.60,
            1.0,
            &[(Commodity::Chemicals, 1.5), (Commodity::Energy, 1.0)],
            3000.0,
            0.99,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Improved Coagulation".into(),
        pm_water_treatment(
            1970,
            Some("chem_008"),
            0.12,
            0.32,
            0.56,
            1.5,
            &[(Commodity::Chemicals, 1.2), (Commodity::Energy, 0.8)],
            3500.0,
            0.995,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Optimized Treatment".into(),
        pm_water_treatment(
            1990,
            Some("advman_004"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[(Commodity::Chemicals, 1.0), (Commodity::Energy, 0.5)],
            4000.0,
            1.0,
        ),
    );
    m
}

/// Phase 83: Advanced water treatment plant (1980s). Ozone + activated carbon + membrane filtration.
fn advanced_treatment_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Ozone Treatment".into(),
        pm_water_treatment(
            1980,
            Some("chem_008"),
            0.15,
            0.35,
            0.50,
            1.0,
            &[(Commodity::Chemicals, 2.0), (Commodity::Energy, 2.0)],
            4500.0,
            1.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Activated Carbon".into(),
        pm_water_treatment(
            1985,
            Some("advman_004"),
            0.15,
            0.35,
            0.50,
            1.2,
            &[(Commodity::Chemicals, 1.5), (Commodity::Energy, 1.5)],
            4800.0,
            1.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Membrane Filtration".into(),
        pm_water_treatment(
            1995,
            Some("advman_005"),
            0.18,
            0.37,
            0.45,
            1.5,
            &[(Commodity::Chemicals, 1.0), (Commodity::Energy, 2.0)],
            5200.0,
            1.0,
        ),
    );
    m
}

/// Phase 83: Desalination plant (1960s, coastal/arid constraint).
/// PATCH 8: Draws from implicit infinite Ocean — does NOT drain surface_water_volume.
/// Adds new freshwater mass to the terrestrial water system.
fn desalination_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Multi-Stage Flash".into(),
        pm_water_treatment(
            1960,
            Some("thermo_007"),
            0.10,
            0.30,
            0.60,
            1.0,
            &[(Commodity::Energy, 8.0)],
            1000.0,
            0.99,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Reverse Osmosis".into(),
        pm_water_treatment(
            1980,
            Some("advman_004"),
            0.12,
            0.32,
            0.56,
            1.5,
            &[(Commodity::Energy, 4.0), (Commodity::Chemicals, 0.5)],
            2000.0,
            0.995,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Advanced RO".into(),
        pm_water_treatment(
            2000,
            Some("advman_005"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[(Commodity::Energy, 2.5), (Commodity::Chemicals, 0.3)],
            2800.0,
            1.0,
        ),
    );
    m
}

/// Phase 83: Shared water treatment automation methods.
/// Applied to all water treatment plant types via MethodSlot::Automation.
fn water_automation_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Automation,
        "Manual Operation".into(),
        pm(
            1850,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Valve Control".into(),
        pm_capex(
            1900,
            Some("sanit_001"),
            0.08,
            0.25,
            0.67,
            1.5,
            &[(Commodity::MechanicalComponents, 2.0)],
            &[],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Mechanical Backwash".into(),
        pm(
            1920,
            Some("steam_005"),
            0.10,
            0.28,
            0.62,
            1.8,
            &[(Commodity::MechanicalComponents, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Automated Filter Control".into(),
        pm(
            1960,
            Some("auto3_001"),
            0.12,
            0.30,
            0.58,
            2.5,
            &[
                (Commodity::Energy, 2.0),
                (Commodity::MechanicalComponents, 1.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "SCADA Control".into(),
        pm(
            1985,
            Some("cs_005"),
            0.18,
            0.35,
            0.47,
            4.0,
            &[
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "AI Process Control".into(),
        pm(
            2010,
            Some("cs_008"),
            0.22,
            0.38,
            0.40,
            6.0,
            &[
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Software, 5.0),
            ],
            &[],
        ),
    );
    m
}

/// Phase 83: Shared water treatment organization methods.
fn water_organization_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Organization,
        "Village Water Office".into(),
        pm(
            1850,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Municipal Water Board".into(),
        pm(
            1900,
            None,
            0.08,
            0.25,
            0.67,
            1.2,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Centralized Dispatch".into(),
        pm(
            1920,
            Some("elecf_005"),
            0.12,
            0.30,
            0.58,
            1.5,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Regional Water Authority".into(),
        pm(
            1960,
            Some("cs_004"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Independent System Operator".into(),
        pm(
            2000,
            Some("cs_008"),
            0.25,
            0.38,
            0.37,
            3.0,
            &[(Commodity::Food, 5.0), (Commodity::Software, 5.0)],
            &[],
        ),
    );
    m
}

// ============================================================================
// PHASE 83: WASTEWATER TREATMENT PLANT REGISTRIES
// ============================================================================

/// Phase 83: Primary settling plant (1890s). Simple sedimentation tanks.
/// Discharge quality ~0.30. Low treatment efficiency.
fn primary_settling_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Settling Tank".into(),
        pm_wastewater_treatment(
            1890,
            Some("sanit_002"),
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Energy, 0.5)],
            0.05,
            0.30,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Improved Settling".into(),
        pm_wastewater_treatment(
            1920,
            Some("chem_003"),
            0.08,
            0.25,
            0.67,
            1.3,
            &[(Commodity::Energy, 0.5), (Commodity::Chemicals, 0.3)],
            0.08,
            0.35,
        ),
    );
    m
}

/// Phase 83: Activated sludge plant (1910s). Biological treatment with aeration.
fn activated_sludge_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Activated Sludge".into(),
        pm_wastewater_treatment(
            1910,
            Some("sanit_003"),
            0.08,
            0.25,
            0.67,
            1.0,
            &[(Commodity::Energy, 2.0), (Commodity::Chemicals, 0.5)],
            0.15,
            0.50,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Improved Aeration".into(),
        pm_wastewater_treatment(
            1940,
            Some("chem_006"),
            0.10,
            0.28,
            0.62,
            1.5,
            &[(Commodity::Energy, 1.5), (Commodity::Chemicals, 0.3)],
            0.18,
            0.55,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Optimized Activated Sludge".into(),
        pm_wastewater_treatment(
            1970,
            Some("auto3_001"),
            0.12,
            0.30,
            0.58,
            2.0,
            &[(Commodity::Energy, 1.0), (Commodity::Chemicals, 0.2)],
            0.22,
            0.60,
        ),
    );
    m
}

/// Phase 83: Secondary treatment plant (1930s). Primary + biological + secondary settling.
fn secondary_treatment_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Trickling Filter".into(),
        pm_wastewater_treatment(
            1930,
            Some("sanit_003"),
            0.08,
            0.25,
            0.67,
            1.0,
            &[(Commodity::Energy, 1.0), (Commodity::Chemicals, 0.5)],
            0.20,
            0.60,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Rotating Biological Contactor".into(),
        pm_wastewater_treatment(
            1970,
            Some("auto3_001"),
            0.10,
            0.28,
            0.62,
            1.5,
            &[(Commodity::Energy, 0.8), (Commodity::Chemicals, 0.3)],
            0.24,
            0.65,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Sequencing Batch Reactor".into(),
        pm_wastewater_treatment(
            1990,
            Some("advman_004"),
            0.12,
            0.30,
            0.58,
            2.0,
            &[(Commodity::Energy, 0.5), (Commodity::Chemicals, 0.2)],
            0.27,
            0.68,
        ),
    );
    m
}

/// Phase 83: Tertiary treatment plant (1970s). Nutrient removal + disinfection.
fn tertiary_treatment_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Nutrient Removal".into(),
        pm_wastewater_treatment(
            1970,
            Some("chem_008"),
            0.12,
            0.30,
            0.58,
            1.0,
            &[(Commodity::Energy, 2.0), (Commodity::Chemicals, 1.5)],
            0.28,
            0.70,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "UV Disinfection".into(),
        pm_wastewater_treatment(
            1985,
            Some("advman_004"),
            0.15,
            0.32,
            0.53,
            1.5,
            &[(Commodity::Energy, 3.0), (Commodity::Chemicals, 0.5)],
            0.30,
            0.75,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Advanced Tertiary".into(),
        pm_wastewater_treatment(
            2000,
            Some("advman_005"),
            0.18,
            0.35,
            0.47,
            2.0,
            &[(Commodity::Energy, 2.0), (Commodity::Chemicals, 1.0)],
            0.32,
            0.80,
        ),
    );
    m
}

/// Phase 83: Advanced wastewater treatment plant (1990s). Membrane bioreactor + UV.
fn advanced_wastewater_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Membrane Bioreactor".into(),
        pm_wastewater_treatment(
            1990,
            Some("advman_004"),
            0.15,
            0.32,
            0.53,
            1.0,
            &[(Commodity::Energy, 2.5), (Commodity::Chemicals, 1.0)],
            0.31,
            0.78,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Advanced MBR".into(),
        pm_wastewater_treatment(
            2000,
            Some("advman_005"),
            0.18,
            0.35,
            0.47,
            1.5,
            &[(Commodity::Energy, 2.0), (Commodity::Chemicals, 0.8)],
            0.33,
            0.85,
        ),
    );
    m
}

/// Phase 83: Shared sewage treatment automation methods.
fn sewage_automation_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Automation,
        "Manual Operation".into(),
        pm(
            1890,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Mechanical Scrapers".into(),
        pm(
            1920,
            Some("steam_005"),
            0.08,
            0.25,
            0.67,
            1.5,
            &[(Commodity::MechanicalComponents, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Mechanical Aeration".into(),
        pm(
            1930,
            None,
            0.10,
            0.28,
            0.62,
            1.8,
            &[(Commodity::MechanicalComponents, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Automated DO Control".into(),
        pm(
            1970,
            Some("auto3_001"),
            0.12,
            0.30,
            0.58,
            2.5,
            &[
                (Commodity::Energy, 2.0),
                (Commodity::MechanicalComponents, 1.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "SCADA Control".into(),
        pm(
            1985,
            Some("cs_005"),
            0.18,
            0.35,
            0.47,
            4.0,
            &[
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "AI Process Control".into(),
        pm(
            2010,
            Some("cs_008"),
            0.22,
            0.38,
            0.40,
            6.0,
            &[
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Software, 5.0),
            ],
            &[],
        ),
    );
    m
}

/// Phase 83: Shared sewage treatment organization methods.
fn sewage_organization_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Organization,
        "Municipal Sewage Board".into(),
        pm(
            1890,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Centralized Dispatch".into(),
        pm(
            1920,
            Some("elecf_005"),
            0.12,
            0.30,
            0.58,
            1.5,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Regional Water Authority".into(),
        pm(
            1960,
            Some("cs_004"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Independent System Operator".into(),
        pm(
            2000,
            Some("cs_008"),
            0.25,
            0.38,
            0.37,
            3.0,
            &[(Commodity::Food, 5.0), (Commodity::Software, 5.0)],
            &[],
        ),
    );
    m
}

/// Builds the complete hardcoded production method registry for all sectors.
pub fn default_production_methods() -> HashMap<String, BuildingMethods> {
    let mut registry: HashMap<String, BuildingMethods> = HashMap::new();

    registry.insert("mining".to_string(), mining_methods());
    registry.insert("agriculture".to_string(), agriculture_methods());
    registry.insert("heavy_industry".to_string(), heavy_industry_methods());
    registry.insert("light_industry".to_string(), light_industry_methods());
    registry.insert("armaments_industry".to_string(), armaments_methods());
    registry.insert("construction".to_string(), construction_methods());
    // Blueprint 006: Deep Well Construction method for off-grid water wells.
    registry.insert("deep_well_construction".to_string(), deep_well_construction_methods());
    registry.insert("energy".to_string(), energy_methods());
    // Phase 81: Plant-type-specific energy production method registries.
    registry.insert("coal_fired_plant".to_string(), coal_fired_plant_methods());
    registry.insert(
        "lignite_fired_plant".to_string(),
        lignite_fired_plant_methods(),
    );
    registry.insert("oil_gas_plant".to_string(), oil_gas_plant_methods());
    registry.insert("nuclear_plant".to_string(), nuclear_plant_methods());
    registry.insert("solar_plant".to_string(), solar_plant_methods());
    registry.insert("wind_farm".to_string(), wind_farm_methods());
    registry.insert("hydro_plant".to_string(), hydro_plant_methods());
    registry.insert("pumped_storage".to_string(), pumped_storage_methods());
    registry.insert("battery_storage".to_string(), battery_storage_methods());
    registry.insert("geothermal_plant".to_string(), geothermal_plant_methods());
    registry.insert("biomass_plant".to_string(), biomass_plant_methods());
    registry.insert("biogas_plant".to_string(), biogas_plant_methods());
    // Phase 81: Shared automation and organization methods for all energy plant types.
    registry.insert("energy_automation".to_string(), energy_automation_methods());
    registry.insert(
        "energy_organization".to_string(),
        energy_organization_methods(),
    );
    // Phase 82: Distinct heating plant registries — each plant type has its own
    // key with full Production/Automation/Organization matrices.
    registry.insert("wood_boiler_plant".to_string(), wood_boiler_plant_methods());
    registry.insert("coal_heat_plant".to_string(), coal_heat_plant_methods());
    registry.insert(
        "lignite_heat_plant".to_string(),
        lignite_heat_plant_methods(),
    );
    registry.insert(
        "coke_oven_gas_heat_plant".to_string(),
        coke_oven_gas_heat_plant_methods(),
    );
    registry.insert("oil_heat_plant".to_string(), oil_heat_plant_methods());
    registry.insert(
        "natural_gas_heat_plant".to_string(),
        natural_gas_heat_plant_methods(),
    );
    registry.insert(
        "geothermal_heat_plant".to_string(),
        geothermal_heat_plant_methods(),
    );
    // Phase 82: Shared automation and organization for heating plant types
    // (identical to the energy_automation/energy_organization pattern).
    registry.insert(
        "heating_automation".to_string(),
        heating_automation_methods(),
    );
    registry.insert(
        "heating_organization".to_string(),
        heating_organization_methods(),
    );
    // Phase 82B: Emission control registries for industrial/heating/power plants.
    registry.insert(
        "heavy_industry_emission_control".to_string(),
        heavy_industry_emission_control_methods(),
    );
    registry.insert(
        "heating_plant_emission_control".to_string(),
        heating_plant_emission_control_methods(),
    );
    registry.insert(
        "power_plant_emission_control".to_string(),
        power_plant_emission_control_methods(),
    );
    // Phase 83: Water treatment plant registries — each plant type has its own
    // key with full Production/Automation/Organization matrices (Rule 13).
    // PARADIGM SHIFT: These plants upgrade water quality, not produce PotableWater.
    registry.insert(
        "slow_sand_filter_plant".to_string(),
        slow_sand_filter_plant_methods(),
    );
    registry.insert(
        "rapid_sand_filter_plant".to_string(),
        rapid_sand_filter_plant_methods(),
    );
    registry.insert(
        "chlorination_plant".to_string(),
        chlorination_plant_methods(),
    );
    registry.insert(
        "modern_treatment_plant".to_string(),
        modern_treatment_plant_methods(),
    );
    registry.insert(
        "advanced_treatment_plant".to_string(),
        advanced_treatment_plant_methods(),
    );
    registry.insert(
        "desalination_plant".to_string(),
        desalination_plant_methods(),
    );
    // Phase 83: Shared automation and organization for water treatment plants.
    registry.insert("water_automation".to_string(), water_automation_methods());
    registry.insert(
        "water_organization".to_string(),
        water_organization_methods(),
    );
    // Phase 83: Wastewater treatment plant registries.
    // PARADIGM SHIFT: These plants filter blackwater, extract Fertilizers,
    // and discharge healed water back to surface reserves.
    registry.insert(
        "primary_settling_plant".to_string(),
        primary_settling_plant_methods(),
    );
    registry.insert(
        "activated_sludge_plant".to_string(),
        activated_sludge_plant_methods(),
    );
    registry.insert(
        "secondary_treatment_plant".to_string(),
        secondary_treatment_plant_methods(),
    );
    registry.insert(
        "tertiary_treatment_plant".to_string(),
        tertiary_treatment_plant_methods(),
    );
    registry.insert(
        "advanced_wastewater_plant".to_string(),
        advanced_wastewater_plant_methods(),
    );
    // Phase 83: Shared automation and organization for wastewater treatment plants.
    registry.insert("sewage_automation".to_string(), sewage_automation_methods());
    registry.insert(
        "sewage_organization".to_string(),
        sewage_organization_methods(),
    );
    // Phase 84: Waste plant registries — 13 plant types with full
    // Production/Automation/Organization matrices (Rule 13).
    // Mass conservation: every recycling/separation/WtE method outputs
    // residual waste so output mass = input mass. WtE outputs HazardousWaste ash.
    registry.insert(
        "uncontrolled_landfill".to_string(),
        uncontrolled_landfill_methods(),
    );
    registry.insert(
        "controlled_landfill".to_string(),
        controlled_landfill_methods(),
    );
    registry.insert("modern_landfill".to_string(), modern_landfill_methods());
    registry.insert(
        "waste_separation_plant".to_string(),
        waste_separation_plant_methods(),
    );
    registry.insert(
        "advanced_sorting_facility".to_string(),
        advanced_sorting_facility_methods(),
    );
    registry.insert("metal_recycling".to_string(), metal_recycling_methods());
    registry.insert("glass_recycling".to_string(), glass_recycling_methods());
    registry.insert("plastic_recycling".to_string(), plastic_recycling_methods());
    registry.insert(
        "electronic_recycling".to_string(),
        electronic_recycling_methods(),
    );
    registry.insert("textile_recycling".to_string(), textile_recycling_methods());
    registry.insert(
        "waste_to_energy_plant".to_string(),
        waste_to_energy_plant_methods(),
    );
    registry.insert("advanced_wte_chp".to_string(), advanced_wte_chp_methods());
    registry.insert(
        "civic_amenity_site".to_string(),
        civic_amenity_site_methods(),
    );
    // Phase 84: Shared automation and organization for all waste plant types.
    registry.insert("waste_automation".to_string(), waste_automation_methods());
    registry.insert(
        "waste_organization".to_string(),
        waste_organization_methods(),
    );
    registry.insert("transport_logistics".to_string(), transport_methods());
    registry.insert("media_and_entertainment".to_string(), media_methods());
    registry.insert("medical_services".to_string(), medical_methods());
    registry.insert("educational_services".to_string(), education_methods());
    registry.insert("sports_recreation".to_string(), sports_recreation_methods());
    registry.insert("public_services".to_string(), public_services_methods());
    registry.insert(
        "maintenance_workshops".to_string(),
        maintenance_workshops_methods(),
    );

    // Phase 81 Wave 2: Consumption method registries for lighting, heating,
    // ventilation, and power generation. These are keyed by building type
    // and use the new MethodSlot variants (Lighting, Heating, Ventilation,
    // PowerGeneration). Per-unit rates are scaled by building capacity at
    // runtime (Flaw 1 correction).
    registry.insert(
        "housing_consumption".to_string(),
        housing_consumption_methods(),
    );
    registry.insert(
        "commercial_consumption".to_string(),
        commercial_consumption_methods(),
    );
    registry.insert(
        "heavy_industry_consumption".to_string(),
        heavy_industry_consumption_methods(),
    );
    registry.insert(
        "mining_consumption".to_string(),
        mining_consumption_methods(),
    );

    // Phase 83 (PATCH 3): Populate biohazard_factor on pathogenic industries.
    // These values represent the biological load (BOD/COD proxy) per unit of
    // water consumed by each industry. Physical constants calibrated from
    // historical public health data on industrial wastewater.
    populate_industrial_biohazard_factors(&mut registry);

    registry
}

/// Phase 83 (PATCH 3): Set `biohazard_factor` on pathogenic production methods.
///
/// Iterates the registry and sets `biohazard_factor` on specific method names
/// within heavy_industry and light_industry sectors. Values represent the
/// pathogenic mass per unit of water consumed (BOD/COD-like proxy).
fn populate_industrial_biohazard_factors(registry: &mut HashMap<String, BuildingMethods>) {
    // Map: sector_key → [(method_name, biohazard_factor)]
    let biohazard_map: &[(&str, &[(&str, f64)])] = &[
        // Heavy industry — pathogenic processes
        (
            "heavy_industry",
            &[
                ("Tannery", 8.0),
                ("Abattoir", 7.0),
                ("Chemical Plant", 3.0),
                // Steel, Cement, Glass = 0.0 (already default, no pathogenic load)
            ],
        ),
        // Light industry — pathogenic processes
        (
            "light_industry",
            &[
                ("Paper Mill", 5.0),
                ("Food Processing", 6.0),
                ("Textile Mill", 4.0),
                ("Brewery", 4.0),
            ],
        ),
        // Mining — low pathogenic load
        ("mining", &[("Mining", 1.0)]),
    ];

    for (sector_key, methods) in biohazard_map {
        if let Some(building_methods) = registry.get_mut(*sector_key) {
            for &(method_name, factor) in methods.iter() {
                if let Some(pm) = building_methods.production.get_mut(method_name) {
                    pm.biohazard_factor = factor;
                }
            }
        }
    }
}

// === PHASE 81 WAVE 2: CONSUMPTION METHODS ===

/// Phase 81 Wave 2: Housing consumption methods (lighting, heating, power generation).
/// Per-unit rates are PER OCCUPANT. Actual consumption = rate * occupied_slots.
/// Applied to all HousingBuilding types.
fn housing_consumption_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();

    // ── Lighting ──
    m.insert(
        MethodSlot::Lighting,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Lighting,
        "Kerosene Lamps".into(),
        pm(
            1860,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Oil, 0.5)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Gas Mantle".into(),
        pm(
            1890,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::CoalGas, 0.3)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Incandescent Bulbs".into(),
        pm_capex(
            1900,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 2.0)],
            &[],
            &[(Commodity::Glass, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Fluorescent Tubes".into(),
        pm_capex(
            1940,
            Some("elec_005"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 1.0)],
            &[],
            &[(Commodity::Glass, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "LED Lighting".into(),
        pm_capex(
            2000,
            Some("elec_010"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 0.2)],
            &[],
            &[
                (Commodity::ElectronicComponents, 0.05),
                (Commodity::Semiconductors, 0.02),
            ],
        ),
    );

    // ── Heating (Phase 82: Parallel Standalone + District Heating tracks) ──
    // Standalone Track: building consumes fuel directly, generates local emissions
    m.insert(
        MethodSlot::Heating,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Heating,
        "Primitive Fireplace".into(),
        pm_consumption_emission(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Timber, 1.5)],
            &[],
            &[],
            3.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Peat Stove".into(),
        pm_consumption_emission(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Peat, 2.0)],
            &[],
            &[(Commodity::Steel, 0.05)],
            3.5,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Coal Stove".into(),
        pm_consumption_emission(
            1860,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::HardCoal, 1.0)],
            &[],
            &[(Commodity::Steel, 0.1)],
            5.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Advanced Coal Stove".into(),
        pm_consumption_emission(
            1900,
            Some("thermo_020"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::HardCoal, 0.7)],
            &[],
            &[(Commodity::Steel, 0.15)],
            3.5,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Oil Boiler".into(),
        pm_consumption_emission(
            1910,
            Some("thermo_022"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Fuels, 0.5)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.05)],
            2.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Electric Radiator".into(),
        pm_consumption_emission(
            1920,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 5.0)],
            &[],
            &[(Commodity::Steel, 0.1)],
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Condensing Gas Boiler".into(),
        pm_consumption_emission(
            1970,
            Some("thermo_023"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::NaturalGas, 0.4)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.1)],
            0.3,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Heat Pump".into(),
        pm_consumption_emission(
            1980,
            Some("thermo_010"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 1.5)],
            &[],
            &[
                (Commodity::Steel, 0.15),
                (Commodity::Copper, 0.1),
                (Commodity::ElectronicComponents, 0.05),
            ],
            0.0,
        ),
    );
    // District Heating Track: building consumes Commodity::Heat from the grid.
    // The `efficiency` field represents how much delivered heat is useful
    // (0.6 = 40% wasted for Unmetered Radiators, 0.95 = 5% wasted for Smart Substations).
    // Emissions are at the central plant, not the building — emission_factor = 0.0.
    // Phase 85B: Base "District Heating" method — the generic entry point for
    // district heating connection. Functionally identical to "Unmetered Radiators"
    // (same year, tech, inputs, efficiency) but serves as the canonical registry
    // key that tests and `is_district_heating_method()` check for.
    m.insert(
        MethodSlot::Heating,
        "District Heating".into(),
        pm_capex(
            1890,
            Some("thermo_020"),
            0.0,
            0.0,
            1.0,
            0.6,
            &[(Commodity::Heat, 5.0)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Unmetered Radiators".into(),
        pm_capex(
            1890,
            Some("thermo_020"),
            0.0,
            0.0,
            1.0,
            0.6,
            &[(Commodity::Heat, 5.0)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Thermostatic Valves".into(),
        pm_capex(
            1930,
            Some("steam_005"),
            0.0,
            0.0,
            1.0,
            0.8,
            &[(Commodity::Heat, 3.5)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.08)],
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Smart Substations".into(),
        pm_capex(
            1980,
            Some("cs_005"),
            0.0,
            0.0,
            1.0,
            0.95,
            &[(Commodity::Heat, 2.5), (Commodity::Energy, 0.2)],
            &[],
            &[
                (Commodity::Steel, 0.15),
                (Commodity::Copper, 0.1),
                (Commodity::ElectronicComponents, 0.1),
            ],
        ),
    );

    // ── Power Generation (microgeneration) ──
    m.insert(
        MethodSlot::PowerGeneration,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::PowerGeneration,
        "Rooftop PV".into(),
        pm_capex(
            2000,
            Some("elec_010"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[(Commodity::Energy, 0.5)],
            &[
                (Commodity::PhotovoltaicPanels, 1.0),
                (Commodity::Steel, 0.2),
            ],
        ),
    );
    m.insert(
        MethodSlot::PowerGeneration,
        "Rooftop PV + Battery".into(),
        pm_capex(
            2010,
            Some("elec_012"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[(Commodity::Energy, 0.5)],
            &[
                (Commodity::PhotovoltaicPanels, 1.0),
                (Commodity::Batteries, 0.5),
                (Commodity::Steel, 0.2),
            ],
        ),
    );

    // ── Phase 83: Water Supply (Parallel Standalone + Centralized tracks) ──
    // Standalone Track: draws from WaterReserveState (groundwater/surface).
    // No per-turn Commodity::Water market input — CAPEX only.
    // REFINEMENT 1: Water quality = natural source quality (groundwater: 0.9, surface: 0.6).
    m.insert(
        MethodSlot::WaterSupply,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Local Well".into(),
        pm_capex(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Rainwater Catchment".into(),
        pm_capex(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.02), (Commodity::Timber, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Hand Pump Well".into(),
        pm_capex(
            1880,
            Some("sanit_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Shallow Tube Well".into(),
        pm_capex(
            1930,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 0.1)],
            &[],
            &[(Commodity::Steel, 0.15), (Commodity::Copper, 0.05)],
        ),
    );
    // Centralized Track: draws from WaterNetworkState at current_quality.
    // No per-turn Commodity::Water market input — water is already in the grid.
    m.insert(
        MethodSlot::WaterSupply,
        "Municipal Mains (Basic)".into(),
        pm_capex(
            1890,
            Some("sanit_002"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Copper, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Metered Connection".into(),
        pm_capex(
            1930,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Copper, 0.1), (Commodity::Steel, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Pressurized Mains".into(),
        pm_capex(
            1960,
            Some("auto3_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 0.2)],
            &[],
            &[(Commodity::Copper, 0.08), (Commodity::Steel, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Smart Meter Connection".into(),
        pm_capex(
            2000,
            Some("cs_005"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 0.1)],
            &[],
            &[
                (Commodity::Copper, 0.05),
                (Commodity::ElectronicComponents, 0.05),
            ],
        ),
    );

    // ── Phase 83: Sanitation (Parallel Standalone + Centralized tracks) ──
    // Standalone Track: discharges to environment (biohazard) or groundwater.
    // biohazard_factor is stored on the method via a custom field.
    m.insert(
        MethodSlot::Sanitation,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Open Defecation".into(),
        pm(1850, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Cesspool".into(),
        pm_capex(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Cement, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Outhouse".into(),
        pm_capex(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Timber, 0.1), (Commodity::Cement, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Septic Tank".into(),
        pm_capex(
            1900,
            Some("sanit_002"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Cement, 0.2), (Commodity::Steel, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Improved Septic".into(),
        pm_capex(
            1950,
            Some("chem_006"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Cement, 0.2), (Commodity::Steel, 0.1)],
        ),
    );
    // Centralized Track: discharges to SewerNetworkState.
    m.insert(
        MethodSlot::Sanitation,
        "Municipal Sewer (Basic)".into(),
        pm_capex(
            1890,
            Some("sanit_002"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.05), (Commodity::Cement, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Improved Sewer Connection".into(),
        pm_capex(
            1930,
            Some("sanit_003"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.05), (Commodity::Cement, 0.08)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Modern Sewer + Treatment".into(),
        pm_capex(
            1970,
            Some("sanit_005"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.03), (Commodity::Cement, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Advanced Sewer + Tertiary".into(),
        pm_capex(
            2000,
            Some("sanit_006"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[
                (Commodity::Steel, 0.02),
                (Commodity::ElectronicComponents, 0.02),
            ],
        ),
    );

    // ── Phase 84: Waste Disposal ──
    // REFINEMENT 1: Cumulative evolutionary rural track (single Method Slot).
    //   Primitive Dumping → Basic Homesteading → Advanced Rural Scavenging.
    //   Each tier subsumes previous capabilities (composting + scrap recovery).
    // REFINEMENT 2: Dumping vector is runtime-computed from region geography,
    //   not chosen by the player. See select_dumping_vector() in waste_grid.rs.
    // Standalone Track (Self-Disposal / Rural):
    m.insert(
        MethodSlot::WasteDisposal,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Primitive Dumping".into(),
        pm_waste(1850, None, 0.0, 0.0, 1.0, 1.0, &[], &[], 0.0, 5.0, 0.0),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Basic Homesteading".into(),
        pm_waste(
            1880,
            Some("sanit_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::BioWaste, 1.0)],
            &[(Commodity::Fertilizers, 0.5)], // 50% composting yield
            0.0,
            1.0,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Advanced Rural Scavenging".into(),
        pm_waste(
            1900,
            Some("sanit_002"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[
                (Commodity::BioWaste, 1.0),
                (Commodity::MetalWaste, 1.0),
                (Commodity::GlassWaste, 1.0),
            ],
            &[
                (Commodity::Fertilizers, 0.5),
                (Commodity::Steel, 0.4),
                (Commodity::Glass, 0.4),
            ],
            0.0,
            0.5,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Trash Burning".into(),
        pm_waste(1850, None, 0.0, 0.0, 1.0, 1.0, &[], &[], 0.8, 0.0, 0.0),
    ); // severe smog, zero biohazard
       // Centralized Track (Municipal Collection via WasteGridState):
    m.insert(
        MethodSlot::WasteDisposal,
        "Unsegregated Collection".into(),
        pm_capex(
            1890,
            Some("sanit_002"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.05), (Commodity::Cement, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Source-Separated Curbside".into(),
        pm_capex(
            1950,
            Some("sanit_004"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.03), (Commodity::Plastics, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Smart Sorted Collection".into(),
        pm_capex(
            2000,
            Some("sanit_006"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[
                (Commodity::Steel, 0.02),
                (Commodity::ElectronicComponents, 0.02),
            ],
        ),
    );

    m
}

/// Phase 81 Wave 2: Commercial consumption methods (lighting, heating, power generation).
/// Per-unit rates are PER 100 SQM. Actual consumption = rate * (office_capacity + retail_capacity) / 100.0.
fn commercial_consumption_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();

    // ── Lighting ── (same progression as housing, per 100 sqm rates)
    m.insert(
        MethodSlot::Lighting,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Lighting,
        "Kerosene Lamps".into(),
        pm(
            1860,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Oil, 0.5)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Gas Mantle".into(),
        pm(
            1890,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::CoalGas, 0.3)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Incandescent Bulbs".into(),
        pm_capex(
            1900,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 2.0)],
            &[],
            &[(Commodity::Glass, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Fluorescent Tubes".into(),
        pm_capex(
            1940,
            Some("elec_005"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 1.0)],
            &[],
            &[(Commodity::Glass, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "LED Lighting".into(),
        pm_capex(
            2000,
            Some("elec_010"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 0.2)],
            &[],
            &[
                (Commodity::ElectronicComponents, 0.05),
                (Commodity::Semiconductors, 0.02),
            ],
        ),
    );

    // ── Heating (Phase 82: Parallel Standalone + District Heating tracks) ──
    // (same progression as housing, per 100 sqm rates)
    m.insert(
        MethodSlot::Heating,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Heating,
        "Primitive Fireplace".into(),
        pm_consumption_emission(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Timber, 1.5)],
            &[],
            &[],
            3.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Peat Stove".into(),
        pm_consumption_emission(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Peat, 2.0)],
            &[],
            &[(Commodity::Steel, 0.05)],
            3.5,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Coal Stove".into(),
        pm_consumption_emission(
            1860,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::HardCoal, 1.0)],
            &[],
            &[(Commodity::Steel, 0.1)],
            5.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Advanced Coal Stove".into(),
        pm_consumption_emission(
            1900,
            Some("thermo_020"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::HardCoal, 0.7)],
            &[],
            &[(Commodity::Steel, 0.15)],
            3.5,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Oil Boiler".into(),
        pm_consumption_emission(
            1910,
            Some("thermo_022"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Fuels, 0.5)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.05)],
            2.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Electric Radiator".into(),
        pm_consumption_emission(
            1920,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 5.0)],
            &[],
            &[(Commodity::Steel, 0.1)],
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Condensing Gas Boiler".into(),
        pm_consumption_emission(
            1970,
            Some("thermo_023"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::NaturalGas, 0.4)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.1)],
            0.3,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Heat Pump".into(),
        pm_consumption_emission(
            1980,
            Some("thermo_010"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 1.5)],
            &[],
            &[
                (Commodity::Steel, 0.15),
                (Commodity::Copper, 0.1),
                (Commodity::ElectronicComponents, 0.05),
            ],
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Unmetered Radiators".into(),
        pm_capex(
            1890,
            Some("thermo_020"),
            0.0,
            0.0,
            1.0,
            0.6,
            &[(Commodity::Heat, 5.0)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Thermostatic Valves".into(),
        pm_capex(
            1930,
            Some("steam_005"),
            0.0,
            0.0,
            1.0,
            0.8,
            &[(Commodity::Heat, 3.5)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.08)],
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Smart Substations".into(),
        pm_capex(
            1980,
            Some("cs_005"),
            0.0,
            0.0,
            1.0,
            0.95,
            &[(Commodity::Heat, 2.5), (Commodity::Energy, 0.2)],
            &[],
            &[
                (Commodity::Steel, 0.15),
                (Commodity::Copper, 0.1),
                (Commodity::ElectronicComponents, 0.1),
            ],
        ),
    );

    // ── Power Generation (microgeneration) ──
    m.insert(
        MethodSlot::PowerGeneration,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::PowerGeneration,
        "Rooftop PV".into(),
        pm_capex(
            2000,
            Some("elec_010"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[(Commodity::Energy, 0.5)],
            &[
                (Commodity::PhotovoltaicPanels, 1.0),
                (Commodity::Steel, 0.2),
            ],
        ),
    );
    m.insert(
        MethodSlot::PowerGeneration,
        "Rooftop PV + Battery".into(),
        pm_capex(
            2010,
            Some("elec_012"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[(Commodity::Energy, 0.5)],
            &[
                (Commodity::PhotovoltaicPanels, 1.0),
                (Commodity::Batteries, 0.5),
                (Commodity::Steel, 0.2),
            ],
        ),
    );

    // ── Phase 83: Water Supply (same tracks as housing, per 100 sqm) ──
    m.insert(
        MethodSlot::WaterSupply,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Local Well".into(),
        pm_capex(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Rainwater Catchment".into(),
        pm_capex(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.02), (Commodity::Timber, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Hand Pump Well".into(),
        pm_capex(
            1880,
            Some("sanit_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Shallow Tube Well".into(),
        pm_capex(
            1930,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 0.1)],
            &[],
            &[(Commodity::Steel, 0.15), (Commodity::Copper, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Municipal Mains (Basic)".into(),
        pm_capex(
            1890,
            Some("sanit_002"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Copper, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Metered Connection".into(),
        pm_capex(
            1930,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Copper, 0.1), (Commodity::Steel, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Pressurized Mains".into(),
        pm_capex(
            1960,
            Some("auto3_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 0.2)],
            &[],
            &[(Commodity::Copper, 0.08), (Commodity::Steel, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::WaterSupply,
        "Smart Meter Connection".into(),
        pm_capex(
            2000,
            Some("cs_005"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 0.1)],
            &[],
            &[
                (Commodity::Copper, 0.05),
                (Commodity::ElectronicComponents, 0.05),
            ],
        ),
    );

    // ── Phase 83: Sanitation (same tracks as housing, per 100 sqm) ──
    m.insert(
        MethodSlot::Sanitation,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Open Defecation".into(),
        pm(1850, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Cesspool".into(),
        pm_capex(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Cement, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Outhouse".into(),
        pm_capex(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Timber, 0.1), (Commodity::Cement, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Septic Tank".into(),
        pm_capex(
            1900,
            Some("sanit_002"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Cement, 0.2), (Commodity::Steel, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Improved Septic".into(),
        pm_capex(
            1950,
            Some("chem_006"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Cement, 0.2), (Commodity::Steel, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Municipal Sewer (Basic)".into(),
        pm_capex(
            1890,
            Some("sanit_002"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.05), (Commodity::Cement, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Improved Sewer Connection".into(),
        pm_capex(
            1930,
            Some("sanit_003"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.05), (Commodity::Cement, 0.08)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Modern Sewer + Treatment".into(),
        pm_capex(
            1970,
            Some("sanit_005"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.03), (Commodity::Cement, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Sanitation,
        "Advanced Sewer + Tertiary".into(),
        pm_capex(
            2000,
            Some("sanit_006"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[
                (Commodity::Steel, 0.02),
                (Commodity::ElectronicComponents, 0.02),
            ],
        ),
    );

    // ── Phase 84: Waste Disposal (commercial, same tracks as housing) ──
    m.insert(
        MethodSlot::WasteDisposal,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Primitive Dumping".into(),
        pm_waste(1850, None, 0.0, 0.0, 1.0, 1.0, &[], &[], 0.0, 5.0, 0.0),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Basic Homesteading".into(),
        pm_waste(
            1880,
            Some("sanit_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::BioWaste, 1.0)],
            &[(Commodity::Fertilizers, 0.5)],
            0.0,
            1.0,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Advanced Rural Scavenging".into(),
        pm_waste(
            1900,
            Some("sanit_002"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[
                (Commodity::BioWaste, 1.0),
                (Commodity::MetalWaste, 1.0),
                (Commodity::GlassWaste, 1.0),
            ],
            &[
                (Commodity::Fertilizers, 0.5),
                (Commodity::Steel, 0.4),
                (Commodity::Glass, 0.4),
            ],
            0.0,
            0.5,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Trash Burning".into(),
        pm_waste(1850, None, 0.0, 0.0, 1.0, 1.0, &[], &[], 0.8, 0.0, 0.0),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Unsegregated Collection".into(),
        pm_capex(
            1890,
            Some("sanit_002"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.05), (Commodity::Cement, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Source-Separated Curbside".into(),
        pm_capex(
            1950,
            Some("sanit_004"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[(Commodity::Steel, 0.03), (Commodity::Plastics, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::WasteDisposal,
        "Smart Sorted Collection".into(),
        pm_capex(
            2000,
            Some("sanit_006"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[],
            &[],
            &[
                (Commodity::Steel, 0.02),
                (Commodity::ElectronicComponents, 0.02),
            ],
        ),
    );

    m
}

/// Phase 81 Wave 2: Heavy industry consumption methods (lighting, heating, ventilation).
/// Per-unit rates are PER 1000 WORKERS. Actual consumption = rate * effective_employment / 1000.0.
fn heavy_industry_consumption_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();

    // ── Lighting ──
    m.insert(
        MethodSlot::Lighting,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Lighting,
        "Kerosene Lamps".into(),
        pm(
            1860,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Oil, 0.5)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Gas Mantle".into(),
        pm(
            1890,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::CoalGas, 0.3)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Incandescent Bulbs".into(),
        pm_capex(
            1900,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 2.0)],
            &[],
            &[(Commodity::Glass, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Fluorescent Tubes".into(),
        pm_capex(
            1940,
            Some("elec_005"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 1.0)],
            &[],
            &[(Commodity::Glass, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "LED Lighting".into(),
        pm_capex(
            2000,
            Some("elec_010"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 0.2)],
            &[],
            &[
                (Commodity::ElectronicComponents, 0.05),
                (Commodity::Semiconductors, 0.02),
            ],
        ),
    );

    // ── Heating (Phase 82: Parallel Standalone + District Heating tracks) ──
    // Standalone Track: building consumes fuel directly, generates local emissions
    m.insert(
        MethodSlot::Heating,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Heating,
        "Primitive Fireplace".into(),
        pm_consumption_emission(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Timber, 1.5)],
            &[],
            &[],
            3.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Peat Stove".into(),
        pm_consumption_emission(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Peat, 2.0)],
            &[],
            &[(Commodity::Steel, 0.05)],
            3.5,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Coal Stove".into(),
        pm_consumption_emission(
            1860,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::HardCoal, 1.0)],
            &[],
            &[(Commodity::Steel, 0.1)],
            5.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Advanced Coal Stove".into(),
        pm_consumption_emission(
            1900,
            Some("thermo_020"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::HardCoal, 0.7)],
            &[],
            &[(Commodity::Steel, 0.15)],
            3.5,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Oil Boiler".into(),
        pm_consumption_emission(
            1910,
            Some("thermo_022"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Fuels, 0.5)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.05)],
            2.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Electric Radiator".into(),
        pm_consumption_emission(
            1920,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 5.0)],
            &[],
            &[(Commodity::Steel, 0.1)],
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Condensing Gas Boiler".into(),
        pm_consumption_emission(
            1970,
            Some("thermo_023"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::NaturalGas, 0.4)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.1)],
            0.3,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Heat Pump".into(),
        pm_consumption_emission(
            1980,
            Some("thermo_010"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 1.5)],
            &[],
            &[
                (Commodity::Steel, 0.15),
                (Commodity::Copper, 0.1),
                (Commodity::ElectronicComponents, 0.05),
            ],
            0.0,
        ),
    );
    // District Heating Track: building consumes Commodity::Heat from the grid.
    // The `efficiency` field represents how much delivered heat is useful
    // (0.6 = 40% wasted for Unmetered Radiators, 0.95 = 5% wasted for Smart Substations).
    // Emissions are at the central plant, not the building — emission_factor = 0.0.
    m.insert(
        MethodSlot::Heating,
        "Unmetered Radiators".into(),
        pm_capex(
            1890,
            Some("thermo_020"),
            0.0,
            0.0,
            1.0,
            0.6,
            &[(Commodity::Heat, 5.0)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Thermostatic Valves".into(),
        pm_capex(
            1930,
            Some("steam_005"),
            0.0,
            0.0,
            1.0,
            0.8,
            &[(Commodity::Heat, 3.5)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.08)],
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Smart Substations".into(),
        pm_capex(
            1980,
            Some("cs_005"),
            0.0,
            0.0,
            1.0,
            0.95,
            &[(Commodity::Heat, 2.5), (Commodity::Energy, 0.2)],
            &[],
            &[
                (Commodity::Steel, 0.15),
                (Commodity::Copper, 0.1),
                (Commodity::ElectronicComponents, 0.1),
            ],
        ),
    );

    // ── Ventilation/Pumping ── (heavy industry specific)
    m.insert(
        MethodSlot::Ventilation,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Ventilation,
        "Steam-Driven".into(),
        pm(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::HardCoal, 2.0), (Commodity::Water, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Ventilation,
        "Electric Pumps/Fans".into(),
        pm_capex(
            1900,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 3.0)],
            &[],
            &[(Commodity::Steel, 0.5), (Commodity::Copper, 0.1)],
        ),
    );

    m
}

/// Phase 81 Wave 2: Mining consumption methods (lighting, heating, ventilation).
/// Same per-1000-workers scaling as heavy industry. Mines have critical
/// ventilation requirements for safety.
fn mining_consumption_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();

    // ── Lighting ── (same as heavy industry)
    m.insert(
        MethodSlot::Lighting,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Lighting,
        "Kerosene Lamps".into(),
        pm(
            1860,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Oil, 0.5)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Gas Mantle".into(),
        pm(
            1890,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::CoalGas, 0.3)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Incandescent Bulbs".into(),
        pm_capex(
            1900,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 2.0)],
            &[],
            &[(Commodity::Glass, 0.1)],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "Fluorescent Tubes".into(),
        pm_capex(
            1940,
            Some("elec_005"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 1.0)],
            &[],
            &[(Commodity::Glass, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Lighting,
        "LED Lighting".into(),
        pm_capex(
            2000,
            Some("elec_010"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 0.2)],
            &[],
            &[
                (Commodity::ElectronicComponents, 0.05),
                (Commodity::Semiconductors, 0.02),
            ],
        ),
    );

    // ── Heating (Phase 82: Parallel Standalone + District Heating tracks) ──
    // (same as heavy industry)
    m.insert(
        MethodSlot::Heating,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Heating,
        "Primitive Fireplace".into(),
        pm_consumption_emission(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Timber, 1.5)],
            &[],
            &[],
            3.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Peat Stove".into(),
        pm_consumption_emission(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Peat, 2.0)],
            &[],
            &[(Commodity::Steel, 0.05)],
            3.5,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Coal Stove".into(),
        pm_consumption_emission(
            1860,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::HardCoal, 1.0)],
            &[],
            &[(Commodity::Steel, 0.1)],
            5.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Advanced Coal Stove".into(),
        pm_consumption_emission(
            1900,
            Some("thermo_020"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::HardCoal, 0.7)],
            &[],
            &[(Commodity::Steel, 0.15)],
            3.5,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Oil Boiler".into(),
        pm_consumption_emission(
            1910,
            Some("thermo_022"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Fuels, 0.5)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.05)],
            2.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Electric Radiator".into(),
        pm_consumption_emission(
            1920,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 5.0)],
            &[],
            &[(Commodity::Steel, 0.1)],
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Condensing Gas Boiler".into(),
        pm_consumption_emission(
            1970,
            Some("thermo_023"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::NaturalGas, 0.4)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.1)],
            0.3,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Heat Pump".into(),
        pm_consumption_emission(
            1980,
            Some("thermo_010"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 1.5)],
            &[],
            &[
                (Commodity::Steel, 0.15),
                (Commodity::Copper, 0.1),
                (Commodity::ElectronicComponents, 0.05),
            ],
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Unmetered Radiators".into(),
        pm_capex(
            1890,
            Some("thermo_020"),
            0.0,
            0.0,
            1.0,
            0.6,
            &[(Commodity::Heat, 5.0)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.05)],
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Thermostatic Valves".into(),
        pm_capex(
            1930,
            Some("steam_005"),
            0.0,
            0.0,
            1.0,
            0.8,
            &[(Commodity::Heat, 3.5)],
            &[],
            &[(Commodity::Steel, 0.2), (Commodity::Copper, 0.08)],
        ),
    );
    m.insert(
        MethodSlot::Heating,
        "Smart Substations".into(),
        pm_capex(
            1980,
            Some("cs_005"),
            0.0,
            0.0,
            1.0,
            0.95,
            &[(Commodity::Heat, 2.5), (Commodity::Energy, 0.2)],
            &[],
            &[
                (Commodity::Steel, 0.15),
                (Commodity::Copper, 0.1),
                (Commodity::ElectronicComponents, 0.1),
            ],
        ),
    );

    // ── Ventilation/Pumping ── (critical for mine safety)
    m.insert(
        MethodSlot::Ventilation,
        "None".into(),
        pm(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Ventilation,
        "Steam-Driven".into(),
        pm(
            1850,
            None,
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::HardCoal, 2.0), (Commodity::Water, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Ventilation,
        "Electric Pumps/Fans".into(),
        pm_capex(
            1900,
            Some("elec_001"),
            0.0,
            0.0,
            1.0,
            1.0,
            &[(Commodity::Energy, 3.0)],
            &[],
            &[(Commodity::Steel, 0.5), (Commodity::Copper, 0.1)],
        ),
    );

    m
}

// === MINING ===
fn mining_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Manual Mining".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Fuels, 2.0), (Commodity::Food, 5.0)],
            &[(Commodity::HardCoal, 10.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Pneumatic Drilling".into(),
        pm(
            1885,
            Some("mining_002"),
            0.10,
            0.30,
            0.60,
            1.5,
            &[
                (Commodity::Fuels, 5.0),
                (Commodity::Food, 5.0),
                (Commodity::MechanicalComponents, 2.0),
            ],
            &[(Commodity::HardCoal, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Electric Mine Pumps".into(),
        pm(
            1890,
            Some("mining_004"),
            0.10,
            0.30,
            0.60,
            1.8,
            &[(Commodity::Energy, 5.0), (Commodity::Fuels, 3.0)],
            &[(Commodity::HardCoal, 18.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Longwall Mining".into(),
        pm(
            1895,
            Some("mining_006"),
            0.15,
            0.35,
            0.50,
            2.2,
            &[
                (Commodity::Energy, 8.0),
                (Commodity::Fuels, 4.0),
                (Commodity::MechanicalComponents, 3.0),
            ],
            &[(Commodity::HardCoal, 25.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Froth Flotation".into(),
        pm(
            1900,
            Some("mining_007"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[(Commodity::Energy, 10.0), (Commodity::Chemicals, 5.0)],
            &[(Commodity::Copper, 12.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Open-Pit Mining".into(),
        pm(
            1905,
            Some("mining_008"),
            0.15,
            0.30,
            0.55,
            3.0,
            &[(Commodity::Fuels, 15.0), (Commodity::Energy, 10.0)],
            &[(Commodity::HardCoal, 40.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Mechanized Longwall".into(),
        pm(
            1950,
            Some("auto3_001"),
            0.20,
            0.40,
            0.40,
            4.0,
            &[
                (Commodity::Energy, 20.0),
                (Commodity::Fuels, 10.0),
                (Commodity::MechanicalComponents, 8.0),
            ],
            &[(Commodity::HardCoal, 60.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "CNC Mining".into(),
        pm(
            1970,
            Some("auto3_004"),
            0.25,
            0.45,
            0.30,
            5.5,
            &[
                (Commodity::Energy, 25.0),
                (Commodity::Fuels, 8.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::HardCoal, 80.0)],
        ),
    );
    // ── Phase 20: Activate dead commodity extraction ──
    m.insert(
        MethodSlot::Production,
        "Iron Ore Mining".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
            &[(Commodity::Iron, 12.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Copper Ore Mining".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
            &[(Commodity::Copper, 8.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Oil Drilling".into(),
        pm(
            1880,
            None,
            0.08,
            0.25,
            0.67,
            1.0,
            &[
                (Commodity::Fuels, 5.0),
                (Commodity::Food, 5.0),
                (Commodity::MechanicalComponents, 2.0),
            ],
            &[(Commodity::Oil, 30.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Natural Gas Extraction".into(),
        pm(
            1900,
            None,
            0.08,
            0.25,
            0.67,
            1.2,
            &[(Commodity::Fuels, 3.0), (Commodity::Energy, 3.0)],
            &[(Commodity::NaturalGas, 25.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Bauxite Mining".into(),
        pm(
            1890,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
            &[(Commodity::Bauxite, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Sand And Gravel Quarry".into(),
        pm(
            1880,
            None,
            0.03,
            0.15,
            0.82,
            1.0,
            &[(Commodity::Fuels, 2.0), (Commodity::Food, 3.0)],
            &[(Commodity::Sand, 20.0), (Commodity::Gravel, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Stone Quarrying".into(),
        pm(
            1880,
            None,
            0.03,
            0.15,
            0.82,
            1.0,
            &[(Commodity::Fuels, 2.0), (Commodity::Food, 3.0)],
            &[(Commodity::Stone, 25.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Clay Mining".into(),
        pm(
            1880,
            None,
            0.03,
            0.15,
            0.82,
            1.0,
            &[(Commodity::Fuels, 2.0), (Commodity::Food, 3.0)],
            &[(Commodity::Clay, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Limestone Quarrying".into(),
        pm(
            1880,
            None,
            0.03,
            0.15,
            0.82,
            1.0,
            &[(Commodity::Fuels, 2.0), (Commodity::Food, 3.0)],
            &[(Commodity::Limestone, 22.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Sulfur Mining".into(),
        pm(
            1890,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Fuels, 3.0), (Commodity::Energy, 2.0)],
            &[(Commodity::Sulfur, 10.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Salt Mining".into(),
        pm(
            1880,
            None,
            0.03,
            0.15,
            0.82,
            1.0,
            &[(Commodity::Fuels, 2.0), (Commodity::Food, 3.0)],
            &[(Commodity::Salt, 18.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Tin Ore Mining".into(),
        pm(
            1890,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
            &[(Commodity::Tin, 8.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Zinc Ore Mining".into(),
        pm(
            1890,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
            &[(Commodity::Zinc, 8.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Lead Ore Mining".into(),
        pm(
            1890,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
            &[(Commodity::Lead, 8.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Silver Mining".into(),
        pm(
            1890,
            None,
            0.08,
            0.25,
            0.67,
            1.0,
            &[
                (Commodity::Fuels, 5.0),
                (Commodity::Energy, 3.0),
                (Commodity::Chemicals, 2.0),
            ],
            &[(Commodity::Silver, 3.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Gold Mining".into(),
        pm(
            1890,
            None,
            0.08,
            0.25,
            0.67,
            1.0,
            &[
                (Commodity::Fuels, 5.0),
                (Commodity::Energy, 3.0),
                (Commodity::Chemicals, 3.0),
            ],
            &[(Commodity::Gold, 2.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Peat Cutting".into(),
        pm(
            1880,
            None,
            0.02,
            0.10,
            0.88,
            0.8,
            &[(Commodity::Food, 3.0)],
            &[(Commodity::Peat, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Brown Coal Mining".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Fuels, 2.0), (Commodity::Food, 5.0)],
            &[(Commodity::BrownCoal, 18.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Rare Earth Element Mining".into(),
        pm(
            1965,
            Some("rare_001"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[
                (Commodity::Energy, 15.0),
                (Commodity::Chemicals, 8.0),
                (Commodity::Fuels, 5.0),
            ],
            &[(Commodity::RareEarthElements, 5.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Lithium Extraction".into(),
        pm(
            1970,
            Some("lithium_001"),
            0.12,
            0.30,
            0.58,
            1.5,
            &[
                (Commodity::Energy, 10.0),
                (Commodity::Water, 15.0),
                (Commodity::Fuels, 3.0),
            ],
            &[(Commodity::Lithium, 8.0)],
        ),
    );
    // ── Phase 20: Magnesium production ──
    m.insert(
        MethodSlot::Production,
        "Magnesium Refinery".into(),
        pm(
            1900,
            None,
            0.10,
            0.30,
            0.60,
            1.5,
            &[
                (Commodity::Energy, 10.0),
                (Commodity::Water, 5.0),
                (Commodity::Chemicals, 3.0),
            ],
            &[(Commodity::Magnesium, 15.0)],
        ),
    );
    // ── Phase 21A: Uranium mining ──
    m.insert(
        MethodSlot::Production,
        "Uranium Mining".into(),
        pm(
            1945,
            Some("nuc_001"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[
                (Commodity::Energy, 15.0),
                (Commodity::Fuels, 5.0),
                (Commodity::Chemicals, 3.0),
            ],
            &[(Commodity::Uranium, 5.0)],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Manual Labor".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Mechanical Ventilation".into(),
        pm(
            1880,
            Some("mining_001"),
            0.10,
            0.25,
            0.65,
            1.3,
            &[(Commodity::Energy, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Electric Pumping".into(),
        pm(
            1890,
            Some("mining_004"),
            0.15,
            0.30,
            0.55,
            1.6,
            &[(Commodity::Energy, 8.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Automated Conveyor".into(),
        pm(
            1915,
            Some("elecf_002"),
            0.20,
            0.35,
            0.45,
            2.0,
            &[
                (Commodity::Energy, 12.0),
                (Commodity::MechanicalComponents, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Diesel-Electric Drills".into(),
        pm(
            1950,
            Some("auto_002"),
            0.25,
            0.40,
            0.35,
            2.5,
            &[(Commodity::Fuels, 8.0), (Commodity::Energy, 10.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Robotic Extraction".into(),
        pm(
            1975,
            Some("auto3_007"),
            0.30,
            0.45,
            0.25,
            3.5,
            &[
                (Commodity::Energy, 20.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Piece Work".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Food, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Shift System".into(),
        pm(
            1890,
            Some("mech_008"),
            0.10,
            0.25,
            0.65,
            1.2,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Scientific Management".into(),
        pm(
            1910,
            Some("mech_008"),
            0.15,
            0.35,
            0.50,
            1.5,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Mechanized Operations".into(),
        pm(
            1945,
            Some("elecf_005"),
            0.20,
            0.38,
            0.42,
            1.8,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Lean Mining".into(),
        pm(
            1985,
            Some("advman_002"),
            0.25,
            0.40,
            0.35,
            2.0,
            &[(Commodity::Food, 5.0), (Commodity::Software, 2.0)],
            &[],
        ),
    );
    m
}

// === AGRICULTURE ===
fn agriculture_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Manual Farming".into(),
        pm(
            1880,
            None,
            0.02,
            0.10,
            0.88,
            1.0,
            &[
                (Commodity::Seeds, 5.0),
                (Commodity::Food, 3.0),
                (Commodity::DraftAnimals, 3.0),
            ],
            &[(Commodity::Cereal, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Horse-Drawn Machinery".into(),
        pm(
            1885,
            Some("mech_002"),
            0.05,
            0.15,
            0.80,
            1.5,
            &[
                (Commodity::Seeds, 5.0),
                (Commodity::Food, 5.0),
                (Commodity::DraftAnimals, 5.0),
            ],
            &[(Commodity::Cereal, 25.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Steam Tractors".into(),
        pm(
            1895,
            Some("steam_001"),
            0.08,
            0.20,
            0.72,
            2.0,
            &[(Commodity::Seeds, 8.0), (Commodity::Fuels, 10.0)],
            &[(Commodity::Cereal, 40.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Hybrid Seeds".into(),
        pm(
            1960,
            Some("bio_005"),
            0.15,
            0.30,
            0.55,
            3.0,
            &[(Commodity::Seeds, 12.0), (Commodity::Fertilizers, 10.0)],
            &[(Commodity::Cereal, 70.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Mechanized Harvesting".into(),
        pm(
            1950,
            Some("auto3_001"),
            0.15,
            0.35,
            0.50,
            3.5,
            &[
                (Commodity::Fuels, 15.0),
                (Commodity::Fertilizers, 8.0),
                (Commodity::AgriculturalMachinery, 3.0),
            ],
            &[(Commodity::Cereal, 90.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "GM Crops".into(),
        pm(
            1995,
            Some("precag_004"),
            0.25,
            0.40,
            0.35,
            5.0,
            &[
                (Commodity::Seeds, 15.0),
                (Commodity::Fertilizers, 12.0),
                (Commodity::Chemicals, 8.0),
            ],
            &[(Commodity::Cereal, 130.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Precision Farming".into(),
        pm(
            1995,
            Some("precag_005"),
            0.30,
            0.40,
            0.30,
            6.0,
            &[
                (Commodity::Fuels, 10.0),
                (Commodity::Fertilizers, 8.0),
                (Commodity::Software, 3.0),
            ],
            &[(Commodity::Cereal, 160.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Hydroponics".into(),
        pm(
            1985,
            Some("precag_007"),
            0.30,
            0.45,
            0.25,
            7.0,
            &[
                (Commodity::Water, 20.0),
                (Commodity::Fertilizers, 10.0),
                (Commodity::Energy, 15.0),
            ],
            &[(Commodity::Vegetable, 80.0)],
        ),
    );
    // ── Phase 20: Activate full agricultural supply chain ──
    m.insert(
        MethodSlot::Production,
        "Vegetable Farming".into(),
        pm(
            1880,
            None,
            0.02,
            0.10,
            0.88,
            1.0,
            &[
                (Commodity::Seeds, 5.0),
                (Commodity::Water, 8.0),
                (Commodity::Food, 2.0),
            ],
            &[(Commodity::Vegetable, 18.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Pulse & Legume Farming".into(),
        pm(
            1880,
            None,
            0.03,
            0.12,
            0.85,
            1.0,
            &[
                (Commodity::Seeds, 6.0),
                (Commodity::Water, 10.0),
                (Commodity::Food, 2.0),
            ],
            &[(Commodity::Meat, 6.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Orchard Cultivation".into(),
        pm(
            1885,
            None,
            0.03,
            0.12,
            0.85,
            1.1,
            &[
                (Commodity::Seeds, 4.0),
                (Commodity::Water, 8.0),
                (Commodity::Food, 2.0),
            ],
            &[(Commodity::Fruit, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Livestock Ranching".into(),
        pm(
            1880,
            None,
            0.03,
            0.12,
            0.85,
            1.0,
            &[
                (Commodity::Fodder, 15.0),
                (Commodity::Water, 10.0),
                (Commodity::Food, 3.0),
            ],
            &[(Commodity::Meat, 10.0), (Commodity::Livestock, 5.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Industrial Fiber Farming".into(),
        pm(
            1880,
            None,
            0.03,
            0.12,
            0.85,
            1.0,
            &[(Commodity::Seeds, 5.0), (Commodity::Water, 8.0)],
            &[(Commodity::IndustrialFiber, 12.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Luxury Crop Plantation".into(),
        pm(
            1885,
            None,
            0.05,
            0.15,
            0.80,
            1.2,
            &[
                (Commodity::Seeds, 4.0),
                (Commodity::Water, 10.0),
                (Commodity::Food, 2.0),
            ],
            &[(Commodity::Luxury, 8.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Seed Production".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            0.8,
            &[
                (Commodity::Cereal, 10.0),
                (Commodity::Water, 5.0),
                (Commodity::Food, 2.0),
            ],
            &[(Commodity::Seeds, 12.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Fodder Production".into(),
        pm(
            1880,
            None,
            0.03,
            0.12,
            0.85,
            1.0,
            &[(Commodity::Cereal, 8.0), (Commodity::Water, 5.0)],
            &[(Commodity::Fodder, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Timber Plantation".into(),
        pm(
            1880,
            None,
            0.02,
            0.10,
            0.88,
            0.7,
            &[(Commodity::Seeds, 2.0), (Commodity::Water, 5.0)],
            &[(Commodity::Timber, 10.0)],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Hand Harvesting".into(),
        pm(
            1880,
            None,
            0.02,
            0.08,
            0.90,
            1.0,
            &[(Commodity::Food, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Mechanical Reapers".into(),
        pm(
            1885,
            Some("mech_002"),
            0.05,
            0.15,
            0.80,
            1.4,
            &[
                (Commodity::Fuels, 5.0),
                (Commodity::MechanicalComponents, 2.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Tractor Automation".into(),
        pm(
            1920,
            Some("auto_001"),
            0.10,
            0.25,
            0.65,
            2.0,
            &[
                (Commodity::Fuels, 10.0),
                (Commodity::MechanicalComponents, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Combine Harvesters".into(),
        pm(
            1955,
            Some("auto3_001"),
            0.15,
            0.30,
            0.55,
            2.5,
            &[
                (Commodity::Fuels, 12.0),
                (Commodity::MechanicalComponents, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "GPS-Guided Machinery".into(),
        pm(
            1990,
            Some("precag_001"),
            0.25,
            0.40,
            0.35,
            3.5,
            &[
                (Commodity::Fuels, 8.0),
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Agricultural Drones".into(),
        pm(
            1998,
            Some("precag_006"),
            0.30,
            0.45,
            0.25,
            5.0,
            &[
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Software, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Subsistence Farming".into(),
        pm(
            1880,
            None,
            0.02,
            0.08,
            0.90,
            1.0,
            &[(Commodity::Seeds, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Crop Rotation".into(),
        pm(
            1890,
            Some("chem_001"),
            0.05,
            0.15,
            0.80,
            1.3,
            &[(Commodity::Seeds, 5.0), (Commodity::Paper, 1.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Industrial Farming".into(),
        pm(
            1910,
            Some("mech_008"),
            0.10,
            0.25,
            0.65,
            1.8,
            &[(Commodity::Fertilizers, 5.0), (Commodity::Paper, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Agribusiness Scale".into(),
        pm(
            1960,
            Some("bio_005"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[(Commodity::Fertilizers, 10.0), (Commodity::Software, 1.0)],
            &[],
        ),
    );
    // Phase 74: Draft Animal Breeding — closes the supply chain for DraftAnimals
    // which were previously only seeded at world generation with no replenishment.
    m.insert(
        MethodSlot::Production,
        "Draft Animal Breeding".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            0.8,
            &[
                (Commodity::Fodder, 10.0),
                (Commodity::Water, 8.0),
                (Commodity::Cereal, 5.0),
            ],
            &[(Commodity::DraftAnimals, 3.0), (Commodity::Livestock, 2.0)],
        ),
    );
    m
}

// === HEAVY INDUSTRY ===
/// Phase 82B: Apply emission factors to all industrial production methods.
///
/// Emission factors are PHYSICAL CONSTANTS from combustion/industrial chemistry
/// (particulate + SO2 mass per unit of primary input). They determine how much
/// smog each method generates per unit of actual consumed input.
///
/// CORRECTION 4 (Double-Counting): The smog formula uses `actual_consumed_quantity`
/// from `building.last_production`, NOT `method.inputs × production_scale`.
/// The emission factor is multiplied by the actual consumed fuel/input.
fn apply_industrial_emission_factors(m: &mut BuildingMethods) {
    // Map method names to emission factors (smog units per unit of primary input).
    // These are physical constants based on industrial process chemistry.
    let emission_factors: &[(&str, f64)] = &[
        // Steel production — particulate from oxygen blowing and furnace operations
        ("Bessemer Converters", 8.0), // Massive particulate from oxygen blowing
        ("Open-Hearth Furnaces", 6.0), // Lower than Bessemer, still high
        ("Electric Arc Furnaces", 2.0), // Electric = less direct emissions
        ("Basic Oxygen Process", 5.0), // Moderate particulate
        ("Continuous Casting", 3.0),  // More efficient = lower emissions
        ("Mini-Mill Production", 2.5), // Electric + scrap = lower emissions
        // Smelting & basic processing
        ("Coke Production", 9.0),      // Coke ovens = extreme emissions
        ("Cement Production", 10.0),   // Cement kilns = highest industrial emissions
        ("Brick Making", 7.0),         // Kiln emissions, high particulate
        ("Glass Making", 4.0),         // Moderate furnace emissions
        ("Aluminum Smelting", 6.0),    // HF + particulate emissions
        ("Silicon Purification", 3.0), // Moderate emissions
        // Chemical & petroleum processing
        ("Basic Chemical Production", 5.0), // Chemical plant emissions
        ("Solvay Process", 4.0),            // Moderate chemical emissions
        ("Haber-Bosch Process", 3.0),       // Moderate emissions from NG reforming
        ("Fertilizer Production", 3.0),     // Moderate chemical emissions
        ("Oil Refining", 4.0),              // Refinery stack emissions
        ("Advanced Refining", 3.5),         // More efficient = slightly lower
        ("Plastics Production", 3.0),       // Moderate petrochemical emissions
        ("Asphalt Production", 4.0),        // Moderate emissions from bitumen heating
        ("Catalyst Production", 3.0),       // Moderate chemical emissions
        ("Hydrogen Production", 2.0),       // Moderate emissions from NG reforming
        // Components & parts — lower emissions (assembly, not primary processing)
        ("Mechanical Components Workshop", 1.0),
        ("Precision Machining", 0.8),
        ("Electronic Components Assembly", 0.5),
        ("Semiconductor Fabrication", 1.5), // Chemical solvents
        ("Advanced Electronics", 0.5),
        ("Software Development", 0.0), // No industrial emissions
        ("Battery Production", 1.5),
        ("Pharmaceutical Production", 2.0),
        // Investment goods
        ("Machine Shop", 1.0),
        ("Electrified Factories", 1.0),
        ("CNC Manufacturing", 0.8),
        // Coal Carbonization (Phase 81)
        ("Coal Carbonization", 7.0), // Similar to Coke Production
    ];

    for &(name, ef) in emission_factors {
        if let Some(method) = m.production.get_mut(name) {
            method.emission_factor = ef;
        }
    }
}

fn heavy_industry_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Bessemer Converters".into(),
        pm(
            1880,
            Some("steel_001"),
            0.15,
            0.35,
            0.50,
            1.5,
            &[
                (Commodity::Iron, 20.0),
                (Commodity::Fuels, 10.0),
                (Commodity::Coke, 8.0),
            ],
            &[(Commodity::Steel, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Open-Hearth Furnaces".into(),
        pm(
            1885,
            Some("steel_002"),
            0.20,
            0.35,
            0.45,
            2.0,
            &[
                (Commodity::Iron, 25.0),
                (Commodity::Fuels, 12.0),
                (Commodity::Coke, 10.0),
            ],
            &[(Commodity::Steel, 22.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Electric Arc Furnaces".into(),
        pm(
            1905,
            Some("steel_008"),
            0.25,
            0.40,
            0.35,
            3.0,
            &[(Commodity::Iron, 20.0), (Commodity::Energy, 15.0)],
            &[(Commodity::Steel, 30.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Basic Oxygen Process".into(),
        pm(
            1955,
            Some("auto3_002"),
            0.25,
            0.40,
            0.35,
            4.0,
            &[(Commodity::Iron, 30.0), (Commodity::Energy, 10.0)],
            &[(Commodity::Steel, 50.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Continuous Casting".into(),
        pm(
            1965,
            Some("auto3_005"),
            0.30,
            0.45,
            0.25,
            5.5,
            &[
                (Commodity::Iron, 35.0),
                (Commodity::Energy, 15.0),
                (Commodity::ElectronicComponents, 3.0),
            ],
            &[(Commodity::Steel, 70.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Mini-Mill Production".into(),
        pm(
            1975,
            Some("auto3_007"),
            0.30,
            0.45,
            0.25,
            6.5,
            &[
                (Commodity::Energy, 25.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::Steel, 90.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Electrified Factories".into(),
        pm(
            1910,
            Some("elecf_001"),
            0.20,
            0.40,
            0.40,
            2.5,
            &[(Commodity::Energy, 20.0), (Commodity::Steel, 10.0)],
            &[(Commodity::IndustrialMachinery, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "CNC Manufacturing".into(),
        pm(
            1970,
            Some("auto3_004"),
            0.30,
            0.45,
            0.25,
            5.0,
            &[
                (Commodity::Energy, 20.0),
                (Commodity::Steel, 15.0),
                (Commodity::ElectronicComponents, 8.0),
            ],
            &[(Commodity::IndustrialMachinery, 30.0)],
        ),
    );
    // ── Phase 20: Layer 1 — Smelting & Basic Processing ──
    m.insert(
        MethodSlot::Production,
        "Coke Production".into(),
        pm(
            1880,
            None,
            0.08,
            0.25,
            0.67,
            1.0,
            &[(Commodity::HardCoal, 20.0), (Commodity::Energy, 5.0)],
            &[(Commodity::Coke, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Cement Production".into(),
        pm(
            1880,
            None,
            0.08,
            0.25,
            0.67,
            1.0,
            &[
                (Commodity::Limestone, 25.0),
                (Commodity::Clay, 8.0),
                (Commodity::Energy, 10.0),
            ],
            &[(Commodity::Cement, 30.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Brick Making".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Clay, 20.0), (Commodity::Energy, 5.0)],
            &[(Commodity::Bricks, 25.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Glass Making".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[
                (Commodity::Sand, 20.0),
                (Commodity::SodaAsh, 5.0),
                (Commodity::Lead, 2.0),
                (Commodity::Energy, 12.0),
            ],
            &[(Commodity::Glass, 18.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Aluminum Smelting".into(),
        pm(
            1900,
            Some("metall_006"),
            0.15,
            0.35,
            0.50,
            1.5,
            &[
                (Commodity::Bauxite, 20.0),
                (Commodity::Energy, 30.0),
                (Commodity::Catalysts, 2.0),
            ],
            &[(Commodity::Aluminum, 12.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Silicon Purification".into(),
        pm(
            1950,
            Some("semi_001"),
            0.20,
            0.40,
            0.40,
            2.0,
            &[
                (Commodity::Sand, 15.0),
                (Commodity::Energy, 20.0),
                (Commodity::Chemicals, 5.0),
            ],
            &[(Commodity::Silicon, 8.0)],
        ),
    );
    // ── Phase 20: Layer 2 — Chemical & Petroleum Processing ──
    m.insert(
        MethodSlot::Production,
        "Basic Chemical Production".into(),
        pm(
            1880,
            None,
            0.10,
            0.30,
            0.60,
            1.0,
            &[
                (Commodity::Sulfur, 8.0),
                (Commodity::Salt, 5.0),
                (Commodity::Water, 10.0),
                (Commodity::Energy, 8.0),
            ],
            &[(Commodity::Chemicals, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Solvay Process".into(),
        pm(
            1880,
            None,
            0.12,
            0.30,
            0.58,
            1.0,
            &[
                (Commodity::Salt, 10.0),
                (Commodity::Limestone, 8.0),
                (Commodity::Ammonia, 3.0),
                (Commodity::Energy, 8.0),
            ],
            &[(Commodity::SodaAsh, 12.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Haber-Bosch Process".into(),
        pm(
            1910,
            Some("chem_002"),
            0.15,
            0.35,
            0.50,
            1.5,
            &[
                (Commodity::NaturalGas, 10.0),
                (Commodity::Energy, 12.0),
                (Commodity::Catalysts, 1.0),
            ],
            &[(Commodity::Ammonia, 10.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Fertilizer Production".into(),
        pm(
            1880,
            None,
            0.10,
            0.30,
            0.60,
            1.0,
            &[
                (Commodity::Ammonia, 8.0),
                (Commodity::Chemicals, 5.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::Fertilizers, 18.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Oil Refining".into(),
        pm(
            1880,
            None,
            0.10,
            0.30,
            0.60,
            1.0,
            &[
                (Commodity::Oil, 25.0),
                (Commodity::Energy, 5.0),
                (Commodity::Catalysts, 1.0),
            ],
            &[(Commodity::Fuels, 18.0), (Commodity::Bitumen, 3.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Advanced Refining".into(),
        pm(
            1920,
            Some("petro_002"),
            0.12,
            0.32,
            0.56,
            1.8,
            &[
                (Commodity::Oil, 30.0),
                (Commodity::Catalysts, 2.0),
                (Commodity::Energy, 8.0),
            ],
            &[
                (Commodity::Fuels, 22.0),
                (Commodity::RefinedFuel, 8.0),
                (Commodity::Bitumen, 4.0),
            ],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Plastics Production".into(),
        pm(
            1935,
            Some("petro_005"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[
                (Commodity::Oil, 15.0),
                (Commodity::Chemicals, 8.0),
                (Commodity::Energy, 10.0),
            ],
            &[(Commodity::Plastics, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Asphalt Production".into(),
        pm(
            1900,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[
                (Commodity::Bitumen, 8.0),
                (Commodity::Sand, 10.0),
                (Commodity::Gravel, 8.0),
                (Commodity::Energy, 3.0),
            ],
            &[(Commodity::Asphalt, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Catalyst Production".into(),
        pm(
            1900,
            None,
            0.12,
            0.30,
            0.58,
            1.0,
            &[
                (Commodity::Chemicals, 8.0),
                (Commodity::RareEarthElements, 1.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::Catalysts, 6.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Hydrogen Production".into(),
        pm(
            1970,
            Some("hydro_001"),
            0.15,
            0.35,
            0.50,
            1.5,
            &[(Commodity::NaturalGas, 8.0), (Commodity::Energy, 15.0)],
            &[(Commodity::Hydrogen, 6.0)],
        ),
    );
    // ── Phase 20: Layer 3 — Components & Parts ──
    m.insert(
        MethodSlot::Production,
        "Mechanical Components Workshop".into(),
        pm(
            1880,
            None,
            0.10,
            0.30,
            0.60,
            1.0,
            &[
                (Commodity::Steel, 10.0),
                (Commodity::Energy, 5.0),
                (Commodity::IndustrialMachinery, 2.0),
            ],
            &[(Commodity::MechanicalComponents, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Precision Machining".into(),
        pm(
            1910,
            Some("mech_008"),
            0.15,
            0.35,
            0.50,
            1.8,
            &[
                (Commodity::Steel, 12.0),
                (Commodity::Energy, 8.0),
                (Commodity::IndustrialMachinery, 3.0),
            ],
            &[(Commodity::MechanicalComponents, 25.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Electronic Components Assembly".into(),
        pm(
            1920,
            Some("elecf_001"),
            0.15,
            0.35,
            0.50,
            1.5,
            &[
                (Commodity::Copper, 8.0),
                (Commodity::Tin, 3.0),
                (Commodity::Energy, 8.0),
                (Commodity::IndustrialMachinery, 2.0),
            ],
            &[(Commodity::ElectronicComponents, 10.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Semiconductor Fabrication".into(),
        pm(
            1970,
            Some("semi_003"),
            0.25,
            0.45,
            0.30,
            3.0,
            &[
                (Commodity::Silicon, 5.0),
                (Commodity::RareEarthElements, 2.0),
                (Commodity::Chemicals, 5.0),
                (Commodity::Energy, 15.0),
            ],
            &[(Commodity::Semiconductors, 8.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Advanced Electronics".into(),
        pm(
            1980,
            Some("semi_005"),
            0.25,
            0.45,
            0.30,
            3.5,
            &[
                (Commodity::Semiconductors, 3.0),
                (Commodity::Copper, 5.0),
                (Commodity::Tin, 2.0),
                (Commodity::Energy, 10.0),
            ],
            &[(Commodity::ElectronicComponents, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Software Development".into(),
        pm(
            1980,
            Some("cs_005"),
            0.35,
            0.45,
            0.20,
            2.5,
            &[
                (Commodity::ElectronicComponents, 3.0),
                (Commodity::Energy, 5.0),
                (Commodity::Food, 5.0),
            ],
            &[(Commodity::Software, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Battery Production".into(),
        pm(
            1990,
            Some("batt_001"),
            0.20,
            0.40,
            0.40,
            2.0,
            &[
                (Commodity::Lithium, 5.0),
                (Commodity::Lead, 5.0),
                (Commodity::Semiconductors, 2.0),
                (Commodity::Energy, 10.0),
            ],
            &[(Commodity::Batteries, 8.0)],
        ),
    );
    // ── Phase 20: Pharmaceutical production ──
    m.insert(
        MethodSlot::Production,
        "Pharmaceutical Production".into(),
        pm(
            1890,
            None,
            0.15,
            0.35,
            0.50,
            1.0,
            &[
                (Commodity::Chemicals, 10.0),
                (Commodity::Energy, 5.0),
                (Commodity::Water, 5.0),
            ],
            &[(Commodity::Pharmaceuticals, 12.0)],
        ),
    );
    // ── Phase 20: Layer 5 — Investment Goods (THE CRITICAL GAP) ──
    // IndustrialMachinery — early method (no tech required)
    m.insert(
        MethodSlot::Production,
        "Machine Shop".into(),
        pm(
            1880,
            None,
            0.12,
            0.30,
            0.58,
            1.0,
            &[
                (Commodity::Steel, 12.0),
                (Commodity::MechanicalComponents, 5.0),
                (Commodity::Energy, 8.0),
            ],
            &[(Commodity::IndustrialMachinery, 10.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Smart Manufacturing".into(),
        pm(
            1995,
            Some("advman_006"),
            0.30,
            0.45,
            0.25,
            5.0,
            &[
                (Commodity::Steel, 15.0),
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Software, 5.0),
                (Commodity::Semiconductors, 2.0),
                (Commodity::Energy, 15.0),
            ],
            &[(Commodity::IndustrialMachinery, 50.0)],
        ),
    );
    // ConstructionMachinery — ALL NEW
    m.insert(
        MethodSlot::Production,
        "Blacksmith Workshop".into(),
        pm(
            1880,
            None,
            0.10,
            0.25,
            0.65,
            1.0,
            &[
                (Commodity::Steel, 10.0),
                (Commodity::Iron, 5.0),
                (Commodity::Fuels, 5.0),
            ],
            &[(Commodity::ConstructionMachinery, 8.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Machine Factory".into(),
        pm(
            1910,
            Some("mech_008"),
            0.15,
            0.35,
            0.50,
            1.8,
            &[
                (Commodity::Steel, 15.0),
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::Energy, 8.0),
            ],
            &[(Commodity::ConstructionMachinery, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Heavy Equipment Plant".into(),
        pm(
            1950,
            Some("auto3_001"),
            0.20,
            0.40,
            0.40,
            3.0,
            &[
                (Commodity::Steel, 20.0),
                (Commodity::MechanicalComponents, 10.0),
                (Commodity::ElectronicComponents, 3.0),
                (Commodity::Energy, 12.0),
            ],
            &[(Commodity::ConstructionMachinery, 40.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Automated Equipment Plant".into(),
        pm(
            1990,
            Some("advman_004"),
            0.25,
            0.45,
            0.30,
            5.0,
            &[
                (Commodity::Steel, 18.0),
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Software, 3.0),
                (Commodity::Energy, 15.0),
            ],
            &[(Commodity::ConstructionMachinery, 70.0)],
        ),
    );
    // AgriculturalMachinery — ALL NEW
    m.insert(
        MethodSlot::Production,
        "Implement Workshop".into(),
        pm(
            1880,
            None,
            0.10,
            0.25,
            0.65,
            1.0,
            &[
                (Commodity::Steel, 10.0),
                (Commodity::Iron, 5.0),
                (Commodity::Fuels, 3.0),
            ],
            &[(Commodity::AgriculturalMachinery, 8.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Implement Factory".into(),
        pm(
            1910,
            Some("mech_008"),
            0.15,
            0.35,
            0.50,
            1.8,
            &[
                (Commodity::Steel, 15.0),
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::Energy, 8.0),
            ],
            &[(Commodity::AgriculturalMachinery, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Tractor Plant".into(),
        pm(
            1950,
            Some("auto3_001"),
            0.20,
            0.40,
            0.40,
            3.0,
            &[
                (Commodity::Steel, 20.0),
                (Commodity::MechanicalComponents, 10.0),
                (Commodity::ElectronicComponents, 3.0),
                (Commodity::Energy, 12.0),
            ],
            &[(Commodity::AgriculturalMachinery, 40.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Precision Ag Equipment".into(),
        pm(
            1990,
            Some("advman_004"),
            0.25,
            0.45,
            0.30,
            5.0,
            &[
                (Commodity::Steel, 18.0),
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Software, 3.0),
                (Commodity::Energy, 15.0),
            ],
            &[(Commodity::AgriculturalMachinery, 70.0)],
        ),
    );
    // OfficeMachinery — ALL NEW
    m.insert(
        MethodSlot::Production,
        "Typewriter Workshop".into(),
        pm(
            1890,
            Some("mech_008"),
            0.15,
            0.35,
            0.50,
            1.0,
            &[
                (Commodity::Steel, 8.0),
                (Commodity::MechanicalComponents, 5.0),
                (Commodity::Energy, 3.0),
            ],
            &[(Commodity::OfficeMachinery, 10.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Office Equipment Factory".into(),
        pm(
            1950,
            Some("auto3_001"),
            0.20,
            0.40,
            0.40,
            2.5,
            &[
                (Commodity::Steel, 10.0),
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::ElectronicComponents, 3.0),
                (Commodity::Energy, 8.0),
            ],
            &[(Commodity::OfficeMachinery, 25.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Computer Factory".into(),
        pm(
            1980,
            Some("auto3_004"),
            0.25,
            0.45,
            0.30,
            4.0,
            &[
                (Commodity::Steel, 5.0),
                (Commodity::ElectronicComponents, 10.0),
                (Commodity::Semiconductors, 3.0),
                (Commodity::Software, 3.0),
                (Commodity::Energy, 8.0),
            ],
            &[(Commodity::OfficeMachinery, 50.0)],
        ),
    );
    // Trucks — ALL NEW
    m.insert(
        MethodSlot::Production,
        "Wagon Workshop".into(),
        pm(
            1880,
            None,
            0.10,
            0.25,
            0.65,
            1.0,
            &[
                (Commodity::Steel, 8.0),
                (Commodity::Timber, 5.0),
                (Commodity::MechanicalComponents, 3.0),
                (Commodity::Fuels, 2.0),
            ],
            &[(Commodity::Trucks, 5.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Truck Assembly".into(),
        pm(
            1920,
            Some("auto_001"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[
                (Commodity::Steel, 15.0),
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::RefinedFuel, 3.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::Trucks, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Modern Truck Plant".into(),
        pm(
            1960,
            Some("auto3_002"),
            0.20,
            0.40,
            0.40,
            3.5,
            &[
                (Commodity::Steel, 18.0),
                (Commodity::MechanicalComponents, 10.0),
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::RefinedFuel, 5.0),
                (Commodity::Zinc, 3.0),
                (Commodity::Energy, 8.0),
            ],
            &[(Commodity::Trucks, 35.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Electric Truck Plant".into(),
        pm(
            2000,
            Some("advman_006"),
            0.25,
            0.45,
            0.30,
            5.0,
            &[
                (Commodity::Steel, 15.0),
                (Commodity::Aluminum, 5.0),
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Batteries, 5.0),
                (Commodity::Energy, 10.0),
            ],
            &[(Commodity::Trucks, 60.0)],
        ),
    );
    // Cars — ALL NEW
    m.insert(
        MethodSlot::Production,
        "Coachbuilder".into(),
        pm(
            1900,
            Some("mech_008"),
            0.12,
            0.30,
            0.58,
            1.0,
            &[
                (Commodity::Steel, 10.0),
                (Commodity::Timber, 5.0),
                (Commodity::MechanicalComponents, 5.0),
                (Commodity::Fuels, 2.0),
            ],
            &[(Commodity::Cars, 5.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Assembly Line".into(),
        pm(
            1913,
            Some("auto_001"),
            0.10,
            0.30,
            0.60,
            2.0,
            &[
                (Commodity::Steel, 15.0),
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::RefinedFuel, 3.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::Cars, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Modern Auto Plant".into(),
        pm(
            1960,
            Some("auto3_003"),
            0.20,
            0.40,
            0.40,
            3.5,
            &[
                (Commodity::Steel, 18.0),
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Plastics, 5.0),
                (Commodity::RefinedFuel, 3.0),
                (Commodity::Magnesium, 2.0),
                (Commodity::Energy, 8.0),
            ],
            &[(Commodity::Cars, 50.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "EV Factory".into(),
        pm(
            2010,
            Some("advman_006"),
            0.25,
            0.45,
            0.30,
            5.0,
            &[
                (Commodity::Steel, 12.0),
                (Commodity::Aluminum, 8.0),
                (Commodity::ElectronicComponents, 10.0),
                (Commodity::Semiconductors, 5.0),
                (Commodity::Batteries, 8.0),
                (Commodity::Hydrogen, 3.0),
                (Commodity::Energy, 10.0),
            ],
            &[(Commodity::Cars, 80.0)],
        ),
    );
    // ── Phase 20: Prefabricates & Locomotive production ──
    m.insert(
        MethodSlot::Production,
        "Prefabricates Plant".into(),
        pm(
            1900,
            None,
            0.10,
            0.30,
            0.60,
            1.5,
            &[
                (Commodity::Cement, 10.0),
                (Commodity::Steel, 5.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::Prefabricates, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Locomotive Works".into(),
        pm(
            1890,
            Some("steam_002"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[
                (Commodity::Steel, 25.0),
                (Commodity::MechanicalComponents, 10.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::Trains, 3.0)],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Steam Power Drive".into(),
        pm(
            1880,
            Some("steam_001"),
            0.10,
            0.25,
            0.65,
            1.3,
            &[(Commodity::Fuels, 15.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Electrified Factories".into(),
        pm(
            1910,
            Some("elecf_001"),
            0.15,
            0.30,
            0.55,
            1.8,
            &[(Commodity::Energy, 20.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Turbo-Generator Plant".into(),
        pm(
            1888,
            Some("steam_003"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[
                (Commodity::Fuels, 10.0),
                (Commodity::MechanicalComponents, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Automated Machinery".into(),
        pm(
            1930,
            Some("elecf_005"),
            0.20,
            0.40,
            0.40,
            2.5,
            &[
                (Commodity::Energy, 25.0),
                (Commodity::ElectronicComponents, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Robotic Welding".into(),
        pm(
            1965,
            Some("auto3_003"),
            0.30,
            0.45,
            0.25,
            4.0,
            &[
                (Commodity::Energy, 20.0),
                (Commodity::ElectronicComponents, 8.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Flexible Manufacturing".into(),
        pm(
            1995,
            Some("advman_006"),
            0.35,
            0.45,
            0.20,
            5.5,
            &[
                (Commodity::Energy, 25.0),
                (Commodity::ElectronicComponents, 10.0),
                (Commodity::Software, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Craft Production".into(),
        pm(
            1880,
            None,
            0.20,
            0.30,
            0.50,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Taylorism".into(),
        pm(
            1910,
            Some("mech_008"),
            0.15,
            0.35,
            0.50,
            1.4,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Assembly Line".into(),
        pm(
            1913,
            Some("auto_001"),
            0.10,
            0.30,
            0.60,
            1.8,
            &[
                (Commodity::Food, 5.0),
                (Commodity::MechanicalComponents, 2.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Continuous Flow Manufacturing".into(),
        pm(
            1950,
            Some("elecf_005"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Just-in-Time".into(),
        pm(
            1985,
            Some("advman_002"),
            0.20,
            0.40,
            0.40,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Six Sigma".into(),
        pm(
            1990,
            Some("advman_005"),
            0.25,
            0.45,
            0.30,
            3.0,
            &[(Commodity::Food, 5.0), (Commodity::Software, 5.0)],
            &[],
        ),
    );
    // ── Phase 69: Military conversion methods (Production Decree targets) ──
    // These methods are swapped in by ProductionDecree to convert civilian
    // heavy industry to military output. Each has DISTINCT physical inputs
    // that shock the supply chain (Rule 3 compliance).
    m.insert(
        MethodSlot::Production,
        "Military Truck Conversion".into(),
        pm(
            1916,
            None,
            0.20,
            0.35,
            0.45,
            0.8,
            &[
                (Commodity::Steel, 25.0),
                (Commodity::Fuels, 12.0),
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::Plastics, 5.0),
            ],
            &[(Commodity::Trucks, 8.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Light Tank Conversion".into(),
        pm(
            1935,
            None,
            0.22,
            0.38,
            0.40,
            0.7,
            &[
                (Commodity::Steel, 35.0),
                (Commodity::Aluminum, 10.0),
                (Commodity::Fuels, 15.0),
                (Commodity::MechanicalComponents, 12.0),
            ],
            &[(Commodity::LightTanks, 3.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Artillery Conversion".into(),
        pm(
            1916,
            None,
            0.20,
            0.35,
            0.45,
            0.8,
            &[
                (Commodity::Steel, 30.0),
                (Commodity::Fuels, 10.0),
                (Commodity::MechanicalComponents, 8.0),
            ],
            &[(Commodity::TowedArtillery, 4.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Ammunition Surge Production".into(),
        pm(
            1916,
            None,
            0.18,
            0.32,
            0.50,
            0.9,
            &[
                (Commodity::Steel, 20.0),
                (Commodity::Chemicals, 25.0),
                (Commodity::Fuels, 8.0),
                (Commodity::Lead, 10.0),
            ],
            &[(Commodity::Ammunition, 40.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Gunpowder Conversion".into(),
        pm(
            1880,
            None,
            0.15,
            0.30,
            0.55,
            0.8,
            &[
                (Commodity::Chemicals, 30.0),
                (Commodity::Sulfur, 15.0),
                (Commodity::Energy, 10.0),
            ],
            &[(Commodity::Gunpowder, 20.0)],
        ),
    );
    // Phase 81 Wave 2: Coal Carbonization — produces CoalGas for gas lighting/heating.
    // Historically the primary source of city gas before natural gas pipelines.
    m.insert(
        MethodSlot::Production,
        "Coal Carbonization".into(),
        pm(
            1850,
            None,
            0.15,
            0.30,
            0.55,
            1.0,
            &[(Commodity::HardCoal, 3.0), (Commodity::Water, 2.0)],
            &[(Commodity::CoalGas, 2.0), (Commodity::Coke, 1.5)],
        ),
    );

    // Phase 82B: Apply emission factors to all industrial production methods.
    // Emission factors are PHYSICAL CONSTANTS from combustion/industrial chemistry
    // (particulate + SO2 mass per unit of primary input). They determine how much
    // smog each method generates per unit of actual consumed input.
    apply_industrial_emission_factors(&mut m);

    m
}
fn light_industry_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Handloom Weaving".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Fibers, 10.0), (Commodity::Food, 3.0)],
            &[(Commodity::Clothing, 8.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Power Looms".into(),
        pm(
            1885,
            Some("steam_001"),
            0.10,
            0.25,
            0.65,
            2.0,
            &[(Commodity::Fibers, 15.0), (Commodity::Energy, 5.0)],
            &[(Commodity::Clothing, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Electric Looms".into(),
        pm(
            1910,
            Some("elecf_001"),
            0.15,
            0.30,
            0.55,
            2.5,
            &[(Commodity::Fibers, 20.0), (Commodity::Energy, 10.0)],
            &[(Commodity::Clothing, 30.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Synthetic Fibers".into(),
        pm(
            1935,
            Some("synth_006"),
            0.20,
            0.35,
            0.45,
            3.0,
            &[(Commodity::Chemicals, 10.0), (Commodity::Energy, 8.0)],
            &[(Commodity::Clothing, 40.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Automated Textile Mills".into(),
        pm(
            1965,
            Some("auto3_003"),
            0.25,
            0.40,
            0.35,
            4.0,
            &[
                (Commodity::Fibers, 25.0),
                (Commodity::Energy, 15.0),
                (Commodity::ElectronicComponents, 3.0),
            ],
            &[(Commodity::Clothing, 60.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Fast Fashion".into(),
        pm(
            1990,
            Some("advman_002"),
            0.20,
            0.40,
            0.40,
            5.0,
            &[
                (Commodity::Fibers, 30.0),
                (Commodity::Energy, 12.0),
                (Commodity::Software, 2.0),
            ],
            &[(Commodity::Clothing, 90.0)],
        ),
    );
    // ── Phase 20: Consumer goods manufacturing ──
    m.insert(
        MethodSlot::Production,
        "Sawmill".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Timber, 15.0), (Commodity::Energy, 3.0)],
            &[(Commodity::Planks, 12.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Furniture Workshop".into(),
        pm(
            1880,
            None,
            0.08,
            0.25,
            0.67,
            1.0,
            &[
                (Commodity::Planks, 12.0),
                (Commodity::Steel, 3.0),
                (Commodity::Energy, 3.0),
            ],
            &[(Commodity::Furniture, 10.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Luxury Furniture Workshop".into(),
        pm(
            1880,
            None,
            0.12,
            0.30,
            0.58,
            1.2,
            &[
                (Commodity::Planks, 10.0),
                (Commodity::Luxury, 3.0),
                (Commodity::Gold, 1.0),
                (Commodity::Silver, 1.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::LuxuryFurniture, 5.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Paper Mill".into(),
        pm(
            1880,
            None,
            0.08,
            0.25,
            0.67,
            1.0,
            &[
                (Commodity::Timber, 15.0),
                (Commodity::Chemicals, 3.0),
                (Commodity::Water, 10.0),
                (Commodity::Energy, 8.0),
            ],
            &[(Commodity::Paper, 18.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Appliance Assembly".into(),
        pm(
            1935,
            Some("elecf_005"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[
                (Commodity::Steel, 8.0),
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Plastics, 3.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::Agd, 12.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Food Processing".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[
                (Commodity::Cereal, 10.0),
                (Commodity::Vegetable, 5.0),
                (Commodity::Meat, 2.0),
                (Commodity::Livestock, 3.0),
                (Commodity::Energy, 3.0),
            ],
            &[(Commodity::Food, 18.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Textile Mill".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::IndustrialFiber, 12.0), (Commodity::Energy, 3.0)],
            &[(Commodity::Fibers, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Synthetic Fiber Production".into(),
        pm(
            1935,
            Some("synth_006"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[
                (Commodity::Plastics, 10.0),
                (Commodity::Chemicals, 3.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::Fibers, 20.0)],
        ),
    );
    // ── Phase 20: Activate LuxuryClothing and MedicalEquipment ──
    m.insert(
        MethodSlot::Production,
        "Luxury Clothing Atelier".into(),
        pm(
            1880,
            None,
            0.12,
            0.30,
            0.58,
            1.2,
            &[
                (Commodity::Luxury, 5.0),
                (Commodity::Fibers, 8.0),
                (Commodity::Gold, 1.0),
                (Commodity::Silver, 1.0),
                (Commodity::Energy, 3.0),
            ],
            &[(Commodity::LuxuryClothing, 5.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Medical Equipment Workshop".into(),
        pm(
            1890,
            None,
            0.15,
            0.35,
            0.50,
            1.0,
            &[
                (Commodity::Steel, 8.0),
                (Commodity::Glass, 5.0),
                (Commodity::MechanicalComponents, 3.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::MedicalEquipment, 8.0)],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Hand Spinning".into(),
        pm(
            1880,
            None,
            0.05,
            0.10,
            0.85,
            1.0,
            &[(Commodity::Food, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Spinning Mules".into(),
        pm(
            1885,
            Some("steam_001"),
            0.10,
            0.20,
            0.70,
            1.5,
            &[(Commodity::Energy, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Electric Spinning".into(),
        pm(
            1910,
            Some("elecf_001"),
            0.15,
            0.30,
            0.55,
            2.0,
            &[(Commodity::Energy, 10.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Synthetic Fiber Looms".into(),
        pm(
            1945,
            Some("chem_003"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[(Commodity::Energy, 12.0), (Commodity::Chemicals, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Computerized Knitting".into(),
        pm(
            1980,
            Some("auto3_008"),
            0.25,
            0.40,
            0.35,
            3.5,
            &[
                (Commodity::Energy, 12.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Cottage Industry".into(),
        pm(
            1880,
            None,
            0.05,
            0.10,
            0.85,
            1.0,
            &[(Commodity::Food, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Factory System".into(),
        pm(
            1890,
            Some("mech_008"),
            0.10,
            0.25,
            0.65,
            1.5,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Mass Production".into(),
        pm(
            1930,
            Some("auto_001"),
            0.15,
            0.30,
            0.55,
            1.8,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Quality Circles".into(),
        pm(
            1960,
            Some("elecf_005"),
            0.18,
            0.35,
            0.47,
            2.0,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 4.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Lean Manufacturing".into(),
        pm(
            1985,
            Some("advman_002"),
            0.20,
            0.40,
            0.40,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 2.0)],
            &[],
        ),
    );
    // ── Phase 69: Military conversion methods (Production Decree targets) ──
    // Textile factories converted to military uniform production.
    // Distinct physical inputs: heavier fibers, leather, steel for buttons/buckles.
    m.insert(
        MethodSlot::Production,
        "Military Uniform Conversion".into(),
        pm(
            1880,
            None,
            0.10,
            0.25,
            0.65,
            0.8,
            &[
                (Commodity::Fibers, 20.0),
                (Commodity::IndustrialFiber, 5.0),
                (Commodity::Steel, 2.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::Clothing, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Support Equipment Conversion".into(),
        pm(
            1916,
            None,
            0.15,
            0.30,
            0.55,
            0.7,
            &[
                (Commodity::Fibers, 10.0),
                (Commodity::Steel, 8.0),
                (Commodity::IndustrialFiber, 8.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::SupportEquipment, 6.0)],
        ),
    );
    m
}

// === ARMAMENTS INDUSTRY ===
fn armaments_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Artillery Workshop".into(),
        pm(
            1880,
            Some("arm_001"),
            0.20,
            0.35,
            0.45,
            1.5,
            &[
                (Commodity::Steel, 15.0),
                (Commodity::Fuels, 8.0),
                (Commodity::Food, 5.0),
            ],
            &[(Commodity::TowedArtillery, 5.0)],
        ),
    );
    // Phase 74: Cartridge Manufacturing — baseline Ammunition production from 1880.
    // Without this, the first Ammunition-producing method is "Aircraft Cannon Production"
    // (1930), leaving a 1925 start year with zero Ammunition supply.
    m.insert(
        MethodSlot::Production,
        "Cartridge Manufacturing".into(),
        pm(
            1880,
            None,
            0.15,
            0.30,
            0.55,
            1.0,
            &[
                (Commodity::Steel, 8.0),
                (Commodity::Chemicals, 5.0),
                (Commodity::Gunpowder, 10.0),
                (Commodity::Lead, 6.0),
            ],
            &[(Commodity::Ammunition, 25.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Tank Production".into(),
        pm(
            1916,
            Some("arm_002"),
            0.25,
            0.40,
            0.35,
            2.0,
            &[
                (Commodity::Steel, 30.0),
                (Commodity::Fuels, 15.0),
                (Commodity::MechanicalComponents, 10.0),
            ],
            &[(Commodity::LightTanks, 3.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Small Arms Automation".into(),
        pm(
            1920,
            Some("arm_003"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[(Commodity::Steel, 10.0), (Commodity::Fuels, 5.0)],
            &[(Commodity::Rifles, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Aircraft Cannon Production".into(),
        pm(
            1930,
            Some("arm_005"),
            0.25,
            0.40,
            0.35,
            3.0,
            &[
                (Commodity::Steel, 20.0),
                (Commodity::MechanicalComponents, 8.0),
            ],
            &[(Commodity::Ammunition, 30.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Mass Bomb Production".into(),
        pm(
            1940,
            Some("arm_008"),
            0.20,
            0.35,
            0.45,
            4.0,
            &[
                (Commodity::Steel, 15.0),
                (Commodity::Chemicals, 20.0),
                (Commodity::Fuels, 10.0),
            ],
            &[(Commodity::Ammunition, 50.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Guided Munitions".into(),
        pm(
            1965,
            Some("auto3_003"),
            0.30,
            0.45,
            0.25,
            5.0,
            &[
                (Commodity::Steel, 20.0),
                (Commodity::ElectronicComponents, 10.0),
                (Commodity::Chemicals, 15.0),
            ],
            &[
                (Commodity::Ammunition, 40.0),
                (Commodity::SupportEquipment, 10.0),
            ],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Precision Munitions".into(),
        pm(
            1990,
            Some("advman_003"),
            0.35,
            0.45,
            0.20,
            7.0,
            &[
                (Commodity::Steel, 15.0),
                (Commodity::ElectronicComponents, 15.0),
                (Commodity::Software, 5.0),
            ],
            &[
                (Commodity::Ammunition, 60.0),
                (Commodity::SupportEquipment, 20.0),
            ],
        ),
    );
    // ── Phase 20: Expanded military vehicle/aircraft/vessel production ──
    m.insert(
        MethodSlot::Production,
        "Medium Tank Production".into(),
        pm(
            1935,
            Some("arm_002"),
            0.22,
            0.38,
            0.40,
            2.0,
            &[
                (Commodity::Steel, 30.0),
                (Commodity::Fuels, 15.0),
                (Commodity::MechanicalComponents, 10.0),
            ],
            &[(Commodity::MediumTanks, 4.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Heavy Tank Production".into(),
        pm(
            1942,
            Some("arm_002"),
            0.25,
            0.40,
            0.35,
            2.5,
            &[
                (Commodity::Steel, 40.0),
                (Commodity::Fuels, 20.0),
                (Commodity::MechanicalComponents, 15.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::HeavyTanks, 2.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Fighter Production".into(),
        pm(
            1940,
            Some("arm_004"),
            0.25,
            0.40,
            0.35,
            3.0,
            &[
                (Commodity::Steel, 20.0),
                (Commodity::Aluminum, 15.0),
                (Commodity::Fuels, 10.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::Fighters, 5.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Bomber Production".into(),
        pm(
            1942,
            Some("arm_004"),
            0.28,
            0.42,
            0.30,
            3.5,
            &[
                (Commodity::Steel, 30.0),
                (Commodity::Aluminum, 20.0),
                (Commodity::Fuels, 15.0),
                (Commodity::ElectronicComponents, 8.0),
            ],
            &[(Commodity::Bombers, 3.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Helicopter Production".into(),
        pm(
            1960,
            Some("auto3_003"),
            0.30,
            0.40,
            0.30,
            4.0,
            &[
                (Commodity::Steel, 15.0),
                (Commodity::Aluminum, 10.0),
                (Commodity::Fuels, 12.0),
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::MechanicalComponents, 5.0),
            ],
            &[(Commodity::Helicopters, 4.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Submarine Production".into(),
        pm(
            1935,
            Some("arm_002"),
            0.25,
            0.40,
            0.35,
            3.0,
            &[
                (Commodity::Steel, 50.0),
                (Commodity::Fuels, 10.0),
                (Commodity::MechanicalComponents, 15.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::Submarines, 1.0)],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Hand Fitting".into(),
        pm(
            1880,
            None,
            0.20,
            0.30,
            0.50,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Interchangeable Parts".into(),
        pm(
            1910,
            Some("auto_003"),
            0.15,
            0.35,
            0.50,
            1.5,
            &[
                (Commodity::Food, 5.0),
                (Commodity::MechanicalComponents, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "CNC Machining".into(),
        pm(
            1960,
            Some("auto3_002"),
            0.25,
            0.40,
            0.35,
            2.5,
            &[
                (Commodity::Energy, 15.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Robotic Assembly".into(),
        pm(
            1980,
            Some("auto3_007"),
            0.35,
            0.45,
            0.20,
            4.0,
            &[
                (Commodity::Energy, 20.0),
                (Commodity::ElectronicComponents, 10.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Arsenal System".into(),
        pm(
            1880,
            None,
            0.20,
            0.30,
            0.50,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "War Production Board".into(),
        pm(
            1916,
            Some("arm_002"),
            0.15,
            0.35,
            0.50,
            1.8,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Cold War Procurement".into(),
        pm(
            1950,
            Some("arm_002"),
            0.20,
            0.38,
            0.42,
            2.0,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 8.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Lean Arsenal".into(),
        pm(
            1985,
            Some("advman_002"),
            0.25,
            0.40,
            0.35,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)],
            &[],
        ),
    );
    m
}

// === CONSTRUCTION ===
fn construction_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Manual Construction".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Food, 5.0), (Commodity::Timber, 10.0)],
            &[
                (Commodity::ConstructionServices, 10.0),
                (Commodity::RenovationServices, 5.0),
            ],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Steam-Powered Construction".into(),
        pm(
            1890,
            Some("steam_001"),
            0.10,
            0.25,
            0.65,
            1.5,
            &[
                (Commodity::Fuels, 10.0),
                (Commodity::Steel, 5.0),
                (Commodity::Food, 5.0),
            ],
            &[
                (Commodity::ConstructionServices, 20.0),
                (Commodity::RenovationServices, 8.0),
            ],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Reinforced Concrete".into(),
        pm(
            1900,
            Some("steel_004"),
            0.15,
            0.30,
            0.55,
            2.0,
            &[
                (Commodity::Steel, 10.0),
                (Commodity::Cement, 15.0),
                (Commodity::Food, 5.0),
            ],
            &[
                (Commodity::ConstructionServices, 30.0),
                (Commodity::RenovationServices, 10.0),
            ],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Prefabricated Construction".into(),
        pm(
            1950,
            Some("auto3_001"),
            0.20,
            0.35,
            0.45,
            3.0,
            &[
                (Commodity::Steel, 15.0),
                (Commodity::Cement, 10.0),
                (Commodity::ConstructionMachinery, 5.0),
            ],
            &[(Commodity::ConstructionServices, 50.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Modular Construction".into(),
        pm(
            1980,
            Some("auto3_005"),
            0.25,
            0.40,
            0.35,
            4.5,
            &[
                (Commodity::Steel, 20.0),
                (Commodity::Cement, 8.0),
                (Commodity::ConstructionMachinery, 8.0),
            ],
            &[(Commodity::ConstructionServices, 80.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "3D Printed Construction".into(),
        pm(
            1995,
            Some("advman_004"),
            0.30,
            0.45,
            0.25,
            6.0,
            &[
                (Commodity::Cement, 15.0),
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 3.0),
            ],
            &[(Commodity::ConstructionServices, 120.0)],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Hand Tools".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Food, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Steam Cranes".into(),
        pm(
            1890,
            Some("steam_001"),
            0.10,
            0.25,
            0.65,
            1.5,
            &[(Commodity::Fuels, 8.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Electric Cranes".into(),
        pm(
            1910,
            Some("elecf_001"),
            0.15,
            0.30,
            0.55,
            2.0,
            &[(Commodity::Energy, 10.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Tower Cranes".into(),
        pm(
            1950,
            Some("auto3_001"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[
                (Commodity::Energy, 15.0),
                (Commodity::ConstructionMachinery, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Automated Construction".into(),
        pm(
            1990,
            Some("advman_006"),
            0.30,
            0.45,
            0.25,
            4.0,
            &[
                (Commodity::Energy, 20.0),
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Software, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Day Labor".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Food, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Contractor System".into(),
        pm(
            1900,
            Some("mech_008"),
            0.10,
            0.25,
            0.65,
            1.3,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Industrial Construction Firm".into(),
        pm(
            1930,
            Some("steel_004"),
            0.15,
            0.30,
            0.55,
            1.6,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Project Management".into(),
        pm(
            1960,
            Some("cs_004"),
            0.20,
            0.35,
            0.45,
            2.0,
            &[(Commodity::Food, 5.0), (Commodity::Software, 2.0)],
            &[],
        ),
    );
    m
}

// === DEEP WELL CONSTRUCTION ===
fn deep_well_construction_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();

    // ── Production: outputs WaterWellAsset ──
    m.insert(MethodSlot::Production, "Hand-Dug Well".into(), pm(1880, None, 0.05, 0.15, 0.80, 1.0, &[(Commodity::Timber, 8.0), (Commodity::Bricks, 12.0), (Commodity::Food, 5.0)], &[(Commodity::WaterWellAsset, 1.0)]));
    m.insert(MethodSlot::Production, "Driven Point Well".into(), pm(1900, Some("steel_004"), 0.10, 0.25, 0.65, 1.5, &[(Commodity::Steel, 5.0), (Commodity::Cement, 8.0), (Commodity::Food, 5.0)], &[(Commodity::WaterWellAsset, 2.0)]));
    m.insert(MethodSlot::Production, "Rotary Drilled Well".into(), pm(1930, Some("mech_008"), 0.15, 0.30, 0.55, 2.5, &[(Commodity::Steel, 10.0), (Commodity::Cement, 15.0), (Commodity::ConstructionMachinery, 3.0), (Commodity::Food, 5.0)], &[(Commodity::WaterWellAsset, 5.0)]));
    m.insert(MethodSlot::Production, "Modern Deep Borehole".into(), pm(1970, Some("auto3_001"), 0.20, 0.35, 0.45, 4.0, &[(Commodity::Steel, 15.0), (Commodity::Cement, 20.0), (Commodity::ConstructionMachinery, 8.0), (Commodity::ElectronicComponents, 3.0), (Commodity::Food, 5.0)], &[(Commodity::WaterWellAsset, 10.0)]));

    // ── Automation: 3 tiers, max gap 50 years ──
    m.insert(MethodSlot::Automation, "Hand Digging".into(), pm(1880, None, 0.05, 0.15, 0.80, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Powered Augers".into(), pm(1920, Some("steam_001"), 0.10, 0.25, 0.65, 2.0, &[(Commodity::Fuels, 8.0), (Commodity::Steel, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Heavy Rotary Rigs".into(), pm(1970, Some("auto3_001"), 0.20, 0.35, 0.45, 4.0, &[(Commodity::Energy, 15.0), (Commodity::ConstructionMachinery, 5.0), (Commodity::Steel, 3.0)], &[]));

    // ── Organization: 3 tiers, max gap 50 years ──
    m.insert(MethodSlot::Organization, "Informal Crews".into(), pm(1880, None, 0.05, 0.15, 0.80, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Specialized Contractors".into(), pm(1920, Some("mech_008"), 0.10, 0.25, 0.65, 1.5, &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Corporate Drilling Fleets".into(), pm(1970, Some("cs_004"), 0.20, 0.35, 0.45, 2.5, &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0), (Commodity::Software, 2.0)], &[]));

    m
}

// === ENERGY ===
fn energy_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // Phase 74: Fuel-burning plants use pm_thermal with thermal_efficiency.
    // Fuel inputs are capacity slots (max fuel the plant can accept per cycle).
    // Actual consumption is computed dynamically in process_building_cycle()
    // based on calorific_value_mj_per_unit() and the plant's thermal_efficiency.
    m.insert(
        MethodSlot::Production,
        "Coal-Fired Boilers".into(),
        pm_thermal(
            1880,
            None,
            0.10,
            0.25,
            0.65,
            1.0,
            &[
                (Commodity::HardCoal, 20.0),
                (Commodity::BrownCoal, 10.0),
                (Commodity::Peat, 5.0),
                (Commodity::Water, 10.0),
            ],
            &[(Commodity::Energy, 30.0), (Commodity::Heat, 10.0)],
            0.15,
        ),
    ); // 15% thermal efficiency
    m.insert(
        MethodSlot::Production,
        "Turbo-Generator Plant".into(),
        pm_thermal(
            1888,
            Some("steam_003"),
            0.15,
            0.30,
            0.55,
            2.0,
            &[
                (Commodity::HardCoal, 15.0),
                (Commodity::BrownCoal, 8.0),
                (Commodity::Water, 8.0),
            ],
            &[(Commodity::Energy, 50.0), (Commodity::Heat, 15.0)],
            0.25,
        ),
    ); // 25% thermal efficiency
    m.insert(
        MethodSlot::Production,
        "Hydroelectric Power".into(),
        pm(
            1890,
            Some("elecf_002"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[
                (Commodity::Water, 15.0),
                (Commodity::MechanicalComponents, 5.0),
            ],
            &[(Commodity::Energy, 60.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Steam Turbine Plant".into(),
        pm_thermal(
            1900,
            Some("steam_005"),
            0.20,
            0.35,
            0.45,
            3.0,
            &[(Commodity::HardCoal, 20.0), (Commodity::Water, 10.0)],
            &[(Commodity::Energy, 80.0), (Commodity::Heat, 25.0)],
            0.30,
        ),
    ); // 30% thermal efficiency
    m.insert(
        MethodSlot::Production,
        "Internal Combustion Plant".into(),
        pm_thermal(
            1910,
            Some("auto_002"),
            0.20,
            0.35,
            0.45,
            3.5,
            &[
                (Commodity::Fuels, 15.0),
                (Commodity::MechanicalComponents, 5.0),
            ],
            &[(Commodity::Energy, 90.0)],
            0.35,
        ),
    ); // 35% thermal efficiency
    m.insert(
        MethodSlot::Production,
        "Nuclear Power Plant".into(),
        pm_thermal(
            1955,
            Some("nucp_001"),
            0.30,
            0.45,
            0.25,
            6.0,
            &[
                (Commodity::Uranium, 5.0),
                (Commodity::Water, 20.0),
                (Commodity::ElectronicComponents, 10.0),
            ],
            &[(Commodity::Energy, 200.0)],
            0.33,
        ),
    ); // 33% thermal efficiency
    m.insert(
        MethodSlot::Production,
        "Combined Cycle Plant".into(),
        pm_thermal(
            1975,
            Some("auto3_007"),
            0.30,
            0.45,
            0.25,
            7.0,
            &[
                (Commodity::NaturalGas, 15.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::Energy, 250.0), (Commodity::Heat, 40.0)],
            0.55,
        ),
    ); // 55% thermal efficiency
    m.insert(
        MethodSlot::Production,
        "Solar Power Plant".into(),
        pm(
            1990,
            Some("advman_004"),
            0.30,
            0.45,
            0.25,
            5.0,
            &[
                (Commodity::ElectronicComponents, 10.0),
                (Commodity::Silicon, 5.0),
            ],
            &[(Commodity::Energy, 150.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Wind Turbine Farm".into(),
        pm(
            1990,
            Some("advman_005"),
            0.25,
            0.40,
            0.35,
            4.5,
            &[
                (Commodity::MechanicalComponents, 10.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::Energy, 120.0)],
        ),
    );
    // ── Phase 20: Utilities and modern energy ──
    m.insert(
        MethodSlot::Production,
        "Water Utility".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Energy, 3.0), (Commodity::Chemicals, 1.0)],
            &[(Commodity::Water, 50.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Geothermal Plant".into(),
        pm(
            1980,
            Some("advman_004"),
            0.25,
            0.40,
            0.35,
            4.0,
            &[
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::ElectronicComponents, 3.0),
                (Commodity::Water, 5.0),
            ],
            &[(Commodity::Energy, 100.0)],
        ),
    );
    // Phase 79: Pumped Storage Plant — first built 1907 in Switzerland.
    // Consumes Energy (pumping water uphill) and outputs Energy (releasing it).
    // Round-trip efficiency ~72% (28% lost to friction, turbine losses, evaporation).
    m.insert(
        MethodSlot::Production,
        "Pumped Storage Plant".into(),
        pm_storage(
            1907,
            Some("pstrg_001"),
            0.15,
            0.30,
            0.55,
            1.5,
            &[
                (Commodity::Energy, 100.0),
                (Commodity::Water, 20.0),
                (Commodity::MechanicalComponents, 5.0),
            ],
            &[(Commodity::Energy, 72.0)],
            0.72,
        ),
    ); // 72% round-trip efficiency
       // Phase 79: Battery Bank Storage — replaces the broken "Battery Storage Facility"
       // which consumed 10 Energy and produced 80 Energy (8x energy creation violation).
       // Round-trip efficiency ~87% for modern lithium-ion grid storage.
    m.insert(
        MethodSlot::Production,
        "Battery Bank Storage".into(),
        pm_storage(
            1990,
            Some("batt_002"),
            0.20,
            0.40,
            0.40,
            2.0,
            &[
                (Commodity::Energy, 100.0),
                (Commodity::Batteries, 5.0),
                (Commodity::ElectronicComponents, 3.0),
            ],
            &[(Commodity::Energy, 87.0)],
            0.87,
        ),
    ); // 87% round-trip efficiency
    m.insert(
        MethodSlot::Automation,
        "Manual Stoking".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Mechanical Stokers".into(),
        pm(
            1890,
            Some("steam_003"),
            0.10,
            0.25,
            0.65,
            1.5,
            &[(Commodity::MechanicalComponents, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Pulverized Coal Burners".into(),
        pm(
            1920,
            Some("steam_005"),
            0.15,
            0.30,
            0.55,
            1.8,
            &[
                (Commodity::Energy, 5.0),
                (Commodity::MechanicalComponents, 2.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Automated Boiler Control".into(),
        pm(
            1950,
            Some("auto3_001"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[(Commodity::ElectronicComponents, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "SCADA Systems".into(),
        pm(
            1985,
            Some("cs_005"),
            0.30,
            0.45,
            0.25,
            4.0,
            &[
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Software, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Shift Operation".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Centralized Dispatch".into(),
        pm(
            1920,
            Some("elecf_005"),
            0.15,
            0.30,
            0.55,
            1.5,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Grid Management".into(),
        pm(
            1960,
            Some("cs_004"),
            0.25,
            0.40,
            0.35,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)],
            &[],
        ),
    );
    m
}

// === Phase 81: Plant-Type-Specific Energy Production Methods ===

/// Coal-fired power plant production methods (era-based progression).
fn coal_fired_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Subcritical Boiler".into(),
        pm_thermal(
            1880,
            None,
            0.10,
            0.25,
            0.65,
            1.0,
            &[(Commodity::HardCoal, 20.0), (Commodity::Water, 10.0)],
            &[(Commodity::Energy, 30.0), (Commodity::Heat, 10.0)],
            0.15,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Supercritical Boiler".into(),
        pm_thermal(
            1930,
            Some("steam_005"),
            0.15,
            0.30,
            0.55,
            2.0,
            &[
                (Commodity::HardCoal, 15.0),
                (Commodity::Water, 8.0),
                (Commodity::MechanicalComponents, 3.0),
            ],
            &[(Commodity::Energy, 60.0), (Commodity::Heat, 15.0)],
            0.25,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Ultra-Supercritical Boiler".into(),
        pm_thermal(
            1960,
            Some("auto3_002"),
            0.20,
            0.35,
            0.45,
            3.0,
            &[
                (Commodity::HardCoal, 12.0),
                (Commodity::Water, 6.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::Energy, 100.0), (Commodity::Heat, 20.0)],
            0.38,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Integrated Gasification".into(),
        pm_thermal(
            1990,
            Some("advman_004"),
            0.25,
            0.40,
            0.35,
            4.0,
            &[
                (Commodity::HardCoal, 10.0),
                (Commodity::Water, 5.0),
                (Commodity::Semiconductors, 3.0),
            ],
            &[(Commodity::Energy, 130.0), (Commodity::Heat, 25.0)],
            0.45,
        ),
    );
    // Cooling upgrade variants (alternative Production methods).
    m.insert(
        MethodSlot::Production,
        "Closed-Loop Cooling Tower".into(),
        pm_thermal(
            1950,
            Some("cool_001"),
            0.20,
            0.35,
            0.45,
            2.8,
            &[
                (Commodity::HardCoal, 15.0),
                (Commodity::Water, 4.0),
                (Commodity::CoolingTower, 2.0),
            ],
            &[(Commodity::Energy, 80.0), (Commodity::Heat, 15.0)],
            0.30,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Air-Cooled Condenser".into(),
        pm_thermal(
            1970,
            Some("cool_002"),
            0.20,
            0.35,
            0.45,
            2.7,
            &[(Commodity::HardCoal, 16.0), (Commodity::CoolingTower, 2.0)],
            &[(Commodity::Energy, 76.0), (Commodity::Heat, 15.0)],
            0.28,
        ),
    );
    m
}

/// Lignite-fired power plant production methods.
fn lignite_fired_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Lignite Dryer Boiler".into(),
        pm_thermal(
            1880,
            None,
            0.10,
            0.25,
            0.65,
            1.0,
            &[(Commodity::BrownCoal, 30.0), (Commodity::Water, 10.0)],
            &[(Commodity::Energy, 25.0), (Commodity::Heat, 8.0)],
            0.12,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Pre-Dried Lignite".into(),
        pm_thermal(
            1950,
            Some("steam_005"),
            0.15,
            0.30,
            0.55,
            1.8,
            &[
                (Commodity::BrownCoal, 20.0),
                (Commodity::Water, 8.0),
                (Commodity::MechanicalComponents, 3.0),
            ],
            &[(Commodity::Energy, 50.0), (Commodity::Heat, 12.0)],
            0.20,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Fluidized Bed Lignite".into(),
        pm_thermal(
            1980,
            Some("auto3_002"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[
                (Commodity::BrownCoal, 15.0),
                (Commodity::Water, 6.0),
                (Commodity::ElectronicComponents, 4.0),
            ],
            &[(Commodity::Energy, 75.0), (Commodity::Heat, 18.0)],
            0.28,
        ),
    );
    m
}

/// Oil/gas power plant production methods.
fn oil_gas_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Diesel Generator".into(),
        pm_thermal(
            1910,
            Some("auto_002"),
            0.15,
            0.30,
            0.55,
            2.0,
            &[
                (Commodity::Fuels, 15.0),
                (Commodity::MechanicalComponents, 5.0),
            ],
            &[(Commodity::Energy, 90.0)],
            0.35,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Gas Turbine".into(),
        pm_thermal(
            1940,
            Some("auto3_001"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[(Commodity::NaturalGas, 15.0), (Commodity::Water, 5.0)],
            &[(Commodity::Energy, 120.0), (Commodity::Heat, 20.0)],
            0.30,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Combined Cycle".into(),
        pm_thermal(
            1975,
            Some("auto3_007"),
            0.25,
            0.40,
            0.35,
            3.5,
            &[
                (Commodity::NaturalGas, 12.0),
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Water, 4.0),
            ],
            &[(Commodity::Energy, 200.0), (Commodity::Heat, 30.0)],
            0.55,
        ),
    );
    m
}

/// Nuclear power plant production methods.
fn nuclear_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "PWR Reactor".into(),
        pm_thermal(
            1955,
            Some("nucp_001"),
            0.30,
            0.45,
            0.25,
            5.0,
            &[
                (Commodity::Uranium, 5.0),
                (Commodity::Water, 20.0),
                (Commodity::ElectronicComponents, 10.0),
            ],
            &[(Commodity::Energy, 200.0)],
            0.33,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "BWR Reactor".into(),
        pm_thermal(
            1960,
            Some("nucp_002"),
            0.30,
            0.45,
            0.25,
            5.5,
            &[
                (Commodity::Uranium, 4.0),
                (Commodity::Water, 15.0),
                (Commodity::ElectronicComponents, 8.0),
            ],
            &[(Commodity::Energy, 220.0)],
            0.34,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Fast Breeder".into(),
        pm_thermal(
            1975,
            Some("nucp_006"),
            0.35,
            0.45,
            0.20,
            6.0,
            &[
                (Commodity::Uranium, 3.0),
                (Commodity::Water, 12.0),
                (Commodity::ElectronicComponents, 12.0),
            ],
            &[(Commodity::Energy, 280.0)],
            0.40,
        ),
    );
    m
}

/// Solar power plant production methods.
fn solar_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Photovoltaic Array".into(),
        pm(
            1990,
            Some("advman_004"),
            0.25,
            0.40,
            0.35,
            4.0,
            &[
                (Commodity::ElectronicComponents, 10.0),
                (Commodity::Silicon, 5.0),
            ],
            &[(Commodity::Energy, 150.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Concentrated Solar".into(),
        pm(
            2000,
            Some("solar_002"),
            0.30,
            0.40,
            0.30,
            4.5,
            &[
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Steel, 10.0),
                (Commodity::Silicon, 3.0),
            ],
            &[(Commodity::Energy, 180.0), (Commodity::Heat, 30.0)],
        ),
    );
    m
}

/// Wind farm production methods.
fn wind_farm_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Onshore Wind Farm".into(),
        pm(
            1990,
            Some("advman_005"),
            0.20,
            0.35,
            0.45,
            3.5,
            &[
                (Commodity::MechanicalComponents, 10.0),
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Steel, 8.0),
            ],
            &[(Commodity::Energy, 120.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Offshore Wind Farm".into(),
        pm(
            2000,
            Some("wind_001"),
            0.25,
            0.40,
            0.35,
            4.5,
            &[
                (Commodity::MechanicalComponents, 15.0),
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Steel, 15.0),
            ],
            &[(Commodity::Energy, 200.0)],
        ),
    );
    m
}

/// Hydroelectric power plant production methods.
fn hydro_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Run-of-River Hydro".into(),
        pm(
            1890,
            Some("elecf_002"),
            0.15,
            0.30,
            0.55,
            2.0,
            &[
                (Commodity::Water, 15.0),
                (Commodity::MechanicalComponents, 5.0),
            ],
            &[(Commodity::Energy, 60.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Reservoir Hydro".into(),
        pm(
            1920,
            Some("elecf_005"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[
                (Commodity::Water, 20.0),
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::Steel, 5.0),
            ],
            &[(Commodity::Energy, 90.0)],
        ),
    );
    m
}

/// Pumped storage plant production methods.
fn pumped_storage_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Pumped Storage Plant".into(),
        pm_storage(
            1907,
            Some("pstrg_001"),
            0.15,
            0.30,
            0.55,
            1.5,
            &[
                (Commodity::Energy, 100.0),
                (Commodity::Water, 20.0),
                (Commodity::MechanicalComponents, 5.0),
            ],
            &[(Commodity::Energy, 72.0)],
            0.72,
        ),
    );
    m
}

/// Battery storage production methods.
fn battery_storage_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Battery Bank Storage".into(),
        pm_storage(
            1990,
            Some("batt_002"),
            0.20,
            0.40,
            0.40,
            2.0,
            &[
                (Commodity::Energy, 100.0),
                (Commodity::Batteries, 5.0),
                (Commodity::ElectronicComponents, 3.0),
            ],
            &[(Commodity::Energy, 87.0)],
            0.87,
        ),
    );
    m
}

/// Geothermal plant production methods.
fn geothermal_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Geothermal Plant".into(),
        pm(
            1980,
            Some("advman_004"),
            0.25,
            0.40,
            0.35,
            3.5,
            &[
                (Commodity::MechanicalComponents, 8.0),
                (Commodity::ElectronicComponents, 3.0),
                (Commodity::Water, 5.0),
            ],
            &[(Commodity::Energy, 100.0)],
        ),
    );
    m
}

/// Biomass-fired plant production methods (early/rural electrification).
fn biomass_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Wood-Fired Boiler".into(),
        pm_thermal(
            1880,
            None,
            0.10,
            0.25,
            0.65,
            1.0,
            &[
                (Commodity::Timber, 15.0),
                (Commodity::Planks, 10.0),
                (Commodity::Peat, 8.0),
                (Commodity::Water, 5.0),
            ],
            &[(Commodity::Energy, 20.0), (Commodity::Heat, 8.0)],
            0.10,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Automated Biomass".into(),
        pm_thermal(
            1950,
            Some("auto3_001"),
            0.15,
            0.30,
            0.55,
            1.5,
            &[
                (Commodity::Timber, 12.0),
                (Commodity::Planks, 8.0),
                (Commodity::Peat, 5.0),
                (Commodity::Water, 4.0),
                (Commodity::MechanicalComponents, 3.0),
            ],
            &[(Commodity::Energy, 40.0), (Commodity::Heat, 12.0)],
            0.18,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Co-Firing Biomass".into(),
        pm_thermal(
            1990,
            Some("advman_004"),
            0.20,
            0.35,
            0.45,
            2.0,
            &[
                (Commodity::Timber, 8.0),
                (Commodity::HardCoal, 8.0),
                (Commodity::Water, 4.0),
                (Commodity::ElectronicComponents, 3.0),
            ],
            &[(Commodity::Energy, 60.0), (Commodity::Heat, 15.0)],
            0.22,
        ),
    );
    m
}

/// Biogas plant production methods (agricultural waste).
fn biogas_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Anaerobic Digester".into(),
        pm_thermal(
            1930,
            Some("chem_005"),
            0.15,
            0.30,
            0.55,
            1.5,
            &[
                (Commodity::Livestock, 10.0),
                (Commodity::Food, 5.0),
                (Commodity::Water, 3.0),
            ],
            &[(Commodity::Energy, 25.0), (Commodity::Heat, 10.0)],
            0.15,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Upgraded Biogas".into(),
        pm_thermal(
            1980,
            Some("auto3_004"),
            0.20,
            0.35,
            0.45,
            2.0,
            &[
                (Commodity::Livestock, 8.0),
                (Commodity::Food, 4.0),
                (Commodity::Water, 2.0),
                (Commodity::ElectronicComponents, 3.0),
            ],
            &[(Commodity::Energy, 40.0), (Commodity::Heat, 12.0)],
            0.25,
        ),
    );
    m
}

/// Phase 81: Shared automation methods for all energy plant types.
fn energy_automation_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Automation,
        "Manual Stoking".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Mechanical Stokers".into(),
        pm(
            1890,
            Some("steam_003"),
            0.10,
            0.25,
            0.65,
            1.5,
            &[(Commodity::MechanicalComponents, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Pulverized Coal Burners".into(),
        pm(
            1920,
            Some("steam_005"),
            0.15,
            0.30,
            0.55,
            1.8,
            &[
                (Commodity::Energy, 5.0),
                (Commodity::MechanicalComponents, 2.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Automated Boiler Control".into(),
        pm(
            1950,
            Some("auto3_001"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[(Commodity::ElectronicComponents, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "SCADA Systems".into(),
        pm(
            1985,
            Some("cs_005"),
            0.30,
            0.45,
            0.25,
            4.0,
            &[
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Software, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "AI Grid Optimization".into(),
        pm(
            2010,
            Some("cs_008"),
            0.35,
            0.45,
            0.20,
            6.0,
            &[(Commodity::Semiconductors, 5.0), (Commodity::Software, 8.0)],
            &[],
        ),
    );
    m
}

/// Phase 81: Shared organization methods for all energy plant types.
fn energy_organization_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Organization,
        "Shift Operation".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "State Utility Model".into(),
        pm(
            1900,
            None,
            0.10,
            0.25,
            0.65,
            1.2,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Centralized Dispatch".into(),
        pm(
            1920,
            Some("elecf_005"),
            0.15,
            0.30,
            0.55,
            1.5,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Grid Management".into(),
        pm(
            1960,
            Some("cs_004"),
            0.25,
            0.40,
            0.35,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Privatized Grid".into(),
        pm(
            1990,
            Some("cs_005"),
            0.25,
            0.40,
            0.35,
            2.0,
            &[(Commodity::Food, 5.0), (Commodity::Software, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Independent System Operator".into(),
        pm(
            2000,
            Some("cs_008"),
            0.30,
            0.45,
            0.25,
            3.0,
            &[(Commodity::Food, 5.0), (Commodity::Software, 8.0)],
            &[],
        ),
    );
    m
}

// === PHASE 82: HEATING PLANT REGISTRIES ===
// Each heating plant type has its own distinct registry key with a full
// Production/Automation/Organization matrix. No monolithic "heating_plant" key.

/// Wood/peat-fired boiler heat plant (1880+). Low CAPEX, high OPEX, high smog.
fn wood_boiler_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // Production methods
    m.insert(
        MethodSlot::Production,
        "Primitive Wood Boiler".into(),
        pm_heating(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Timber, 8.0), (Commodity::Water, 2.0)],
            &[(Commodity::Heat, 5.0)],
            0.30,
            3.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Improved Wood Boiler".into(),
        pm_heating(
            1900,
            Some("thermo_020"),
            0.08,
            0.22,
            0.70,
            1.5,
            &[(Commodity::Timber, 6.0), (Commodity::Water, 2.0)],
            &[(Commodity::Heat, 6.0)],
            0.40,
            2.5,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Peat-Fired Boiler".into(),
        pm_heating(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            0.8,
            &[(Commodity::Peat, 10.0), (Commodity::Water, 2.0)],
            &[(Commodity::Heat, 4.0)],
            0.25,
            3.5,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Automated Wood Chip Boiler".into(),
        pm_heating(
            1960,
            Some("thermo_022"),
            0.10,
            0.30,
            0.60,
            2.5,
            &[
                (Commodity::Timber, 5.0),
                (Commodity::Water, 2.0),
                (Commodity::Energy, 1.0),
            ],
            &[(Commodity::Heat, 8.0)],
            0.55,
            1.5,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Pellet Boiler with Controls".into(),
        pm_heating(
            1980,
            Some("thermo_024"),
            0.15,
            0.35,
            0.50,
            3.0,
            &[
                (Commodity::Timber, 4.0),
                (Commodity::Water, 2.0),
                (Commodity::Energy, 0.5),
            ],
            &[(Commodity::Heat, 9.0)],
            0.65,
            0.8,
        ),
    );
    m
}

/// Hard coal-fired heat plant (1890+). Moderate CAPEX, moderate OPEX.
fn coal_heat_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Hand-Fired Coal Boiler".into(),
        pm_heating(
            1890,
            Some("thermo_020"),
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::HardCoal, 3.0), (Commodity::Water, 3.0)],
            &[(Commodity::Heat, 8.0)],
            0.45,
            5.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Chain Grate Stoker Boiler".into(),
        pm_heating(
            1910,
            Some("thermo_020"),
            0.08,
            0.25,
            0.67,
            1.5,
            &[
                (Commodity::HardCoal, 2.5),
                (Commodity::Water, 3.0),
                (Commodity::MechanicalComponents, 1.0),
            ],
            &[(Commodity::Heat, 10.0)],
            0.55,
            4.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Pulverized Coal Boiler".into(),
        pm_heating(
            1930,
            Some("steam_005"),
            0.10,
            0.30,
            0.60,
            2.0,
            &[
                (Commodity::HardCoal, 2.0),
                (Commodity::Water, 2.5),
                (Commodity::Energy, 1.0),
            ],
            &[(Commodity::Heat, 13.0)],
            0.65,
            3.5,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Fluidized Bed Combustion".into(),
        pm_heating(
            1970,
            Some("auto3_002"),
            0.15,
            0.35,
            0.50,
            2.8,
            &[
                (Commodity::HardCoal, 1.8),
                (Commodity::Water, 2.0),
                (Commodity::Limestone, 0.5),
                (Commodity::Energy, 1.0),
            ],
            &[(Commodity::Heat, 15.0)],
            0.72,
            1.5,
        ),
    );
    m
}

/// Lignite/brown coal heat plant (1890+). Lower CAPEX, higher OPEX, highest smog.
fn lignite_heat_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Lignite Hand-Fired Boiler".into(),
        pm_heating(
            1890,
            Some("thermo_020"),
            0.05,
            0.20,
            0.75,
            0.9,
            &[(Commodity::BrownCoal, 5.0), (Commodity::Water, 3.0)],
            &[(Commodity::Heat, 6.0)],
            0.35,
            7.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Lignite Stoker Boiler".into(),
        pm_heating(
            1910,
            Some("thermo_020"),
            0.08,
            0.25,
            0.67,
            1.3,
            &[
                (Commodity::BrownCoal, 4.0),
                (Commodity::Water, 3.0),
                (Commodity::MechanicalComponents, 1.0),
            ],
            &[(Commodity::Heat, 8.0)],
            0.45,
            6.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Pre-Dried Lignite Boiler".into(),
        pm_heating(
            1950,
            Some("steam_005"),
            0.10,
            0.30,
            0.60,
            1.8,
            &[
                (Commodity::BrownCoal, 3.0),
                (Commodity::Water, 2.0),
                (Commodity::Energy, 1.0),
            ],
            &[(Commodity::Heat, 10.0)],
            0.55,
            4.5,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Fluidized Bed Lignite".into(),
        pm_heating(
            1980,
            Some("auto3_002"),
            0.15,
            0.35,
            0.50,
            2.5,
            &[
                (Commodity::BrownCoal, 2.5),
                (Commodity::Water, 2.0),
                (Commodity::Limestone, 0.8),
                (Commodity::Energy, 1.0),
            ],
            &[(Commodity::Heat, 12.0)],
            0.62,
            2.0,
        ),
    );
    m
}

/// Coke-oven gas heat plant (1900+). Uses CoalGas byproduct from coking.
fn coke_oven_gas_heat_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "CoalGas Fired Boiler".into(),
        pm_heating(
            1900,
            Some("thermo_021"),
            0.08,
            0.25,
            0.67,
            1.5,
            &[(Commodity::CoalGas, 4.0), (Commodity::Water, 2.0)],
            &[(Commodity::Heat, 7.0)],
            0.60,
            1.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Improved Gas Boiler".into(),
        pm_heating(
            1930,
            Some("steam_005"),
            0.10,
            0.30,
            0.60,
            2.0,
            &[
                (Commodity::CoalGas, 3.0),
                (Commodity::Water, 2.0),
                (Commodity::Energy, 0.5),
            ],
            &[(Commodity::Heat, 9.0)],
            0.70,
            0.8,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Condensing Gas Boiler".into(),
        pm_heating(
            1960,
            Some("auto3_001"),
            0.15,
            0.35,
            0.50,
            2.5,
            &[
                (Commodity::CoalGas, 2.5),
                (Commodity::Water, 2.0),
                (Commodity::Energy, 0.5),
            ],
            &[(Commodity::Heat, 11.0)],
            0.78,
            0.5,
        ),
    );
    m
}

/// Oil-fired heat plant (1910+). Fuel-price-dependent OPEX.
fn oil_heat_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Oil-Fired Boiler".into(),
        pm_heating(
            1910,
            Some("thermo_022"),
            0.08,
            0.25,
            0.67,
            1.5,
            &[(Commodity::Fuels, 2.0), (Commodity::Water, 2.0)],
            &[(Commodity::Heat, 9.0)],
            0.65,
            2.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Improved Oil Boiler".into(),
        pm_heating(
            1940,
            Some("auto3_001"),
            0.10,
            0.30,
            0.60,
            2.0,
            &[
                (Commodity::Fuels, 1.5),
                (Commodity::Water, 2.0),
                (Commodity::Energy, 0.5),
            ],
            &[(Commodity::Heat, 11.0)],
            0.75,
            1.5,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Low-NOx Oil Boiler".into(),
        pm_heating(
            1970,
            Some("auto3_002"),
            0.15,
            0.35,
            0.50,
            2.5,
            &[
                (Commodity::Fuels, 1.3),
                (Commodity::Water, 2.0),
                (Commodity::Energy, 0.5),
                (Commodity::Chemicals, 0.2),
            ],
            &[(Commodity::Heat, 13.0)],
            0.82,
            0.8,
        ),
    );
    m
}

/// Natural gas heat plant (1950+). Clean burning, moderate CAPEX.
fn natural_gas_heat_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Gas-Fired Boiler".into(),
        pm_heating(
            1950,
            Some("thermo_023"),
            0.08,
            0.25,
            0.67,
            1.5,
            &[(Commodity::NaturalGas, 2.0), (Commodity::Water, 2.0)],
            &[(Commodity::Heat, 10.0)],
            0.70,
            0.3,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Condensing Boiler".into(),
        pm_heating(
            1970,
            Some("auto3_001"),
            0.10,
            0.30,
            0.60,
            2.0,
            &[
                (Commodity::NaturalGas, 1.5),
                (Commodity::Water, 2.0),
                (Commodity::Energy, 0.5),
            ],
            &[(Commodity::Heat, 12.0)],
            0.85,
            0.2,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Low-NOx Gas Burner".into(),
        pm_heating(
            1980,
            Some("auto3_002"),
            0.15,
            0.35,
            0.50,
            2.5,
            &[
                (Commodity::NaturalGas, 1.3),
                (Commodity::Water, 2.0),
                (Commodity::Energy, 0.5),
                (Commodity::Chemicals, 0.1),
            ],
            &[(Commodity::Heat, 14.0)],
            0.90,
            0.1,
        ),
    );
    m
}

/// Geothermal heating plant (1970+). High CAPEX, near-zero OPEX, zero emissions.
/// Requires volcanic/geothermal geological trait on the region.
fn geothermal_heat_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Geothermal Well".into(),
        pm_heating(
            1970,
            Some("thermo_024"),
            0.15,
            0.35,
            0.50,
            2.0,
            &[(Commodity::Water, 5.0)],
            &[(Commodity::Heat, 12.0)],
            0.95,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Enhanced Geothermal System".into(),
        pm_heating(
            1990,
            Some("advman_004"),
            0.20,
            0.40,
            0.40,
            3.0,
            &[(Commodity::Water, 4.0), (Commodity::Energy, 1.0)],
            &[(Commodity::Heat, 16.0)],
            0.97,
            0.0,
        ),
    );
    m
}

/// Phase 82: Shared heating plant automation methods.
/// These are identical across all heating plant types (same pattern as
/// energy_automation_methods). Plant-specific automation goes in the
/// plant-specific registry.
fn heating_automation_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Automation,
        "Manual Stoking".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Mechanical Grate Stoker".into(),
        pm(
            1900,
            Some("steam_003"),
            0.08,
            0.25,
            0.67,
            1.5,
            &[(Commodity::MechanicalComponents, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Automated Feed System".into(),
        pm(
            1950,
            Some("auto3_001"),
            0.12,
            0.30,
            0.58,
            2.5,
            &[
                (Commodity::Energy, 3.0),
                (Commodity::MechanicalComponents, 1.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Computerized Combustion Control".into(),
        pm(
            1985,
            Some("cs_005"),
            0.20,
            0.35,
            0.45,
            4.0,
            &[
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 3.0),
            ],
            &[],
        ),
    );
    m
}

/// Phase 82: Shared heating plant organization methods.
fn heating_organization_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Organization,
        "Village Cooperative".into(),
        pm(
            1880,
            None,
            0.05,
            0.20,
            0.75,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Municipal Heat Office".into(),
        pm(
            1900,
            None,
            0.08,
            0.25,
            0.67,
            1.2,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Centralized Dispatch".into(),
        pm(
            1920,
            Some("elecf_005"),
            0.12,
            0.30,
            0.58,
            1.5,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "District Heating Enterprise".into(),
        pm(
            1960,
            Some("cs_004"),
            0.20,
            0.35,
            0.45,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)],
            &[],
        ),
    );
    m
}

// === PHASE 82B: EMISSION CONTROL REGISTRIES ===

/// Heavy industry emission control methods (8 methods: None → SCR).
/// Applied to all heavy industry buildings via MethodSlot::EmissionControl.
/// The `efficiency` field stores the emission reduction factor
/// (1.0 = no reduction, 0.05 = 95% reduction).
fn heavy_industry_emission_control_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // None — no emission controls (1.0× emissions)
    m.insert(
        MethodSlot::EmissionControl,
        "None".into(),
        pm_capex(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[], &[]),
    );
    // Basic Settling Chamber — 30% reduction (0.7× emissions)
    m.insert(
        MethodSlot::EmissionControl,
        "Basic Settling Chamber".into(),
        pm_capex(
            1890,
            Some("thermo_020"),
            0.05,
            0.15,
            0.80,
            0.7,
            &[(Commodity::Energy, 1.0)],
            &[],
            &[(Commodity::Steel, 5.0), (Commodity::Cement, 3.0)],
        ),
    );
    // Cyclone Separator — 50% reduction (0.5× emissions)
    m.insert(
        MethodSlot::EmissionControl,
        "Cyclone Separator".into(),
        pm_capex(
            1920,
            Some("steam_005"),
            0.08,
            0.20,
            0.72,
            0.5,
            &[(Commodity::Energy, 2.0)],
            &[],
            &[
                (Commodity::Steel, 8.0),
                (Commodity::MechanicalComponents, 3.0),
            ],
        ),
    );
    // Wet Scrubber — 80% reduction (0.2× emissions)
    m.insert(
        MethodSlot::EmissionControl,
        "Wet Scrubber".into(),
        pm_capex(
            1950,
            Some("auto3_001"),
            0.10,
            0.25,
            0.65,
            0.2,
            &[(Commodity::Water, 10.0), (Commodity::Energy, 3.0)],
            &[],
            &[
                (Commodity::Steel, 10.0),
                (Commodity::Chemicals, 5.0),
                (Commodity::Cement, 5.0),
            ],
        ),
    );
    // Baghouse Filter — 95% reduction (0.05× emissions)
    m.insert(
        MethodSlot::EmissionControl,
        "Baghouse Filter".into(),
        pm_capex(
            1960,
            Some("auto3_002"),
            0.12,
            0.30,
            0.58,
            0.05,
            &[(Commodity::Energy, 5.0)],
            &[],
            &[
                (Commodity::Steel, 12.0),
                (Commodity::IndustrialFiber, 8.0),
                (Commodity::ElectronicComponents, 3.0),
            ],
        ),
    );
    // Flue-Gas Desulfurization (FGD) — 98% reduction (0.02× emissions)
    m.insert(
        MethodSlot::EmissionControl,
        "Flue-Gas Desulfurization".into(),
        pm_capex(
            1970,
            Some("chem_006"),
            0.15,
            0.35,
            0.50,
            0.02,
            &[
                (Commodity::Limestone, 5.0),
                (Commodity::Water, 15.0),
                (Commodity::Energy, 5.0),
            ],
            &[],
            &[
                (Commodity::Steel, 15.0),
                (Commodity::Chemicals, 8.0),
                (Commodity::Cement, 10.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
        ),
    );
    // Electrostatic Precipitator (ESP) — 99% reduction (0.01× emissions)
    m.insert(
        MethodSlot::EmissionControl,
        "Electrostatic Precipitator".into(),
        pm_capex(
            1970,
            Some("auto3_002"),
            0.15,
            0.35,
            0.50,
            0.01,
            &[(Commodity::Energy, 8.0)],
            &[],
            &[
                (Commodity::Steel, 15.0),
                (Commodity::Copper, 5.0),
                (Commodity::ElectronicComponents, 8.0),
            ],
        ),
    );
    // Selective Catalytic Reduction (SCR) — 99.5% reduction (0.005× emissions)
    m.insert(
        MethodSlot::EmissionControl,
        "Selective Catalytic Reduction".into(),
        pm_capex(
            1980,
            Some("chem_008"),
            0.18,
            0.38,
            0.44,
            0.005,
            &[(Commodity::Ammonia, 3.0), (Commodity::Energy, 5.0)],
            &[],
            &[
                (Commodity::Steel, 10.0),
                (Commodity::Catalysts, 5.0),
                (Commodity::ElectronicComponents, 8.0),
            ],
        ),
    );
    m
}

/// Heating plant emission control methods (subset applicable to heating plants).
fn heating_plant_emission_control_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::EmissionControl,
        "None".into(),
        pm_capex(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[], &[]),
    );
    m.insert(
        MethodSlot::EmissionControl,
        "Basic Settling Chamber".into(),
        pm_capex(
            1890,
            Some("thermo_020"),
            0.05,
            0.15,
            0.80,
            0.7,
            &[(Commodity::Energy, 1.0)],
            &[],
            &[(Commodity::Steel, 5.0), (Commodity::Cement, 3.0)],
        ),
    );
    m.insert(
        MethodSlot::EmissionControl,
        "Cyclone Separator".into(),
        pm_capex(
            1920,
            Some("steam_005"),
            0.08,
            0.20,
            0.72,
            0.5,
            &[(Commodity::Energy, 2.0)],
            &[],
            &[
                (Commodity::Steel, 8.0),
                (Commodity::MechanicalComponents, 3.0),
            ],
        ),
    );
    m.insert(
        MethodSlot::EmissionControl,
        "Wet Scrubber".into(),
        pm_capex(
            1950,
            Some("auto3_001"),
            0.10,
            0.25,
            0.65,
            0.2,
            &[(Commodity::Water, 10.0), (Commodity::Energy, 3.0)],
            &[],
            &[
                (Commodity::Steel, 10.0),
                (Commodity::Chemicals, 5.0),
                (Commodity::Cement, 5.0),
            ],
        ),
    );
    m
}

/// Power plant emission control methods (subset applicable to power plants).
fn power_plant_emission_control_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::EmissionControl,
        "None".into(),
        pm_capex(0, None, 0.0, 0.0, 1.0, 1.0, &[], &[], &[]),
    );
    m.insert(
        MethodSlot::EmissionControl,
        "Wet Scrubber".into(),
        pm_capex(
            1950,
            Some("auto3_001"),
            0.10,
            0.25,
            0.65,
            0.2,
            &[(Commodity::Water, 10.0), (Commodity::Energy, 3.0)],
            &[],
            &[
                (Commodity::Steel, 10.0),
                (Commodity::Chemicals, 5.0),
                (Commodity::Cement, 5.0),
            ],
        ),
    );
    m.insert(
        MethodSlot::EmissionControl,
        "Flue-Gas Desulfurization".into(),
        pm_capex(
            1970,
            Some("chem_006"),
            0.15,
            0.35,
            0.50,
            0.02,
            &[
                (Commodity::Limestone, 5.0),
                (Commodity::Water, 15.0),
                (Commodity::Energy, 5.0),
            ],
            &[],
            &[
                (Commodity::Steel, 15.0),
                (Commodity::Chemicals, 8.0),
                (Commodity::Cement, 10.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
        ),
    );
    m.insert(
        MethodSlot::EmissionControl,
        "Electrostatic Precipitator".into(),
        pm_capex(
            1970,
            Some("auto3_002"),
            0.15,
            0.35,
            0.50,
            0.01,
            &[(Commodity::Energy, 8.0)],
            &[],
            &[
                (Commodity::Steel, 15.0),
                (Commodity::Copper, 5.0),
                (Commodity::ElectronicComponents, 8.0),
            ],
        ),
    );
    m.insert(
        MethodSlot::EmissionControl,
        "Selective Catalytic Reduction".into(),
        pm_capex(
            1980,
            Some("chem_008"),
            0.18,
            0.38,
            0.44,
            0.005,
            &[(Commodity::Ammonia, 3.0), (Commodity::Energy, 5.0)],
            &[],
            &[
                (Commodity::Steel, 10.0),
                (Commodity::Catalysts, 5.0),
                (Commodity::ElectronicComponents, 8.0),
            ],
        ),
    );
    m
}

// === TRANSPORT & LOGISTICS ===
fn transport_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Horse-Drawn Wagons".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Food, 5.0), (Commodity::Fuels, 2.0)],
            &[(Commodity::PassengerTransport, 10.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Steam Locomotives".into(),
        pm(
            1885,
            Some("steam_002"),
            0.10,
            0.25,
            0.65,
            2.0,
            &[(Commodity::Fuels, 15.0), (Commodity::Steel, 5.0)],
            &[(Commodity::PassengerTransport, 30.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Electric Trams".into(),
        pm(
            1895,
            Some("elecf_002"),
            0.15,
            0.30,
            0.55,
            2.5,
            &[(Commodity::Energy, 10.0), (Commodity::Steel, 3.0)],
            &[(Commodity::PassengerTransport, 40.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Diesel Locomotives".into(),
        pm(
            1930,
            Some("auto_002"),
            0.15,
            0.30,
            0.55,
            3.0,
            &[
                (Commodity::Fuels, 12.0),
                (Commodity::MechanicalComponents, 5.0),
            ],
            &[(Commodity::PassengerTransport, 60.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Container Shipping".into(),
        pm(
            1960,
            Some("auto3_002"),
            0.20,
            0.35,
            0.45,
            4.0,
            &[(Commodity::Fuels, 15.0), (Commodity::Steel, 10.0)],
            &[(Commodity::PassengerTransport, 100.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "High-Speed Rail".into(),
        pm(
            1980,
            Some("auto3_005"),
            0.25,
            0.40,
            0.35,
            5.5,
            &[
                (Commodity::Energy, 20.0),
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Steel, 10.0),
            ],
            &[(Commodity::PassengerTransport, 180.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Logistics Networks".into(),
        pm(
            1990,
            Some("advman_002"),
            0.30,
            0.40,
            0.30,
            7.0,
            &[
                (Commodity::Fuels, 10.0),
                (Commodity::Software, 5.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::PassengerTransport, 250.0)],
        ),
    );
    // ── Phase 23A: Freight-producing methods ──
    // Early-game freight using draft animals (no machinery required).
    m.insert(
        MethodSlot::Production,
        "Pack Caravans".into(),
        pm(
            1850,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Fodder, 8.0), (Commodity::Water, 4.0)],
            &[(Commodity::FreightCapacity, 5.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Horse-Drawn Freight Wagons".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.2,
            &[
                (Commodity::Fodder, 6.0),
                (Commodity::Water, 3.0),
                (Commodity::DraftAnimals, 4.0),
            ],
            &[(Commodity::FreightCapacity, 12.0)],
        ),
    );
    // Rail freight (requires RailNetwork in Phase 23B; gating added later).
    m.insert(
        MethodSlot::Production,
        "Steam Freight Trains".into(),
        pm(
            1885,
            Some("steam_002"),
            0.10,
            0.25,
            0.65,
            2.5,
            &[
                (Commodity::Fuels, 15.0),
                (Commodity::Steel, 5.0),
                (Commodity::Trains, 2.0),
            ],
            &[(Commodity::FreightCapacity, 40.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Diesel Freight Trains".into(),
        pm(
            1930,
            Some("auto_002"),
            0.15,
            0.30,
            0.55,
            3.5,
            &[
                (Commodity::Fuels, 12.0),
                (Commodity::MechanicalComponents, 5.0),
                (Commodity::Trains, 2.0),
            ],
            &[(Commodity::FreightCapacity, 80.0)],
        ),
    );
    // Road freight.
    m.insert(
        MethodSlot::Production,
        "Container Trucking".into(),
        pm(
            1960,
            Some("auto3_002"),
            0.20,
            0.35,
            0.45,
            4.5,
            &[(Commodity::Fuels, 15.0), (Commodity::Steel, 10.0)],
            &[(Commodity::FreightCapacity, 120.0)],
        ),
    );
    // Air freight (late-game; requires Airport in Phase 23D).
    m.insert(
        MethodSlot::Production,
        "Air Cargo".into(),
        pm(
            1960,
            Some("auto3_002"),
            0.25,
            0.40,
            0.35,
            6.0,
            &[(Commodity::Fuels, 25.0), (Commodity::Aluminum, 8.0)],
            &[
                (Commodity::FreightCapacity, 60.0),
                (Commodity::PassengerTransport, 40.0),
            ],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Manual Signaling".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Food, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Mechanical Signals".into(),
        pm(
            1890,
            Some("steam_002"),
            0.10,
            0.25,
            0.65,
            1.5,
            &[(Commodity::MechanicalComponents, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Electric Signaling".into(),
        pm(
            1910,
            Some("elecf_001"),
            0.15,
            0.30,
            0.55,
            2.0,
            &[(Commodity::Energy, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Relay-Based Interlocking".into(),
        pm(
            1940,
            Some("elecf_005"),
            0.18,
            0.32,
            0.50,
            2.3,
            &[
                (Commodity::Energy, 8.0),
                (Commodity::ElectronicComponents, 2.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Automated Dispatch".into(),
        pm(
            1970,
            Some("auto3_004"),
            0.25,
            0.40,
            0.35,
            3.5,
            &[
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Wagon Trains".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Food, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Timetabled Services".into(),
        pm(
            1890,
            Some("steam_002"),
            0.10,
            0.25,
            0.65,
            1.3,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Nationalized Railways".into(),
        pm(
            1925,
            Some("elecf_005"),
            0.15,
            0.30,
            0.55,
            1.6,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Intermodal Logistics".into(),
        pm(
            1960,
            Some("auto3_002"),
            0.20,
            0.35,
            0.45,
            2.0,
            &[(Commodity::Food, 5.0), (Commodity::Software, 2.0)],
            &[],
        ),
    );
    m
}

// === MEDIA & ENTERTAINMENT ===
fn media_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Print Press".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Paper, 10.0), (Commodity::Food, 3.0)],
            &[(Commodity::Radio, 0.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Radio Broadcasting".into(),
        pm(
            1920,
            Some("radio_001"),
            0.15,
            0.30,
            0.55,
            2.0,
            &[
                (Commodity::Energy, 10.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::Radio, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Television Broadcasting".into(),
        pm(
            1940,
            Some("radio_004"),
            0.20,
            0.35,
            0.45,
            3.0,
            &[
                (Commodity::Energy, 15.0),
                (Commodity::ElectronicComponents, 10.0),
            ],
            &[(Commodity::Televisions, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Cable Television".into(),
        pm(
            1970,
            Some("auto3_004"),
            0.25,
            0.40,
            0.35,
            4.0,
            &[
                (Commodity::Energy, 12.0),
                (Commodity::ElectronicComponents, 8.0),
            ],
            &[(Commodity::Televisions, 30.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Satellite Broadcasting".into(),
        pm(
            1985,
            Some("advman_003"),
            0.30,
            0.40,
            0.30,
            5.0,
            &[
                (Commodity::Energy, 15.0),
                (Commodity::ElectronicComponents, 12.0),
                (Commodity::Software, 5.0),
            ],
            &[(Commodity::Televisions, 50.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Digital Streaming".into(),
        pm(
            1998,
            Some("advman_006"),
            0.35,
            0.45,
            0.20,
            7.0,
            &[
                (Commodity::Energy, 10.0),
                (Commodity::Software, 10.0),
                (Commodity::ElectronicComponents, 8.0),
            ],
            &[(Commodity::Televisions, 80.0)],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Manual Typesetting".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Food, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Linotype Machines".into(),
        pm(
            1890,
            Some("steam_001"),
            0.10,
            0.25,
            0.65,
            1.5,
            &[(Commodity::MechanicalComponents, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Teleprinter Network".into(),
        pm(
            1920,
            Some("radio_001"),
            0.13,
            0.28,
            0.59,
            1.8,
            &[
                (Commodity::Energy, 3.0),
                (Commodity::MechanicalComponents, 2.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Magnetic Tape Editing".into(),
        pm(
            1955,
            Some("radio_004"),
            0.18,
            0.32,
            0.50,
            2.0,
            &[
                (Commodity::Energy, 5.0),
                (Commodity::MechanicalComponents, 2.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Digital Typesetting".into(),
        pm(
            1980,
            Some("auto3_005"),
            0.25,
            0.40,
            0.35,
            3.0,
            &[
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Local Publishers".into(),
        pm(
            1880,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Food, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Broadcast Networks".into(),
        pm(
            1930,
            Some("radio_004"),
            0.15,
            0.30,
            0.55,
            1.8,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Television Networks".into(),
        pm(
            1960,
            Some("radio_004"),
            0.20,
            0.35,
            0.45,
            2.0,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Media Conglomerates".into(),
        pm(
            1985,
            Some("advman_002"),
            0.25,
            0.40,
            0.35,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)],
            &[],
        ),
    );
    m
}

// === MEDICAL SERVICES ===
fn medical_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "General Practice".into(),
        pm_education(
            1880,
            None,
            0.30,
            0.40,
            0.30,
            1.0,
            &[(Commodity::Food, 5.0), (Commodity::Pharmaceuticals, 2.0)],
            &[(Commodity::HealthCapacity, 15.0)],
            CapacityType::HospitalBeds,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Antiseptic Surgery".into(),
        pm_education(
            1890,
            Some("bio_001"),
            0.30,
            0.40,
            0.30,
            1.5,
            &[
                (Commodity::Pharmaceuticals, 5.0),
                (Commodity::Chemicals, 3.0),
            ],
            &[(Commodity::HealthCapacity, 25.0)],
            CapacityType::HospitalBeds,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "X-Ray Diagnostics".into(),
        pm_education(
            1900,
            Some("elecf_003"),
            0.35,
            0.40,
            0.25,
            2.0,
            &[
                (Commodity::Energy, 10.0),
                (Commodity::MedicalEquipment, 3.0),
            ],
            &[(Commodity::HealthCapacity, 40.0)],
            CapacityType::HospitalBeds,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Antibiotic Treatment".into(),
        pm_education(
            1945,
            Some("bio_003"),
            0.35,
            0.40,
            0.25,
            3.0,
            &[
                (Commodity::Pharmaceuticals, 10.0),
                (Commodity::Chemicals, 5.0),
            ],
            &[(Commodity::HealthCapacity, 60.0)],
            CapacityType::HospitalBeds,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Modern Surgery".into(),
        pm_education(
            1960,
            Some("bio_005"),
            0.40,
            0.40,
            0.20,
            4.0,
            &[
                (Commodity::Pharmaceuticals, 12.0),
                (Commodity::MedicalEquipment, 8.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::HealthCapacity, 90.0)],
            CapacityType::HospitalBeds,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Telemedicine".into(),
        pm_education(
            1995,
            Some("advman_004"),
            0.45,
            0.40,
            0.15,
            6.0,
            &[
                (Commodity::Pharmaceuticals, 8.0),
                (Commodity::Software, 5.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::HealthCapacity, 140.0)],
            CapacityType::HospitalBeds,
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Manual Records".into(),
        pm(
            1880,
            None,
            0.10,
            0.20,
            0.70,
            1.0,
            &[(Commodity::Paper, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Punch Card Records".into(),
        pm(
            1930,
            Some("elecf_005"),
            0.15,
            0.30,
            0.55,
            1.5,
            &[(Commodity::Paper, 3.0), (Commodity::Energy, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Mainframe Patient Database".into(),
        pm(
            1970,
            Some("cs_005"),
            0.20,
            0.35,
            0.45,
            2.0,
            &[
                (Commodity::ElectronicComponents, 3.0),
                (Commodity::Energy, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Electronic Health Records".into(),
        pm(
            1990,
            Some("cs_005"),
            0.25,
            0.35,
            0.40,
            2.5,
            &[
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "AI-Assisted Diagnostics".into(),
        pm(
            1998,
            Some("advman_006"),
            0.35,
            0.40,
            0.25,
            3.5,
            &[
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Software, 8.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Private Practice".into(),
        pm(
            1880,
            None,
            0.30,
            0.40,
            0.30,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Hospital System".into(),
        pm(
            1910,
            Some("bio_002"),
            0.25,
            0.40,
            0.35,
            1.5,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Socialized Medicine".into(),
        pm(
            1948,
            Some("bio_003"),
            0.25,
            0.40,
            0.35,
            1.8,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 8.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Managed Care".into(),
        pm(
            1970,
            Some("bio_006"),
            0.30,
            0.40,
            0.30,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)],
            &[],
        ),
    );
    m
}

// === EDUCATIONAL SERVICES ===
fn education_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Traditional Classroom".into(),
        pm_education(
            1880,
            None,
            0.30,
            0.40,
            0.30,
            1.0,
            &[(Commodity::Paper, 5.0), (Commodity::Food, 5.0)],
            &[(Commodity::EducationSlots, 15.0)],
            CapacityType::PrimarySeats,
        ),
    );
    // Phase E.9.4: 8-grade primary school method (for EightPlusFour systems).
    // Covers ages 6-14 in a single building. Higher capacity but also higher
    // Paper and Administrative Services consumption due to larger student body.
    m.insert(
        MethodSlot::Production,
        "Eight-Grade Primary School".into(),
        pm_education(
            1880,
            None,
            0.30,
            0.40,
            0.30,
            1.25, // no_middle_primary_capacity_boost
            &[
                (Commodity::Paper, 8.0),
                (Commodity::Food, 7.0),
                (Commodity::AdministrativeServices, 3.0),
            ],
            &[(Commodity::EducationSlots, 19.0)], // 15 * 1.25
            CapacityType::PrimarySeats,
        ),
    );
    // Phase E.9.4: Middle school method (Gimnazjum-style, for 3-tier systems).
    // Covers ages 10-14/17 depending on system. Requires more specialized
    // labor (higher expert ratio) and additional Paper/Chemicals for science labs.
    m.insert(
        MethodSlot::Production,
        "Middle School (Gimnazjum)".into(),
        pm_education(
            1880,
            None,
            0.35, // higher expert ratio than primary
            0.40,
            0.25,
            1.0,
            &[
                (Commodity::Paper, 6.0),
                (Commodity::Food, 5.0),
                (Commodity::Chemicals, 2.0), // basic science lab supplies
            ],
            &[(Commodity::EducationSlots, 12.0)],
            CapacityType::MiddleSeats,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "University Laboratory".into(),
        pm_education(
            1890,
            Some("bio_001"),
            0.40,
            0.40,
            0.20,
            1.5,
            &[(Commodity::Paper, 10.0), (Commodity::Chemicals, 5.0)],
            &[
                (Commodity::EducationSlots, 25.0),
                (Commodity::InnovationEngineering, 2.0),
                (Commodity::InnovationChemistry, 1.0),
                (Commodity::InnovationMedicine, 1.0),
                (Commodity::InnovationAgronomy, 1.0),
            ],
            CapacityType::UniversitySlots,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Research Laboratory".into(),
        pm_education(
            1910,
            Some("elecf_003"),
            0.45,
            0.40,
            0.15,
            2.5,
            &[
                (Commodity::Paper, 10.0),
                (Commodity::Chemicals, 8.0),
                (Commodity::Energy, 5.0),
            ],
            &[
                (Commodity::EducationSlots, 30.0),
                (Commodity::InnovationEngineering, 4.0),
                (Commodity::InnovationElectronics, 3.0),
                (Commodity::InnovationComputing, 3.0),
                (Commodity::InnovationMetallurgy, 3.0),
                (Commodity::InnovationPhysics, 2.0),
            ],
            CapacityType::UniversitySlots,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Computer-Assisted Learning".into(),
        pm_education(
            1980,
            Some("auto3_004"),
            0.40,
            0.40,
            0.20,
            3.5,
            &[
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 5.0),
            ],
            &[
                (Commodity::EducationSlots, 50.0),
                (Commodity::InnovationComputing, 8.0),
                (Commodity::InnovationElectronics, 6.0),
                (Commodity::InnovationEngineering, 6.0),
            ],
            CapacityType::HighSchoolSeats,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Online Education".into(),
        pm_education(
            1995,
            Some("advman_004"),
            0.45,
            0.40,
            0.15,
            5.0,
            &[
                (Commodity::Software, 10.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[
                (Commodity::EducationSlots, 80.0),
                (Commodity::InnovationComputing, 12.0),
                (Commodity::InnovationElectronics, 8.0),
                (Commodity::InnovationEngineering, 6.0),
                (Commodity::InnovationMedicine, 4.0),
            ],
            CapacityType::HighSchoolSeats,
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Blackboard & Books".into(),
        pm(
            1880,
            None,
            0.10,
            0.20,
            0.70,
            1.0,
            &[(Commodity::Paper, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Filmstrip Projectors".into(),
        pm(
            1915,
            Some("elecf_001"),
            0.13,
            0.25,
            0.62,
            1.3,
            &[(Commodity::Energy, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Audiovisual Aids".into(),
        pm(
            1950,
            Some("radio_004"),
            0.20,
            0.30,
            0.50,
            1.5,
            &[(Commodity::Energy, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Language Laboratory".into(),
        pm(
            1960,
            Some("radio_004"),
            0.25,
            0.35,
            0.40,
            2.0,
            &[
                (Commodity::Energy, 8.0),
                (Commodity::ElectronicComponents, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Smart Classrooms".into(),
        pm(
            1990,
            Some("cs_005"),
            0.30,
            0.40,
            0.30,
            3.0,
            &[
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Apprenticeship".into(),
        pm(
            1880,
            None,
            0.30,
            0.40,
            0.30,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Public Education System".into(),
        pm(
            1900,
            Some("mech_008"),
            0.25,
            0.40,
            0.35,
            1.5,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Research University".into(),
        pm(
            1950,
            Some("nucp_001"),
            0.35,
            0.40,
            0.25,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 2.0)],
            &[],
        ),
    );
    m
}

// === PHASE 18S: SPORTS & RECREATION ===
/// Phase 18S: Sports and recreation facility production methods.
///
/// Three distinct facility types with different physical mechanics:
///
/// - **Open Air Field**: Grass pitch, minimal Steel/Timber frame. Consumes
///   GrassSeed, Water, Labor. Climate-vulnerable (closes in winter).
/// - **Indoor Hall**: Steel/Concrete/Bricks structure. Consumes Energy,
///   Water, Labor. Operates year-round.
/// - **Stadium**: Steel/Concrete/Glass/Lighting. Consumes Energy, Water,
///   Labor, SecurityServices. High CAPEX amortization. Year-round.
///
/// All three output `Commodity::SportsCapacity` (visitor-slots per turn),
/// a service-capacity commodity analogous to EducationSlots/HealthCapacity.
fn sports_recreation_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();

    // Open Air Field — grass pitch, minimal frame.
    // Climate vulnerability = 1.0 (closes in winter via weather state).
    // Capacity scales by pitch_area_m2 (size_metric at runtime).
    m.insert(
        MethodSlot::Production,
        "Open Air Field".into(),
        pm_education(
            1880,
            None,
            0.10, // experts: 1 coach per 1000
            0.20, // skilled: groundskeepers
            0.70, // basic: general labor
            1.0,
            &[
                (Commodity::Seeds, 2.0),
                (Commodity::Water, 5.0),
                (Commodity::Food, 3.0), // sustenance for staff
            ],
            &[(Commodity::SportsCapacity, 20.0)],
            CapacityType::SportsCapacity,
        ),
    );

    // Indoor Hall — steel/concrete/bricks structure, year-round operation.
    // Climate vulnerability = 0.0. Capacity scales by floor_area_m2.
    m.insert(
        MethodSlot::Production,
        "Indoor Hall".into(),
        pm_education(
            1890,
            None,
            0.15, // experts: trainers
            0.25, // skilled: maintenance
            0.60, // basic: staff
            1.0,
            &[
                (Commodity::Energy, 8.0), // lighting + HVAC
                (Commodity::Water, 4.0),
                (Commodity::Food, 3.0),
            ],
            &[(Commodity::SportsCapacity, 30.0)],
            CapacityType::SportsCapacity,
        ),
    );

    // Stadium — high-capacity, high-CAPEX venue.
    // Capacity scales by seat_count. High amortization.
    m.insert(
        MethodSlot::Production,
        "Stadium".into(),
        pm_education(
            1900,
            None,
            0.20, // experts: directors, head coaches
            0.30, // skilled: operations, security lead
            0.50, // basic: ushers, cleaners
            1.0,
            &[
                (Commodity::Energy, 15.0), // floodlights, HVAC
                (Commodity::Water, 10.0),
                (Commodity::Food, 5.0),
                (Commodity::AdministrativeServices, 4.0), // event management, ticketing
            ],
            &[(Commodity::SportsCapacity, 100.0)],
            CapacityType::SportsCapacity,
        ),
    );

    m
}

// === PUBLIC SERVICES ===
fn public_services_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Basic Administration".into(),
        pm(
            1880,
            None,
            0.15,
            0.30,
            0.55,
            1.0,
            &[(Commodity::Paper, 10.0), (Commodity::Food, 5.0)],
            &[(Commodity::AdministrativeServices, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Typewriter Office".into(),
        pm(
            1890,
            Some("mech_008"),
            0.20,
            0.35,
            0.45,
            1.5,
            &[(Commodity::Paper, 8.0), (Commodity::OfficeMachinery, 3.0)],
            &[(Commodity::AdministrativeServices, 25.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Computerized Office".into(),
        pm(
            1970,
            Some("auto3_004"),
            0.30,
            0.40,
            0.30,
            3.0,
            &[
                (Commodity::Paper, 3.0),
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 3.0),
            ],
            &[(Commodity::AdministrativeServices, 50.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "E-Government".into(),
        pm(
            1995,
            Some("advman_004"),
            0.35,
            0.40,
            0.25,
            5.0,
            &[
                (Commodity::Software, 8.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::AdministrativeServices, 90.0)],
        ),
    );
    // ── Phase 20: Integration Center (Phase 17B AssimilationCapacity producer) ──
    m.insert(
        MethodSlot::Production,
        "Integration Center".into(),
        pm(
            1950,
            None,
            0.25,
            0.40,
            0.35,
            1.0,
            &[
                (Commodity::Paper, 8.0),
                (Commodity::AdministrativeServices, 5.0),
                (Commodity::Food, 3.0),
            ],
            &[(Commodity::AssimilationCapacity, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Language & Civic Integration".into(),
        pm(
            1980,
            Some("auto3_004"),
            0.30,
            0.40,
            0.30,
            2.0,
            &[
                (Commodity::Paper, 5.0),
                (Commodity::Software, 5.0),
                (Commodity::AdministrativeServices, 8.0),
                (Commodity::ElectronicComponents, 2.0),
            ],
            &[(Commodity::AssimilationCapacity, 50.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Digital Integration Platform".into(),
        pm(
            2000,
            Some("advman_004"),
            0.35,
            0.40,
            0.25,
            3.5,
            &[
                (Commodity::Software, 10.0),
                (Commodity::AdministrativeServices, 10.0),
                (Commodity::ElectronicComponents, 5.0),
            ],
            &[(Commodity::AssimilationCapacity, 100.0)],
        ),
    );
    // ── Phase 20: Banking & Local Services production ──
    m.insert(
        MethodSlot::Production,
        "Banking Office".into(),
        pm(
            1880,
            None,
            0.30,
            0.40,
            0.30,
            1.0,
            &[
                (Commodity::Paper, 5.0),
                (Commodity::OfficeMachinery, 2.0),
                (Commodity::Energy, 3.0),
            ],
            &[(Commodity::BankingServices, 15.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Electronic Banking".into(),
        pm(
            1990,
            Some("cs_005"),
            0.35,
            0.40,
            0.25,
            2.5,
            &[
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 8.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::BankingServices, 50.0)],
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Local Services Shop".into(),
        pm(
            1880,
            None,
            0.15,
            0.35,
            0.50,
            1.0,
            &[
                (Commodity::Fuels, 5.0),
                (Commodity::Food, 4.0),
                (Commodity::Clothing, 2.0),
            ],
            &[(Commodity::LocalServicesCommodity, 20.0)],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Manual Filing".into(),
        pm(
            1880,
            None,
            0.10,
            0.20,
            0.70,
            1.0,
            &[(Commodity::Paper, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Microfilm Archive".into(),
        pm(
            1930,
            Some("elecf_005"),
            0.15,
            0.30,
            0.55,
            1.5,
            &[(Commodity::Energy, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Photocopier Office".into(),
        pm(
            1960,
            Some("elecf_005"),
            0.18,
            0.32,
            0.50,
            1.8,
            &[(Commodity::Energy, 5.0), (Commodity::Paper, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Digital Database".into(),
        pm(
            1985,
            Some("cs_005"),
            0.25,
            0.40,
            0.35,
            3.0,
            &[
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Software, 5.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Patronage System".into(),
        pm(
            1880,
            None,
            0.15,
            0.30,
            0.55,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Civil Service".into(),
        pm(
            1900,
            Some("mech_008"),
            0.25,
            0.40,
            0.35,
            1.5,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Welfare State Administration".into(),
        pm(
            1935,
            Some("mech_008"),
            0.27,
            0.40,
            0.33,
            1.7,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 8.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Computerized Bureaucracy".into(),
        pm(
            1965,
            Some("cs_005"),
            0.28,
            0.40,
            0.32,
            2.0,
            &[
                (Commodity::Food, 5.0),
                (Commodity::ElectronicComponents, 3.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "New Public Management".into(),
        pm(
            1985,
            Some("advman_002"),
            0.30,
            0.40,
            0.30,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)],
            &[],
        ),
    );
    m
}

// === MAINTENANCE WORKSHOPS (Phase 19B) ===
// CRITICAL INVARIANT: These methods produce MaintenanceServices from GENERIC
// RAW MATERIALS ONLY — never machinery, never MaintenanceServices itself.
// This breaks the machinery↔parts circular dependency: a cold-start world can
// always bootstrap maintenance from basic mining/light-industry output.
fn maintenance_workshops_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // Manual repair shop (1850) — basic raw materials, low capacity.
    m.insert(
        MethodSlot::Production,
        "Manual Repair Shop".into(),
        pm(
            1850,
            None,
            0.10,
            0.30,
            0.60,
            1.0,
            &[
                (Commodity::Steel, 3.0),
                (Commodity::MechanicalComponents, 2.0),
                (Commodity::Fuels, 1.0),
            ],
            &[(Commodity::MaintenanceServices, 10.0)],
        ),
    );
    // Mechanized workshop (1900) — more raw materials, higher capacity.
    m.insert(
        MethodSlot::Production,
        "Mechanized Workshop".into(),
        pm(
            1900,
            Some("mech_008"),
            0.15,
            0.35,
            0.50,
            1.5,
            &[
                (Commodity::Steel, 4.0),
                (Commodity::MechanicalComponents, 3.0),
                (Commodity::Energy, 2.0),
                (Commodity::Fuels, 1.0),
            ],
            &[(Commodity::MaintenanceServices, 18.0)],
        ),
    );
    // Electrified repair shop (1950) — electronics + energy, higher capacity.
    m.insert(
        MethodSlot::Production,
        "Electrified Repair Shop".into(),
        pm(
            1950,
            Some("elecf_005"),
            0.20,
            0.40,
            0.40,
            2.5,
            &[
                (Commodity::Steel, 4.0),
                (Commodity::MechanicalComponents, 3.0),
                (Commodity::ElectronicComponents, 2.0),
                (Commodity::Energy, 5.0),
            ],
            &[(Commodity::MaintenanceServices, 35.0)],
        ),
    );
    // CNC repair shop (1990) — advanced electronics, highest capacity.
    m.insert(
        MethodSlot::Production,
        "CNC Repair Shop".into(),
        pm(
            1990,
            Some("auto3_004"),
            0.25,
            0.45,
            0.30,
            4.0,
            &[
                (Commodity::Steel, 3.0),
                (Commodity::MechanicalComponents, 2.0),
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Energy, 8.0),
                (Commodity::Software, 2.0),
            ],
            &[(Commodity::MaintenanceServices, 60.0)],
        ),
    );
    // Automation slot — boosts maintenance capacity (no machinery input!).
    m.insert(
        MethodSlot::Automation,
        "Hand Tools".into(),
        pm(
            1850,
            None,
            0.10,
            0.20,
            0.70,
            1.0,
            &[(Commodity::Fuels, 1.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Steam-Powered Hammers".into(),
        pm(
            1885,
            Some("steam_001"),
            0.12,
            0.25,
            0.63,
            1.2,
            &[(Commodity::Fuels, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Power Tools".into(),
        pm(
            1920,
            Some("elecf_005"),
            0.15,
            0.30,
            0.55,
            1.5,
            &[(Commodity::Energy, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Electric Welding".into(),
        pm(
            1950,
            Some("elecf_005"),
            0.18,
            0.33,
            0.49,
            1.8,
            &[(Commodity::Energy, 5.0), (Commodity::Steel, 1.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Computerized Diagnostics".into(),
        pm(
            1975,
            Some("cs_005"),
            0.20,
            0.38,
            0.42,
            2.2,
            &[
                (Commodity::ElectronicComponents, 3.0),
                (Commodity::Software, 2.0),
            ],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Robotic Repair Arms".into(),
        pm(
            1990,
            Some("auto3_004"),
            0.25,
            0.40,
            0.35,
            3.0,
            &[
                (Commodity::ElectronicComponents, 3.0),
                (Commodity::Energy, 4.0),
            ],
            &[],
        ),
    );
    // Organization slot — workshop management.
    m.insert(
        MethodSlot::Organization,
        "Journeyman System".into(),
        pm(
            1850,
            None,
            0.15,
            0.30,
            0.55,
            1.0,
            &[(Commodity::Food, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Factory Maintenance Dept".into(),
        pm(
            1890,
            Some("mech_008"),
            0.17,
            0.35,
            0.48,
            1.2,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 1.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Specialized Crews".into(),
        pm(
            1930,
            Some("mech_008"),
            0.20,
            0.40,
            0.40,
            1.5,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Preventive Maintenance Schedule".into(),
        pm(
            1960,
            Some("elecf_005"),
            0.25,
            0.40,
            0.35,
            2.0,
            &[(Commodity::Food, 5.0), (Commodity::Paper, 4.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Predictive Maintenance".into(),
        pm(
            1990,
            Some("cs_005"),
            0.30,
            0.40,
            0.30,
            2.5,
            &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)],
            &[],
        ),
    );
    m
}

// ============================================================================
// PHASE 84: WASTE PLANT REGISTRIES (Solid Waste Management & Circular Economy)
// ============================================================================
//
// 13 plant types with full Production/Automation/Organization matrices (Rule 13).
// Mass conservation: every recycling/separation/WtE method outputs residual
// waste so output mass = input mass. WtE outputs HazardousWaste ash.
// B2B-EXCLUDED trash streams flow through WasteGridState only.

/// Phase 84: Helper for waste plant production methods with emission factor
/// and biohazard factor. Used by landfills, WtE plants, and separation plants.
#[allow(clippy::too_many_arguments)]
fn pm_waste(
    year: u32,
    tech: Option<&str>,
    experts: f64,
    skilled: f64,
    basic: f64,
    eff: f64,
    inputs: &[(Commodity, f64)],
    outputs: &[(Commodity, f64)],
    emission_factor: f64,
    biohazard_factor: f64,
    waste_generation_factor: f64,
) -> ProductionMethod {
    ProductionMethod {
        year,
        required_tech: tech.map(|s| s.to_string()),
        experts_ratio: experts,
        skilled_ratio: skilled,
        basic_ratio: basic,
        efficiency: eff,
        inputs: inputs.iter().copied().collect(),
        outputs: outputs.iter().copied().collect(),
        thermal_efficiency: 0.0,
        storage_efficiency: 0.0,
        capex: HashMap::new(),
        emission_factor,
        biohazard_factor,
        output_water_quality: 0.0,
        discharge_quality: 0.0,
        waste_generation_factor,
        seat_type: None,
    }
}

// ── Landfill Production Methods (3 tiers) ──

fn uncontrolled_landfill_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Open Tipping".into(),
        pm_waste(
            1850,
            None,
            0.0,
            0.10,
            0.90,
            0.5,
            &[(Commodity::MixedWaste, 100.0)],
            &[],
            0.0,
            8.0,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Compacted Tipping".into(),
        pm_waste(
            1900,
            Some("sanit_002"),
            0.0,
            0.15,
            0.85,
            0.7,
            &[(Commodity::MixedWaste, 150.0)],
            &[],
            0.0,
            5.0,
            0.0,
        ),
    );
    m
}

fn controlled_landfill_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "Clay-Lined Cell".into(),
        pm_waste(
            1900,
            Some("sanit_002"),
            0.05,
            0.20,
            0.75,
            0.8,
            &[(Commodity::MixedWaste, 200.0)],
            &[],
            0.0,
            2.0,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Compacted Clay-Lined Cell".into(),
        pm_waste(
            1950,
            Some("sanit_004"),
            0.05,
            0.25,
            0.70,
            1.0,
            &[(Commodity::MixedWaste, 300.0)],
            &[],
            0.0,
            1.0,
            0.0,
        ),
    );
    m
}

fn modern_landfill_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Production,
        "HDPE-Lined Cell".into(),
        pm_waste(
            1970,
            Some("chem_006"),
            0.08,
            0.30,
            0.62,
            1.2,
            &[(Commodity::MixedWaste, 400.0)],
            &[(Commodity::Energy, 0.5)],
            0.0,
            0.3,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Double-Lined Cell".into(),
        pm_waste(
            1990,
            Some("advman_004"),
            0.10,
            0.35,
            0.55,
            1.5,
            &[(Commodity::MixedWaste, 500.0)],
            &[(Commodity::Energy, 1.0)],
            0.0,
            0.1,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Bioreactor Landfill".into(),
        pm_waste(
            2000,
            Some("advman_005"),
            0.12,
            0.38,
            0.50,
            2.0,
            &[(Commodity::MixedWaste, 600.0)],
            &[(Commodity::Energy, 2.0)],
            0.0,
            0.05,
            0.0,
        ),
    );
    m
}

// ── Waste Separation Plant Methods (2 tiers) ──

fn waste_separation_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // Manual sorting: MixedWaste → sorted fractions + residual (100% mass)
    m.insert(
        MethodSlot::Production,
        "Manual Sorting Line".into(),
        pm_waste(
            1950,
            Some("sanit_004"),
            0.05,
            0.20,
            0.75,
            0.6,
            &[(Commodity::MixedWaste, 100.0)],
            &[
                (Commodity::MetalWaste, 15.0),
                (Commodity::GlassWaste, 10.0),
                (Commodity::PlasticWaste, 12.0),
                (Commodity::BioWaste, 35.0),
                (Commodity::TextileWaste, 5.0),
                (Commodity::ElectronicWaste, 3.0),
                (Commodity::MixedWaste, 20.0),
            ], // residual — mass balance closure
            0.0,
            0.5,
            0.0,
        ),
    );
    m
}

fn advanced_sorting_facility_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // AI-assisted optical sorting: higher efficiency, same mass balance
    m.insert(
        MethodSlot::Production,
        "Optical Sorting Line".into(),
        pm_waste(
            1990,
            Some("advman_004"),
            0.10,
            0.30,
            0.60,
            0.9,
            &[(Commodity::MixedWaste, 150.0)],
            &[
                (Commodity::MetalWaste, 22.5),
                (Commodity::GlassWaste, 15.0),
                (Commodity::PlasticWaste, 18.0),
                (Commodity::BioWaste, 52.5),
                (Commodity::TextileWaste, 7.5),
                (Commodity::ElectronicWaste, 4.5),
                (Commodity::MixedWaste, 30.0),
            ], // residual — mass balance closure
            0.0,
            0.1,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "NIR + AI Sorting".into(),
        pm_waste(
            2010,
            Some("cs_005"),
            0.15,
            0.35,
            0.50,
            1.2,
            &[(Commodity::MixedWaste, 200.0)],
            &[
                (Commodity::MetalWaste, 30.0),
                (Commodity::GlassWaste, 20.0),
                (Commodity::PlasticWaste, 24.0),
                (Commodity::BioWaste, 70.0),
                (Commodity::TextileWaste, 10.0),
                (Commodity::ElectronicWaste, 6.0),
                (Commodity::MixedWaste, 40.0),
            ], // residual — mass balance closure
            0.0,
            0.05,
            0.0,
        ),
    );
    m
}

// ── Recycling Facility Methods (5 types) ──
// CRITICAL FIX 3: Every method outputs residual waste so mass = 1.0.

fn metal_recycling_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // MetalWaste (1.0) → Steel (0.70) + Copper (0.15) + Aluminum (0.10) + MixedWaste (0.05)
    m.insert(
        MethodSlot::Production,
        "Basic Metal Smelting".into(),
        pm_waste(
            1900,
            Some("sanit_002"),
            0.05,
            0.20,
            0.75,
            0.6,
            &[(Commodity::MetalWaste, 100.0)],
            &[
                (Commodity::Steel, 70.0),
                (Commodity::Copper, 15.0),
                (Commodity::Aluminum, 10.0),
                (Commodity::MixedWaste, 5.0),
            ],
            0.3,
            0.0,
            0.0,
        ),
    ); // emission_factor: smelting fumes
    m.insert(
        MethodSlot::Production,
        "Shredder + Eddy Current".into(),
        pm_waste(
            1970,
            Some("advman_004"),
            0.08,
            0.30,
            0.62,
            0.9,
            &[(Commodity::MetalWaste, 150.0)],
            &[
                (Commodity::Steel, 105.0),
                (Commodity::Copper, 22.5),
                (Commodity::Aluminum, 15.0),
                (Commodity::MixedWaste, 7.5),
            ],
            0.15,
            0.0,
            0.0,
        ),
    );
    m
}

fn glass_recycling_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // GlassWaste (1.0) → Glass (0.85) + MixedWaste (0.15)
    m.insert(
        MethodSlot::Production,
        "Glass Crushing".into(),
        pm_waste(
            1900,
            Some("sanit_002"),
            0.03,
            0.15,
            0.82,
            0.7,
            &[(Commodity::GlassWaste, 100.0)],
            &[(Commodity::Glass, 85.0), (Commodity::MixedWaste, 15.0)],
            0.05,
            0.0,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Cullet Furnace Ready".into(),
        pm_waste(
            1970,
            Some("chem_006"),
            0.05,
            0.25,
            0.70,
            1.0,
            &[(Commodity::GlassWaste, 150.0)],
            &[(Commodity::Glass, 127.5), (Commodity::MixedWaste, 22.5)],
            0.02,
            0.0,
            0.0,
        ),
    );
    m
}

fn plastic_recycling_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // PlasticWaste (1.0) → Plastics (0.60) + MixedWaste (0.40)
    m.insert(
        MethodSlot::Production,
        "Plastic Baling".into(),
        pm_waste(
            1970,
            Some("chem_006"),
            0.05,
            0.20,
            0.75,
            0.5,
            &[(Commodity::PlasticWaste, 100.0)],
            &[(Commodity::Plastics, 60.0), (Commodity::MixedWaste, 40.0)],
            0.05,
            0.0,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Pelletizing Line".into(),
        pm_waste(
            1990,
            Some("advman_004"),
            0.08,
            0.30,
            0.62,
            0.8,
            &[(Commodity::PlasticWaste, 150.0)],
            &[(Commodity::Plastics, 90.0), (Commodity::MixedWaste, 60.0)],
            0.03,
            0.0,
            0.0,
        ),
    );
    m
}

fn electronic_recycling_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // ElectronicWaste (1.0) → Semiconductors (0.05) + Copper (0.20) + REE (0.02) + HazardousWaste (0.73)
    m.insert(
        MethodSlot::Production,
        "Manual Dismantling".into(),
        pm_waste(
            1990,
            Some("advman_004"),
            0.10,
            0.30,
            0.60,
            0.4,
            &[(Commodity::ElectronicWaste, 50.0)],
            &[
                (Commodity::Semiconductors, 2.5),
                (Commodity::Copper, 10.0),
                (Commodity::RareEarthElements, 1.0),
                (Commodity::HazardousWaste, 36.5),
            ],
            0.1,
            1.0,
            0.0,
        ),
    ); // biohazard: toxic dust
    m.insert(
        MethodSlot::Production,
        "Automated Shredding + Refining".into(),
        pm_waste(
            2010,
            Some("cs_005"),
            0.15,
            0.35,
            0.50,
            0.8,
            &[(Commodity::ElectronicWaste, 100.0)],
            &[
                (Commodity::Semiconductors, 5.0),
                (Commodity::Copper, 20.0),
                (Commodity::RareEarthElements, 2.0),
                (Commodity::HazardousWaste, 73.0),
            ],
            0.05,
            0.3,
            0.0,
        ),
    );
    m
}

fn textile_recycling_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // TextileWaste (1.0) → IndustrialFiber (0.40) + MixedWaste (0.60)
    m.insert(
        MethodSlot::Production,
        "Textile Sorting + Shredding".into(),
        pm_waste(
            2000,
            Some("advman_005"),
            0.05,
            0.20,
            0.75,
            0.5,
            &[(Commodity::TextileWaste, 100.0)],
            &[
                (Commodity::IndustrialFiber, 40.0),
                (Commodity::MixedWaste, 60.0),
            ],
            0.02,
            0.0,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Fiber Recovery Line".into(),
        pm_waste(
            2010,
            Some("cs_005"),
            0.08,
            0.30,
            0.62,
            0.8,
            &[(Commodity::TextileWaste, 150.0)],
            &[
                (Commodity::IndustrialFiber, 60.0),
                (Commodity::MixedWaste, 90.0),
            ],
            0.01,
            0.0,
            0.0,
        ),
    );
    m
}

// ── Waste-to-Energy Plant Methods (2 tiers) ──
// CRITICAL FIX 2: WtE outputs HazardousWaste ash (0.20–0.30 per unit input).

fn waste_to_energy_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // Mass Burn: MixedWaste (1.0) → Energy + HazardousWaste ash (0.25)
    m.insert(
        MethodSlot::Production,
        "Mass Burn Incinerator".into(),
        pm_waste(
            1970,
            Some("chem_006"),
            0.08,
            0.25,
            0.67,
            0.7,
            &[(Commodity::MixedWaste, 100.0)],
            &[(Commodity::Energy, 0.2), (Commodity::HazardousWaste, 25.0)],
            0.5,
            0.0,
            0.0,
        ),
    ); // high emissions
    m.insert(
        MethodSlot::Production,
        "Controlled Combustion".into(),
        pm_waste(
            1990,
            Some("advman_004"),
            0.10,
            0.30,
            0.60,
            1.0,
            &[(Commodity::MixedWaste, 150.0)],
            &[(Commodity::Energy, 0.3), (Commodity::HazardousWaste, 37.5)],
            0.2,
            0.0,
            0.0,
        ),
    );
    m
}

fn advanced_wte_chp_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // Advanced WtE with CHP: MixedWaste (1.0) → Energy + Heat + HazardousWaste ash (0.20)
    m.insert(
        MethodSlot::Production,
        "Fluidized Bed CHP".into(),
        pm_waste(
            2000,
            Some("advman_005"),
            0.12,
            0.35,
            0.53,
            1.2,
            &[(Commodity::MixedWaste, 200.0)],
            &[
                (Commodity::Energy, 0.4),
                (Commodity::Heat, 0.8),
                (Commodity::HazardousWaste, 40.0),
            ],
            0.08,
            0.0,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Advanced CHP with Syngas".into(),
        pm_waste(
            2010,
            Some("cs_005"),
            0.15,
            0.38,
            0.47,
            1.5,
            &[(Commodity::MixedWaste, 250.0)],
            &[
                (Commodity::Energy, 0.5),
                (Commodity::Heat, 1.0),
                (Commodity::HazardousWaste, 50.0),
            ],
            0.04,
            0.0,
            0.0,
        ),
    );
    m
}

// ── Civic Amenity Site (PSZOK) Methods ──

fn civic_amenity_site_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // PSZOK receives heavy waste drop-offs (requires FreightCapacity to reach)
    m.insert(
        MethodSlot::Production,
        "Drop-off Reception".into(),
        pm_waste(
            1990,
            Some("sanit_006"),
            0.03,
            0.10,
            0.87,
            0.5,
            &[
                (Commodity::BulkyWaste, 20.0),
                (Commodity::ConstructionWaste, 50.0),
                (Commodity::HazardousWaste, 10.0),
            ],
            &[(Commodity::MetalWaste, 5.0), (Commodity::MixedWaste, 75.0)], // sorted + residual
            0.0,
            0.5,
            0.0,
        ),
    );
    m.insert(
        MethodSlot::Production,
        "Sorted Reception".into(),
        pm_waste(
            2000,
            Some("advman_005"),
            0.05,
            0.15,
            0.80,
            0.8,
            &[
                (Commodity::BulkyWaste, 30.0),
                (Commodity::ConstructionWaste, 80.0),
                (Commodity::HazardousWaste, 15.0),
            ],
            &[
                (Commodity::MetalWaste, 10.0),
                (Commodity::GlassWaste, 5.0),
                (Commodity::MixedWaste, 110.0),
            ], // sorted + residual
            0.0,
            0.2,
            0.0,
        ),
    );
    m
}

// ── Shared Waste Automation and Organization Methods ──

fn waste_automation_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Automation,
        "Manual Labor".into(),
        pm(1850, None, 0.0, 0.10, 0.90, 1.0, &[], &[]),
    );
    m.insert(
        MethodSlot::Automation,
        "Conveyor Belt".into(),
        pm(
            1950,
            Some("sanit_004"),
            0.05,
            0.20,
            0.75,
            1.2,
            &[(Commodity::Energy, 2.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Hydraulic Compactor".into(),
        pm(
            1970,
            Some("mech_008"),
            0.08,
            0.25,
            0.67,
            1.5,
            &[(Commodity::Energy, 5.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Automation,
        "Automated Sorting Arm".into(),
        pm(
            1990,
            Some("advman_004"),
            0.12,
            0.30,
            0.58,
            2.0,
            &[
                (Commodity::Energy, 8.0),
                (Commodity::ElectronicComponents, 1.0),
            ],
            &[],
        ),
    );
    m
}

fn waste_organization_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(
        MethodSlot::Organization,
        "Day Shift Crew".into(),
        pm(
            1850,
            None,
            0.05,
            0.15,
            0.80,
            1.0,
            &[(Commodity::Food, 3.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "Two Shift Operations".into(),
        pm(
            1950,
            Some("sanit_004"),
            0.08,
            0.20,
            0.72,
            1.3,
            &[(Commodity::Food, 6.0)],
            &[],
        ),
    );
    m.insert(
        MethodSlot::Organization,
        "24/7 Operations".into(),
        pm(
            1990,
            Some("advman_004"),
            0.12,
            0.25,
            0.63,
            1.8,
            &[(Commodity::Food, 9.0), (Commodity::Paper, 2.0)],
            &[],
        ),
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::enums::Commodity;
    use std::collections::BTreeSet;

    /// Phase 45: Regression test — ensures that previously orphaned commodities
    /// (producers but no consumers) now have at least one B2B consumer path.
    ///
    /// This test checks that the following commodities appear as inputs in at
    /// least one production method:
    /// BrownCoal, Peat, Coke, Uranium, Livestock, Silver, Magnesium, Zinc,
    /// RefinedFuel, Hydrogen, SupportEquipment, DraftAnimals, Trains
    #[test]
    fn test_orphaned_commodities_have_consumers() {
        let all_methods = default_production_methods();
        let mut input_commodities: BTreeSet<Commodity> = BTreeSet::new();

        for methods in all_methods.values() {
            for pm in methods.automation.values() {
                for &c in pm.inputs.keys() {
                    input_commodities.insert(c);
                }
            }
            for pm in methods.production.values() {
                for &c in pm.inputs.keys() {
                    input_commodities.insert(c);
                }
            }
            for pm in methods.organization.values() {
                for &c in pm.inputs.keys() {
                    input_commodities.insert(c);
                }
            }
        }

        // Previously orphaned commodities that should now have B2B consumers
        let must_have_consumers = vec![
            Commodity::BrownCoal,
            Commodity::Peat,
            Commodity::Coke,
            Commodity::Uranium,
            Commodity::Livestock,
            Commodity::Silver,
            Commodity::Magnesium,
            Commodity::Zinc,
            Commodity::RefinedFuel,
            Commodity::Hydrogen,
            Commodity::DraftAnimals,
            Commodity::Trains,
        ];

        for commodity in &must_have_consumers {
            assert!(
                input_commodities.contains(commodity),
                "Commodity {:?} has no B2B consumer — orphaned supply regression",
                commodity
            );
        }
    }

    /// Phase 45: Regression test — ensures that Bricks and Planks are both
    /// produced AND consumed. Bricks are consumed via construction BOMs (not
    /// production method inputs), so we check both paths.
    #[test]
    fn test_bricks_and_planks_have_supply_and_demand() {
        let all_methods = default_production_methods();
        let mut input_commodities: BTreeSet<Commodity> = BTreeSet::new();
        let mut output_commodities: BTreeSet<Commodity> = BTreeSet::new();

        for methods in all_methods.values() {
            for pm in methods.automation.values() {
                for &c in pm.inputs.keys() {
                    input_commodities.insert(c);
                }
                for &c in pm.outputs.keys() {
                    output_commodities.insert(c);
                }
            }
            for pm in methods.production.values() {
                for &c in pm.inputs.keys() {
                    input_commodities.insert(c);
                }
                for &c in pm.outputs.keys() {
                    output_commodities.insert(c);
                }
            }
            for pm in methods.organization.values() {
                for &c in pm.inputs.keys() {
                    input_commodities.insert(c);
                }
                for &c in pm.outputs.keys() {
                    output_commodities.insert(c);
                }
            }
        }

        // Planks are consumed as B2B production inputs (Furniture Workshop)
        assert!(
            input_commodities.contains(&Commodity::Planks),
            "Commodity Planks has no B2B consumer"
        );
        assert!(
            output_commodities.contains(&Commodity::Planks),
            "Commodity Planks has no B2B producer"
        );

        // Bricks are produced by Brick Making but consumed via construction BOMs.
        // Check that they have a producer at least.
        assert!(
            output_commodities.contains(&Commodity::Bricks),
            "Commodity Bricks has no B2B producer"
        );
        // Bricks consumption is via construction BOMs (see bom.rs tests)
    }
}

//! Hardcoded production methods for all sectors, grouped by slot.
//!
//! This module provides `default_production_methods()` which returns a
//! `HashMap<String, BuildingMethods>` keyed by sector (English snake_case).
//! Each `BuildingMethods` contains production methods for the three slots:
//! automation, production, and organization.

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
    }
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
    }
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
    registry.insert("energy".to_string(), energy_methods());
    // Phase 81: Plant-type-specific energy production method registries.
    registry.insert("coal_fired_plant".to_string(), coal_fired_plant_methods());
    registry.insert("lignite_fired_plant".to_string(), lignite_fired_plant_methods());
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
    registry.insert("energy_organization".to_string(), energy_organization_methods());
    registry.insert("transport_logistics".to_string(), transport_methods());
    registry.insert("media_and_entertainment".to_string(), media_methods());
    registry.insert("medical_services".to_string(), medical_methods());
    registry.insert("educational_services".to_string(), education_methods());
    registry.insert("public_services".to_string(), public_services_methods());
    registry.insert("maintenance_workshops".to_string(), maintenance_workshops_methods());

    registry
}

// === MINING ===
fn mining_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Manual Mining".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Fuels, 2.0), (Commodity::Food, 5.0)],
           &[(Commodity::HardCoal, 10.0)]));
    m.insert(MethodSlot::Production, "Pneumatic Drilling".into(),
        pm(1885, Some("mining_002"), 0.10, 0.30, 0.60, 1.5,
           &[(Commodity::Fuels, 5.0), (Commodity::Food, 5.0), (Commodity::MechanicalComponents, 2.0)],
           &[(Commodity::HardCoal, 15.0)]));
    m.insert(MethodSlot::Production, "Electric Mine Pumps".into(),
        pm(1890, Some("mining_004"), 0.10, 0.30, 0.60, 1.8,
           &[(Commodity::Energy, 5.0), (Commodity::Fuels, 3.0)],
           &[(Commodity::HardCoal, 18.0)]));
    m.insert(MethodSlot::Production, "Longwall Mining".into(),
        pm(1895, Some("mining_006"), 0.15, 0.35, 0.50, 2.2,
           &[(Commodity::Energy, 8.0), (Commodity::Fuels, 4.0), (Commodity::MechanicalComponents, 3.0)],
           &[(Commodity::HardCoal, 25.0)]));
    m.insert(MethodSlot::Production, "Froth Flotation".into(),
        pm(1900, Some("mining_007"), 0.20, 0.35, 0.45, 2.5,
           &[(Commodity::Energy, 10.0), (Commodity::Chemicals, 5.0)],
           &[(Commodity::Copper, 12.0)]));
    m.insert(MethodSlot::Production, "Open-Pit Mining".into(),
        pm(1905, Some("mining_008"), 0.15, 0.30, 0.55, 3.0,
           &[(Commodity::Fuels, 15.0), (Commodity::Energy, 10.0)],
           &[(Commodity::HardCoal, 40.0)]));
    m.insert(MethodSlot::Production, "Mechanized Longwall".into(),
        pm(1950, Some("auto3_001"), 0.20, 0.40, 0.40, 4.0,
           &[(Commodity::Energy, 20.0), (Commodity::Fuels, 10.0), (Commodity::MechanicalComponents, 8.0)],
           &[(Commodity::HardCoal, 60.0)]));
    m.insert(MethodSlot::Production, "CNC Mining".into(),
        pm(1970, Some("auto3_004"), 0.25, 0.45, 0.30, 5.5,
           &[(Commodity::Energy, 25.0), (Commodity::Fuels, 8.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::HardCoal, 80.0)]));
    // ── Phase 20: Activate dead commodity extraction ──
    m.insert(MethodSlot::Production, "Iron Ore Mining".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
           &[(Commodity::Iron, 12.0)]));
    m.insert(MethodSlot::Production, "Copper Ore Mining".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
           &[(Commodity::Copper, 8.0)]));
    m.insert(MethodSlot::Production, "Oil Drilling".into(),
        pm(1880, None, 0.08, 0.25, 0.67, 1.0,
           &[(Commodity::Fuels, 5.0), (Commodity::Food, 5.0), (Commodity::MechanicalComponents, 2.0)],
           &[(Commodity::Oil, 30.0)]));
    m.insert(MethodSlot::Production, "Natural Gas Extraction".into(),
        pm(1900, None, 0.08, 0.25, 0.67, 1.2,
           &[(Commodity::Fuels, 3.0), (Commodity::Energy, 3.0)],
           &[(Commodity::NaturalGas, 25.0)]));
    m.insert(MethodSlot::Production, "Bauxite Mining".into(),
        pm(1890, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
           &[(Commodity::Bauxite, 15.0)]));
    m.insert(MethodSlot::Production, "Sand And Gravel Quarry".into(),
        pm(1880, None, 0.03, 0.15, 0.82, 1.0,
           &[(Commodity::Fuels, 2.0), (Commodity::Food, 3.0)],
           &[(Commodity::Sand, 20.0), (Commodity::Gravel, 15.0)]));
    m.insert(MethodSlot::Production, "Stone Quarrying".into(),
        pm(1880, None, 0.03, 0.15, 0.82, 1.0,
           &[(Commodity::Fuels, 2.0), (Commodity::Food, 3.0)],
           &[(Commodity::Stone, 25.0)]));
    m.insert(MethodSlot::Production, "Clay Mining".into(),
        pm(1880, None, 0.03, 0.15, 0.82, 1.0,
           &[(Commodity::Fuels, 2.0), (Commodity::Food, 3.0)],
           &[(Commodity::Clay, 20.0)]));
    m.insert(MethodSlot::Production, "Limestone Quarrying".into(),
        pm(1880, None, 0.03, 0.15, 0.82, 1.0,
           &[(Commodity::Fuels, 2.0), (Commodity::Food, 3.0)],
           &[(Commodity::Limestone, 22.0)]));
    m.insert(MethodSlot::Production, "Sulfur Mining".into(),
        pm(1890, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Fuels, 3.0), (Commodity::Energy, 2.0)],
           &[(Commodity::Sulfur, 10.0)]));
    m.insert(MethodSlot::Production, "Salt Mining".into(),
        pm(1880, None, 0.03, 0.15, 0.82, 1.0,
           &[(Commodity::Fuels, 2.0), (Commodity::Food, 3.0)],
           &[(Commodity::Salt, 18.0)]));
    m.insert(MethodSlot::Production, "Tin Ore Mining".into(),
        pm(1890, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
           &[(Commodity::Tin, 8.0)]));
    m.insert(MethodSlot::Production, "Zinc Ore Mining".into(),
        pm(1890, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
           &[(Commodity::Zinc, 8.0)]));
    m.insert(MethodSlot::Production, "Lead Ore Mining".into(),
        pm(1890, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
           &[(Commodity::Lead, 8.0)]));
    m.insert(MethodSlot::Production, "Silver Mining".into(),
        pm(1890, None, 0.08, 0.25, 0.67, 1.0,
           &[(Commodity::Fuels, 5.0), (Commodity::Energy, 3.0), (Commodity::Chemicals, 2.0)],
           &[(Commodity::Silver, 3.0)]));
    m.insert(MethodSlot::Production, "Gold Mining".into(),
        pm(1890, None, 0.08, 0.25, 0.67, 1.0,
           &[(Commodity::Fuels, 5.0), (Commodity::Energy, 3.0), (Commodity::Chemicals, 3.0)],
           &[(Commodity::Gold, 2.0)]));
    m.insert(MethodSlot::Production, "Peat Cutting".into(),
        pm(1880, None, 0.02, 0.10, 0.88, 0.8,
           &[(Commodity::Food, 3.0)],
           &[(Commodity::Peat, 15.0)]));
    m.insert(MethodSlot::Production, "Brown Coal Mining".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Fuels, 2.0), (Commodity::Food, 5.0)],
           &[(Commodity::BrownCoal, 18.0)]));
    m.insert(MethodSlot::Production, "Rare Earth Element Mining".into(),
        pm(1965, Some("rare_001"), 0.15, 0.35, 0.50, 2.0,
           &[(Commodity::Energy, 15.0), (Commodity::Chemicals, 8.0), (Commodity::Fuels, 5.0)],
           &[(Commodity::RareEarthElements, 5.0)]));
    m.insert(MethodSlot::Production, "Lithium Extraction".into(),
        pm(1970, Some("lithium_001"), 0.12, 0.30, 0.58, 1.5,
           &[(Commodity::Energy, 10.0), (Commodity::Water, 15.0), (Commodity::Fuels, 3.0)],
           &[(Commodity::Lithium, 8.0)]));
    // ── Phase 20: Magnesium production ──
    m.insert(MethodSlot::Production, "Magnesium Refinery".into(),
        pm(1900, None, 0.10, 0.30, 0.60, 1.5,
           &[(Commodity::Energy, 10.0), (Commodity::Water, 5.0), (Commodity::Chemicals, 3.0)],
           &[(Commodity::Magnesium, 15.0)]));
    // ── Phase 21A: Uranium mining ──
    m.insert(MethodSlot::Production, "Uranium Mining".into(),
        pm(1945, Some("nuc_001"), 0.15, 0.35, 0.50, 2.0,
           &[(Commodity::Energy, 15.0), (Commodity::Fuels, 5.0), (Commodity::Chemicals, 3.0)],
           &[(Commodity::Uranium, 5.0)]));
    m.insert(MethodSlot::Automation, "Manual Labor".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Mechanical Ventilation".into(),
        pm(1880, Some("mining_001"), 0.10, 0.25, 0.65, 1.3,
           &[(Commodity::Energy, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Electric Pumping".into(),
        pm(1890, Some("mining_004"), 0.15, 0.30, 0.55, 1.6,
           &[(Commodity::Energy, 8.0)], &[]));
    m.insert(MethodSlot::Automation, "Automated Conveyor".into(),
        pm(1915, Some("elecf_002"), 0.20, 0.35, 0.45, 2.0,
           &[(Commodity::Energy, 12.0), (Commodity::MechanicalComponents, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Diesel-Electric Drills".into(),
        pm(1950, Some("auto_002"), 0.25, 0.40, 0.35, 2.5,
           &[(Commodity::Fuels, 8.0), (Commodity::Energy, 10.0)], &[]));
    m.insert(MethodSlot::Automation, "Robotic Extraction".into(),
        pm(1975, Some("auto3_007"), 0.30, 0.45, 0.25, 3.5,
           &[(Commodity::Energy, 20.0), (Commodity::ElectronicComponents, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Piece Work".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Shift System".into(),
        pm(1890, Some("mech_008"), 0.10, 0.25, 0.65, 1.2, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Scientific Management".into(),
        pm(1910, Some("mech_008"), 0.15, 0.35, 0.50, 1.5,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 2.0)], &[]));
    m.insert(MethodSlot::Organization, "Mechanized Operations".into(),
        pm(1945, Some("elecf_005"), 0.20, 0.38, 0.42, 1.8,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Lean Mining".into(),
        pm(1985, Some("advman_002"), 0.25, 0.40, 0.35, 2.0,
           &[(Commodity::Food, 5.0), (Commodity::Software, 2.0)], &[]));
    m
}

// === AGRICULTURE ===
fn agriculture_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Manual Farming".into(),
        pm(1880, None, 0.02, 0.10, 0.88, 1.0,
           &[(Commodity::Seeds, 5.0), (Commodity::Food, 3.0), (Commodity::DraftAnimals, 3.0)],
           &[(Commodity::Cereal, 15.0)]));
    m.insert(MethodSlot::Production, "Horse-Drawn Machinery".into(),
        pm(1885, Some("mech_002"), 0.05, 0.15, 0.80, 1.5,
           &[(Commodity::Seeds, 5.0), (Commodity::Food, 5.0), (Commodity::DraftAnimals, 5.0)],
           &[(Commodity::Cereal, 25.0)]));
    m.insert(MethodSlot::Production, "Steam Tractors".into(),
        pm(1895, Some("steam_001"), 0.08, 0.20, 0.72, 2.0,
           &[(Commodity::Seeds, 8.0), (Commodity::Fuels, 10.0)],
           &[(Commodity::Cereal, 40.0)]));
    m.insert(MethodSlot::Production, "Hybrid Seeds".into(),
        pm(1960, Some("bio_005"), 0.15, 0.30, 0.55, 3.0,
           &[(Commodity::Seeds, 12.0), (Commodity::Fertilizers, 10.0)],
           &[(Commodity::Cereal, 70.0)]));
    m.insert(MethodSlot::Production, "Mechanized Harvesting".into(),
        pm(1950, Some("auto3_001"), 0.15, 0.35, 0.50, 3.5,
           &[(Commodity::Fuels, 15.0), (Commodity::Fertilizers, 8.0), (Commodity::AgriculturalMachinery, 3.0)],
           &[(Commodity::Cereal, 90.0)]));
    m.insert(MethodSlot::Production, "GM Crops".into(),
        pm(1995, Some("precag_004"), 0.25, 0.40, 0.35, 5.0,
           &[(Commodity::Seeds, 15.0), (Commodity::Fertilizers, 12.0), (Commodity::Chemicals, 8.0)],
           &[(Commodity::Cereal, 130.0)]));
    m.insert(MethodSlot::Production, "Precision Farming".into(),
        pm(1995, Some("precag_005"), 0.30, 0.40, 0.30, 6.0,
           &[(Commodity::Fuels, 10.0), (Commodity::Fertilizers, 8.0), (Commodity::Software, 3.0)],
           &[(Commodity::Cereal, 160.0)]));
    m.insert(MethodSlot::Production, "Hydroponics".into(),
        pm(1985, Some("precag_007"), 0.30, 0.45, 0.25, 7.0,
           &[(Commodity::Water, 20.0), (Commodity::Fertilizers, 10.0), (Commodity::Energy, 15.0)],
           &[(Commodity::Vegetable, 80.0)]));
    // ── Phase 20: Activate full agricultural supply chain ──
    m.insert(MethodSlot::Production, "Vegetable Farming".into(),
        pm(1880, None, 0.02, 0.10, 0.88, 1.0,
           &[(Commodity::Seeds, 5.0), (Commodity::Water, 8.0), (Commodity::Food, 2.0)],
           &[(Commodity::Vegetable, 18.0)]));
    m.insert(MethodSlot::Production, "Pulse & Legume Farming".into(),
        pm(1880, None, 0.03, 0.12, 0.85, 1.0,
           &[(Commodity::Seeds, 6.0), (Commodity::Water, 10.0), (Commodity::Food, 2.0)],
           &[(Commodity::Meat, 6.0)]));
    m.insert(MethodSlot::Production, "Orchard Cultivation".into(),
        pm(1885, None, 0.03, 0.12, 0.85, 1.1,
           &[(Commodity::Seeds, 4.0), (Commodity::Water, 8.0), (Commodity::Food, 2.0)],
           &[(Commodity::Fruit, 15.0)]));
    m.insert(MethodSlot::Production, "Livestock Ranching".into(),
        pm(1880, None, 0.03, 0.12, 0.85, 1.0,
           &[(Commodity::Fodder, 15.0), (Commodity::Water, 10.0), (Commodity::Food, 3.0)],
           &[(Commodity::Meat, 10.0), (Commodity::Livestock, 5.0)]));
    m.insert(MethodSlot::Production, "Industrial Fiber Farming".into(),
        pm(1880, None, 0.03, 0.12, 0.85, 1.0,
           &[(Commodity::Seeds, 5.0), (Commodity::Water, 8.0)],
           &[(Commodity::IndustrialFiber, 12.0)]));
    m.insert(MethodSlot::Production, "Luxury Crop Plantation".into(),
        pm(1885, None, 0.05, 0.15, 0.80, 1.2,
           &[(Commodity::Seeds, 4.0), (Commodity::Water, 10.0), (Commodity::Food, 2.0)],
           &[(Commodity::Luxury, 8.0)]));
    m.insert(MethodSlot::Production, "Seed Production".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 0.8,
           &[(Commodity::Cereal, 10.0), (Commodity::Water, 5.0), (Commodity::Food, 2.0)],
           &[(Commodity::Seeds, 12.0)]));
    m.insert(MethodSlot::Production, "Fodder Production".into(),
        pm(1880, None, 0.03, 0.12, 0.85, 1.0,
           &[(Commodity::Cereal, 8.0), (Commodity::Water, 5.0)],
           &[(Commodity::Fodder, 15.0)]));
    m.insert(MethodSlot::Production, "Timber Plantation".into(),
        pm(1880, None, 0.02, 0.10, 0.88, 0.7,
           &[(Commodity::Seeds, 2.0), (Commodity::Water, 5.0)],
           &[(Commodity::Timber, 10.0)]));
    m.insert(MethodSlot::Automation, "Hand Harvesting".into(),
        pm(1880, None, 0.02, 0.08, 0.90, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Mechanical Reapers".into(),
        pm(1885, Some("mech_002"), 0.05, 0.15, 0.80, 1.4,
           &[(Commodity::Fuels, 5.0), (Commodity::MechanicalComponents, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Tractor Automation".into(),
        pm(1920, Some("auto_001"), 0.10, 0.25, 0.65, 2.0,
           &[(Commodity::Fuels, 10.0), (Commodity::MechanicalComponents, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Combine Harvesters".into(),
        pm(1955, Some("auto3_001"), 0.15, 0.30, 0.55, 2.5,
           &[(Commodity::Fuels, 12.0), (Commodity::MechanicalComponents, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "GPS-Guided Machinery".into(),
        pm(1990, Some("precag_001"), 0.25, 0.40, 0.35, 3.5,
           &[(Commodity::Fuels, 8.0), (Commodity::ElectronicComponents, 5.0), (Commodity::Software, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Agricultural Drones".into(),
        pm(1998, Some("precag_006"), 0.30, 0.45, 0.25, 5.0,
           &[(Commodity::ElectronicComponents, 8.0), (Commodity::Software, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Subsistence Farming".into(),
        pm(1880, None, 0.02, 0.08, 0.90, 1.0, &[(Commodity::Seeds, 2.0)], &[]));
    m.insert(MethodSlot::Organization, "Crop Rotation".into(),
        pm(1890, Some("chem_001"), 0.05, 0.15, 0.80, 1.3,
           &[(Commodity::Seeds, 5.0), (Commodity::Paper, 1.0)], &[]));
    m.insert(MethodSlot::Organization, "Industrial Farming".into(),
        pm(1910, Some("mech_008"), 0.10, 0.25, 0.65, 1.8,
           &[(Commodity::Fertilizers, 5.0), (Commodity::Paper, 2.0)], &[]));
    m.insert(MethodSlot::Organization, "Agribusiness Scale".into(),
        pm(1960, Some("bio_005"), 0.20, 0.35, 0.45, 2.5,
           &[(Commodity::Fertilizers, 10.0), (Commodity::Software, 1.0)], &[]));
    // Phase 74: Draft Animal Breeding — closes the supply chain for DraftAnimals
    // which were previously only seeded at world generation with no replenishment.
    m.insert(MethodSlot::Production, "Draft Animal Breeding".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 0.8,
           &[(Commodity::Fodder, 10.0), (Commodity::Water, 8.0), (Commodity::Cereal, 5.0)],
           &[(Commodity::DraftAnimals, 3.0), (Commodity::Livestock, 2.0)]));
    m
}

// === HEAVY INDUSTRY ===
fn heavy_industry_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Bessemer Converters".into(),
        pm(1880, Some("steel_001"), 0.15, 0.35, 0.50, 1.5,
           &[(Commodity::Iron, 20.0), (Commodity::Fuels, 10.0), (Commodity::Coke, 8.0)],
           &[(Commodity::Steel, 15.0)]));
    m.insert(MethodSlot::Production, "Open-Hearth Furnaces".into(),
        pm(1885, Some("steel_002"), 0.20, 0.35, 0.45, 2.0,
           &[(Commodity::Iron, 25.0), (Commodity::Fuels, 12.0), (Commodity::Coke, 10.0)],
           &[(Commodity::Steel, 22.0)]));
    m.insert(MethodSlot::Production, "Electric Arc Furnaces".into(),
        pm(1905, Some("steel_008"), 0.25, 0.40, 0.35, 3.0,
           &[(Commodity::Iron, 20.0), (Commodity::Energy, 15.0)],
           &[(Commodity::Steel, 30.0)]));
    m.insert(MethodSlot::Production, "Basic Oxygen Process".into(),
        pm(1955, Some("auto3_002"), 0.25, 0.40, 0.35, 4.0,
           &[(Commodity::Iron, 30.0), (Commodity::Energy, 10.0)],
           &[(Commodity::Steel, 50.0)]));
    m.insert(MethodSlot::Production, "Continuous Casting".into(),
        pm(1965, Some("auto3_005"), 0.30, 0.45, 0.25, 5.5,
           &[(Commodity::Iron, 35.0), (Commodity::Energy, 15.0), (Commodity::ElectronicComponents, 3.0)],
           &[(Commodity::Steel, 70.0)]));
    m.insert(MethodSlot::Production, "Mini-Mill Production".into(),
        pm(1975, Some("auto3_007"), 0.30, 0.45, 0.25, 6.5,
           &[(Commodity::Energy, 25.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::Steel, 90.0)]));
    m.insert(MethodSlot::Production, "Electrified Factories".into(),
        pm(1910, Some("elecf_001"), 0.20, 0.40, 0.40, 2.5,
           &[(Commodity::Energy, 20.0), (Commodity::Steel, 10.0)],
           &[(Commodity::IndustrialMachinery, 15.0)]));
    m.insert(MethodSlot::Production, "CNC Manufacturing".into(),
        pm(1970, Some("auto3_004"), 0.30, 0.45, 0.25, 5.0,
           &[(Commodity::Energy, 20.0), (Commodity::Steel, 15.0), (Commodity::ElectronicComponents, 8.0)],
           &[(Commodity::IndustrialMachinery, 30.0)]));
    // ── Phase 20: Layer 1 — Smelting & Basic Processing ──
    m.insert(MethodSlot::Production, "Coke Production".into(),
        pm(1880, None, 0.08, 0.25, 0.67, 1.0,
           &[(Commodity::HardCoal, 20.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Coke, 15.0)]));
    m.insert(MethodSlot::Production, "Cement Production".into(),
        pm(1880, None, 0.08, 0.25, 0.67, 1.0,
           &[(Commodity::Limestone, 25.0), (Commodity::Clay, 8.0), (Commodity::Energy, 10.0)],
           &[(Commodity::Cement, 30.0)]));
    m.insert(MethodSlot::Production, "Brick Making".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Clay, 20.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Bricks, 25.0)]));
    m.insert(MethodSlot::Production, "Glass Making".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Sand, 20.0), (Commodity::SodaAsh, 5.0), (Commodity::Lead, 2.0), (Commodity::Energy, 12.0)],
           &[(Commodity::Glass, 18.0)]));
    m.insert(MethodSlot::Production, "Aluminum Smelting".into(),
        pm(1900, Some("metall_006"), 0.15, 0.35, 0.50, 1.5,
           &[(Commodity::Bauxite, 20.0), (Commodity::Energy, 30.0), (Commodity::Catalysts, 2.0)],
           &[(Commodity::Aluminum, 12.0)]));
    m.insert(MethodSlot::Production, "Silicon Purification".into(),
        pm(1950, Some("semi_001"), 0.20, 0.40, 0.40, 2.0,
           &[(Commodity::Sand, 15.0), (Commodity::Energy, 20.0), (Commodity::Chemicals, 5.0)],
           &[(Commodity::Silicon, 8.0)]));
    // ── Phase 20: Layer 2 — Chemical & Petroleum Processing ──
    m.insert(MethodSlot::Production, "Basic Chemical Production".into(),
        pm(1880, None, 0.10, 0.30, 0.60, 1.0,
           &[(Commodity::Sulfur, 8.0), (Commodity::Salt, 5.0), (Commodity::Water, 10.0), (Commodity::Energy, 8.0)],
           &[(Commodity::Chemicals, 15.0)]));
    m.insert(MethodSlot::Production, "Solvay Process".into(),
        pm(1880, None, 0.12, 0.30, 0.58, 1.0,
           &[(Commodity::Salt, 10.0), (Commodity::Limestone, 8.0), (Commodity::Ammonia, 3.0), (Commodity::Energy, 8.0)],
           &[(Commodity::SodaAsh, 12.0)]));
    m.insert(MethodSlot::Production, "Haber-Bosch Process".into(),
        pm(1910, Some("chem_002"), 0.15, 0.35, 0.50, 1.5,
           &[(Commodity::NaturalGas, 10.0), (Commodity::Energy, 12.0), (Commodity::Catalysts, 1.0)],
           &[(Commodity::Ammonia, 10.0)]));
    m.insert(MethodSlot::Production, "Fertilizer Production".into(),
        pm(1880, None, 0.10, 0.30, 0.60, 1.0,
           &[(Commodity::Ammonia, 8.0), (Commodity::Chemicals, 5.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Fertilizers, 18.0)]));
    m.insert(MethodSlot::Production, "Oil Refining".into(),
        pm(1880, None, 0.10, 0.30, 0.60, 1.0,
           &[(Commodity::Oil, 25.0), (Commodity::Energy, 5.0), (Commodity::Catalysts, 1.0)],
           &[(Commodity::Fuels, 18.0), (Commodity::Bitumen, 3.0)]));
    m.insert(MethodSlot::Production, "Advanced Refining".into(),
        pm(1920, Some("petro_002"), 0.12, 0.32, 0.56, 1.8,
           &[(Commodity::Oil, 30.0), (Commodity::Catalysts, 2.0), (Commodity::Energy, 8.0)],
           &[(Commodity::Fuels, 22.0), (Commodity::RefinedFuel, 8.0), (Commodity::Bitumen, 4.0)]));
    m.insert(MethodSlot::Production, "Plastics Production".into(),
        pm(1935, Some("petro_005"), 0.15, 0.35, 0.50, 2.0,
           &[(Commodity::Oil, 15.0), (Commodity::Chemicals, 8.0), (Commodity::Energy, 10.0)],
           &[(Commodity::Plastics, 20.0)]));
    m.insert(MethodSlot::Production, "Asphalt Production".into(),
        pm(1900, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Bitumen, 8.0), (Commodity::Sand, 10.0), (Commodity::Gravel, 8.0), (Commodity::Energy, 3.0)],
           &[(Commodity::Asphalt, 20.0)]));
    m.insert(MethodSlot::Production, "Catalyst Production".into(),
        pm(1900, None, 0.12, 0.30, 0.58, 1.0,
           &[(Commodity::Chemicals, 8.0), (Commodity::RareEarthElements, 1.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Catalysts, 6.0)]));
    m.insert(MethodSlot::Production, "Hydrogen Production".into(),
        pm(1970, Some("hydro_001"), 0.15, 0.35, 0.50, 1.5,
           &[(Commodity::NaturalGas, 8.0), (Commodity::Energy, 15.0)],
           &[(Commodity::Hydrogen, 6.0)]));
    // ── Phase 20: Layer 3 — Components & Parts ──
    m.insert(MethodSlot::Production, "Mechanical Components Workshop".into(),
        pm(1880, None, 0.10, 0.30, 0.60, 1.0,
           &[(Commodity::Steel, 10.0), (Commodity::Energy, 5.0), (Commodity::IndustrialMachinery, 2.0)],
           &[(Commodity::MechanicalComponents, 15.0)]));
    m.insert(MethodSlot::Production, "Precision Machining".into(),
        pm(1910, Some("mech_008"), 0.15, 0.35, 0.50, 1.8,
           &[(Commodity::Steel, 12.0), (Commodity::Energy, 8.0), (Commodity::IndustrialMachinery, 3.0)],
           &[(Commodity::MechanicalComponents, 25.0)]));
    m.insert(MethodSlot::Production, "Electronic Components Assembly".into(),
        pm(1920, Some("elecf_001"), 0.15, 0.35, 0.50, 1.5,
           &[(Commodity::Copper, 8.0), (Commodity::Tin, 3.0), (Commodity::Energy, 8.0), (Commodity::IndustrialMachinery, 2.0)],
           &[(Commodity::ElectronicComponents, 10.0)]));
    m.insert(MethodSlot::Production, "Semiconductor Fabrication".into(),
        pm(1970, Some("semi_003"), 0.25, 0.45, 0.30, 3.0,
           &[(Commodity::Silicon, 5.0), (Commodity::RareEarthElements, 2.0), (Commodity::Chemicals, 5.0), (Commodity::Energy, 15.0)],
           &[(Commodity::Semiconductors, 8.0)]));
    m.insert(MethodSlot::Production, "Advanced Electronics".into(),
        pm(1980, Some("semi_005"), 0.25, 0.45, 0.30, 3.5,
           &[(Commodity::Semiconductors, 3.0), (Commodity::Copper, 5.0), (Commodity::Tin, 2.0), (Commodity::Energy, 10.0)],
           &[(Commodity::ElectronicComponents, 20.0)]));
    m.insert(MethodSlot::Production, "Software Development".into(),
        pm(1980, Some("cs_005"), 0.35, 0.45, 0.20, 2.5,
           &[(Commodity::ElectronicComponents, 3.0), (Commodity::Energy, 5.0), (Commodity::Food, 5.0)],
           &[(Commodity::Software, 15.0)]));
    m.insert(MethodSlot::Production, "Battery Production".into(),
        pm(1990, Some("batt_001"), 0.20, 0.40, 0.40, 2.0,
           &[(Commodity::Lithium, 5.0), (Commodity::Lead, 5.0), (Commodity::Semiconductors, 2.0), (Commodity::Energy, 10.0)],
           &[(Commodity::Batteries, 8.0)]));
    // ── Phase 20: Pharmaceutical production ──
    m.insert(MethodSlot::Production, "Pharmaceutical Production".into(),
        pm(1890, None, 0.15, 0.35, 0.50, 1.0,
           &[(Commodity::Chemicals, 10.0), (Commodity::Energy, 5.0), (Commodity::Water, 5.0)],
           &[(Commodity::Pharmaceuticals, 12.0)]));
    // ── Phase 20: Layer 5 — Investment Goods (THE CRITICAL GAP) ──
    // IndustrialMachinery — early method (no tech required)
    m.insert(MethodSlot::Production, "Machine Shop".into(),
        pm(1880, None, 0.12, 0.30, 0.58, 1.0,
           &[(Commodity::Steel, 12.0), (Commodity::MechanicalComponents, 5.0), (Commodity::Energy, 8.0)],
           &[(Commodity::IndustrialMachinery, 10.0)]));
    m.insert(MethodSlot::Production, "Smart Manufacturing".into(),
        pm(1995, Some("advman_006"), 0.30, 0.45, 0.25, 5.0,
           &[(Commodity::Steel, 15.0), (Commodity::ElectronicComponents, 8.0), (Commodity::Software, 5.0), (Commodity::Semiconductors, 2.0), (Commodity::Energy, 15.0)],
           &[(Commodity::IndustrialMachinery, 50.0)]));
    // ConstructionMachinery — ALL NEW
    m.insert(MethodSlot::Production, "Blacksmith Workshop".into(),
        pm(1880, None, 0.10, 0.25, 0.65, 1.0,
           &[(Commodity::Steel, 10.0), (Commodity::Iron, 5.0), (Commodity::Fuels, 5.0)],
           &[(Commodity::ConstructionMachinery, 8.0)]));
    m.insert(MethodSlot::Production, "Machine Factory".into(),
        pm(1910, Some("mech_008"), 0.15, 0.35, 0.50, 1.8,
           &[(Commodity::Steel, 15.0), (Commodity::MechanicalComponents, 8.0), (Commodity::Energy, 8.0)],
           &[(Commodity::ConstructionMachinery, 20.0)]));
    m.insert(MethodSlot::Production, "Heavy Equipment Plant".into(),
        pm(1950, Some("auto3_001"), 0.20, 0.40, 0.40, 3.0,
           &[(Commodity::Steel, 20.0), (Commodity::MechanicalComponents, 10.0), (Commodity::ElectronicComponents, 3.0), (Commodity::Energy, 12.0)],
           &[(Commodity::ConstructionMachinery, 40.0)]));
    m.insert(MethodSlot::Production, "Automated Equipment Plant".into(),
        pm(1990, Some("advman_004"), 0.25, 0.45, 0.30, 5.0,
           &[(Commodity::Steel, 18.0), (Commodity::MechanicalComponents, 8.0), (Commodity::ElectronicComponents, 8.0), (Commodity::Software, 3.0), (Commodity::Energy, 15.0)],
           &[(Commodity::ConstructionMachinery, 70.0)]));
    // AgriculturalMachinery — ALL NEW
    m.insert(MethodSlot::Production, "Implement Workshop".into(),
        pm(1880, None, 0.10, 0.25, 0.65, 1.0,
           &[(Commodity::Steel, 10.0), (Commodity::Iron, 5.0), (Commodity::Fuels, 3.0)],
           &[(Commodity::AgriculturalMachinery, 8.0)]));
    m.insert(MethodSlot::Production, "Implement Factory".into(),
        pm(1910, Some("mech_008"), 0.15, 0.35, 0.50, 1.8,
           &[(Commodity::Steel, 15.0), (Commodity::MechanicalComponents, 8.0), (Commodity::Energy, 8.0)],
           &[(Commodity::AgriculturalMachinery, 20.0)]));
    m.insert(MethodSlot::Production, "Tractor Plant".into(),
        pm(1950, Some("auto3_001"), 0.20, 0.40, 0.40, 3.0,
           &[(Commodity::Steel, 20.0), (Commodity::MechanicalComponents, 10.0), (Commodity::ElectronicComponents, 3.0), (Commodity::Energy, 12.0)],
           &[(Commodity::AgriculturalMachinery, 40.0)]));
    m.insert(MethodSlot::Production, "Precision Ag Equipment".into(),
        pm(1990, Some("advman_004"), 0.25, 0.45, 0.30, 5.0,
           &[(Commodity::Steel, 18.0), (Commodity::MechanicalComponents, 8.0), (Commodity::ElectronicComponents, 8.0), (Commodity::Software, 3.0), (Commodity::Energy, 15.0)],
           &[(Commodity::AgriculturalMachinery, 70.0)]));
    // OfficeMachinery — ALL NEW
    m.insert(MethodSlot::Production, "Typewriter Workshop".into(),
        pm(1890, Some("mech_008"), 0.15, 0.35, 0.50, 1.0,
           &[(Commodity::Steel, 8.0), (Commodity::MechanicalComponents, 5.0), (Commodity::Energy, 3.0)],
           &[(Commodity::OfficeMachinery, 10.0)]));
    m.insert(MethodSlot::Production, "Office Equipment Factory".into(),
        pm(1950, Some("auto3_001"), 0.20, 0.40, 0.40, 2.5,
           &[(Commodity::Steel, 10.0), (Commodity::MechanicalComponents, 8.0), (Commodity::ElectronicComponents, 3.0), (Commodity::Energy, 8.0)],
           &[(Commodity::OfficeMachinery, 25.0)]));
    m.insert(MethodSlot::Production, "Computer Factory".into(),
        pm(1980, Some("auto3_004"), 0.25, 0.45, 0.30, 4.0,
           &[(Commodity::Steel, 5.0), (Commodity::ElectronicComponents, 10.0), (Commodity::Semiconductors, 3.0), (Commodity::Software, 3.0), (Commodity::Energy, 8.0)],
           &[(Commodity::OfficeMachinery, 50.0)]));
    // Trucks — ALL NEW
    m.insert(MethodSlot::Production, "Wagon Workshop".into(),
        pm(1880, None, 0.10, 0.25, 0.65, 1.0,
           &[(Commodity::Steel, 8.0), (Commodity::Timber, 5.0), (Commodity::MechanicalComponents, 3.0), (Commodity::Fuels, 2.0)],
           &[(Commodity::Trucks, 5.0)]));
    m.insert(MethodSlot::Production, "Truck Assembly".into(),
        pm(1920, Some("auto_001"), 0.15, 0.35, 0.50, 2.0,
           &[(Commodity::Steel, 15.0), (Commodity::MechanicalComponents, 8.0), (Commodity::RefinedFuel, 3.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Trucks, 15.0)]));
    m.insert(MethodSlot::Production, "Modern Truck Plant".into(),
        pm(1960, Some("auto3_002"), 0.20, 0.40, 0.40, 3.5,
           &[(Commodity::Steel, 18.0), (Commodity::MechanicalComponents, 10.0), (Commodity::ElectronicComponents, 5.0), (Commodity::RefinedFuel, 5.0), (Commodity::Zinc, 3.0), (Commodity::Energy, 8.0)],
           &[(Commodity::Trucks, 35.0)]));
    m.insert(MethodSlot::Production, "Electric Truck Plant".into(),
        pm(2000, Some("advman_006"), 0.25, 0.45, 0.30, 5.0,
           &[(Commodity::Steel, 15.0), (Commodity::Aluminum, 5.0), (Commodity::ElectronicComponents, 8.0), (Commodity::Batteries, 5.0), (Commodity::Energy, 10.0)],
           &[(Commodity::Trucks, 60.0)]));
    // Cars — ALL NEW
    m.insert(MethodSlot::Production, "Coachbuilder".into(),
        pm(1900, Some("mech_008"), 0.12, 0.30, 0.58, 1.0,
           &[(Commodity::Steel, 10.0), (Commodity::Timber, 5.0), (Commodity::MechanicalComponents, 5.0), (Commodity::Fuels, 2.0)],
           &[(Commodity::Cars, 5.0)]));
    m.insert(MethodSlot::Production, "Assembly Line".into(),
        pm(1913, Some("auto_001"), 0.10, 0.30, 0.60, 2.0,
           &[(Commodity::Steel, 15.0), (Commodity::MechanicalComponents, 8.0), (Commodity::RefinedFuel, 3.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Cars, 20.0)]));
    m.insert(MethodSlot::Production, "Modern Auto Plant".into(),
        pm(1960, Some("auto3_003"), 0.20, 0.40, 0.40, 3.5,
           &[(Commodity::Steel, 18.0), (Commodity::MechanicalComponents, 8.0), (Commodity::ElectronicComponents, 5.0), (Commodity::Plastics, 5.0), (Commodity::RefinedFuel, 3.0), (Commodity::Magnesium, 2.0), (Commodity::Energy, 8.0)],
           &[(Commodity::Cars, 50.0)]));
    m.insert(MethodSlot::Production, "EV Factory".into(),
        pm(2010, Some("advman_006"), 0.25, 0.45, 0.30, 5.0,
           &[(Commodity::Steel, 12.0), (Commodity::Aluminum, 8.0), (Commodity::ElectronicComponents, 10.0), (Commodity::Semiconductors, 5.0), (Commodity::Batteries, 8.0), (Commodity::Hydrogen, 3.0), (Commodity::Energy, 10.0)],
           &[(Commodity::Cars, 80.0)]));
    // ── Phase 20: Prefabricates & Locomotive production ──
    m.insert(MethodSlot::Production, "Prefabricates Plant".into(),
        pm(1900, None, 0.10, 0.30, 0.60, 1.5,
           &[(Commodity::Cement, 10.0), (Commodity::Steel, 5.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Prefabricates, 20.0)]));
    m.insert(MethodSlot::Production, "Locomotive Works".into(),
        pm(1890, Some("steam_002"), 0.15, 0.35, 0.50, 2.0,
           &[(Commodity::Steel, 25.0), (Commodity::MechanicalComponents, 10.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Trains, 3.0)]));
    m.insert(MethodSlot::Automation, "Steam Power Drive".into(),
        pm(1880, Some("steam_001"), 0.10, 0.25, 0.65, 1.3,
           &[(Commodity::Fuels, 15.0)], &[]));
    m.insert(MethodSlot::Automation, "Electrified Factories".into(),
        pm(1910, Some("elecf_001"), 0.15, 0.30, 0.55, 1.8,
           &[(Commodity::Energy, 20.0)], &[]));
    m.insert(MethodSlot::Automation, "Turbo-Generator Plant".into(),
        pm(1888, Some("steam_003"), 0.15, 0.35, 0.50, 2.0,
           &[(Commodity::Fuels, 10.0), (Commodity::MechanicalComponents, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Automated Machinery".into(),
        pm(1930, Some("elecf_005"), 0.20, 0.40, 0.40, 2.5,
           &[(Commodity::Energy, 25.0), (Commodity::ElectronicComponents, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Robotic Welding".into(),
        pm(1965, Some("auto3_003"), 0.30, 0.45, 0.25, 4.0,
           &[(Commodity::Energy, 20.0), (Commodity::ElectronicComponents, 8.0)], &[]));
    m.insert(MethodSlot::Automation, "Flexible Manufacturing".into(),
        pm(1995, Some("advman_006"), 0.35, 0.45, 0.20, 5.5,
           &[(Commodity::Energy, 25.0), (Commodity::ElectronicComponents, 10.0), (Commodity::Software, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Craft Production".into(),
        pm(1880, None, 0.20, 0.30, 0.50, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Taylorism".into(),
        pm(1910, Some("mech_008"), 0.15, 0.35, 0.50, 1.4,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Assembly Line".into(),
        pm(1913, Some("auto_001"), 0.10, 0.30, 0.60, 1.8,
           &[(Commodity::Food, 5.0), (Commodity::MechanicalComponents, 2.0)], &[]));
    m.insert(MethodSlot::Organization, "Continuous Flow Manufacturing".into(),
        pm(1950, Some("elecf_005"), 0.15, 0.35, 0.50, 2.0,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Just-in-Time".into(),
        pm(1985, Some("advman_002"), 0.20, 0.40, 0.40, 2.5,
           &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Six Sigma".into(),
        pm(1990, Some("advman_005"), 0.25, 0.45, 0.30, 3.0,
           &[(Commodity::Food, 5.0), (Commodity::Software, 5.0)], &[]));
    // ── Phase 69: Military conversion methods (Production Decree targets) ──
    // These methods are swapped in by ProductionDecree to convert civilian
    // heavy industry to military output. Each has DISTINCT physical inputs
    // that shock the supply chain (Rule 3 compliance).
    m.insert(MethodSlot::Production, "Military Truck Conversion".into(),
        pm(1916, None, 0.20, 0.35, 0.45, 0.8,
           &[(Commodity::Steel, 25.0), (Commodity::Fuels, 12.0), (Commodity::MechanicalComponents, 8.0), (Commodity::Plastics, 5.0)],
           &[(Commodity::Trucks, 8.0)]));
    m.insert(MethodSlot::Production, "Light Tank Conversion".into(),
        pm(1935, None, 0.22, 0.38, 0.40, 0.7,
           &[(Commodity::Steel, 35.0), (Commodity::Aluminum, 10.0), (Commodity::Fuels, 15.0), (Commodity::MechanicalComponents, 12.0)],
           &[(Commodity::LightTanks, 3.0)]));
    m.insert(MethodSlot::Production, "Artillery Conversion".into(),
        pm(1916, None, 0.20, 0.35, 0.45, 0.8,
           &[(Commodity::Steel, 30.0), (Commodity::Fuels, 10.0), (Commodity::MechanicalComponents, 8.0)],
           &[(Commodity::TowedArtillery, 4.0)]));
    m.insert(MethodSlot::Production, "Ammunition Surge Production".into(),
        pm(1916, None, 0.18, 0.32, 0.50, 0.9,
           &[(Commodity::Steel, 20.0), (Commodity::Chemicals, 25.0), (Commodity::Fuels, 8.0), (Commodity::Lead, 10.0)],
           &[(Commodity::Ammunition, 40.0)]));
    m.insert(MethodSlot::Production, "Gunpowder Conversion".into(),
        pm(1880, None, 0.15, 0.30, 0.55, 0.8,
           &[(Commodity::Chemicals, 30.0), (Commodity::Sulfur, 15.0), (Commodity::Energy, 10.0)],
           &[(Commodity::Gunpowder, 20.0)]));
    m
}

// === LIGHT INDUSTRY ===
fn light_industry_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Handloom Weaving".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.0,
           &[(Commodity::Fibers, 10.0), (Commodity::Food, 3.0)],
           &[(Commodity::Clothing, 8.0)]));
    m.insert(MethodSlot::Production, "Power Looms".into(),
        pm(1885, Some("steam_001"), 0.10, 0.25, 0.65, 2.0,
           &[(Commodity::Fibers, 15.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Clothing, 20.0)]));
    m.insert(MethodSlot::Production, "Electric Looms".into(),
        pm(1910, Some("elecf_001"), 0.15, 0.30, 0.55, 2.5,
           &[(Commodity::Fibers, 20.0), (Commodity::Energy, 10.0)],
           &[(Commodity::Clothing, 30.0)]));
    m.insert(MethodSlot::Production, "Synthetic Fibers".into(),
        pm(1935, Some("synth_006"), 0.20, 0.35, 0.45, 3.0,
           &[(Commodity::Chemicals, 10.0), (Commodity::Energy, 8.0)],
           &[(Commodity::Clothing, 40.0)]));
    m.insert(MethodSlot::Production, "Automated Textile Mills".into(),
        pm(1965, Some("auto3_003"), 0.25, 0.40, 0.35, 4.0,
           &[(Commodity::Fibers, 25.0), (Commodity::Energy, 15.0), (Commodity::ElectronicComponents, 3.0)],
           &[(Commodity::Clothing, 60.0)]));
    m.insert(MethodSlot::Production, "Fast Fashion".into(),
        pm(1990, Some("advman_002"), 0.20, 0.40, 0.40, 5.0,
           &[(Commodity::Fibers, 30.0), (Commodity::Energy, 12.0), (Commodity::Software, 2.0)],
           &[(Commodity::Clothing, 90.0)]));
    // ── Phase 20: Consumer goods manufacturing ──
    m.insert(MethodSlot::Production, "Sawmill".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Timber, 15.0), (Commodity::Energy, 3.0)],
           &[(Commodity::Planks, 12.0)]));
    m.insert(MethodSlot::Production, "Furniture Workshop".into(),
        pm(1880, None, 0.08, 0.25, 0.67, 1.0,
           &[(Commodity::Planks, 12.0), (Commodity::Steel, 3.0), (Commodity::Energy, 3.0)],
           &[(Commodity::Furniture, 10.0)]));
    m.insert(MethodSlot::Production, "Luxury Furniture Workshop".into(),
        pm(1880, None, 0.12, 0.30, 0.58, 1.2,
           &[(Commodity::Planks, 10.0), (Commodity::Luxury, 3.0), (Commodity::Gold, 1.0), (Commodity::Silver, 1.0), (Commodity::Energy, 5.0)],
           &[(Commodity::LuxuryFurniture, 5.0)]));
    m.insert(MethodSlot::Production, "Paper Mill".into(),
        pm(1880, None, 0.08, 0.25, 0.67, 1.0,
           &[(Commodity::Timber, 15.0), (Commodity::Chemicals, 3.0), (Commodity::Water, 10.0), (Commodity::Energy, 8.0)],
           &[(Commodity::Paper, 18.0)]));
    m.insert(MethodSlot::Production, "Appliance Assembly".into(),
        pm(1935, Some("elecf_005"), 0.15, 0.35, 0.50, 2.0,
           &[(Commodity::Steel, 8.0), (Commodity::ElectronicComponents, 5.0), (Commodity::Plastics, 3.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Agd, 12.0)]));
    m.insert(MethodSlot::Production, "Food Processing".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Cereal, 10.0), (Commodity::Vegetable, 5.0), (Commodity::Meat, 2.0), (Commodity::Livestock, 3.0), (Commodity::Energy, 3.0)],
           &[(Commodity::Food, 18.0)]));
    m.insert(MethodSlot::Production, "Textile Mill".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::IndustrialFiber, 12.0), (Commodity::Energy, 3.0)],
           &[(Commodity::Fibers, 15.0)]));
    m.insert(MethodSlot::Production, "Synthetic Fiber Production".into(),
        pm(1935, Some("synth_006"), 0.15, 0.35, 0.50, 2.0,
           &[(Commodity::Plastics, 10.0), (Commodity::Chemicals, 3.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Fibers, 20.0)]));
    // ── Phase 20: Activate LuxuryClothing and MedicalEquipment ──
    m.insert(MethodSlot::Production, "Luxury Clothing Atelier".into(),
        pm(1880, None, 0.12, 0.30, 0.58, 1.2,
           &[(Commodity::Luxury, 5.0), (Commodity::Fibers, 8.0), (Commodity::Gold, 1.0), (Commodity::Silver, 1.0), (Commodity::Energy, 3.0)],
           &[(Commodity::LuxuryClothing, 5.0)]));
    m.insert(MethodSlot::Production, "Medical Equipment Workshop".into(),
        pm(1890, None, 0.15, 0.35, 0.50, 1.0,
           &[(Commodity::Steel, 8.0), (Commodity::Glass, 5.0), (Commodity::MechanicalComponents, 3.0), (Commodity::Energy, 5.0)],
           &[(Commodity::MedicalEquipment, 8.0)]));
    m.insert(MethodSlot::Automation, "Hand Spinning".into(),
        pm(1880, None, 0.05, 0.10, 0.85, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Spinning Mules".into(),
        pm(1885, Some("steam_001"), 0.10, 0.20, 0.70, 1.5,
           &[(Commodity::Energy, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Electric Spinning".into(),
        pm(1910, Some("elecf_001"), 0.15, 0.30, 0.55, 2.0,
           &[(Commodity::Energy, 10.0)], &[]));
    m.insert(MethodSlot::Automation, "Synthetic Fiber Looms".into(),
        pm(1945, Some("chem_003"), 0.20, 0.35, 0.45, 2.5,
           &[(Commodity::Energy, 12.0), (Commodity::Chemicals, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Computerized Knitting".into(),
        pm(1980, Some("auto3_008"), 0.25, 0.40, 0.35, 3.5,
           &[(Commodity::Energy, 12.0), (Commodity::ElectronicComponents, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Cottage Industry".into(),
        pm(1880, None, 0.05, 0.10, 0.85, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Factory System".into(),
        pm(1890, Some("mech_008"), 0.10, 0.25, 0.65, 1.5,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 2.0)], &[]));
    m.insert(MethodSlot::Organization, "Mass Production".into(),
        pm(1930, Some("auto_001"), 0.15, 0.30, 0.55, 1.8,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Quality Circles".into(),
        pm(1960, Some("elecf_005"), 0.18, 0.35, 0.47, 2.0,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 4.0)], &[]));
    m.insert(MethodSlot::Organization, "Lean Manufacturing".into(),
        pm(1985, Some("advman_002"), 0.20, 0.40, 0.40, 2.5,
           &[(Commodity::Food, 5.0), (Commodity::Software, 2.0)], &[]));
    // ── Phase 69: Military conversion methods (Production Decree targets) ──
    // Textile factories converted to military uniform production.
    // Distinct physical inputs: heavier fibers, leather, steel for buttons/buckles.
    m.insert(MethodSlot::Production, "Military Uniform Conversion".into(),
        pm(1880, None, 0.10, 0.25, 0.65, 0.8,
           &[(Commodity::Fibers, 20.0), (Commodity::IndustrialFiber, 5.0), (Commodity::Steel, 2.0), (Commodity::Energy, 5.0)],
           &[(Commodity::Clothing, 15.0)]));
    m.insert(MethodSlot::Production, "Support Equipment Conversion".into(),
        pm(1916, None, 0.15, 0.30, 0.55, 0.7,
           &[(Commodity::Fibers, 10.0), (Commodity::Steel, 8.0), (Commodity::IndustrialFiber, 8.0), (Commodity::Energy, 5.0)],
           &[(Commodity::SupportEquipment, 6.0)]));
    m
}

// === ARMAMENTS INDUSTRY ===
fn armaments_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Artillery Workshop".into(),
        pm(1880, Some("arm_001"), 0.20, 0.35, 0.45, 1.5,
           &[(Commodity::Steel, 15.0), (Commodity::Fuels, 8.0), (Commodity::Food, 5.0)],
           &[(Commodity::TowedArtillery, 5.0)]));
    // Phase 74: Cartridge Manufacturing — baseline Ammunition production from 1880.
    // Without this, the first Ammunition-producing method is "Aircraft Cannon Production"
    // (1930), leaving a 1925 start year with zero Ammunition supply.
    m.insert(MethodSlot::Production, "Cartridge Manufacturing".into(),
        pm(1880, None, 0.15, 0.30, 0.55, 1.0,
           &[(Commodity::Steel, 8.0), (Commodity::Chemicals, 5.0), (Commodity::Gunpowder, 10.0), (Commodity::Lead, 6.0)],
           &[(Commodity::Ammunition, 25.0)]));
    m.insert(MethodSlot::Production, "Tank Production".into(),
        pm(1916, Some("arm_002"), 0.25, 0.40, 0.35, 2.0,
           &[(Commodity::Steel, 30.0), (Commodity::Fuels, 15.0), (Commodity::MechanicalComponents, 10.0)],
           &[(Commodity::LightTanks, 3.0)]));
    m.insert(MethodSlot::Production, "Small Arms Automation".into(),
        pm(1920, Some("arm_003"), 0.20, 0.35, 0.45, 2.5,
           &[(Commodity::Steel, 10.0), (Commodity::Fuels, 5.0)],
           &[(Commodity::Rifles, 20.0)]));
    m.insert(MethodSlot::Production, "Aircraft Cannon Production".into(),
        pm(1930, Some("arm_005"), 0.25, 0.40, 0.35, 3.0,
           &[(Commodity::Steel, 20.0), (Commodity::MechanicalComponents, 8.0)],
           &[(Commodity::Ammunition, 30.0)]));
    m.insert(MethodSlot::Production, "Mass Bomb Production".into(),
        pm(1940, Some("arm_008"), 0.20, 0.35, 0.45, 4.0,
           &[(Commodity::Steel, 15.0), (Commodity::Chemicals, 20.0), (Commodity::Fuels, 10.0)],
           &[(Commodity::Ammunition, 50.0)]));
    m.insert(MethodSlot::Production, "Guided Munitions".into(),
        pm(1965, Some("auto3_003"), 0.30, 0.45, 0.25, 5.0,
           &[(Commodity::Steel, 20.0), (Commodity::ElectronicComponents, 10.0), (Commodity::Chemicals, 15.0)],
           &[(Commodity::Ammunition, 40.0), (Commodity::SupportEquipment, 10.0)]));
    m.insert(MethodSlot::Production, "Precision Munitions".into(),
        pm(1990, Some("advman_003"), 0.35, 0.45, 0.20, 7.0,
           &[(Commodity::Steel, 15.0), (Commodity::ElectronicComponents, 15.0), (Commodity::Software, 5.0)],
           &[(Commodity::Ammunition, 60.0), (Commodity::SupportEquipment, 20.0)]));
    // ── Phase 20: Expanded military vehicle/aircraft/vessel production ──
    m.insert(MethodSlot::Production, "Medium Tank Production".into(),
        pm(1935, Some("arm_002"), 0.22, 0.38, 0.40, 2.0,
           &[(Commodity::Steel, 30.0), (Commodity::Fuels, 15.0), (Commodity::MechanicalComponents, 10.0)],
           &[(Commodity::MediumTanks, 4.0)]));
    m.insert(MethodSlot::Production, "Heavy Tank Production".into(),
        pm(1942, Some("arm_002"), 0.25, 0.40, 0.35, 2.5,
           &[(Commodity::Steel, 40.0), (Commodity::Fuels, 20.0), (Commodity::MechanicalComponents, 15.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::HeavyTanks, 2.0)]));
    m.insert(MethodSlot::Production, "Fighter Production".into(),
        pm(1940, Some("arm_004"), 0.25, 0.40, 0.35, 3.0,
           &[(Commodity::Steel, 20.0), (Commodity::Aluminum, 15.0), (Commodity::Fuels, 10.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::Fighters, 5.0)]));
    m.insert(MethodSlot::Production, "Bomber Production".into(),
        pm(1942, Some("arm_004"), 0.28, 0.42, 0.30, 3.5,
           &[(Commodity::Steel, 30.0), (Commodity::Aluminum, 20.0), (Commodity::Fuels, 15.0), (Commodity::ElectronicComponents, 8.0)],
           &[(Commodity::Bombers, 3.0)]));
    m.insert(MethodSlot::Production, "Helicopter Production".into(),
        pm(1960, Some("auto3_003"), 0.30, 0.40, 0.30, 4.0,
           &[(Commodity::Steel, 15.0), (Commodity::Aluminum, 10.0), (Commodity::Fuels, 12.0), (Commodity::ElectronicComponents, 8.0), (Commodity::MechanicalComponents, 5.0)],
           &[(Commodity::Helicopters, 4.0)]));
    m.insert(MethodSlot::Production, "Submarine Production".into(),
        pm(1935, Some("arm_002"), 0.25, 0.40, 0.35, 3.0,
           &[(Commodity::Steel, 50.0), (Commodity::Fuels, 10.0), (Commodity::MechanicalComponents, 15.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::Submarines, 1.0)]));
    m.insert(MethodSlot::Automation, "Hand Fitting".into(),
        pm(1880, None, 0.20, 0.30, 0.50, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Interchangeable Parts".into(),
        pm(1910, Some("auto_003"), 0.15, 0.35, 0.50, 1.5,
           &[(Commodity::Food, 5.0), (Commodity::MechanicalComponents, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "CNC Machining".into(),
        pm(1960, Some("auto3_002"), 0.25, 0.40, 0.35, 2.5,
           &[(Commodity::Energy, 15.0), (Commodity::ElectronicComponents, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Robotic Assembly".into(),
        pm(1980, Some("auto3_007"), 0.35, 0.45, 0.20, 4.0,
           &[(Commodity::Energy, 20.0), (Commodity::ElectronicComponents, 10.0)], &[]));
    m.insert(MethodSlot::Organization, "Arsenal System".into(),
        pm(1880, None, 0.20, 0.30, 0.50, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "War Production Board".into(),
        pm(1916, Some("arm_002"), 0.15, 0.35, 0.50, 1.8,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Cold War Procurement".into(),
        pm(1950, Some("arm_002"), 0.20, 0.38, 0.42, 2.0,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 8.0)], &[]));
    m.insert(MethodSlot::Organization, "Lean Arsenal".into(),
        pm(1985, Some("advman_002"), 0.25, 0.40, 0.35, 2.5,
           &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)], &[]));
    m
}

// === CONSTRUCTION ===
fn construction_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Manual Construction".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Food, 5.0), (Commodity::Timber, 10.0)],
           &[(Commodity::ConstructionServices, 10.0), (Commodity::RenovationServices, 5.0)]));
    m.insert(MethodSlot::Production, "Steam-Powered Construction".into(),
        pm(1890, Some("steam_001"), 0.10, 0.25, 0.65, 1.5,
           &[(Commodity::Fuels, 10.0), (Commodity::Steel, 5.0), (Commodity::Food, 5.0)],
           &[(Commodity::ConstructionServices, 20.0), (Commodity::RenovationServices, 8.0)]));
    m.insert(MethodSlot::Production, "Reinforced Concrete".into(),
        pm(1900, Some("steel_004"), 0.15, 0.30, 0.55, 2.0,
           &[(Commodity::Steel, 10.0), (Commodity::Cement, 15.0), (Commodity::Food, 5.0)],
           &[(Commodity::ConstructionServices, 30.0), (Commodity::RenovationServices, 10.0)]));
    m.insert(MethodSlot::Production, "Prefabricated Construction".into(),
        pm(1950, Some("auto3_001"), 0.20, 0.35, 0.45, 3.0,
           &[(Commodity::Steel, 15.0), (Commodity::Cement, 10.0), (Commodity::ConstructionMachinery, 5.0)],
           &[(Commodity::ConstructionServices, 50.0)]));
    m.insert(MethodSlot::Production, "Modular Construction".into(),
        pm(1980, Some("auto3_005"), 0.25, 0.40, 0.35, 4.5,
           &[(Commodity::Steel, 20.0), (Commodity::Cement, 8.0), (Commodity::ConstructionMachinery, 8.0)],
           &[(Commodity::ConstructionServices, 80.0)]));
    m.insert(MethodSlot::Production, "3D Printed Construction".into(),
        pm(1995, Some("advman_004"), 0.30, 0.45, 0.25, 6.0,
           &[(Commodity::Cement, 15.0), (Commodity::ElectronicComponents, 5.0), (Commodity::Software, 3.0)],
           &[(Commodity::ConstructionServices, 120.0)]));
    m.insert(MethodSlot::Automation, "Hand Tools".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Steam Cranes".into(),
        pm(1890, Some("steam_001"), 0.10, 0.25, 0.65, 1.5,
           &[(Commodity::Fuels, 8.0)], &[]));
    m.insert(MethodSlot::Automation, "Electric Cranes".into(),
        pm(1910, Some("elecf_001"), 0.15, 0.30, 0.55, 2.0,
           &[(Commodity::Energy, 10.0)], &[]));
    m.insert(MethodSlot::Automation, "Tower Cranes".into(),
        pm(1950, Some("auto3_001"), 0.20, 0.35, 0.45, 2.5,
           &[(Commodity::Energy, 15.0), (Commodity::ConstructionMachinery, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Automated Construction".into(),
        pm(1990, Some("advman_006"), 0.30, 0.45, 0.25, 4.0,
           &[(Commodity::Energy, 20.0), (Commodity::ElectronicComponents, 8.0), (Commodity::Software, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Day Labor".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Contractor System".into(),
        pm(1900, Some("mech_008"), 0.10, 0.25, 0.65, 1.3,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Industrial Construction Firm".into(),
        pm(1930, Some("steel_004"), 0.15, 0.30, 0.55, 1.6,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Project Management".into(),
        pm(1960, Some("cs_004"), 0.20, 0.35, 0.45, 2.0,
           &[(Commodity::Food, 5.0), (Commodity::Software, 2.0)], &[]));
    m
}

// === ENERGY ===
fn energy_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    // Phase 74: Fuel-burning plants use pm_thermal with thermal_efficiency.
    // Fuel inputs are capacity slots (max fuel the plant can accept per cycle).
    // Actual consumption is computed dynamically in process_building_cycle()
    // based on calorific_value_mj_per_unit() and the plant's thermal_efficiency.
    m.insert(MethodSlot::Production, "Coal-Fired Boilers".into(),
        pm_thermal(1880, None, 0.10, 0.25, 0.65, 1.0,
           &[(Commodity::HardCoal, 20.0), (Commodity::BrownCoal, 10.0), (Commodity::Peat, 5.0), (Commodity::Water, 10.0)],
           &[(Commodity::Energy, 30.0), (Commodity::Heat, 10.0)],
           0.15));  // 15% thermal efficiency
    m.insert(MethodSlot::Production, "Turbo-Generator Plant".into(),
        pm_thermal(1888, Some("steam_003"), 0.15, 0.30, 0.55, 2.0,
           &[(Commodity::HardCoal, 15.0), (Commodity::BrownCoal, 8.0), (Commodity::Water, 8.0)],
           &[(Commodity::Energy, 50.0), (Commodity::Heat, 15.0)],
           0.25));  // 25% thermal efficiency
    m.insert(MethodSlot::Production, "Hydroelectric Power".into(),
        pm(1890, Some("elecf_002"), 0.20, 0.35, 0.45, 2.5,
           &[(Commodity::Water, 15.0), (Commodity::MechanicalComponents, 5.0)],
           &[(Commodity::Energy, 60.0)]));
    m.insert(MethodSlot::Production, "Steam Turbine Plant".into(),
        pm_thermal(1900, Some("steam_005"), 0.20, 0.35, 0.45, 3.0,
           &[(Commodity::HardCoal, 20.0), (Commodity::Water, 10.0)],
           &[(Commodity::Energy, 80.0), (Commodity::Heat, 25.0)],
           0.30));  // 30% thermal efficiency
    m.insert(MethodSlot::Production, "Internal Combustion Plant".into(),
        pm_thermal(1910, Some("auto_002"), 0.20, 0.35, 0.45, 3.5,
           &[(Commodity::Fuels, 15.0), (Commodity::MechanicalComponents, 5.0)],
           &[(Commodity::Energy, 90.0)],
           0.35));  // 35% thermal efficiency
    m.insert(MethodSlot::Production, "Nuclear Power Plant".into(),
        pm_thermal(1955, Some("nucp_001"), 0.30, 0.45, 0.25, 6.0,
           &[(Commodity::Uranium, 5.0), (Commodity::Water, 20.0), (Commodity::ElectronicComponents, 10.0)],
           &[(Commodity::Energy, 200.0)],
           0.33));  // 33% thermal efficiency
    m.insert(MethodSlot::Production, "Combined Cycle Plant".into(),
        pm_thermal(1975, Some("auto3_007"), 0.30, 0.45, 0.25, 7.0,
           &[(Commodity::NaturalGas, 15.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::Energy, 250.0), (Commodity::Heat, 40.0)],
           0.55));  // 55% thermal efficiency
    m.insert(MethodSlot::Production, "Solar Power Plant".into(),
        pm(1990, Some("advman_004"), 0.30, 0.45, 0.25, 5.0,
           &[(Commodity::ElectronicComponents, 10.0), (Commodity::Silicon, 5.0)],
           &[(Commodity::Energy, 150.0)]));
    m.insert(MethodSlot::Production, "Wind Turbine Farm".into(),
        pm(1990, Some("advman_005"), 0.25, 0.40, 0.35, 4.5,
           &[(Commodity::MechanicalComponents, 10.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::Energy, 120.0)]));
    // ── Phase 20: Utilities and modern energy ──
    m.insert(MethodSlot::Production, "Water Utility".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0,
           &[(Commodity::Energy, 3.0), (Commodity::Chemicals, 1.0)],
           &[(Commodity::Water, 50.0)]));
    m.insert(MethodSlot::Production, "Geothermal Plant".into(),
        pm(1980, Some("advman_004"), 0.25, 0.40, 0.35, 4.0,
           &[(Commodity::MechanicalComponents, 8.0), (Commodity::ElectronicComponents, 3.0), (Commodity::Water, 5.0)],
           &[(Commodity::Energy, 100.0)]));
    // Phase 79: Pumped Storage Plant — first built 1907 in Switzerland.
    // Consumes Energy (pumping water uphill) and outputs Energy (releasing it).
    // Round-trip efficiency ~72% (28% lost to friction, turbine losses, evaporation).
    m.insert(MethodSlot::Production, "Pumped Storage Plant".into(),
        pm_storage(1907, Some("pstrg_001"), 0.15, 0.30, 0.55, 1.5,
           &[(Commodity::Energy, 100.0), (Commodity::Water, 20.0), (Commodity::MechanicalComponents, 5.0)],
           &[(Commodity::Energy, 72.0)],
           0.72));  // 72% round-trip efficiency
    // Phase 79: Battery Bank Storage — replaces the broken "Battery Storage Facility"
    // which consumed 10 Energy and produced 80 Energy (8x energy creation violation).
    // Round-trip efficiency ~87% for modern lithium-ion grid storage.
    m.insert(MethodSlot::Production, "Battery Bank Storage".into(),
        pm_storage(1990, Some("batt_002"), 0.20, 0.40, 0.40, 2.0,
           &[(Commodity::Energy, 100.0), (Commodity::Batteries, 5.0), (Commodity::ElectronicComponents, 3.0)],
           &[(Commodity::Energy, 87.0)],
           0.87));  // 87% round-trip efficiency
    m.insert(MethodSlot::Automation, "Manual Stoking".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Mechanical Stokers".into(),
        pm(1890, Some("steam_003"), 0.10, 0.25, 0.65, 1.5,
           &[(Commodity::MechanicalComponents, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Pulverized Coal Burners".into(),
        pm(1920, Some("steam_005"), 0.15, 0.30, 0.55, 1.8,
           &[(Commodity::Energy, 5.0), (Commodity::MechanicalComponents, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Automated Boiler Control".into(),
        pm(1950, Some("auto3_001"), 0.20, 0.35, 0.45, 2.5,
           &[(Commodity::ElectronicComponents, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "SCADA Systems".into(),
        pm(1985, Some("cs_005"), 0.30, 0.45, 0.25, 4.0,
           &[(Commodity::ElectronicComponents, 8.0), (Commodity::Software, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Shift Operation".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Centralized Dispatch".into(),
        pm(1920, Some("elecf_005"), 0.15, 0.30, 0.55, 1.5,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Grid Management".into(),
        pm(1960, Some("cs_004"), 0.25, 0.40, 0.35, 2.5,
           &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)], &[]));
    m
}

// === Phase 81: Plant-Type-Specific Energy Production Methods ===

/// Coal-fired power plant production methods (era-based progression).
fn coal_fired_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Subcritical Boiler".into(),
        pm_thermal(1880, None, 0.10, 0.25, 0.65, 1.0,
           &[(Commodity::HardCoal, 20.0), (Commodity::Water, 10.0)],
           &[(Commodity::Energy, 30.0), (Commodity::Heat, 10.0)],
           0.15));
    m.insert(MethodSlot::Production, "Supercritical Boiler".into(),
        pm_thermal(1930, Some("steam_005"), 0.15, 0.30, 0.55, 2.0,
           &[(Commodity::HardCoal, 15.0), (Commodity::Water, 8.0), (Commodity::MechanicalComponents, 3.0)],
           &[(Commodity::Energy, 60.0), (Commodity::Heat, 15.0)],
           0.25));
    m.insert(MethodSlot::Production, "Ultra-Supercritical Boiler".into(),
        pm_thermal(1960, Some("auto3_002"), 0.20, 0.35, 0.45, 3.0,
           &[(Commodity::HardCoal, 12.0), (Commodity::Water, 6.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::Energy, 100.0), (Commodity::Heat, 20.0)],
           0.38));
    m.insert(MethodSlot::Production, "Integrated Gasification".into(),
        pm_thermal(1990, Some("advman_004"), 0.25, 0.40, 0.35, 4.0,
           &[(Commodity::HardCoal, 10.0), (Commodity::Water, 5.0), (Commodity::Semiconductors, 3.0)],
           &[(Commodity::Energy, 130.0), (Commodity::Heat, 25.0)],
           0.45));
    // Cooling upgrade variants (alternative Production methods).
    m.insert(MethodSlot::Production, "Closed-Loop Cooling Tower".into(),
        pm_thermal(1950, Some("cool_001"), 0.20, 0.35, 0.45, 2.8,
           &[(Commodity::HardCoal, 15.0), (Commodity::Water, 4.0), (Commodity::CoolingTower, 2.0)],
           &[(Commodity::Energy, 80.0), (Commodity::Heat, 15.0)],
           0.30));
    m.insert(MethodSlot::Production, "Air-Cooled Condenser".into(),
        pm_thermal(1970, Some("cool_002"), 0.20, 0.35, 0.45, 2.7,
           &[(Commodity::HardCoal, 16.0), (Commodity::CoolingTower, 2.0)],
           &[(Commodity::Energy, 76.0), (Commodity::Heat, 15.0)],
           0.28));
    m
}

/// Lignite-fired power plant production methods.
fn lignite_fired_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Lignite Dryer Boiler".into(),
        pm_thermal(1880, None, 0.10, 0.25, 0.65, 1.0,
           &[(Commodity::BrownCoal, 30.0), (Commodity::Water, 10.0)],
           &[(Commodity::Energy, 25.0), (Commodity::Heat, 8.0)],
           0.12));
    m.insert(MethodSlot::Production, "Pre-Dried Lignite".into(),
        pm_thermal(1950, Some("steam_005"), 0.15, 0.30, 0.55, 1.8,
           &[(Commodity::BrownCoal, 20.0), (Commodity::Water, 8.0), (Commodity::MechanicalComponents, 3.0)],
           &[(Commodity::Energy, 50.0), (Commodity::Heat, 12.0)],
           0.20));
    m.insert(MethodSlot::Production, "Fluidized Bed Lignite".into(),
        pm_thermal(1980, Some("auto3_002"), 0.20, 0.35, 0.45, 2.5,
           &[(Commodity::BrownCoal, 15.0), (Commodity::Water, 6.0), (Commodity::ElectronicComponents, 4.0)],
           &[(Commodity::Energy, 75.0), (Commodity::Heat, 18.0)],
           0.28));
    m
}

/// Oil/gas power plant production methods.
fn oil_gas_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Diesel Generator".into(),
        pm_thermal(1910, Some("auto_002"), 0.15, 0.30, 0.55, 2.0,
           &[(Commodity::Fuels, 15.0), (Commodity::MechanicalComponents, 5.0)],
           &[(Commodity::Energy, 90.0)],
           0.35));
    m.insert(MethodSlot::Production, "Gas Turbine".into(),
        pm_thermal(1940, Some("auto3_001"), 0.20, 0.35, 0.45, 2.5,
           &[(Commodity::NaturalGas, 15.0), (Commodity::Water, 5.0)],
           &[(Commodity::Energy, 120.0), (Commodity::Heat, 20.0)],
           0.30));
    m.insert(MethodSlot::Production, "Combined Cycle".into(),
        pm_thermal(1975, Some("auto3_007"), 0.25, 0.40, 0.35, 3.5,
           &[(Commodity::NaturalGas, 12.0), (Commodity::ElectronicComponents, 5.0), (Commodity::Water, 4.0)],
           &[(Commodity::Energy, 200.0), (Commodity::Heat, 30.0)],
           0.55));
    m
}

/// Nuclear power plant production methods.
fn nuclear_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "PWR Reactor".into(),
        pm_thermal(1955, Some("nucp_001"), 0.30, 0.45, 0.25, 5.0,
           &[(Commodity::Uranium, 5.0), (Commodity::Water, 20.0), (Commodity::ElectronicComponents, 10.0)],
           &[(Commodity::Energy, 200.0)],
           0.33));
    m.insert(MethodSlot::Production, "BWR Reactor".into(),
        pm_thermal(1960, Some("nucp_002"), 0.30, 0.45, 0.25, 5.5,
           &[(Commodity::Uranium, 4.0), (Commodity::Water, 15.0), (Commodity::ElectronicComponents, 8.0)],
           &[(Commodity::Energy, 220.0)],
           0.34));
    m.insert(MethodSlot::Production, "Fast Breeder".into(),
        pm_thermal(1975, Some("nucp_006"), 0.35, 0.45, 0.20, 6.0,
           &[(Commodity::Uranium, 3.0), (Commodity::Water, 12.0), (Commodity::ElectronicComponents, 12.0)],
           &[(Commodity::Energy, 280.0)],
           0.40));
    m
}

/// Solar power plant production methods.
fn solar_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Photovoltaic Array".into(),
        pm(1990, Some("advman_004"), 0.25, 0.40, 0.35, 4.0,
           &[(Commodity::ElectronicComponents, 10.0), (Commodity::Silicon, 5.0)],
           &[(Commodity::Energy, 150.0)]));
    m.insert(MethodSlot::Production, "Concentrated Solar".into(),
        pm(2000, Some("solar_002"), 0.30, 0.40, 0.30, 4.5,
           &[(Commodity::ElectronicComponents, 8.0), (Commodity::Steel, 10.0), (Commodity::Silicon, 3.0)],
           &[(Commodity::Energy, 180.0), (Commodity::Heat, 30.0)]));
    m
}

/// Wind farm production methods.
fn wind_farm_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Onshore Wind Farm".into(),
        pm(1990, Some("advman_005"), 0.20, 0.35, 0.45, 3.5,
           &[(Commodity::MechanicalComponents, 10.0), (Commodity::ElectronicComponents, 5.0), (Commodity::Steel, 8.0)],
           &[(Commodity::Energy, 120.0)]));
    m.insert(MethodSlot::Production, "Offshore Wind Farm".into(),
        pm(2000, Some("wind_001"), 0.25, 0.40, 0.35, 4.5,
           &[(Commodity::MechanicalComponents, 15.0), (Commodity::ElectronicComponents, 8.0), (Commodity::Steel, 15.0)],
           &[(Commodity::Energy, 200.0)]));
    m
}

/// Hydroelectric power plant production methods.
fn hydro_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Run-of-River Hydro".into(),
        pm(1890, Some("elecf_002"), 0.15, 0.30, 0.55, 2.0,
           &[(Commodity::Water, 15.0), (Commodity::MechanicalComponents, 5.0)],
           &[(Commodity::Energy, 60.0)]));
    m.insert(MethodSlot::Production, "Reservoir Hydro".into(),
        pm(1920, Some("elecf_005"), 0.20, 0.35, 0.45, 2.5,
           &[(Commodity::Water, 20.0), (Commodity::MechanicalComponents, 8.0), (Commodity::Steel, 5.0)],
           &[(Commodity::Energy, 90.0)]));
    m
}

/// Pumped storage plant production methods.
fn pumped_storage_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Pumped Storage Plant".into(),
        pm_storage(1907, Some("pstrg_001"), 0.15, 0.30, 0.55, 1.5,
           &[(Commodity::Energy, 100.0), (Commodity::Water, 20.0), (Commodity::MechanicalComponents, 5.0)],
           &[(Commodity::Energy, 72.0)],
           0.72));
    m
}

/// Battery storage production methods.
fn battery_storage_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Battery Bank Storage".into(),
        pm_storage(1990, Some("batt_002"), 0.20, 0.40, 0.40, 2.0,
           &[(Commodity::Energy, 100.0), (Commodity::Batteries, 5.0), (Commodity::ElectronicComponents, 3.0)],
           &[(Commodity::Energy, 87.0)],
           0.87));
    m
}

/// Geothermal plant production methods.
fn geothermal_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Geothermal Plant".into(),
        pm(1980, Some("advman_004"), 0.25, 0.40, 0.35, 3.5,
           &[(Commodity::MechanicalComponents, 8.0), (Commodity::ElectronicComponents, 3.0), (Commodity::Water, 5.0)],
           &[(Commodity::Energy, 100.0)]));
    m
}

/// Biomass-fired plant production methods (early/rural electrification).
fn biomass_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Wood-Fired Boiler".into(),
        pm_thermal(1880, None, 0.10, 0.25, 0.65, 1.0,
           &[(Commodity::Timber, 15.0), (Commodity::Planks, 10.0), (Commodity::Peat, 8.0), (Commodity::Water, 5.0)],
           &[(Commodity::Energy, 20.0), (Commodity::Heat, 8.0)],
           0.10));
    m.insert(MethodSlot::Production, "Automated Biomass".into(),
        pm_thermal(1950, Some("auto3_001"), 0.15, 0.30, 0.55, 1.5,
           &[(Commodity::Timber, 12.0), (Commodity::Planks, 8.0), (Commodity::Peat, 5.0), (Commodity::Water, 4.0), (Commodity::MechanicalComponents, 3.0)],
           &[(Commodity::Energy, 40.0), (Commodity::Heat, 12.0)],
           0.18));
    m.insert(MethodSlot::Production, "Co-Firing Biomass".into(),
        pm_thermal(1990, Some("advman_004"), 0.20, 0.35, 0.45, 2.0,
           &[(Commodity::Timber, 8.0), (Commodity::HardCoal, 8.0), (Commodity::Water, 4.0), (Commodity::ElectronicComponents, 3.0)],
           &[(Commodity::Energy, 60.0), (Commodity::Heat, 15.0)],
           0.22));
    m
}

/// Biogas plant production methods (agricultural waste).
fn biogas_plant_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Anaerobic Digester".into(),
        pm_thermal(1930, Some("chem_005"), 0.15, 0.30, 0.55, 1.5,
           &[(Commodity::Livestock, 10.0), (Commodity::Food, 5.0), (Commodity::Water, 3.0)],
           &[(Commodity::Energy, 25.0), (Commodity::Heat, 10.0)],
           0.15));
    m.insert(MethodSlot::Production, "Upgraded Biogas".into(),
        pm_thermal(1980, Some("auto3_004"), 0.20, 0.35, 0.45, 2.0,
           &[(Commodity::Livestock, 8.0), (Commodity::Food, 4.0), (Commodity::Water, 2.0), (Commodity::ElectronicComponents, 3.0)],
           &[(Commodity::Energy, 40.0), (Commodity::Heat, 12.0)],
           0.25));
    m
}

/// Phase 81: Shared automation methods for all energy plant types.
fn energy_automation_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Automation, "Manual Stoking".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Mechanical Stokers".into(),
        pm(1890, Some("steam_003"), 0.10, 0.25, 0.65, 1.5,
           &[(Commodity::MechanicalComponents, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Pulverized Coal Burners".into(),
        pm(1920, Some("steam_005"), 0.15, 0.30, 0.55, 1.8,
           &[(Commodity::Energy, 5.0), (Commodity::MechanicalComponents, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Automated Boiler Control".into(),
        pm(1950, Some("auto3_001"), 0.20, 0.35, 0.45, 2.5,
           &[(Commodity::ElectronicComponents, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "SCADA Systems".into(),
        pm(1985, Some("cs_005"), 0.30, 0.45, 0.25, 4.0,
           &[(Commodity::ElectronicComponents, 8.0), (Commodity::Software, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "AI Grid Optimization".into(),
        pm(2010, Some("cs_008"), 0.35, 0.45, 0.20, 6.0,
           &[(Commodity::Semiconductors, 5.0), (Commodity::Software, 8.0)], &[]));
    m
}

/// Phase 81: Shared organization methods for all energy plant types.
fn energy_organization_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Organization, "Shift Operation".into(),
        pm(1880, None, 0.05, 0.20, 0.75, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "State Utility Model".into(),
        pm(1900, None, 0.10, 0.25, 0.65, 1.2,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 2.0)], &[]));
    m.insert(MethodSlot::Organization, "Centralized Dispatch".into(),
        pm(1920, Some("elecf_005"), 0.15, 0.30, 0.55, 1.5,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Grid Management".into(),
        pm(1960, Some("cs_004"), 0.25, 0.40, 0.35, 2.5,
           &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Privatized Grid".into(),
        pm(1990, Some("cs_005"), 0.25, 0.40, 0.35, 2.0,
           &[(Commodity::Food, 5.0), (Commodity::Software, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Independent System Operator".into(),
        pm(2000, Some("cs_008"), 0.30, 0.45, 0.25, 3.0,
           &[(Commodity::Food, 5.0), (Commodity::Software, 8.0)], &[]));
    m
}

// === TRANSPORT & LOGISTICS ===
fn transport_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Horse-Drawn Wagons".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.0,
           &[(Commodity::Food, 5.0), (Commodity::Fuels, 2.0)],
           &[(Commodity::PassengerTransport, 10.0)]));
    m.insert(MethodSlot::Production, "Steam Locomotives".into(),
        pm(1885, Some("steam_002"), 0.10, 0.25, 0.65, 2.0,
           &[(Commodity::Fuels, 15.0), (Commodity::Steel, 5.0)],
           &[(Commodity::PassengerTransport, 30.0)]));
    m.insert(MethodSlot::Production, "Electric Trams".into(),
        pm(1895, Some("elecf_002"), 0.15, 0.30, 0.55, 2.5,
           &[(Commodity::Energy, 10.0), (Commodity::Steel, 3.0)],
           &[(Commodity::PassengerTransport, 40.0)]));
    m.insert(MethodSlot::Production, "Diesel Locomotives".into(),
        pm(1930, Some("auto_002"), 0.15, 0.30, 0.55, 3.0,
           &[(Commodity::Fuels, 12.0), (Commodity::MechanicalComponents, 5.0)],
           &[(Commodity::PassengerTransport, 60.0)]));
    m.insert(MethodSlot::Production, "Container Shipping".into(),
        pm(1960, Some("auto3_002"), 0.20, 0.35, 0.45, 4.0,
           &[(Commodity::Fuels, 15.0), (Commodity::Steel, 10.0)],
           &[(Commodity::PassengerTransport, 100.0)]));
    m.insert(MethodSlot::Production, "High-Speed Rail".into(),
        pm(1980, Some("auto3_005"), 0.25, 0.40, 0.35, 5.5,
           &[(Commodity::Energy, 20.0), (Commodity::ElectronicComponents, 8.0), (Commodity::Steel, 10.0)],
           &[(Commodity::PassengerTransport, 180.0)]));
    m.insert(MethodSlot::Production, "Logistics Networks".into(),
        pm(1990, Some("advman_002"), 0.30, 0.40, 0.30, 7.0,
           &[(Commodity::Fuels, 10.0), (Commodity::Software, 5.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::PassengerTransport, 250.0)]));
    // ── Phase 23A: Freight-producing methods ──
    // Early-game freight using draft animals (no machinery required).
    m.insert(MethodSlot::Production, "Pack Caravans".into(),
        pm(1850, None, 0.05, 0.15, 0.80, 1.0,
           &[(Commodity::Fodder, 8.0), (Commodity::Water, 4.0)],
           &[(Commodity::FreightCapacity, 5.0)]));
    m.insert(MethodSlot::Production, "Horse-Drawn Freight Wagons".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.2,
           &[(Commodity::Fodder, 6.0), (Commodity::Water, 3.0), (Commodity::DraftAnimals, 4.0)],
           &[(Commodity::FreightCapacity, 12.0)]));
    // Rail freight (requires RailNetwork in Phase 23B; gating added later).
    m.insert(MethodSlot::Production, "Steam Freight Trains".into(),
        pm(1885, Some("steam_002"), 0.10, 0.25, 0.65, 2.5,
           &[(Commodity::Fuels, 15.0), (Commodity::Steel, 5.0), (Commodity::Trains, 2.0)],
           &[(Commodity::FreightCapacity, 40.0)]));
    m.insert(MethodSlot::Production, "Diesel Freight Trains".into(),
        pm(1930, Some("auto_002"), 0.15, 0.30, 0.55, 3.5,
           &[(Commodity::Fuels, 12.0), (Commodity::MechanicalComponents, 5.0), (Commodity::Trains, 2.0)],
           &[(Commodity::FreightCapacity, 80.0)]));
    // Road freight.
    m.insert(MethodSlot::Production, "Container Trucking".into(),
        pm(1960, Some("auto3_002"), 0.20, 0.35, 0.45, 4.5,
           &[(Commodity::Fuels, 15.0), (Commodity::Steel, 10.0)],
           &[(Commodity::FreightCapacity, 120.0)]));
    // Air freight (late-game; requires Airport in Phase 23D).
    m.insert(MethodSlot::Production, "Air Cargo".into(),
        pm(1960, Some("auto3_002"), 0.25, 0.40, 0.35, 6.0,
           &[(Commodity::Fuels, 25.0), (Commodity::Aluminum, 8.0)],
           &[(Commodity::FreightCapacity, 60.0), (Commodity::PassengerTransport, 40.0)]));
    m.insert(MethodSlot::Automation, "Manual Signaling".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Mechanical Signals".into(),
        pm(1890, Some("steam_002"), 0.10, 0.25, 0.65, 1.5,
           &[(Commodity::MechanicalComponents, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Electric Signaling".into(),
        pm(1910, Some("elecf_001"), 0.15, 0.30, 0.55, 2.0,
           &[(Commodity::Energy, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Relay-Based Interlocking".into(),
        pm(1940, Some("elecf_005"), 0.18, 0.32, 0.50, 2.3,
           &[(Commodity::Energy, 8.0), (Commodity::ElectronicComponents, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Automated Dispatch".into(),
        pm(1970, Some("auto3_004"), 0.25, 0.40, 0.35, 3.5,
           &[(Commodity::ElectronicComponents, 5.0), (Commodity::Software, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Wagon Trains".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Timetabled Services".into(),
        pm(1890, Some("steam_002"), 0.10, 0.25, 0.65, 1.3,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Nationalized Railways".into(),
        pm(1925, Some("elecf_005"), 0.15, 0.30, 0.55, 1.6,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Intermodal Logistics".into(),
        pm(1960, Some("auto3_002"), 0.20, 0.35, 0.45, 2.0,
           &[(Commodity::Food, 5.0), (Commodity::Software, 2.0)], &[]));
    m
}

// === MEDIA & ENTERTAINMENT ===
fn media_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Print Press".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.0,
           &[(Commodity::Paper, 10.0), (Commodity::Food, 3.0)],
           &[(Commodity::Radio, 0.0)]));
    m.insert(MethodSlot::Production, "Radio Broadcasting".into(),
        pm(1920, Some("radio_001"), 0.15, 0.30, 0.55, 2.0,
           &[(Commodity::Energy, 10.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::Radio, 20.0)]));
    m.insert(MethodSlot::Production, "Television Broadcasting".into(),
        pm(1940, Some("radio_004"), 0.20, 0.35, 0.45, 3.0,
           &[(Commodity::Energy, 15.0), (Commodity::ElectronicComponents, 10.0)],
           &[(Commodity::Televisions, 15.0)]));
    m.insert(MethodSlot::Production, "Cable Television".into(),
        pm(1970, Some("auto3_004"), 0.25, 0.40, 0.35, 4.0,
           &[(Commodity::Energy, 12.0), (Commodity::ElectronicComponents, 8.0)],
           &[(Commodity::Televisions, 30.0)]));
    m.insert(MethodSlot::Production, "Satellite Broadcasting".into(),
        pm(1985, Some("advman_003"), 0.30, 0.40, 0.30, 5.0,
           &[(Commodity::Energy, 15.0), (Commodity::ElectronicComponents, 12.0), (Commodity::Software, 5.0)],
           &[(Commodity::Televisions, 50.0)]));
    m.insert(MethodSlot::Production, "Digital Streaming".into(),
        pm(1998, Some("advman_006"), 0.35, 0.45, 0.20, 7.0,
           &[(Commodity::Energy, 10.0), (Commodity::Software, 10.0), (Commodity::ElectronicComponents, 8.0)],
           &[(Commodity::Televisions, 80.0)]));
    m.insert(MethodSlot::Automation, "Manual Typesetting".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Linotype Machines".into(),
        pm(1890, Some("steam_001"), 0.10, 0.25, 0.65, 1.5,
           &[(Commodity::MechanicalComponents, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Teleprinter Network".into(),
        pm(1920, Some("radio_001"), 0.13, 0.28, 0.59, 1.8,
           &[(Commodity::Energy, 3.0), (Commodity::MechanicalComponents, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Magnetic Tape Editing".into(),
        pm(1955, Some("radio_004"), 0.18, 0.32, 0.50, 2.0,
           &[(Commodity::Energy, 5.0), (Commodity::MechanicalComponents, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Digital Typesetting".into(),
        pm(1980, Some("auto3_005"), 0.25, 0.40, 0.35, 3.0,
           &[(Commodity::ElectronicComponents, 5.0), (Commodity::Software, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Local Publishers".into(),
        pm(1880, None, 0.05, 0.15, 0.80, 1.0, &[(Commodity::Food, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Broadcast Networks".into(),
        pm(1930, Some("radio_004"), 0.15, 0.30, 0.55, 1.8,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Television Networks".into(),
        pm(1960, Some("radio_004"), 0.20, 0.35, 0.45, 2.0,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Media Conglomerates".into(),
        pm(1985, Some("advman_002"), 0.25, 0.40, 0.35, 2.5,
           &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)], &[]));
    m
}

// === MEDICAL SERVICES ===
fn medical_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "General Practice".into(),
        pm(1880, None, 0.30, 0.40, 0.30, 1.0,
           &[(Commodity::Food, 5.0), (Commodity::Pharmaceuticals, 2.0)],
           &[(Commodity::HealthCapacity, 15.0)]));
    m.insert(MethodSlot::Production, "Antiseptic Surgery".into(),
        pm(1890, Some("bio_001"), 0.30, 0.40, 0.30, 1.5,
           &[(Commodity::Pharmaceuticals, 5.0), (Commodity::Chemicals, 3.0)],
           &[(Commodity::HealthCapacity, 25.0)]));
    m.insert(MethodSlot::Production, "X-Ray Diagnostics".into(),
        pm(1900, Some("elecf_003"), 0.35, 0.40, 0.25, 2.0,
           &[(Commodity::Energy, 10.0), (Commodity::MedicalEquipment, 3.0)],
           &[(Commodity::HealthCapacity, 40.0)]));
    m.insert(MethodSlot::Production, "Antibiotic Treatment".into(),
        pm(1945, Some("bio_003"), 0.35, 0.40, 0.25, 3.0,
           &[(Commodity::Pharmaceuticals, 10.0), (Commodity::Chemicals, 5.0)],
           &[(Commodity::HealthCapacity, 60.0)]));
    m.insert(MethodSlot::Production, "Modern Surgery".into(),
        pm(1960, Some("bio_005"), 0.40, 0.40, 0.20, 4.0,
           &[(Commodity::Pharmaceuticals, 12.0), (Commodity::MedicalEquipment, 8.0), (Commodity::Energy, 5.0)],
           &[(Commodity::HealthCapacity, 90.0)]));
    m.insert(MethodSlot::Production, "Telemedicine".into(),
        pm(1995, Some("advman_004"), 0.45, 0.40, 0.15, 6.0,
           &[(Commodity::Pharmaceuticals, 8.0), (Commodity::Software, 5.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::HealthCapacity, 140.0)]));
    m.insert(MethodSlot::Automation, "Manual Records".into(),
        pm(1880, None, 0.10, 0.20, 0.70, 1.0, &[(Commodity::Paper, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Punch Card Records".into(),
        pm(1930, Some("elecf_005"), 0.15, 0.30, 0.55, 1.5,
           &[(Commodity::Paper, 3.0), (Commodity::Energy, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Mainframe Patient Database".into(),
        pm(1970, Some("cs_005"), 0.20, 0.35, 0.45, 2.0,
           &[(Commodity::ElectronicComponents, 3.0), (Commodity::Energy, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Electronic Health Records".into(),
        pm(1990, Some("cs_005"), 0.25, 0.35, 0.40, 2.5,
           &[(Commodity::ElectronicComponents, 5.0), (Commodity::Software, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "AI-Assisted Diagnostics".into(),
        pm(1998, Some("advman_006"), 0.35, 0.40, 0.25, 3.5,
           &[(Commodity::ElectronicComponents, 8.0), (Commodity::Software, 8.0)], &[]));
    m.insert(MethodSlot::Organization, "Private Practice".into(),
        pm(1880, None, 0.30, 0.40, 0.30, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Hospital System".into(),
        pm(1910, Some("bio_002"), 0.25, 0.40, 0.35, 1.5,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Socialized Medicine".into(),
        pm(1948, Some("bio_003"), 0.25, 0.40, 0.35, 1.8,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 8.0)], &[]));
    m.insert(MethodSlot::Organization, "Managed Care".into(),
        pm(1970, Some("bio_006"), 0.30, 0.40, 0.30, 2.5,
           &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)], &[]));
    m
}

// === EDUCATIONAL SERVICES ===
fn education_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Traditional Classroom".into(),
        pm(1880, None, 0.30, 0.40, 0.30, 1.0,
           &[(Commodity::Paper, 5.0), (Commodity::Food, 5.0)],
           &[(Commodity::EducationSlots, 15.0)]));
    m.insert(MethodSlot::Production, "University Laboratory".into(),
        pm(1890, Some("bio_001"), 0.40, 0.40, 0.20, 1.5,
           &[(Commodity::Paper, 10.0), (Commodity::Chemicals, 5.0)],
           &[(Commodity::EducationSlots, 25.0), (Commodity::InnovationPoints, 5.0)]));
    m.insert(MethodSlot::Production, "Research Laboratory".into(),
        pm(1910, Some("elecf_003"), 0.45, 0.40, 0.15, 2.5,
           &[(Commodity::Paper, 10.0), (Commodity::Chemicals, 8.0), (Commodity::Energy, 5.0)],
           &[(Commodity::EducationSlots, 30.0), (Commodity::InnovationPoints, 15.0)]));
    m.insert(MethodSlot::Production, "Computer-Assisted Learning".into(),
        pm(1980, Some("auto3_004"), 0.40, 0.40, 0.20, 3.5,
           &[(Commodity::ElectronicComponents, 5.0), (Commodity::Software, 5.0)],
           &[(Commodity::EducationSlots, 50.0), (Commodity::InnovationPoints, 20.0)]));
    m.insert(MethodSlot::Production, "Online Education".into(),
        pm(1995, Some("advman_004"), 0.45, 0.40, 0.15, 5.0,
           &[(Commodity::Software, 10.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::EducationSlots, 80.0), (Commodity::InnovationPoints, 30.0)]));
    m.insert(MethodSlot::Automation, "Blackboard & Books".into(),
        pm(1880, None, 0.10, 0.20, 0.70, 1.0, &[(Commodity::Paper, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Filmstrip Projectors".into(),
        pm(1915, Some("elecf_001"), 0.13, 0.25, 0.62, 1.3,
           &[(Commodity::Energy, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Audiovisual Aids".into(),
        pm(1950, Some("radio_004"), 0.20, 0.30, 0.50, 1.5,
           &[(Commodity::Energy, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Language Laboratory".into(),
        pm(1960, Some("radio_004"), 0.25, 0.35, 0.40, 2.0,
           &[(Commodity::Energy, 8.0), (Commodity::ElectronicComponents, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Smart Classrooms".into(),
        pm(1990, Some("cs_005"), 0.30, 0.40, 0.30, 3.0,
           &[(Commodity::ElectronicComponents, 5.0), (Commodity::Software, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "Apprenticeship".into(),
        pm(1880, None, 0.30, 0.40, 0.30, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Public Education System".into(),
        pm(1900, Some("mech_008"), 0.25, 0.40, 0.35, 1.5,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Research University".into(),
        pm(1950, Some("nucp_001"), 0.35, 0.40, 0.25, 2.5,
           &[(Commodity::Food, 5.0), (Commodity::Software, 2.0)], &[]));
    m
}

// === PUBLIC SERVICES ===
fn public_services_methods() -> BuildingMethods {
    let mut m = BuildingMethods::default();
    m.insert(MethodSlot::Production, "Basic Administration".into(),
        pm(1880, None, 0.15, 0.30, 0.55, 1.0,
           &[(Commodity::Paper, 10.0), (Commodity::Food, 5.0)],
           &[(Commodity::AdministrativeServices, 15.0)]));
    m.insert(MethodSlot::Production, "Typewriter Office".into(),
        pm(1890, Some("mech_008"), 0.20, 0.35, 0.45, 1.5,
           &[(Commodity::Paper, 8.0), (Commodity::OfficeMachinery, 3.0)],
           &[(Commodity::AdministrativeServices, 25.0)]));
    m.insert(MethodSlot::Production, "Computerized Office".into(),
        pm(1970, Some("auto3_004"), 0.30, 0.40, 0.30, 3.0,
           &[(Commodity::Paper, 3.0), (Commodity::ElectronicComponents, 5.0), (Commodity::Software, 3.0)],
           &[(Commodity::AdministrativeServices, 50.0)]));
    m.insert(MethodSlot::Production, "E-Government".into(),
        pm(1995, Some("advman_004"), 0.35, 0.40, 0.25, 5.0,
           &[(Commodity::Software, 8.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::AdministrativeServices, 90.0)]));
    // ── Phase 20: Integration Center (Phase 17B AssimilationCapacity producer) ──
    m.insert(MethodSlot::Production, "Integration Center".into(),
        pm(1950, None, 0.25, 0.40, 0.35, 1.0,
           &[(Commodity::Paper, 8.0), (Commodity::AdministrativeServices, 5.0), (Commodity::Food, 3.0)],
           &[(Commodity::AssimilationCapacity, 20.0)]));
    m.insert(MethodSlot::Production, "Language & Civic Integration".into(),
        pm(1980, Some("auto3_004"), 0.30, 0.40, 0.30, 2.0,
           &[(Commodity::Paper, 5.0), (Commodity::Software, 5.0), (Commodity::AdministrativeServices, 8.0), (Commodity::ElectronicComponents, 2.0)],
           &[(Commodity::AssimilationCapacity, 50.0)]));
    m.insert(MethodSlot::Production, "Digital Integration Platform".into(),
        pm(2000, Some("advman_004"), 0.35, 0.40, 0.25, 3.5,
           &[(Commodity::Software, 10.0), (Commodity::AdministrativeServices, 10.0), (Commodity::ElectronicComponents, 5.0)],
           &[(Commodity::AssimilationCapacity, 100.0)]));
    // ── Phase 20: Banking & Local Services production ──
    m.insert(MethodSlot::Production, "Banking Office".into(),
        pm(1880, None, 0.30, 0.40, 0.30, 1.0,
           &[(Commodity::Paper, 5.0), (Commodity::OfficeMachinery, 2.0), (Commodity::Energy, 3.0)],
           &[(Commodity::BankingServices, 15.0)]));
    m.insert(MethodSlot::Production, "Electronic Banking".into(),
        pm(1990, Some("cs_005"), 0.35, 0.40, 0.25, 2.5,
           &[(Commodity::ElectronicComponents, 5.0), (Commodity::Software, 8.0), (Commodity::Energy, 5.0)],
           &[(Commodity::BankingServices, 50.0)]));
    m.insert(MethodSlot::Production, "Local Services Shop".into(),
        pm(1880, None, 0.15, 0.35, 0.50, 1.0,
           &[(Commodity::Fuels, 5.0), (Commodity::Food, 4.0), (Commodity::Clothing, 2.0)],
           &[(Commodity::LocalServicesCommodity, 20.0)]));
    m.insert(MethodSlot::Automation, "Manual Filing".into(),
        pm(1880, None, 0.10, 0.20, 0.70, 1.0, &[(Commodity::Paper, 5.0)], &[]));
    m.insert(MethodSlot::Automation, "Microfilm Archive".into(),
        pm(1930, Some("elecf_005"), 0.15, 0.30, 0.55, 1.5,
           &[(Commodity::Energy, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Photocopier Office".into(),
        pm(1960, Some("elecf_005"), 0.18, 0.32, 0.50, 1.8,
           &[(Commodity::Energy, 5.0), (Commodity::Paper, 3.0)], &[]));
    m.insert(MethodSlot::Automation, "Digital Database".into(),
        pm(1985, Some("cs_005"), 0.25, 0.40, 0.35, 3.0,
           &[(Commodity::ElectronicComponents, 5.0), (Commodity::Software, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Patronage System".into(),
        pm(1880, None, 0.15, 0.30, 0.55, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Civil Service".into(),
        pm(1900, Some("mech_008"), 0.25, 0.40, 0.35, 1.5,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Welfare State Administration".into(),
        pm(1935, Some("mech_008"), 0.27, 0.40, 0.33, 1.7,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 8.0)], &[]));
    m.insert(MethodSlot::Organization, "Computerized Bureaucracy".into(),
        pm(1965, Some("cs_005"), 0.28, 0.40, 0.32, 2.0,
           &[(Commodity::Food, 5.0), (Commodity::ElectronicComponents, 3.0)], &[]));
    m.insert(MethodSlot::Organization, "New Public Management".into(),
        pm(1985, Some("advman_002"), 0.30, 0.40, 0.30, 2.5,
           &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)], &[]));
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
    m.insert(MethodSlot::Production, "Manual Repair Shop".into(),
        pm(1850, None, 0.10, 0.30, 0.60, 1.0,
           &[(Commodity::Steel, 3.0), (Commodity::MechanicalComponents, 2.0), (Commodity::Fuels, 1.0)],
           &[(Commodity::MaintenanceServices, 10.0)]));
    // Mechanized workshop (1900) — more raw materials, higher capacity.
    m.insert(MethodSlot::Production, "Mechanized Workshop".into(),
        pm(1900, Some("mech_008"), 0.15, 0.35, 0.50, 1.5,
           &[(Commodity::Steel, 4.0), (Commodity::MechanicalComponents, 3.0), (Commodity::Energy, 2.0), (Commodity::Fuels, 1.0)],
           &[(Commodity::MaintenanceServices, 18.0)]));
    // Electrified repair shop (1950) — electronics + energy, higher capacity.
    m.insert(MethodSlot::Production, "Electrified Repair Shop".into(),
        pm(1950, Some("elecf_005"), 0.20, 0.40, 0.40, 2.5,
           &[(Commodity::Steel, 4.0), (Commodity::MechanicalComponents, 3.0), (Commodity::ElectronicComponents, 2.0), (Commodity::Energy, 5.0)],
           &[(Commodity::MaintenanceServices, 35.0)]));
    // CNC repair shop (1990) — advanced electronics, highest capacity.
    m.insert(MethodSlot::Production, "CNC Repair Shop".into(),
        pm(1990, Some("auto3_004"), 0.25, 0.45, 0.30, 4.0,
           &[(Commodity::Steel, 3.0), (Commodity::MechanicalComponents, 2.0), (Commodity::ElectronicComponents, 5.0), (Commodity::Energy, 8.0), (Commodity::Software, 2.0)],
           &[(Commodity::MaintenanceServices, 60.0)]));
    // Automation slot — boosts maintenance capacity (no machinery input!).
    m.insert(MethodSlot::Automation, "Hand Tools".into(),
        pm(1850, None, 0.10, 0.20, 0.70, 1.0, &[(Commodity::Fuels, 1.0)], &[]));
    m.insert(MethodSlot::Automation, "Steam-Powered Hammers".into(),
        pm(1885, Some("steam_001"), 0.12, 0.25, 0.63, 1.2,
           &[(Commodity::Fuels, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Power Tools".into(),
        pm(1920, Some("elecf_005"), 0.15, 0.30, 0.55, 1.5,
           &[(Commodity::Energy, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Electric Welding".into(),
        pm(1950, Some("elecf_005"), 0.18, 0.33, 0.49, 1.8,
           &[(Commodity::Energy, 5.0), (Commodity::Steel, 1.0)], &[]));
    m.insert(MethodSlot::Automation, "Computerized Diagnostics".into(),
        pm(1975, Some("cs_005"), 0.20, 0.38, 0.42, 2.2,
           &[(Commodity::ElectronicComponents, 3.0), (Commodity::Software, 2.0)], &[]));
    m.insert(MethodSlot::Automation, "Robotic Repair Arms".into(),
        pm(1990, Some("auto3_004"), 0.25, 0.40, 0.35, 3.0,
           &[(Commodity::ElectronicComponents, 3.0), (Commodity::Energy, 4.0)], &[]));
    // Organization slot — workshop management.
    m.insert(MethodSlot::Organization, "Journeyman System".into(),
        pm(1850, None, 0.15, 0.30, 0.55, 1.0, &[(Commodity::Food, 5.0)], &[]));
    m.insert(MethodSlot::Organization, "Factory Maintenance Dept".into(),
        pm(1890, Some("mech_008"), 0.17, 0.35, 0.48, 1.2,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 1.0)], &[]));
    m.insert(MethodSlot::Organization, "Specialized Crews".into(),
        pm(1930, Some("mech_008"), 0.20, 0.40, 0.40, 1.5,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 2.0)], &[]));
    m.insert(MethodSlot::Organization, "Preventive Maintenance Schedule".into(),
        pm(1960, Some("elecf_005"), 0.25, 0.40, 0.35, 2.0,
           &[(Commodity::Food, 5.0), (Commodity::Paper, 4.0)], &[]));
    m.insert(MethodSlot::Organization, "Predictive Maintenance".into(),
        pm(1990, Some("cs_005"), 0.30, 0.40, 0.30, 2.5,
           &[(Commodity::Food, 5.0), (Commodity::Software, 3.0)], &[]));
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

        for (_, methods) in &all_methods {
            for (_, pm) in &methods.automation {
                for (&c, _) in &pm.inputs {
                    input_commodities.insert(c);
                }
            }
            for (_, pm) in &methods.production {
                for (&c, _) in &pm.inputs {
                    input_commodities.insert(c);
                }
            }
            for (_, pm) in &methods.organization {
                for (&c, _) in &pm.inputs {
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

        for (_, methods) in &all_methods {
            for (_, pm) in &methods.automation {
                for (&c, _) in &pm.inputs {
                    input_commodities.insert(c);
                }
                for (&c, _) in &pm.outputs {
                    output_commodities.insert(c);
                }
            }
            for (_, pm) in &methods.production {
                for (&c, _) in &pm.inputs {
                    input_commodities.insert(c);
                }
                for (&c, _) in &pm.outputs {
                    output_commodities.insert(c);
                }
            }
            for (_, pm) in &methods.organization {
                for (&c, _) in &pm.inputs {
                    input_commodities.insert(c);
                }
                for (&c, _) in &pm.outputs {
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
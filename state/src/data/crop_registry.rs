//! Static crop registry for compile-time safe agricultural data
//!
//! This module provides a compile-time initialized crop registry that replaces
//! JSON-based data loading for improved type safety and performance.
//!
//! Stabilization Sprint: All crop names are in English (Rule 12). Added
//! livestock, orchard, and tobacco crop definitions to cover the full
//! agriculture production matrix.

use crate::registries::crops::{CropDefinition, CropCategory, LandType, TurnRange, LaborDemandProfile};
use crate::society::geography::ClimateProfile;
use crate::registries::enums::Commodity;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Global static crop registry
///
/// Returns a reference to the crop registry, initializing it on first call.
/// This uses OnceLock for thread-safe lazy initialization.
pub fn crop_registry() -> &'static HashMap<String, CropDefinition> {
    static REGISTRY: OnceLock<HashMap<String, CropDefinition>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut crops = HashMap::new();

        // Cereals
        // Emergency Stabilization: Shifted for September-start calendar.
        // Sowing: March-April (turns 13-15), Harvest: September-October (turns 1-3).
        crops.insert("wheat".to_string(), CropDefinition {
            id: "wheat".to_string(),
            name: "Wheat".to_string(),
            category: CropCategory::Cereal,
            land_type: LandType::Arable,
            compatible_climates: vec![ClimateProfile::Temperate, ClimateProfile::Continental],
            sowing_schedule: TurnRange { start_turn: 13, end_turn: 15 },
            harvest_schedule: TurnRange { start_turn: 1, end_turn: 3 },
            labor_demand: LaborDemandProfile {
                sowing_fte_per_hectare: 0.12,
                growing_fte_per_hectare: 0.04,
                harvesting_fte_per_hectare: 0.18,
            },
            yields: {
                let mut map = HashMap::new();
                map.insert(Commodity::Cereal, 4.5);
                map.insert(Commodity::Fodder, 2.0);  // Straw for fodder
                map
            },
            seed_cost_per_hectare: 150.0,
            seed_commodity: Commodity::Seeds,
            seed_quantity_per_hectare: 0.05,
            sowing_wage_multiplier: 1.5,
            harvesting_wage_multiplier: 2.8,
        });

        crops.insert("corn".to_string(), CropDefinition {
            id: "corn".to_string(),
            name: "Corn".to_string(),
            category: CropCategory::Cereal,
            land_type: LandType::Arable,
            compatible_climates: vec![ClimateProfile::Temperate, ClimateProfile::Continental],
            sowing_schedule: TurnRange { start_turn: 13, end_turn: 15 },
            harvest_schedule: TurnRange { start_turn: 1, end_turn: 3 },
            labor_demand: LaborDemandProfile {
                sowing_fte_per_hectare: 0.12,
                growing_fte_per_hectare: 0.04,
                harvesting_fte_per_hectare: 0.18,
            },
            yields: {
                let mut map = HashMap::new();
                map.insert(Commodity::Cereal, 5.5);  // Grain
                map.insert(Commodity::Fodder, 8.2);  // Stalks/silage
                map
            },
            seed_cost_per_hectare: 180.0,
            seed_commodity: Commodity::Seeds,
            seed_quantity_per_hectare: 0.06,
            sowing_wage_multiplier: 1.5,
            harvesting_wage_multiplier: 2.8,
        });

        // Vegetables
        // Emergency Stabilization: Shifted for September-start calendar.
        // Sowing: March-April (turns 13-15), Harvest: September-October (turns 1-3).
        crops.insert("potatoes".to_string(), CropDefinition {
            id: "potatoes".to_string(),
            name: "Potatoes".to_string(),
            category: CropCategory::Root,
            land_type: LandType::Arable,
            compatible_climates: vec![ClimateProfile::Temperate, ClimateProfile::Continental],
            sowing_schedule: TurnRange { start_turn: 13, end_turn: 15 },
            harvest_schedule: TurnRange { start_turn: 1, end_turn: 3 },
            labor_demand: LaborDemandProfile {
                sowing_fte_per_hectare: 0.15,
                growing_fte_per_hectare: 0.05,
                harvesting_fte_per_hectare: 0.25,
            },
            yields: {
                let mut map = HashMap::new();
                map.insert(Commodity::Vegetable, 25.0);
                map
            },
            seed_cost_per_hectare: 200.0,
            seed_commodity: Commodity::Seeds,
            seed_quantity_per_hectare: 2.0, // Seed potatoes are bulky
            sowing_wage_multiplier: 1.4,
            harvesting_wage_multiplier: 2.5,
        });

        // Industrial crops
        crops.insert("cotton".to_string(), CropDefinition {
            id: "cotton".to_string(),
            name: "Cotton".to_string(),
            category: CropCategory::Industrial,
            land_type: LandType::Plantation,
            compatible_climates: vec![ClimateProfile::Tropical, ClimateProfile::Coastal],
            sowing_schedule: TurnRange { start_turn: 1, end_turn: 2 },
            harvest_schedule: TurnRange { start_turn: 11, end_turn: 14 },
            labor_demand: LaborDemandProfile {
                sowing_fte_per_hectare: 0.0,  // Plantation skips sowing
                growing_fte_per_hectare: 0.02,
                harvesting_fte_per_hectare: 0.20,
            },
            yields: {
                let mut map = HashMap::new();
                map.insert(Commodity::IndustrialFiber, 1.8);  // Lint
                map.insert(Commodity::Fodder, 3.5);  // Cottonseed meal
                map
            },
            seed_cost_per_hectare: 0.0,  // Plantation established once
            seed_commodity: Commodity::Seeds,
            seed_quantity_per_hectare: 0.0, // Plantation skips sowing
            sowing_wage_multiplier: 1.0,
            harvesting_wage_multiplier: 3.0,
        });

        // Fodder
        // Emergency Stabilization: Shifted for September-start calendar.
        // Sowing: January-February (turns 11-13), Harvest: May-August (turns 20-24).
        crops.insert("alfalfa".to_string(), CropDefinition {
            id: "alfalfa".to_string(),
            name: "Alfalfa".to_string(),
            category: CropCategory::Fodder,
            land_type: LandType::Arable,
            compatible_climates: vec![ClimateProfile::Temperate, ClimateProfile::Continental],
            sowing_schedule: TurnRange { start_turn: 11, end_turn: 13 },
            harvest_schedule: TurnRange { start_turn: 20, end_turn: 24 },
            labor_demand: LaborDemandProfile {
                sowing_fte_per_hectare: 0.08,
                growing_fte_per_hectare: 0.02,
                harvesting_fte_per_hectare: 0.10,
            },
            yields: {
                let mut map = HashMap::new();
                map.insert(Commodity::Fodder, 12.0);
                map
            },
            seed_cost_per_hectare: 80.0,
            seed_commodity: Commodity::Seeds,
            seed_quantity_per_hectare: 0.04,
            sowing_wage_multiplier: 1.2,
            harvesting_wage_multiplier: 1.8,
        });

        // Stabilization Sprint: Livestock (cattle ranching)
        // Pasture-based, yields Meat + Livestock. Plantation land type
        // (perennial pasture, no annual sowing).
        crops.insert("cattle".to_string(), CropDefinition {
            id: "cattle".to_string(),
            name: "Cattle".to_string(),
            category: CropCategory::Fodder, // Uses fodder as feed input
            land_type: LandType::Plantation,
            compatible_climates: vec![
                ClimateProfile::Temperate,
                ClimateProfile::Continental,
                ClimateProfile::Coastal,
            ],
            sowing_schedule: TurnRange { start_turn: 1, end_turn: 2 },
            harvest_schedule: TurnRange { start_turn: 6, end_turn: 20 },
            labor_demand: LaborDemandProfile {
                sowing_fte_per_hectare: 0.0,  // Plantation skips sowing
                growing_fte_per_hectare: 0.03,
                harvesting_fte_per_hectare: 0.08,
            },
            yields: {
                let mut map = HashMap::new();
                map.insert(Commodity::Meat, 0.8);
                map.insert(Commodity::Livestock, 0.3);
                map
            },
            seed_cost_per_hectare: 0.0,
            seed_commodity: Commodity::Seeds,
            seed_quantity_per_hectare: 0.0,
            sowing_wage_multiplier: 1.0,
            harvesting_wage_multiplier: 2.0,
        });

        // Stabilization Sprint: Orchard (fruit trees)
        // Plantation land type (perennial, no annual sowing).
        crops.insert("orchard".to_string(), CropDefinition {
            id: "orchard".to_string(),
            name: "Orchard".to_string(),
            category: CropCategory::Orchard,
            land_type: LandType::Plantation,
            compatible_climates: vec![
                ClimateProfile::Temperate,
                ClimateProfile::Continental,
                ClimateProfile::Coastal,
            ],
            sowing_schedule: TurnRange { start_turn: 1, end_turn: 2 },
            harvest_schedule: TurnRange { start_turn: 1, end_turn: 4 },
            labor_demand: LaborDemandProfile {
                sowing_fte_per_hectare: 0.0,
                growing_fte_per_hectare: 0.03,
                harvesting_fte_per_hectare: 0.15,
            },
            yields: {
                let mut map = HashMap::new();
                map.insert(Commodity::Fruit, 12.0);
                map
            },
            seed_cost_per_hectare: 0.0,
            seed_commodity: Commodity::Seeds,
            seed_quantity_per_hectare: 0.0,
            sowing_wage_multiplier: 1.0,
            harvesting_wage_multiplier: 2.5,
        });

        // Stabilization Sprint: Tobacco (luxury plantation crop)
        crops.insert("tobacco".to_string(), CropDefinition {
            id: "tobacco".to_string(),
            name: "Tobacco".to_string(),
            category: CropCategory::Industrial,
            land_type: LandType::Plantation,
            compatible_climates: vec![
                ClimateProfile::Tropical,
                ClimateProfile::Coastal,
                ClimateProfile::Temperate,
            ],
            sowing_schedule: TurnRange { start_turn: 1, end_turn: 2 },
            harvest_schedule: TurnRange { start_turn: 1, end_turn: 4 },
            labor_demand: LaborDemandProfile {
                sowing_fte_per_hectare: 0.0,
                growing_fte_per_hectare: 0.04,
                harvesting_fte_per_hectare: 0.22,
            },
            yields: {
                let mut map = HashMap::new();
                map.insert(Commodity::Luxury, 1.5);
                map
            },
            seed_cost_per_hectare: 0.0,
            seed_commodity: Commodity::Seeds,
            seed_quantity_per_hectare: 0.0,
            sowing_wage_multiplier: 1.0,
            harvesting_wage_multiplier: 3.0,
        });

        crops
    })
}

//! Static crop registry for compile-time safe agricultural data
//!
//! This module provides a compile-time initialized crop registry that replaces
//! JSON-based data loading for improved type safety and performance.
//!
//! Stabilization Sprint: All crop names are in English (Rule 12). Added
//! livestock, orchard, and tobacco crop definitions to cover the full
//! agriculture production matrix.

use crate::registries::crops::{
    CropCategory, CropDefinition, LaborDemandProfile, LandType, TurnRange,
};
use crate::registries::enums::Commodity;
use crate::society::geography::ClimateProfile;
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
        crops.insert(
            "wheat".to_string(),
            CropDefinition {
                id: "wheat".to_string(),
                name: "Wheat".to_string(),
                category: CropCategory::Cereal,
                land_type: LandType::Arable,
                compatible_climates: vec![ClimateProfile::Temperate, ClimateProfile::Continental],
                sowing_schedule: TurnRange {
                    start_turn: 13,
                    end_turn: 15,
                },
                harvest_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 3,
                },
                labor_demand: LaborDemandProfile {
                    sowing_fte_per_hectare: 0.12,
                    growing_fte_per_hectare: 0.04,
                    harvesting_fte_per_hectare: 0.18,
                },
                yields: {
                    let mut map = HashMap::new();
                    map.insert(Commodity::Cereal, 4.5);
                    map.insert(Commodity::Fodder, 2.0); // Straw for fodder
                    map
                },
                seed_cost_per_hectare: 150.0,
                seed_commodity: Commodity::Seeds,
                seed_quantity_per_hectare: 0.05,
                sowing_wage_multiplier: 1.5,
                harvesting_wage_multiplier: 2.8,
            },
        );

        crops.insert(
            "corn".to_string(),
            CropDefinition {
                id: "corn".to_string(),
                name: "Corn".to_string(),
                category: CropCategory::Cereal,
                land_type: LandType::Arable,
                compatible_climates: vec![ClimateProfile::Temperate, ClimateProfile::Continental],
                sowing_schedule: TurnRange {
                    start_turn: 13,
                    end_turn: 15,
                },
                harvest_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 3,
                },
                labor_demand: LaborDemandProfile {
                    sowing_fte_per_hectare: 0.12,
                    growing_fte_per_hectare: 0.04,
                    harvesting_fte_per_hectare: 0.18,
                },
                yields: {
                    let mut map = HashMap::new();
                    map.insert(Commodity::Cereal, 5.5); // Grain
                    map.insert(Commodity::Fodder, 8.2); // Stalks/silage
                    map
                },
                seed_cost_per_hectare: 180.0,
                seed_commodity: Commodity::Seeds,
                seed_quantity_per_hectare: 0.06,
                sowing_wage_multiplier: 1.5,
                harvesting_wage_multiplier: 2.8,
            },
        );

        // Vegetables
        // Emergency Stabilization: Shifted for September-start calendar.
        // Sowing: March-April (turns 13-15), Harvest: September-October (turns 1-3).
        crops.insert(
            "potatoes".to_string(),
            CropDefinition {
                id: "potatoes".to_string(),
                name: "Potatoes".to_string(),
                category: CropCategory::Root,
                land_type: LandType::Arable,
                compatible_climates: vec![
                    ClimateProfile::Temperate,
                    ClimateProfile::Continental,
                    ClimateProfile::Tropical,
                ],
                sowing_schedule: TurnRange {
                    start_turn: 13,
                    end_turn: 15,
                },
                harvest_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 3,
                },
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
            },
        );

        // Industrial crops
        crops.insert(
            "cotton".to_string(),
            CropDefinition {
                id: "cotton".to_string(),
                name: "Cotton".to_string(),
                category: CropCategory::Industrial,
                land_type: LandType::Plantation,
                compatible_climates: vec![ClimateProfile::Tropical, ClimateProfile::Coastal],
                sowing_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 2,
                },
                harvest_schedule: TurnRange {
                    start_turn: 11,
                    end_turn: 14,
                },
                labor_demand: LaborDemandProfile {
                    sowing_fte_per_hectare: 0.0, // Plantation skips sowing
                    growing_fte_per_hectare: 0.02,
                    harvesting_fte_per_hectare: 0.20,
                },
                yields: {
                    let mut map = HashMap::new();
                    map.insert(Commodity::IndustrialFiber, 1.8); // Lint
                    map.insert(Commodity::Fodder, 3.5); // Cottonseed meal
                    map
                },
                seed_cost_per_hectare: 0.0, // Plantation established once
                seed_commodity: Commodity::Seeds,
                seed_quantity_per_hectare: 0.0, // Plantation skips sowing
                sowing_wage_multiplier: 1.0,
                harvesting_wage_multiplier: 3.0,
            },
        );

        // Fodder
        // Emergency Stabilization: Shifted for September-start calendar.
        // Sowing: January-February (turns 11-13), Harvest: May-August (turns 20-24).
        crops.insert(
            "alfalfa".to_string(),
            CropDefinition {
                id: "alfalfa".to_string(),
                name: "Alfalfa".to_string(),
                category: CropCategory::Fodder,
                land_type: LandType::Arable,
                compatible_climates: vec![ClimateProfile::Temperate, ClimateProfile::Continental],
                sowing_schedule: TurnRange {
                    start_turn: 11,
                    end_turn: 13,
                },
                harvest_schedule: TurnRange {
                    start_turn: 20,
                    end_turn: 24,
                },
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
            },
        );

        // Stabilization Sprint: Livestock (cattle ranching)
        // Pasture-based, yields Meat + Livestock. Plantation land type
        // (perennial pasture, no annual sowing).
        crops.insert(
            "cattle".to_string(),
            CropDefinition {
                id: "cattle".to_string(),
                name: "Cattle".to_string(),
                category: CropCategory::Fodder, // Uses fodder as feed input
                land_type: LandType::Plantation,
                compatible_climates: vec![
                    ClimateProfile::Temperate,
                    ClimateProfile::Continental,
                    ClimateProfile::Coastal,
                ],
                sowing_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 2,
                },
                harvest_schedule: TurnRange {
                    start_turn: 6,
                    end_turn: 20,
                },
                labor_demand: LaborDemandProfile {
                    sowing_fte_per_hectare: 0.0, // Plantation skips sowing
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
            },
        );

        // Stabilization Sprint: Orchard (fruit trees)
        // Plantation land type (perennial, no annual sowing).
        crops.insert(
            "orchard".to_string(),
            CropDefinition {
                id: "orchard".to_string(),
                name: "Orchard".to_string(),
                category: CropCategory::Orchard,
                land_type: LandType::Plantation,
                compatible_climates: vec![
                    ClimateProfile::Temperate,
                    ClimateProfile::Continental,
                    ClimateProfile::Coastal,
                ],
                sowing_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 2,
                },
                harvest_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 4,
                },
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
            },
        );

        // Stabilization Sprint: Tobacco (luxury plantation crop)
        crops.insert(
            "tobacco".to_string(),
            CropDefinition {
                id: "tobacco".to_string(),
                name: "Tobacco".to_string(),
                category: CropCategory::Industrial,
                land_type: LandType::Plantation,
                compatible_climates: vec![
                    ClimateProfile::Tropical,
                    ClimateProfile::Coastal,
                    ClimateProfile::Temperate,
                ],
                sowing_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 2,
                },
                harvest_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 4,
                },
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
            },
        );

        // World Generation & Climate Audit (v0.5.3): Tropical and
        // climate-diverse crops to enable year-round growing in tropical
        // and sub-tropical regions.

        // Rice — Cereal crop for tropical and coastal climates.
        // Supports 2 harvest cycles per year in tropical regions.
        // First cycle: sowing turns 1-2, harvest turns 6-8.
        // (Second cycle is handled by the state machine re-entering Sowing
        // when the sowing window opens again at turns 13-14.)
        crops.insert(
            "rice".to_string(),
            CropDefinition {
                id: "rice".to_string(),
                name: "Rice".to_string(),
                category: CropCategory::Cereal,
                land_type: LandType::Arable,
                compatible_climates: vec![
                    ClimateProfile::Tropical,
                    ClimateProfile::Coastal,
                    ClimateProfile::SubTropical,
                ],
                sowing_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 2,
                },
                harvest_schedule: TurnRange {
                    start_turn: 6,
                    end_turn: 8,
                },
                labor_demand: LaborDemandProfile {
                    sowing_fte_per_hectare: 0.20,  // Labor-intensive transplanting
                    growing_fte_per_hectare: 0.06, // Water management
                    harvesting_fte_per_hectare: 0.25,
                },
                yields: {
                    let mut map = HashMap::new();
                    map.insert(Commodity::Cereal, 6.0); // Higher yield than wheat
                    map.insert(Commodity::Fodder, 3.0); // Rice straw
                    map
                },
                seed_cost_per_hectare: 120.0,
                seed_commodity: Commodity::Seeds,
                seed_quantity_per_hectare: 0.04,
                sowing_wage_multiplier: 1.8,
                harvesting_wage_multiplier: 3.0,
            },
        );

        // Sugarcane — Luxury/industrial crop for tropical climates.
        // Long growing season, high yield. Plantation (perennial ratooning).
        crops.insert(
            "sugarcane".to_string(),
            CropDefinition {
                id: "sugarcane".to_string(),
                name: "Sugarcane".to_string(),
                category: CropCategory::Industrial,
                land_type: LandType::Plantation,
                compatible_climates: vec![
                    ClimateProfile::Tropical,
                    ClimateProfile::Coastal,
                    ClimateProfile::SubTropical,
                ],
                sowing_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 2,
                },
                harvest_schedule: TurnRange {
                    start_turn: 8,
                    end_turn: 14,
                }, // Long harvest
                labor_demand: LaborDemandProfile {
                    sowing_fte_per_hectare: 0.0, // Plantation skips sowing
                    growing_fte_per_hectare: 0.03,
                    harvesting_fte_per_hectare: 0.15,
                },
                yields: {
                    let mut map = HashMap::new();
                    map.insert(Commodity::Luxury, 8.0); // High yield, processed as sugar
                    map.insert(Commodity::Fodder, 4.0); // Bagasse
                    map
                },
                seed_cost_per_hectare: 0.0,
                seed_commodity: Commodity::Seeds,
                seed_quantity_per_hectare: 0.0,
                sowing_wage_multiplier: 1.0,
                harvesting_wage_multiplier: 2.5,
            },
        );

        // Coffee — Luxury plantation crop for tropical and coastal climates.
        crops.insert(
            "coffee".to_string(),
            CropDefinition {
                id: "coffee".to_string(),
                name: "Coffee".to_string(),
                category: CropCategory::Industrial,
                land_type: LandType::Plantation,
                compatible_climates: vec![ClimateProfile::Tropical, ClimateProfile::Coastal],
                sowing_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 2,
                },
                harvest_schedule: TurnRange {
                    start_turn: 4,
                    end_turn: 7,
                },
                labor_demand: LaborDemandProfile {
                    sowing_fte_per_hectare: 0.0,
                    growing_fte_per_hectare: 0.04, // Pruning, shade management
                    harvesting_fte_per_hectare: 0.30, // Hand-picking is labor-intensive
                },
                yields: {
                    let mut map = HashMap::new();
                    map.insert(Commodity::Luxury, 2.5);
                    map
                },
                seed_cost_per_hectare: 0.0,
                seed_commodity: Commodity::Seeds,
                seed_quantity_per_hectare: 0.0,
                sowing_wage_multiplier: 1.0,
                harvesting_wage_multiplier: 3.5,
            },
        );

        // Tea — Luxury plantation crop for tropical and mountainous climates.
        // Very long harvest window (year-round in tropical climates).
        crops.insert(
            "tea".to_string(),
            CropDefinition {
                id: "tea".to_string(),
                name: "Tea".to_string(),
                category: CropCategory::Industrial,
                land_type: LandType::Plantation,
                compatible_climates: vec![ClimateProfile::Tropical, ClimateProfile::Mountainous],
                sowing_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 2,
                },
                harvest_schedule: TurnRange {
                    start_turn: 4,
                    end_turn: 18,
                }, // Very long harvest
                labor_demand: LaborDemandProfile {
                    sowing_fte_per_hectare: 0.0,
                    growing_fte_per_hectare: 0.03,
                    harvesting_fte_per_hectare: 0.25, // Hand-plucking
                },
                yields: {
                    let mut map = HashMap::new();
                    map.insert(Commodity::Luxury, 1.8);
                    map
                },
                seed_cost_per_hectare: 0.0,
                seed_commodity: Commodity::Seeds,
                seed_quantity_per_hectare: 0.0,
                sowing_wage_multiplier: 1.0,
                harvesting_wage_multiplier: 3.0,
            },
        );

        // Soybeans — Versatile cereal/legume crop for temperate AND tropical.
        // Can grow in both climate zones with different schedules.
        // In temperate: sowing spring (turns 13-15), harvest autumn (turns 1-3).
        // In tropical: sowing turns 1-2, harvest turns 6-8 (same as rice cycle).
        crops.insert(
            "soybeans".to_string(),
            CropDefinition {
                id: "soybeans".to_string(),
                name: "Soybeans".to_string(),
                category: CropCategory::Legume,
                land_type: LandType::Arable,
                compatible_climates: vec![
                    ClimateProfile::Temperate,
                    ClimateProfile::Continental,
                    ClimateProfile::Tropical,
                ],
                // Temperate schedule (also used for tropical pre-seeding at game start)
                sowing_schedule: TurnRange {
                    start_turn: 13,
                    end_turn: 15,
                },
                harvest_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 3,
                },
                labor_demand: LaborDemandProfile {
                    sowing_fte_per_hectare: 0.10,
                    growing_fte_per_hectare: 0.03,
                    harvesting_fte_per_hectare: 0.15,
                },
                yields: {
                    let mut map = HashMap::new();
                    map.insert(Commodity::Cereal, 3.0); // Soybeans are a grain legume
                    map.insert(Commodity::Fodder, 2.5); // Soybean meal
                    map
                },
                seed_cost_per_hectare: 110.0,
                seed_commodity: Commodity::Seeds,
                seed_quantity_per_hectare: 0.08,
                sowing_wage_multiplier: 1.3,
                harvesting_wage_multiplier: 2.5,
            },
        );

        // Phase 87+: Citrus — Luxury plantation crop for subtropical climates.
        // Oranges, lemons, limes — Mediterranean/subtropical signature crop.
        crops.insert(
            "citrus".to_string(),
            CropDefinition {
                id: "citrus".to_string(),
                name: "Citrus".to_string(),
                category: CropCategory::Industrial,
                land_type: LandType::Plantation,
                compatible_climates: vec![
                    ClimateProfile::SubTropical,
                    ClimateProfile::Tropical,
                    ClimateProfile::Coastal,
                ],
                sowing_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 2,
                },
                harvest_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 6,
                }, // Long harvest window
                labor_demand: LaborDemandProfile {
                    sowing_fte_per_hectare: 0.0,      // Plantation skips sowing
                    growing_fte_per_hectare: 0.03,    // Pruning, irrigation
                    harvesting_fte_per_hectare: 0.20, // Hand-picking
                },
                yields: {
                    let mut map = HashMap::new();
                    map.insert(Commodity::Luxury, 4.0); // Fresh fruit
                    map.insert(Commodity::Fodder, 1.0); // Citrus pulp for feed
                    map
                },
                seed_cost_per_hectare: 0.0,
                seed_commodity: Commodity::Seeds,
                seed_quantity_per_hectare: 0.0,
                sowing_wage_multiplier: 1.0,
                harvesting_wage_multiplier: 2.5,
            },
        );

        // Phase 87+: Olives — Luxury plantation crop for subtropical/Mediterranean.
        // Olive oil was a major trade commodity in the ancient and medieval world.
        crops.insert(
            "olives".to_string(),
            CropDefinition {
                id: "olives".to_string(),
                name: "Olives".to_string(),
                category: CropCategory::Industrial,
                land_type: LandType::Plantation,
                compatible_climates: vec![
                    ClimateProfile::SubTropical,
                    ClimateProfile::Temperate,
                    ClimateProfile::Coastal,
                ],
                sowing_schedule: TurnRange {
                    start_turn: 1,
                    end_turn: 2,
                },
                harvest_schedule: TurnRange {
                    start_turn: 3,
                    end_turn: 6,
                }, // Autumn harvest
                labor_demand: LaborDemandProfile {
                    sowing_fte_per_hectare: 0.0,      // Plantation skips sowing
                    growing_fte_per_hectare: 0.02,    // Pruning, minimal maintenance
                    harvesting_fte_per_hectare: 0.18, // Hand-harvesting
                },
                yields: {
                    let mut map = HashMap::new();
                    map.insert(Commodity::Luxury, 2.5); // Olive oil
                    map.insert(Commodity::Fodder, 0.5); // Olive cake (press residue)
                    map
                },
                seed_cost_per_hectare: 0.0,
                seed_commodity: Commodity::Seeds,
                seed_quantity_per_hectare: 0.0,
                sowing_wage_multiplier: 1.0,
                harvesting_wage_multiplier: 3.0,
            },
        );

        crops
    })
}

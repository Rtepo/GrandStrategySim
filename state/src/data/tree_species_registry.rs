//! Tree species registry for forestry (Phase 3).
//!
//! Defines tree species with species-specific maturation periods, growth rates,
//! sapling requirements, and yield characteristics. Used by the forestry system
//! to model biological forest growth and harvest, analogous to how the crop
//! registry models agricultural crops.

use crate::registries::enums::Commodity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Definition of a tree species for forestry modeling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TreeSpeciesDefinition {
    /// Species ID (e.g., "pine", "oak", "spruce", "tropical_hardwood")
    pub id: String,
    /// Display name
    pub name: String,
    /// Maturation period in turns (years)
    pub maturation_turns: u32,
    /// Base growth rate per year (fraction of stock)
    pub base_growth_rate: f64,
    /// Saplings required per hectare at planting
    pub saplings_per_hectare: f64,
    /// Base timber yield at maturity (m³ per hectare)
    pub base_yield_per_hectare: f64,
    /// Establishment commodity (always Saplings for trees)
    pub establishment_commodity: Commodity,
    /// Nutrient depletion rate per harvest (0.0–1.0)
    pub nutrient_depletion_rate: f64,
    /// Compatible climate profiles (placeholder for future climate matching)
    pub compatible_climates: Vec<String>,
}

/// Returns the global tree species registry.
pub fn tree_species_registry() -> &'static HashMap<String, TreeSpeciesDefinition> {
    static REGISTRY: OnceLock<HashMap<String, TreeSpeciesDefinition>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut species = HashMap::new();

        species.insert("pine".to_string(), TreeSpeciesDefinition {
            id: "pine".to_string(),
            name: "Pine".to_string(),
            maturation_turns: 72, // 72 years
            base_growth_rate: 0.04, // 4% per year
            saplings_per_hectare: 200.0,
            base_yield_per_hectare: 150.0, // 150 m³/ha at maturity
            establishment_commodity: Commodity::Saplings,
            nutrient_depletion_rate: 0.02,
            compatible_climates: vec!["temperate".to_string(), "continental".to_string(), "boreal".to_string()],
        });

        species.insert("oak".to_string(), TreeSpeciesDefinition {
            id: "oak".to_string(),
            name: "Oak".to_string(),
            maturation_turns: 240, // 240 years
            base_growth_rate: 0.015, // 1.5% per year
            saplings_per_hectare: 150.0,
            base_yield_per_hectare: 250.0, // High-value timber
            establishment_commodity: Commodity::Saplings,
            nutrient_depletion_rate: 0.01,
            compatible_climates: vec!["temperate".to_string(), "mediterranean".to_string()],
        });

        species.insert("spruce".to_string(), TreeSpeciesDefinition {
            id: "spruce".to_string(),
            name: "Spruce".to_string(),
            maturation_turns: 96, // 96 years
            base_growth_rate: 0.03, // 3% per year
            saplings_per_hectare: 250.0,
            base_yield_per_hectare: 180.0,
            establishment_commodity: Commodity::Saplings,
            nutrient_depletion_rate: 0.02,
            compatible_climates: vec!["boreal".to_string(), "continental".to_string(), "temperate".to_string()],
        });

        species.insert("tropical_hardwood".to_string(), TreeSpeciesDefinition {
            id: "tropical_hardwood".to_string(),
            name: "Tropical Hardwood".to_string(),
            maturation_turns: 360, // 360 years
            base_growth_rate: 0.01, // 1% per year
            saplings_per_hectare: 100.0,
            base_yield_per_hectare: 300.0, // Very high-value timber
            establishment_commodity: Commodity::Saplings,
            nutrient_depletion_rate: 0.05,
            compatible_climates: vec!["tropical".to_string(), "subtropical".to_string()],
        });

        species.insert("birch".to_string(), TreeSpeciesDefinition {
            id: "birch".to_string(),
            name: "Birch".to_string(),
            maturation_turns: 60, // 60 years (fast-growing)
            base_growth_rate: 0.05, // 5% per year
            saplings_per_hectare: 300.0,
            base_yield_per_hectare: 100.0, // Lower yield but faster
            establishment_commodity: Commodity::Saplings,
            nutrient_depletion_rate: 0.03,
            compatible_climates: vec!["boreal".to_string(), "temperate".to_string(), "continental".to_string()],
        });

        species
    })
}

/// Get a tree species definition by ID.
pub fn get_tree_species(id: &str) -> Option<&'static TreeSpeciesDefinition> {
    tree_species_registry().get(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_species() {
        let registry = tree_species_registry();
        assert!(registry.contains_key("pine"));
        assert!(registry.contains_key("oak"));
        assert!(registry.contains_key("spruce"));
        assert!(registry.contains_key("tropical_hardwood"));
        assert!(registry.contains_key("birch"));
    }

    #[test]
    fn test_pine_maturation() {
        let pine = get_tree_species("pine").unwrap();
        assert_eq!(pine.maturation_turns, 72);
        assert!((pine.base_growth_rate - 0.04).abs() < 1e-10);
    }

    #[test]
    fn test_oak_slower_than_pine() {
        let oak = get_tree_species("oak").unwrap();
        let pine = get_tree_species("pine").unwrap();
        assert!(oak.maturation_turns > pine.maturation_turns);
        assert!(oak.base_growth_rate < pine.base_growth_rate);
    }
}

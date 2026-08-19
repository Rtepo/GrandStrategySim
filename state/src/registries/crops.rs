//! Agricultural crop registry for Phase 6.3 Agriculture 2.0
//!
//! Defines crop types, categories, land requirements, and labor profiles
//! for the dynamic agricultural state machine.

use crate::society::geography::ClimateProfile;
use crate::registries::enums::Commodity;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Phase 46: Default seed commodity for backward save compatibility.
fn default_seed_commodity() -> Commodity {
    Commodity::Seeds
}

/// Phase 46: Default seed quantity per hectare (50 kg = 0.05 tons).
fn default_seed_quantity_per_hectare() -> f64 {
    0.05
}

/// Crop category for economic classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CropCategory {
    /// Root crops (potatoes, beets)
    Root,
    /// Cereal crops (wheat, rice, barley)
    Cereal,
    /// Legume crops (beans, peas)
    Legume,
    /// Industrial crops (coffee, cotton, tobacco)
    Industrial,
    /// Fodder crops (alfalfa, clover)
    Fodder,
    /// Orchard crops (apples, pears, plums, citrus)
    Orchard,
}

/// Land type requirement for crops
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LandType {
    /// Requires annual sowing
    Arable,
    /// Perennial, skips sowing
    Plantation,
}

/// Turn range for sowing/harvest schedules (1-24)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TurnRange {
    /// Start turn (1-24)
    pub start_turn: u32,
    /// End turn (1-24)
    pub end_turn: u32,
}

/// Labor demand profile per hectare by phase
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaborDemandProfile {
    /// FTE per hectare during Sowing phase
    #[serde(rename = "siew", default)]
    pub sowing_fte_per_hectare: f64,

    /// FTE per hectare during Growing phase
    #[serde(rename = "wzrost", default)]
    pub growing_fte_per_hectare: f64,

    /// FTE per hectare during Harvesting phase
    #[serde(rename = "zbiory", default)]
    pub harvesting_fte_per_hectare: f64,
}

impl Default for LaborDemandProfile {
    fn default() -> Self {
        Self {
            sowing_fte_per_hectare: 0.0,
            growing_fte_per_hectare: 0.0,
            harvesting_fte_per_hectare: 0.0,
        }
    }
}

/// Crop definition loaded from crops.json
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CropDefinition {
    /// Unique crop identifier (e.g., "wheat", "rice", "coffee")
    pub id: String,

    /// Display name (Polish for save compatibility)
    #[serde(rename = "nazwa")]
    pub name: String,

    /// Crop category for economic classification
    #[serde(rename = "kategoria")]
    pub category: CropCategory,

    /// Land type requirement
    #[serde(rename = "typ_gruntu")]
    pub land_type: LandType,

    /// Compatible climate profiles
    #[serde(rename = "klimaty")]
    pub compatible_climates: Vec<ClimateProfile>,

    /// Sowing schedule (turns 1-24)
    #[serde(rename = "okres_siewu")]
    pub sowing_schedule: TurnRange,

    /// Harvest schedule (turns 1-24)
    #[serde(rename = "okres_zbiorów")]
    pub harvest_schedule: TurnRange,

    /// Base FTE demand per hectare by phase
    #[serde(rename = "zapotrzebowanie_fte")]
    pub labor_demand: LaborDemandProfile,

    /// Multi-yield mapping: commodity -> tons per hectare (Phase 6.3.5)
    /// Supports by-products (e.g., corn grain + stalks for fodder)
    #[serde(rename = "plony")]
    pub yields: HashMap<Commodity, f64>,

    /// Seed cost per hectare (currency units)
    #[serde(rename = "koszt_nasion")]
    pub seed_cost_per_hectare: f64,

    /// Phase 46: Physical seed commodity consumed per hectare at sowing.
    /// Defaults to `Commodity::Seeds` for backward save compatibility.
    #[serde(rename = "komodita_nasion", default = "default_seed_commodity")]
    pub seed_commodity: Commodity,

    /// Phase 46: Physical seed quantity (tons) consumed per hectare at sowing.
    /// Default 0.05 (50 kg/hectare) is realistic for cereal sowing.
    #[serde(rename = "ilość_nasion_na_hektar", default = "default_seed_quantity_per_hectare")]
    pub seed_quantity_per_hectare: f64,

    /// Wage multiplier during Sowing phase (data-driven)
    #[serde(rename = "mnożnik_płacy_siewu", default)]
    pub sowing_wage_multiplier: f64,

    /// Wage multiplier during Harvesting phase (data-driven)
    #[serde(rename = "mnożnik_płacy_zbiorów", default)]
    pub harvesting_wage_multiplier: f64,
}

/// Crop registry for agricultural simulation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CropRegistry {
    /// Map of crop definitions by ID
    #[serde(flatten, default)]
    pub crops: HashMap<String, CropDefinition>,
}

impl CropRegistry {
    /// Get a crop definition by ID
    ///
    /// # Arguments
    /// * `id` - Crop identifier
    ///
    /// # Returns
    /// * `Some(&CropDefinition)` if found, `None` otherwise
    pub fn get(&self, id: &str) -> Option<&CropDefinition> {
        self.crops.get(id)
    }
}

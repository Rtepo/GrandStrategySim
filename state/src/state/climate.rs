use crate::society::geography::ClimateProfile;
use crate::state::Season;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Seasonal modifiers for production and consumption (Phase 6.1)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SeasonalModifiers {
    /// Energy consumption multiplier by season (1.0 = baseline)
    #[serde(rename = "mnożnik_konsumpcji_energii", default)]
    pub energy_multiplier: f64,
    
    /// Agricultural yield multiplier by season (1.0 = baseline)
    #[serde(rename = "mnożnik_plonów", default)]
    pub agriculture_multiplier: f64,
    
    /// Services/tourism multiplier by season (1.0 = baseline)
    #[serde(rename = "mnożnik_usług", default)]
    pub services_multiplier: f64,

    /// Phase 6.3: Tourism-specific multiplier (coastal/mountainous regions)
    #[serde(rename = "mnożnik_turystyczny", default)]
    pub tourism_multiplier: f64,

    /// Heating demand multiplier (1.0 = baseline)
    #[serde(rename = "mnożnik_zapotrzebowania_na_ciepło", default)]
    pub heating_demand_multiplier: f64,
    
    /// Construction efficiency multiplier (1.0 = baseline)
    #[serde(rename = "mnożnik_efektywności_budowlanej", default)]
    pub construction_multiplier: f64,
    
    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Climate configuration with climate-season matrix (Phase 6.1)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ClimateConfig {
    /// Mapping: ClimateProfile -> Season -> SeasonalModifiers
    #[serde(rename = "macierz_klimatyczna", default)]
    pub climate_season_matrix: HashMap<(ClimateProfile, Season), SeasonalModifiers>,
    
    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl ClimateConfig {
    /// Get seasonal modifiers for a given climate and season
    pub fn get_modifiers(&self, climate: ClimateProfile, season: Season) -> SeasonalModifiers {
        self.climate_season_matrix
            .get(&(climate, season))
            .cloned()
            .unwrap_or_default()
    }
}

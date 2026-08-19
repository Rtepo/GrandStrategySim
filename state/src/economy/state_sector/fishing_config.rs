//! Fishing and aquaculture configuration.
//!
//! This module defines configuration parameters for fish stock dynamics,
//! fishing quotas, and fish farm operations in Phase 7.1.

use serde::{Deserialize, Serialize};

/// Configuration for fishing and aquaculture mechanics.
///
/// Controls fish stock regeneration, health decay from overfishing,
/// fish farm water quality, and disease risk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FishingConfig {
    /// Health decay rate when overfishing occurs.
    /// Default 0.95 (5% health loss per overfishing event).
    #[serde(rename = "współczynnik_przełowienia", default = "default_overfishing_health_decay")]
    pub overfishing_health_decay: f64,

    /// Minimum health floor — stock never fully collapses.
    /// Default 0.3 (30% of max biomass).
    #[serde(rename = "minimalne_zdrowie", default = "default_min_health_floor")]
    pub min_health_floor: f64,

    /// Health recovery per turn on sustainable fishing.
    /// Default 0.01 (1% recovery per turn).
    #[serde(rename = "odnowienie_zdrowia", default = "default_sustainable_health_recovery")]
    pub sustainable_health_recovery: f64,

    /// Fish farm water quality decay per turn.
    /// Default 0.99 (1% decay per turn).
    #[serde(rename = "spadek_jakości_wody", default = "default_farm_water_quality_decay")]
    pub farm_water_quality_decay: f64,

    /// Fish farm minimum water quality.
    /// Default 0.5 (50%).
    #[serde(rename = "minimalna_jakość_wody", default = "default_farm_min_water_quality")]
    pub farm_min_water_quality: f64,

    /// Fish farm disease risk increase per turn.
    /// Default 0.05.
    #[serde(rename = "wzrost_ryzyka_chorób", default = "default_farm_disease_increase")]
    pub farm_disease_increase: f64,

    /// Fish farm disease risk decrease when conditions are good.
    /// Default 0.02.
    #[serde(rename = "spadek_ryzyka_chorób", default = "default_farm_disease_decrease")]
    pub farm_disease_decrease: f64,

    /// Fish farm maximum disease risk.
    /// Default 0.3 (30%).
    #[serde(rename = "maks_ryzyko_chorób", default = "default_farm_max_disease_risk")]
    pub farm_max_disease_risk: f64,

    /// Initial biomass as fraction of max biomass.
    /// Default 0.8 (80%).
    #[serde(rename = "początkowa_biomasa", default = "default_initial_biomass_ratio")]
    pub initial_biomass_ratio: f64,
}

fn default_overfishing_health_decay() -> f64 {
    0.95
}

fn default_min_health_floor() -> f64 {
    0.3
}

fn default_sustainable_health_recovery() -> f64 {
    0.01
}

fn default_farm_water_quality_decay() -> f64 {
    0.99
}

fn default_farm_min_water_quality() -> f64 {
    0.5
}

fn default_farm_disease_increase() -> f64 {
    0.05
}

fn default_farm_disease_decrease() -> f64 {
    0.02
}

fn default_farm_max_disease_risk() -> f64 {
    0.3
}

fn default_initial_biomass_ratio() -> f64 {
    0.8
}

impl Default for FishingConfig {
    fn default() -> Self {
        Self {
            overfishing_health_decay: default_overfishing_health_decay(),
            min_health_floor: default_min_health_floor(),
            sustainable_health_recovery: default_sustainable_health_recovery(),
            farm_water_quality_decay: default_farm_water_quality_decay(),
            farm_min_water_quality: default_farm_min_water_quality(),
            farm_disease_increase: default_farm_disease_increase(),
            farm_disease_decrease: default_farm_disease_decrease(),
            farm_max_disease_risk: default_farm_max_disease_risk(),
            initial_biomass_ratio: default_initial_biomass_ratio(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = FishingConfig::default();
        assert_eq!(config.overfishing_health_decay, 0.95);
        assert_eq!(config.min_health_floor, 0.3);
        assert_eq!(config.sustainable_health_recovery, 0.01);
        assert_eq!(config.farm_water_quality_decay, 0.99);
        assert_eq!(config.farm_min_water_quality, 0.5);
        assert_eq!(config.farm_disease_increase, 0.05);
        assert_eq!(config.farm_disease_decrease, 0.02);
        assert_eq!(config.farm_max_disease_risk, 0.3);
        assert_eq!(config.initial_biomass_ratio, 0.8);
    }
}

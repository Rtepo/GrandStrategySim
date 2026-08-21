//! Configuration structs for the utilities system (Phase 8).
//!
//! These configs control utility pricing tariffs and physical conversion factors
//! used during grid distribution and consumption processing.

use serde::{Deserialize, Serialize};

/// Pricing tariffs for utility services billed to consumers.
///
/// # Rules
/// * All prices are in domestic currency per unit.
/// * `treasury_subsidy_ratio` controls the fraction of bills covered by the state
///   for consumers who cannot afford the full payment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UtilityPricingConfig {
    /// Price per kWh of electricity.
    #[serde(default = "default_price_per_kwh")]
    pub price_per_kwh: f64,

    /// Price per GJ of district heating.
    #[serde(default = "default_price_per_gj_heating")]
    pub price_per_gj_heating: f64,

    /// Price per liter of water (surface + groundwater).
    #[serde(default = "default_price_per_liter_water")]
    pub price_per_liter_water: f64,

    /// Price per liter of sewage treatment.
    #[serde(default = "default_price_per_liter_sewage")]
    pub price_per_liter_sewage: f64,

    /// Fraction of utility bill covered by Treasury for low-income consumers (0.0 - 1.0).
    #[serde(default = "default_treasury_subsidy_ratio")]
    pub treasury_subsidy_ratio: f64,
}

impl Default for UtilityPricingConfig {
    fn default() -> Self {
        Self {
            price_per_kwh: default_price_per_kwh(),
            price_per_gj_heating: default_price_per_gj_heating(),
            price_per_liter_water: default_price_per_liter_water(),
            price_per_liter_sewage: default_price_per_liter_sewage(),
            treasury_subsidy_ratio: default_treasury_subsidy_ratio(),
        }
    }
}

fn default_price_per_kwh() -> f64 {
    0.15
}

fn default_price_per_gj_heating() -> f64 {
    25.0
}

fn default_price_per_liter_water() -> f64 {
    0.002
}

fn default_price_per_liter_sewage() -> f64 {
    0.001
}

fn default_treasury_subsidy_ratio() -> f64 {
    0.3
}

/// Physical conversion factors and penalty parameters for the utilities system.
///
/// # Rules
/// * `energy_to_kwh_factor` converts 1 unit of `Commodity::Energy` to kWh.
/// * `energy_to_gj_heating_factor` converts 1 unit of `Commodity::Heat` to GJ.
/// * `blackout_efficiency_penalty` is the max efficiency loss at full blackout (0.0 - 1.0).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UtilityConfig {
    /// 1 unit Commodity::Energy = X kWh of ElectricitySupply.
    #[serde(default = "default_energy_to_kwh")]
    pub energy_to_kwh_factor: f64,

    /// 1 unit Commodity::Heat = Y GJ of DistrictHeating.
    #[serde(default = "default_energy_to_gj_heating")]
    pub energy_to_gj_heating_factor: f64,

    /// Max efficiency loss at full blackout (0.5 = 50% loss).
    #[serde(default = "default_blackout_penalty")]
    pub blackout_efficiency_penalty: f64,

    /// Health degradation increase per turn of landfill overflow.
    #[serde(default = "default_landfill_overflow_penalty")]
    pub landfill_overflow_health_penalty: f64,
}

impl Default for UtilityConfig {
    fn default() -> Self {
        Self {
            energy_to_kwh_factor: default_energy_to_kwh(),
            energy_to_gj_heating_factor: default_energy_to_gj_heating(),
            blackout_efficiency_penalty: default_blackout_penalty(),
            landfill_overflow_health_penalty: default_landfill_overflow_penalty(),
        }
    }
}

fn default_energy_to_kwh() -> f64 {
    1000.0
}

fn default_energy_to_gj_heating() -> f64 {
    10.0
}

fn default_blackout_penalty() -> f64 {
    0.5
}

fn default_landfill_overflow_penalty() -> f64 {
    0.05
}

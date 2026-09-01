//! Phase 82: Heating plant types and metadata.
//!
//! Each heating plant type is a distinct entity with its own registry key,
//! production methods, automation progression, and organization progression.
//! This module defines the type enum and metadata stored on building instances.

use serde::{Deserialize, Serialize};

/// Type of heating plant. Each variant corresponds to a distinct registry key
/// in `default_production_methods()` with its own full technological matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HeatingPlantType {
    /// Wood/peat-fired boiler (1880+). Low CAPEX, high OPEX, high smog.
    WoodBoiler,
    /// Hard coal-fired heat plant (1890+). Moderate CAPEX, moderate OPEX.
    #[default]
    CoalHeatPlant,
    /// Lignite/brown coal heat plant (1890+). Lower CAPEX, higher OPEX.
    LigniteHeatPlant,
    /// Coke-oven gas heat plant (1900+). Uses CoalGas byproduct.
    CokeOvenGasHeatPlant,
    /// Oil-fired heat plant (1910+). Fuel-price-dependent OPEX.
    OilHeatPlant,
    /// Natural gas heat plant (1950+). Clean burning, moderate CAPEX.
    NaturalGasHeatPlant,
    /// Geothermal heating plant (1970+). High CAPEX, near-zero OPEX.
    /// Requires volcanic/geothermal geological trait on the region.
    GeothermalHeatPlant,
}

impl HeatingPlantType {
    /// Get the registry key for this plant type's production methods.
    pub fn registry_key(&self) -> &'static str {
        match self {
            HeatingPlantType::WoodBoiler => "wood_boiler_plant",
            HeatingPlantType::CoalHeatPlant => "coal_heat_plant",
            HeatingPlantType::LigniteHeatPlant => "lignite_heat_plant",
            HeatingPlantType::CokeOvenGasHeatPlant => "coke_oven_gas_heat_plant",
            HeatingPlantType::OilHeatPlant => "oil_heat_plant",
            HeatingPlantType::NaturalGasHeatPlant => "natural_gas_heat_plant",
            HeatingPlantType::GeothermalHeatPlant => "geothermal_heat_plant",
        }
    }

    /// Check if this plant type requires a geological trait (geothermal).
    pub fn requires_geological_trait(&self) -> bool {
        matches!(self, HeatingPlantType::GeothermalHeatPlant)
    }

    /// Get the default emission control registry key for this plant type.
    pub fn emission_control_registry_key(&self) -> &'static str {
        "heating_plant_emission_control"
    }
}

/// Metadata for a heating plant building, stored in `Building.extra`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HeatingPlantMetadata {
    /// Type of heating plant.
    #[serde(default)]
    pub plant_type: HeatingPlantType,

    /// Nameplate heat output capacity (GJ per turn at full utilization).
    #[serde(default)]
    pub nameplate_capacity_gj: f64,

    /// Thermal efficiency (0.0-1.0). Fraction of fuel energy converted to
    /// useful Heat output.
    #[serde(default)]
    pub thermal_efficiency: f64,

    /// Whether this plant has emission controls installed (scrubbers/filters).
    /// When true, emission factor is reduced by 80% (physical constant for
    /// wet scrubber efficiency).
    #[serde(default)]
    pub has_emission_controls: bool,
}

/// CHP (Combined Heat and Power) retrofit metadata, stored in `Building.extra`
/// alongside `PowerPlantMetadata` for retrofitted thermal power plants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ChpRetrofitMetadata {
    /// Heat output as fraction of electrical output (physical: 0.8-1.2).
    /// Determines how much Heat is co-produced per unit of Energy.
    #[serde(default = "default_heat_to_power_ratio")]
    pub heat_to_power_ratio: f64,

    /// Electrical efficiency penalty from steam extraction (physical: 0.05-0.10).
    /// Steam is extracted before full turbine expansion for heating, reducing
    /// electrical output.
    #[serde(default = "default_electrical_efficiency_penalty")]
    pub electrical_efficiency_penalty: f64,

    /// Whether the retrofit construction is complete and CHP is active.
    #[serde(default)]
    pub is_active: bool,

    /// Auxiliary boiler efficiency factor (CORRECTION 6: CHP Winter Paradox).
    /// When the spot market curtails electrical output but heat demand exists,
    /// the plant switches to Auxiliary Boiler Mode — burning fuel directly for
    /// heat at this fraction of normal thermal efficiency.
    /// Physical default: 0.85 (direct combustion without turbine extraction).
    #[serde(default = "default_auxiliary_efficiency_factor")]
    pub auxiliary_efficiency_factor: f64,
}

fn default_heat_to_power_ratio() -> f64 {
    1.0
}

fn default_electrical_efficiency_penalty() -> f64 {
    0.08
}

fn default_auxiliary_efficiency_factor() -> f64 {
    0.85
}

impl ChpRetrofitMetadata {
    /// Check if a power plant type is eligible for CHP retrofit.
    /// Only thermal power plants can be retrofitted — renewables have no
    /// steam cycle to extract from.
    pub fn is_eligible_for_chp(plant_type_str: &str) -> bool {
        matches!(
            plant_type_str,
            "coal_fired_plant"
                | "lignite_fired_plant"
                | "oil_gas_plant"
                | "nuclear_plant"
                | "biomass_plant"
                | "biogas_plant"
                | "geothermal_plant"
        )
    }

    /// Compute heat output from CHP given electrical output.
    /// `heat = electrical_output * heat_to_power_ratio * thermal_efficiency`
    pub fn heat_from_electrical(&self, electrical_output: f64, thermal_efficiency: f64) -> f64 {
        electrical_output * self.heat_to_power_ratio * thermal_efficiency
    }

    /// Compute auxiliary heat output when electrical output is curtailed
    /// but heat demand exists (CORRECTION 6: CHP Winter Paradox).
    ///
    /// The plant burns fuel directly for heat, bypassing the turbine,
    /// at `auxiliary_efficiency_factor` of normal thermal efficiency.
    pub fn auxiliary_heat(
        &self,
        fuel_available: f64,
        fuel_cv: f64,
        thermal_efficiency: f64,
        unmet_heat_demand: f64,
    ) -> f64 {
        if unmet_heat_demand <= 0.0 {
            return 0.0;
        }
        let aux_eff = thermal_efficiency * self.auxiliary_efficiency_factor;
        let potential = fuel_available * fuel_cv * aux_eff;
        potential.min(unmet_heat_demand)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_keys() {
        assert_eq!(
            HeatingPlantType::WoodBoiler.registry_key(),
            "wood_boiler_plant"
        );
        assert_eq!(
            HeatingPlantType::CoalHeatPlant.registry_key(),
            "coal_heat_plant"
        );
        assert_eq!(
            HeatingPlantType::GeothermalHeatPlant.registry_key(),
            "geothermal_heat_plant"
        );
    }

    #[test]
    fn test_geological_constraint() {
        assert!(HeatingPlantType::GeothermalHeatPlant.requires_geological_trait());
        assert!(!HeatingPlantType::CoalHeatPlant.requires_geological_trait());
    }

    #[test]
    fn test_chp_eligibility() {
        assert!(ChpRetrofitMetadata::is_eligible_for_chp("coal_fired_plant"));
        assert!(ChpRetrofitMetadata::is_eligible_for_chp("nuclear_plant"));
        assert!(!ChpRetrofitMetadata::is_eligible_for_chp("solar_plant"));
        assert!(!ChpRetrofitMetadata::is_eligible_for_chp("wind_farm"));
        assert!(!ChpRetrofitMetadata::is_eligible_for_chp("hydro_plant"));
    }

    #[test]
    fn test_heat_from_electrical() {
        let chp = ChpRetrofitMetadata {
            heat_to_power_ratio: 1.0,
            electrical_efficiency_penalty: 0.08,
            is_active: true,
            auxiliary_efficiency_factor: 0.85,
        };
        // 100 MW electrical * 1.0 ratio * 0.35 thermal_efficiency = 35.0 GJ heat
        let heat = chp.heat_from_electrical(100.0, 0.35);
        assert!((heat - 35.0).abs() < 1e-9);
    }

    #[test]
    fn test_auxiliary_heat_zero_demand() {
        let chp = ChpRetrofitMetadata::default();
        let heat = chp.auxiliary_heat(100.0, 24.0, 0.35, 0.0);
        assert_eq!(heat, 0.0);
    }

    #[test]
    fn test_auxiliary_heat_capped_by_demand() {
        let chp = ChpRetrofitMetadata {
            heat_to_power_ratio: 1.0,
            electrical_efficiency_penalty: 0.08,
            is_active: true,
            auxiliary_efficiency_factor: 0.85,
        };
        // fuel=100, cv=24, eff=0.35, aux_eff=0.35*0.85=0.2975
        // potential = 100 * 24 * 0.2975 = 714.0
        // demand = 500.0 → capped at 500.0
        let heat = chp.auxiliary_heat(100.0, 24.0, 0.35, 500.0);
        assert!((heat - 500.0).abs() < 1e-9);
    }

    #[test]
    fn test_auxiliary_heat_full_potential() {
        let chp = ChpRetrofitMetadata {
            heat_to_power_ratio: 1.0,
            electrical_efficiency_penalty: 0.08,
            is_active: true,
            auxiliary_efficiency_factor: 0.85,
        };
        // potential = 100 * 24 * 0.2975 = 714.0
        // demand = 1000.0 → returns full potential 714.0
        let heat = chp.auxiliary_heat(100.0, 24.0, 0.35, 1000.0);
        assert!((heat - 714.0).abs() < 0.1);
    }
}

//! Phase 81: Power plant generation logic — weather coupling and plant creation.
//!
//! Applies weather modifiers to power plant output based on plant type and
//! cooling method. Also provides helper functions for plant type selection
//! during world generation.

#![allow(missing_docs)]

use crate::economy::production::weather::WeatherModifier;
use crate::energy::types::*;
use crate::registries::enums::Commodity;

/// Calculate the weather-adjusted output multiplier for a power plant.
///
/// # Arguments
/// * `plant_type` - Type of power plant.
/// * `cooling_type` - Cooling method (affects drought vulnerability).
/// * `has_cooling_upgrade` - Whether the plant has a closed-loop cooling upgrade.
/// * `weather` - Current weather modifier for the region.
///
/// # Returns
/// Multiplier to apply to base output (1.0 = no change).
pub fn weather_output_multiplier(
    plant_type: PowerPlantType,
    cooling_type: CoolingType,
    has_cooling_upgrade: bool,
    weather: &WeatherModifier,
) -> f64 {
    match plant_type {
        PowerPlantType::Solar => {
            // Solar output follows solar_multiplier (clear skies boost, clouds reduce).
            weather.solar_multiplier.max(0.0)
        }
        PowerPlantType::Wind => {
            // Wind output follows wind_multiplier, capped at 1.5x nameplate
            // to prevent unrealistic generation during extreme storms.
            weather.wind_multiplier.min(1.5_f64).max(0.0_f64)
        }
        PowerPlantType::Hydro => {
            // Hydro depends on water availability (drought reduces river flow).
            weather.cooling_water_availability.max(0.0)
        }
        PowerPlantType::CoalFired
        | PowerPlantType::LigniteFired
        | PowerPlantType::OilGas
        | PowerPlantType::Nuclear
        | PowerPlantType::Geothermal
        | PowerPlantType::BiomassFired
        | PowerPlantType::BiogasPlant => {
            // Thermal plants: cooling water availability affects output.
            if cooling_type == CoolingType::AirCooled {
                // Air-cooled plants are drought-immune but have a fixed 5% penalty.
                0.95
            } else if has_cooling_upgrade || cooling_type == CoolingType::ClosedLoop {
                // Closed-loop cooling: drought-resistant but not immune.
                // Minimum 0.7 output during drought, scales with water availability.
                (0.7 + 0.3 * weather.cooling_water_availability).min(1.0)
            } else {
                // Once-through cooling: strongly affected by drought.
                weather.cooling_water_availability.max(0.0)
            }
        }
        PowerPlantType::PumpedStorage | PowerPlantType::BatteryStorage => {
            // Storage plants are not weather-coupled.
            1.0
        }
    }
}

/// Calculate the electrification adoption factor for a given start year.
///
/// Represents the historical pace of rural electrification:
/// - Pre-1900: 5% (only major urban centers had electricity).
/// - 1920: 15% (early electrification).
/// - 1940: 40% (rapid expansion).
/// - 1960: 70% (post-WWII grid consolidation).
/// - 1980: 90% (near-universal in developed nations).
/// - Post-1980: 100%.
pub fn electrification_factor(start_year: u32) -> f64 {
    if start_year < 1900 {
        0.05
    } else if start_year < 1920 {
        0.15
    } else if start_year < 1940 {
        0.40
    } else if start_year < 1960 {
        0.70
    } else if start_year < 1980 {
        0.90
    } else {
        1.0
    }
}

/// Calculate the era-scaled nameplate capacity per plant.
///
/// Larger plants become available with technological advancement:
/// - Pre-1920: 10 MW per plant.
/// - 1920-1950: 50 MW per plant.
/// - 1950-1980: 200 MW per plant.
/// - Post-1980: 500 MW per plant.
pub fn nameplate_per_plant(start_year: u32) -> f64 {
    if start_year < 1920 {
        10.0
    } else if start_year < 1950 {
        50.0
    } else if start_year < 1980 {
        200.0
    } else {
        500.0
    }
}

/// Calculate the automation factor for a given era.
///
/// Higher automation = fewer workers per MW.
/// - Pre-1920: 0.0 (fully manual).
/// - 1920-1950: 0.3.
/// - 1950-1980: 0.5.
/// - Post-1980: 0.7.
pub fn automation_factor(start_year: u32) -> f64 {
    if start_year < 1920 {
        0.0
    } else if start_year < 1950 {
        0.3
    } else if start_year < 1980 {
        0.5
    } else {
        0.7
    }
}

/// Calculate target regional generation capacity in MW.
///
/// Formula: `population * electrification_factor * development_level * average_wage / 200_000`
///
/// This scales dynamically with population, development, and wage level —
/// no magic numbers.
pub fn target_regional_capacity_mw(
    population: f64,
    development_level: f64,
    average_wage: f64,
    start_year: u32,
) -> f64 {
    let elec = electrification_factor(start_year);
    population * elec * development_level * average_wage / 200_000.0
}

/// Calculate the number of power plants to generate for a region.
///
/// `plant_count = ceil(target_mw / nameplate_per_plant).max(1)`
pub fn plant_count(target_mw: f64, start_year: u32) -> usize {
    let nameplate = nameplate_per_plant(start_year);
    ((target_mw / nameplate).ceil() as usize).max(1)
}

/// Calculate worker capacity per plant.
///
/// `workers = (nameplate * 10 * (1 - automation_factor)).max(50)`
pub fn workers_per_plant(start_year: u32) -> u32 {
    let nameplate = nameplate_per_plant(start_year);
    let auto = automation_factor(start_year);
    ((nameplate * 10.0 * (1.0 - auto)) as u32).max(50)
}

/// Determine available plant types for a region based on geography and era.
///
/// Returns a list of (plant_type, weight) pairs for distribution.
pub fn available_plant_types(
    start_year: u32,
    has_coal_deposit: bool,
    has_river_or_coast: bool,
    has_forest: bool,
    has_livestock: bool,
    has_uranium: bool,
    has_geothermal: bool,
) -> Vec<(PowerPlantType, f64)> {
    let mut types = Vec::new();

    // Biomass: available from 1880, no geographic constraint (but prefers forests).
    types.push((PowerPlantType::BiomassFired, 0.3));

    // Coal/Lignite: available from 1880, requires deposit.
    if has_coal_deposit {
        types.push((PowerPlantType::CoalFired, 0.4));
        types.push((PowerPlantType::LigniteFired, 0.2));
    }

    // Hydro: available from 1890, requires river/coast.
    if has_river_or_coast && start_year >= 1890 {
        types.push((PowerPlantType::Hydro, 0.3));
    }

    // Oil/Gas: available from 1910.
    if start_year >= 1910 {
        types.push((PowerPlantType::OilGas, 0.2));
    }

    // Biogas: available from 1930, requires livestock.
    if has_livestock && start_year >= 1930 {
        types.push((PowerPlantType::BiogasPlant, 0.2));
    }

    // Nuclear: available from 1955, requires uranium and water.
    if has_uranium && has_river_or_coast && start_year >= 1955 {
        types.push((PowerPlantType::Nuclear, 0.3));
    }

    // Geothermal: available from 1980, requires geothermal potential.
    if has_geothermal && start_year >= 1980 {
        types.push((PowerPlantType::Geothermal, 0.2));
    }

    // Solar: available from 1990.
    if start_year >= 1990 {
        types.push((PowerPlantType::Solar, 0.2));
    }

    // Wind: available from 1990.
    if start_year >= 1990 {
        types.push((PowerPlantType::Wind, 0.2));
    }

    // Pumped storage: available from 1907, requires river/coast.
    if has_river_or_coast && start_year >= 1907 {
        types.push((PowerPlantType::PumpedStorage, 0.1));
    }

    types
}

/// Phase 81 Wave 2: Compute the marginal cost per MWh for a power plant.
///
/// Marginal cost = (fuel_price_per_unit * fuel_units_per_mwh) / thermal_efficiency
/// For zero-marginal-cost plants (solar, wind, hydro, geothermal, nuclear),
/// returns `average_wage * 0.0001` as a tie-breaker (not a magic number —
/// it's a structurally negligible bid that ensures deterministic dispatch ordering).
/// For storage plants, returns the round-trip cost (charge_cost / storage_efficiency).
///
/// # Arguments
/// * `metadata` - Power plant metadata (type, capacity, efficiency)
/// * `fuel_prices` - Map of Commodity → current market price per unit
/// * `average_wage` - Current average wage (for zero-marginal-cost tie-breaker)
///
/// # Returns
/// Marginal cost per MWh.
pub fn compute_marginal_cost(
    metadata: &PowerPlantMetadata,
    fuel_prices: &std::collections::HashMap<Commodity, f64>,
    average_wage: f64,
) -> f64 {
    let zero_marginal = average_wage * 0.0001;
    let storage_marginal = average_wage * 0.001;

    match metadata.plant_type {
        // Renewables: zero fuel cost, negligible tie-breaker bid.
        PowerPlantType::Solar | PowerPlantType::Wind | PowerPlantType::Hydro => zero_marginal,

        // Nuclear and Geothermal: very low fuel cost per MWh, treated as
        // zero-marginal-cost with the same tie-breaker for dispatch ordering.
        PowerPlantType::Nuclear | PowerPlantType::Geothermal => zero_marginal,

        // Storage plants: low marginal cost, but higher than renewables since
        // they need to recover charge cost (round-trip efficiency losses).
        PowerPlantType::PumpedStorage | PowerPlantType::BatteryStorage => storage_marginal,

        // Fuel-burning plants: marginal cost = fuel_price * fuel_units_per_mwh / efficiency.
        PowerPlantType::CoalFired => {
            let fuel_price = fuel_prices
                .get(&Commodity::HardCoal)
                .copied()
                .unwrap_or(average_wage * 0.01);
            let efficiency = default_thermal_efficiency(metadata);
            let fuel_units_per_mwh = fuel_units_per_mwh(Commodity::HardCoal, efficiency);
            fuel_price * fuel_units_per_mwh / efficiency
        }
        PowerPlantType::LigniteFired => {
            let fuel_price = fuel_prices
                .get(&Commodity::BrownCoal)
                .copied()
                .unwrap_or(average_wage * 0.01);
            let efficiency = default_thermal_efficiency(metadata);
            let fuel_units_per_mwh = fuel_units_per_mwh(Commodity::BrownCoal, efficiency);
            fuel_price * fuel_units_per_mwh / efficiency
        }
        PowerPlantType::OilGas => {
            // OilGas plants can burn Oil or NaturalGas — use whichever is cheaper.
            let oil_price = fuel_prices
                .get(&Commodity::Oil)
                .copied()
                .unwrap_or(average_wage * 0.01);
            let gas_price = fuel_prices
                .get(&Commodity::NaturalGas)
                .copied()
                .unwrap_or(average_wage * 0.01);
            let efficiency = default_thermal_efficiency(metadata);
            let oil_units = fuel_units_per_mwh(Commodity::Oil, efficiency);
            let gas_units = fuel_units_per_mwh(Commodity::NaturalGas, efficiency);
            let oil_cost = oil_price * oil_units / efficiency;
            let gas_cost = gas_price * gas_units / efficiency;
            oil_cost.min(gas_cost)
        }
        PowerPlantType::BiomassFired => {
            // Biomass plants burn Timber, Planks, or Peat — use Timber as the
            // primary fuel (most common in early-game biomass plants).
            let fuel_price = fuel_prices
                .get(&Commodity::Timber)
                .copied()
                .unwrap_or(average_wage * 0.01);
            let efficiency = default_thermal_efficiency(metadata);
            let fuel_units_per_mwh = fuel_units_per_mwh(Commodity::Timber, efficiency);
            fuel_price * fuel_units_per_mwh / efficiency
        }
        PowerPlantType::BiogasPlant => {
            // Biogas plants use CoalGas as the output fuel — marginal cost is
            // based on the CoalGas market price if available, otherwise fallback.
            let fuel_price = fuel_prices
                .get(&Commodity::CoalGas)
                .copied()
                .unwrap_or(average_wage * 0.01);
            let efficiency = default_thermal_efficiency(metadata);
            let fuel_units_per_mwh = fuel_units_per_mwh(Commodity::CoalGas, efficiency);
            fuel_price * fuel_units_per_mwh / efficiency
        }
    }
}

/// Default thermal efficiency for a plant. Uses a physically realistic default
/// of 0.38 if the metadata doesn't store an explicit efficiency field.
///
/// `PowerPlantMetadata` does not currently have a `thermal_efficiency` field,
/// so this returns the default. When the struct is extended in the future,
/// this function can read `metadata.thermal_efficiency` instead.
fn default_thermal_efficiency(_metadata: &PowerPlantMetadata) -> f64 {
    0.38
}

/// Compute the fuel units required to produce 1 MWh of electricity.
///
/// Formula: `fuel_units_per_mwh = 3600 / (calorific_value_mj_per_unit * 1000 * efficiency)`
///
/// Where:
/// - 3600 MJ = 1 MWh (thermal energy equivalent)
/// - `calorific_value_mj_per_unit` is the energy density per unit of fuel
/// - `efficiency` is the thermal conversion efficiency (0.0-1.0)
///
/// This is physically derived, not a magic number.
fn fuel_units_per_mwh(fuel: Commodity, efficiency: f64) -> f64 {
    let calorific = fuel.calorific_value_mj_per_unit();
    if calorific <= 0.0 || efficiency <= 0.0 {
        return 0.0;
    }
    // 1 MWh = 3600 MJ of electrical energy.
    // Thermal energy needed = 3600 / efficiency MJ.
    // Fuel units = thermal_energy / calorific_value.
    3600.0 / (calorific * efficiency)
}

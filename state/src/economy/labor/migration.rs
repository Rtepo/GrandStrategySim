//! Cross-country migration flows (Phase 15B).
//!
//! Each turn, migration pressure is calculated per country based on unrest,
//! poverty, wage differentials, and climate disasters. Migrants flow from
//! high-pressure countries to low-pressure countries.
//!
//! # Design: Two-Pass Settlement
//!
//! Cross-country migration moves population between different `Country` entities
//! in the global state. To avoid holding mutable references to multiple countries
//! simultaneously, we use a two-pass approach:
//!
//! 1. **Collection pass** (read-only): Calculate migration pressure for each
//!    country and produce a list of `MigrationFlow` records.
//! 2. **Settlement pass** (mutable): Apply each flow — deduct population from
//!    origin, add to destination, create `ImmigrantCohort` entries.
//!
//! Population is strictly conserved: origin loses exactly what destination gains.

#![allow(missing_docs)]

use crate::economy::legal_status::LegalStatus;
use crate::entities::Building;
use crate::politics::laws::{DeportationPolicy, MigrationFlow, MigrationReason};
use crate::registries::enums::Commodity;
use crate::state::macro_data::ImmigrantCohort;
use crate::state::Country;
use serde::{Deserialize, Serialize};
use serde_json::Map;
use std::collections::HashMap;

/// Phase R6: Migration configuration — replaces all magic numbers in migration.rs
/// with configurable, serializable values.
///
/// All nominal fiat references (gdp_per_capita_reference, wage_reference) are
/// intended to be overridden at runtime from global macroeconomic aggregates
/// (e.g. world-average GDP per capita, world-average wage) to ensure
/// inflation-proof scaling across Turn 1 to Turn 1,000.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationConfig {
    /// Maximum fraction of population that can emigrate per turn (~0.5% annual).
    #[serde(default = "default_max_emigration_rate")]
    pub max_emigration_rate: f64,
    /// Famine-level emigration rate cap (~2.4% annual).
    #[serde(default = "default_famine_emigration_rate")]
    pub famine_emigration_rate: f64,
    /// Minimum population to remain after emigration.
    #[serde(default = "default_min_population")]
    pub min_population: u64,
    /// PassengerTransport units required per migrant for cross-country travel.
    #[serde(default = "default_transport_units_per_migrant")]
    pub transport_units_per_migrant: f64,
    /// Weight of unrest in migration pressure.
    #[serde(default = "default_unrest_weight")]
    pub unrest_weight: f64,
    /// Weight of poverty in migration pressure.
    #[serde(default = "default_poverty_weight")]
    pub poverty_weight: f64,
    /// Weight of wage differential in migration pressure.
    #[serde(default = "default_wage_weight")]
    pub wage_weight: f64,
    /// Weight of disaster in migration pressure.
    #[serde(default = "default_disaster_weight")]
    pub disaster_weight: f64,
    /// Weight of unemployment in migration pressure.
    #[serde(default = "default_unemployment_weight")]
    pub unemployment_weight: f64,
    /// Weight of subsistence wage shortfall in migration pressure.
    #[serde(default = "default_subsistence_weight")]
    pub subsistence_weight: f64,
    /// Security index below which unrest-driven emigration triggers.
    #[serde(default = "default_unrest_threshold")]
    pub unrest_threshold: f64,
    /// Default safety index when no crime data is available.
    #[serde(default = "default_safety_index")]
    pub default_safety_index: f64,
    /// GDP per capita reference for poverty normalization.
    /// Intended to be overridden with world-average GDP per capita at runtime.
    #[serde(default = "default_gdp_per_capita_reference")]
    pub gdp_per_capita_reference: f64,
    /// Average wage reference for wage-pressure normalization.
    /// Intended to be overridden with world-average wage at runtime.
    #[serde(default = "default_wage_reference")]
    pub wage_reference: f64,
    /// Disaster count normalization divisor (disasters / this = pressure).
    #[serde(default = "default_disaster_normalization")]
    pub disaster_normalization: f64,
    /// Unemployment rate above which unemployment pressure triggers.
    #[serde(default = "default_unemployment_threshold")]
    pub unemployment_threshold: f64,
    /// Minimum attractiveness floor (prevents zero-attraction edge cases).
    #[serde(default = "default_min_attractiveness")]
    pub min_attractiveness: f64,
    /// Minimum pressure to trigger emigration.
    #[serde(default = "default_min_pressure")]
    pub min_pressure: f64,
    /// Schengen attractiveness bonus per partner country.
    #[serde(default = "default_schengen_bonus_per_partner")]
    pub schengen_bonus_per_partner: f64,
    /// Maximum Schengen attractiveness bonus cap.
    #[serde(default = "default_schengen_bonus_cap")]
    pub schengen_bonus_cap: f64,
    /// Border enforcement reduction factor (fraction of migrants blocked).
    #[serde(default = "default_border_enforcement_factor")]
    pub border_enforcement_factor: f64,
    /// Border capacity normalization divisor for deportation capacity.
    #[serde(default = "default_border_capacity_normalization")]
    pub border_capacity_normalization: f64,
    /// Fraction of illegal immigrants deported per turn under Selective policy.
    #[serde(default = "default_selective_deportation_fraction")]
    pub selective_deportation_fraction: f64,
    /// Remittance rate for TemporaryWorker immigrants.
    #[serde(default = "default_temporary_worker_remittance_rate")]
    pub temporary_worker_remittance_rate: f64,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        MigrationConfig {
            max_emigration_rate: default_max_emigration_rate(),
            famine_emigration_rate: default_famine_emigration_rate(),
            min_population: default_min_population(),
            transport_units_per_migrant: default_transport_units_per_migrant(),
            unrest_weight: default_unrest_weight(),
            poverty_weight: default_poverty_weight(),
            wage_weight: default_wage_weight(),
            disaster_weight: default_disaster_weight(),
            unemployment_weight: default_unemployment_weight(),
            subsistence_weight: default_subsistence_weight(),
            unrest_threshold: default_unrest_threshold(),
            default_safety_index: default_safety_index(),
            gdp_per_capita_reference: default_gdp_per_capita_reference(),
            wage_reference: default_wage_reference(),
            disaster_normalization: default_disaster_normalization(),
            unemployment_threshold: default_unemployment_threshold(),
            min_attractiveness: default_min_attractiveness(),
            min_pressure: default_min_pressure(),
            schengen_bonus_per_partner: default_schengen_bonus_per_partner(),
            schengen_bonus_cap: default_schengen_bonus_cap(),
            border_enforcement_factor: default_border_enforcement_factor(),
            border_capacity_normalization: default_border_capacity_normalization(),
            selective_deportation_fraction: default_selective_deportation_fraction(),
            temporary_worker_remittance_rate: default_temporary_worker_remittance_rate(),
        }
    }
}

fn default_max_emigration_rate() -> f64 { 0.0002 }
fn default_famine_emigration_rate() -> f64 { 0.001 }
fn default_min_population() -> u64 { 100 }
fn default_transport_units_per_migrant() -> f64 { 100.0 }
fn default_unrest_weight() -> f64 { 0.25 }
fn default_poverty_weight() -> f64 { 0.20 }
fn default_wage_weight() -> f64 { 0.15 }
fn default_disaster_weight() -> f64 { 0.05 }
fn default_unemployment_weight() -> f64 { 0.15 }
fn default_subsistence_weight() -> f64 { 0.20 }
fn default_unrest_threshold() -> f64 { 40.0 }
fn default_safety_index() -> f64 { 80.0 }
fn default_gdp_per_capita_reference() -> f64 { 10_000.0 }
fn default_wage_reference() -> f64 { 5_000.0 }
fn default_disaster_normalization() -> f64 { 10.0 }
fn default_unemployment_threshold() -> f64 { 0.10 }
fn default_min_attractiveness() -> f64 { 0.01 }
fn default_min_pressure() -> f64 { 0.01 }
fn default_schengen_bonus_per_partner() -> f64 { 0.10 }
fn default_schengen_bonus_cap() -> f64 { 0.50 }
fn default_border_enforcement_factor() -> f64 { 0.5 }
fn default_border_capacity_normalization() -> f64 { 100.0 }
fn default_selective_deportation_fraction() -> f64 { 0.10 }
fn default_temporary_worker_remittance_rate() -> f64 { 0.10 }

/// Sum BorderEnforcementCapacity from all buildings' last_production.
///
/// # Arguments
/// * `buildings` - Slice of buildings to scan.
///
/// # Returns
/// Total border enforcement capacity.
pub fn sum_border_enforcement_capacity(buildings: &[Building]) -> f64 {
    buildings
        .iter()
        .map(|b| {
            *b.last_production
                .get(&Commodity::BorderEnforcementCapacity)
                .unwrap_or(&0.0)
        })
        .sum()
}

/// Phase F2: Sum available PassengerTransport inventory from transport buildings.
///
/// This represents the physical capacity to move people across borders.
/// Migration is clamped to this capacity — people cannot teleport.
///
/// # Arguments
/// * `buildings` - Slice of buildings to scan.
///
/// # Returns
/// Total available PassengerTransport units.
pub fn sum_passenger_transport_capacity(buildings: &[Building]) -> f64 {
    buildings
        .iter()
        .filter(|b| b.sector == crate::registries::enums::Sector::TransportLogistics)
        .map(|b| b.inventory.get(&Commodity::PassengerTransport).copied().unwrap_or(0.0))
        .sum()
}

/// Calculate migration pressure for a country (0.0 = no pressure, 1.0 = max).
///
/// # Arguments
/// * `country` - Country to evaluate.
/// * `buildings` - Buildings for border enforcement capacity.
/// * `disaster_count` - Number of active disasters this turn.
///
/// # Returns
/// Migration pressure score in [0.0, 1.0].
///
/// # Rules
/// * Pressure is driven by unrest (security index), poverty rate, wage level,
///   recent disasters, unemployment (Phase 31), and subsistence wage shortfall
///   (Phase 31).
/// * Higher pressure → more people want to leave.
pub fn calculate_migration_pressure(
    country: &Country,
    _buildings: &[Building],
    disaster_count: u32,
    config: &MigrationConfig,
) -> f64 {
    let population = country.budget.population as f64;
    if population < config.min_population as f64 {
        return 0.0;
    }

    // Unrest: security index below threshold → high unrest
    let security_index = get_nested_f64(
        &country.macro_indicators.extra,
        "crime_rate",
        "safety_index",
    )
    .unwrap_or(config.default_safety_index);
    let unrest = ((config.unrest_threshold - security_index) / config.unrest_threshold)
        .max(0.0)
        .min(1.0);

    // Poverty: GDP per capita below reference → high poverty
    let gdp_pc = country.budget.gdp / population.max(1.0);
    let poverty = (1.0 - (gdp_pc / config.gdp_per_capita_reference).min(1.0)).max(0.0);

    // Wage: low average wage → more likely to emigrate
    let avg_wage = country.macro_indicators.average_wage;
    let wage_pressure = (1.0 - (avg_wage / config.wage_reference).min(1.0)).max(0.0);

    // Disaster: recent disasters increase pressure
    let disaster_pressure = (disaster_count as f64 / config.disaster_normalization).min(1.0);

    // Phase 31: Unemployment pressure (above threshold increases pressure).
    let unemployment_rate = country.macro_indicators.labor_market.unemployment_rate / 100.0;
    let unemployment_pressure = (unemployment_rate - config.unemployment_threshold)
        .max(0.0)
        .min(1.0);

    // Phase 31: Subsistence wage pressure (avg wage below subsistence → pressure).
    let subsistence_wage = crate::politics::crisis_management::compute_subsistence_wage(
        &rustc_hash::FxHashMap::default(), // No market prices available here; use fallback
    );
    let subsistence_pressure = if subsistence_wage > 0.0 && avg_wage < subsistence_wage {
        (1.0 - avg_wage / subsistence_wage).min(1.0)
    } else {
        0.0
    };

    let pressure = config.unrest_weight * unrest
        + config.poverty_weight * poverty
        + config.wage_weight * wage_pressure
        + config.disaster_weight * disaster_pressure
        + config.unemployment_weight * unemployment_pressure
        + config.subsistence_weight * subsistence_pressure;

    pressure.min(1.0)
}

/// Calculate the number of emigrants from a country this turn.
///
/// # Arguments
/// * `country` - Origin country.
/// * `buildings` - Buildings for border enforcement.
/// * `pressure` - Migration pressure score [0.0, 1.0].
/// * `border_enforcement_ratio` - Border enforcement [0.0, 1.0].
///
/// # Returns
/// Number of emigrants (positive integer).
///
/// # Rules
/// * `emigrants = population * pressure * MAX_EMIGRATION_RATE * (1.0 - enforcement_ratio)`
/// * If borders are open, enforcement does not reduce migration.
/// * Minimum population is preserved.
pub fn calculate_emigrants(
    country: &Country,
    pressure: f64,
    border_enforcement_ratio: f64,
    config: &MigrationConfig,
) -> u64 {
    let migration_law = country.politics.migration_law.as_ref();

    let open_borders = migration_law.map(|m| m.open_borders).unwrap_or(false);

    let enforcement_factor = if open_borders {
        1.0 // Open borders: no enforcement reduction
    } else {
        1.0 - border_enforcement_ratio.clamp(0.0, 1.0)
    };

    // Phase 31: Use famine emigration rate when wage is below subsistence.
    let avg_wage = country.macro_indicators.average_wage;
    let subsistence_wage = crate::politics::crisis_management::compute_subsistence_wage(
        &rustc_hash::FxHashMap::default(),
    );
    let famine_mode = subsistence_wage > 0.0 && avg_wage < subsistence_wage;
    let rate = if famine_mode {
        config.famine_emigration_rate
    } else {
        config.max_emigration_rate
    };

    let population = country.budget.population;
    let emigrants = (population as f64 * pressure * rate * enforcement_factor) as u64;

    // Ensure minimum population remains
    let max_emigrants = population.saturating_sub(config.min_population);
    emigrants.min(max_emigrants)
}

/// Collection pass: compute migration flows for all countries.
///
/// # Arguments
/// * `countries` - Map of country name → (country ref, buildings ref, disaster count).
/// * `turn` - Current turn number.
///
/// # Returns
/// Vec of `MigrationFlow` records to be applied in the settlement pass.
///
/// # Rules
/// * Each country with pressure > 0.0 produces emigrants.
/// * Emigrants are distributed to countries with lower pressure (attractiveness).
/// * Population is conserved: total emigrants = total immigrants.
/// * Border enforcement reduces actual migrants (unless open borders).
pub fn collect_migration_flows(
    countries: &HashMap<String, (&Country, &[Building], u32)>,
    turn: u32,
    treaty_registry: Option<&crate::international::treaties::TreatyRegistry>,
    config: &MigrationConfig,
) -> Vec<MigrationFlow> {
    // Phase 67: Helper to check if two countries share a Schengen free movement treaty.
    let has_schengen = |a: &str, b: &str| -> bool {
        treaty_registry.is_some_and(|reg| {
            reg.has_active_clause_between(
                a,
                b,
                &crate::international::treaties::TreatyClause::SchengenFreeMovement,
            )
        })
    };

    // Step 1: Calculate pressure, border enforcement, and transport capacity per country.
    let mut pressures: HashMap<String, f64> = HashMap::new();
    let mut border_capacities: HashMap<String, f64> = HashMap::new();
    let mut transport_capacities: HashMap<String, f64> = HashMap::new();

    for (name, (country, buildings, disaster_count)) in countries.iter() {
        let pressure = calculate_migration_pressure(country, buildings, *disaster_count, config);
        pressures.insert(name.clone(), pressure);

        let border_cap = sum_border_enforcement_capacity(buildings);
        border_capacities.insert(name.clone(), border_cap);

        // Phase F2: PassengerTransport capacity limits how many people can
        // physically travel out of the country. Without transport, migration
        // is zero — people cannot teleport across borders (Rule 19).
        let transport_cap = sum_passenger_transport_capacity(buildings);
        transport_capacities.insert(name.clone(), transport_cap);
    }

    // Step 2: Calculate attractiveness (inverse of pressure, weighted by GDP).
    // Phase 67: Schengen free movement treaties boost attractiveness between participants.
    let mut attractors: Vec<(String, f64)> = countries
        .iter()
        .map(|(name, (country, _, _))| {
            let pressure = pressures[name];
            let gdp_pc = country.budget.gdp / (country.budget.population as f64).max(1.0);
            let mut attractiveness = (1.0 - pressure)
                * (gdp_pc / config.gdp_per_capita_reference)
                    .min(1.0)
                    .max(config.min_attractiveness);
            // Phase 67: Schengen bonus is applied per-origin during distribution,
            // but we also boost the base attractiveness for Schengen members
            // since they have more open economies.
            let schengen_partners = treaty_registry.map_or(0, |reg| {
                reg.treaties
                    .iter()
                    .filter(|t| {
                        t.is_active()
                            && t.participants.contains(name)
                            && t.clauses.contains(
                                &crate::international::treaties::TreatyClause::SchengenFreeMovement,
                            )
                    })
                    .flat_map(|t| t.participants.iter())
                    .filter(|p| *p != name && countries.contains_key(*p))
                    .count()
            });
            if schengen_partners > 0 {
                attractiveness *= 1.0
                    + (config.schengen_bonus_per_partner * schengen_partners as f64)
                        .min(config.schengen_bonus_cap);
            }
            (name.clone(), attractiveness)
        })
        .collect();
    attractors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let total_attractiveness: f64 = attractors.iter().map(|(_, a)| *a).sum();

    // Step 3: Generate flows.
    let mut flows = Vec::new();

    for (origin_name, (origin_country, _, _)) in countries.iter() {
        let pressure = pressures[origin_name];
        if pressure < config.min_pressure {
            continue;
        }

        let origin_border_cap = border_capacities[origin_name];
        // Border enforcement ratio: capacity relative to total border crossings (approx population)
        let origin_enforcement =
            (origin_border_cap / (origin_country.budget.population as f64).max(1.0)).min(1.0);

        let emigrants = calculate_emigrants(origin_country, pressure, origin_enforcement, config);
        if emigrants == 0 {
            continue;
        }

        // Phase F2: Clamp emigration by available PassengerTransport capacity.
        // People cannot teleport across borders — they need physical transport
        // (Rule 19: Strict Logistical Causality).
        let origin_transport_cap = transport_capacities[origin_name];
        let max_emigrants_by_transport =
            (origin_transport_cap / config.transport_units_per_migrant) as u64;
        let emigrants = emigrants.min(max_emigrants_by_transport);
        if emigrants == 0 {
            continue;
        }

        // Track remaining transport capacity as we allocate flows.
        let mut remaining_transport = origin_transport_cap;

        // Determine reason based on dominant pressure factor
        let reason = determine_migration_reason(origin_country, pressure);

        // Distribute emigrants to attractive destinations
        let remaining = if total_attractiveness > 0.0 {
            for (dest_name, dest_attr) in &attractors {
                if dest_name == origin_name {
                    continue;
                }
                if remaining_transport < config.transport_units_per_migrant {
                    break; // No transport capacity left for more migrants
                }
                let share = dest_attr / total_attractiveness;
                let dest_emigrants = (emigrants as f64 * share) as u64;
                if dest_emigrants == 0 {
                    continue;
                }

                // Check destination border enforcement
                let dest_country = countries[dest_name].0;
                let dest_border_cap = border_capacities[dest_name];
                let dest_enforcement =
                    (dest_border_cap / (dest_country.budget.population as f64).max(1.0)).min(1.0);

                let migration_law = dest_country.politics.migration_law.as_ref();
                let open_borders = migration_law.map(|m| m.open_borders).unwrap_or(false);

                // Phase 67: Schengen free movement — zero border enforcement between participants.
                let schengen_active = has_schengen(origin_name, dest_name);

                let actual_migrants = if open_borders || schengen_active {
                    dest_emigrants
                } else {
                    (dest_emigrants as f64 * (1.0 - dest_enforcement * config.border_enforcement_factor)) as u64
                };

                if actual_migrants > 0 {
                    // Phase F2: Clamp by remaining transport capacity.
                    let max_by_transport =
                        (remaining_transport / config.transport_units_per_migrant) as u64;
                    let actual_migrants = actual_migrants.min(max_by_transport);
                    if actual_migrants == 0 {
                        continue;
                    }
                    let transport_consumed =
                        actual_migrants as f64 * config.transport_units_per_migrant;
                    remaining_transport -= transport_consumed;

                    flows.push(MigrationFlow {
                        origin_country: origin_name.clone(),
                        dest_country: dest_name.clone(),
                        count: actual_migrants as i64,
                        reason: reason.clone(),
                        turn,
                        transport_units_consumed: transport_consumed,
                    });
                }
            }
            flows
                .iter()
                .filter(|f| f.origin_country == *origin_name)
                .map(|f| f.count as u64)
                .sum::<u64>()
        } else {
            0
        };

        // If border enforcement blocked some, they become illegal immigrants
        // in the destination (simplified: they still go but are recorded as illegal)
        let blocked = emigrants.saturating_sub(remaining);
        if blocked > 0 && total_attractiveness > 0.0 {
            // Send blocked migrants to the top attractor as illegal immigrants
            if let Some((top_dest, _)) = attractors.first() {
                if top_dest != origin_name {
                    // Phase F2: Illegal border crossings still consume transport.
                    let max_by_transport =
                        (remaining_transport / config.transport_units_per_migrant) as u64;
                    let blocked = blocked.min(max_by_transport);
                    if blocked > 0 {
                        let transport_consumed = blocked as f64 * config.transport_units_per_migrant;
                        flows.push(MigrationFlow {
                            origin_country: origin_name.clone(),
                            dest_country: top_dest.clone(),
                            count: blocked as i64,
                            reason: MigrationReason::Economic,
                            turn,
                            transport_units_consumed: transport_consumed,
                        });
                    }
                }
            }
        }
    }

    flows
}

/// Determine the primary reason for migration based on country conditions.
fn determine_migration_reason(country: &Country, _pressure: f64) -> MigrationReason {
    let security_index = get_nested_f64(
        &country.macro_indicators.extra,
        "crime_rate",
        "safety_index",
    )
    .unwrap_or(80.0);

    if security_index < 20.0 {
        MigrationReason::Unrest
    } else if security_index < 35.0 {
        MigrationReason::Persecution
    } else {
        MigrationReason::Economic
    }
}

/// Settlement pass: apply migration flows to countries.
///
/// # Arguments
/// * `countries` - Mutable map of country name → country.
/// * `flows` - Migration flows to apply.
///
/// # Rules
/// * Origin population decreases by flow count.
/// * Origin class savings are debited by per-capita share × emigrant count (F3).
/// * Destination population increases by flow count.
/// * Destination class savings are credited with emigrant wealth (F3).
/// * ImmigrantCohort entries created in destination.
/// * Population and wealth are strictly conserved.
pub fn apply_migration_flows(
    countries: &mut HashMap<String, &mut Country>,
    buildings_map: &mut HashMap<String, &mut [Building]>,
    flows: &[MigrationFlow],
    config: &MigrationConfig,
) {
    // Aggregate by origin and destination to minimize mutations
    let mut origin_outflows: HashMap<String, u64> = HashMap::new();
    let mut dest_inflows: HashMap<String, Vec<(u64, &MigrationReason)>> = HashMap::new();
    // Phase F2: Aggregate transport units consumed per origin country.
    let mut origin_transport_consumed: HashMap<String, f64> = HashMap::new();

    for flow in flows {
        *origin_outflows
            .entry(flow.origin_country.clone())
            .or_insert(0) += flow.count as u64;
        dest_inflows
            .entry(flow.dest_country.clone())
            .or_default()
            .push((flow.count as u64, &flow.reason));
        *origin_transport_consumed
            .entry(flow.origin_country.clone())
            .or_insert(0.0) += flow.transport_units_consumed;
    }

    // Phase F2: Consume PassengerTransport inventory from origin country buildings.
    // This physically depletes the transport capacity that was used to move migrants.
    for (origin_name, total_consumed) in &origin_transport_consumed {
        if *total_consumed <= 0.0 {
            continue;
        }
        if let Some(buildings) = buildings_map.get_mut(origin_name) {
            // Consume proportionally from all transport buildings that have inventory.
            let total_available: f64 = buildings
                .iter()
                .filter(|b| b.sector == crate::registries::enums::Sector::TransportLogistics)
                .map(|b| b.inventory.get(&Commodity::PassengerTransport).copied().unwrap_or(0.0))
                .sum();
            if total_available <= 0.0 {
                continue;
            }
            let consume_ratio = (*total_consumed / total_available).min(1.0);
            for b in buildings.iter_mut() {
                if b.sector != crate::registries::enums::Sector::TransportLogistics {
                    continue;
                }
                if let Some(transport) = b.inventory.get_mut(&Commodity::PassengerTransport) {
                    let consumed = *transport * consume_ratio;
                    *transport -= consumed;
                }
            }
        }
    }

    // Phase F3: Extract emigrant wealth from origin before reducing population.
    // Compute per-capita savings across all classes, then debit proportionally.
    // The extracted wealth is routed to destinations via a wealth map.
    let mut origin_wealth: HashMap<String, f64> = HashMap::new();
    for (origin_name, total_out) in &origin_outflows {
        if let Some(country) = countries.get_mut(origin_name) {
            let total_class_pop: i64 = country
                .regions
                .iter()
                .flat_map(|r| {
                    r.class_demographics
                        .rural_classes
                        .values()
                        .chain(r.class_demographics.urban_classes.values())
                })
                .map(|d| d.population)
                .sum();

            if total_class_pop <= 0 || *total_out == 0 {
                continue;
            }

            let total_savings: f64 = country
                .regions
                .iter()
                .flat_map(|r| {
                    r.class_demographics
                        .rural_classes
                        .values()
                        .chain(r.class_demographics.urban_classes.values())
                })
                .map(|d| d.savings)
                .sum();

            let per_capita = total_savings / total_class_pop as f64;
            let emigrant_wealth = per_capita * *total_out as f64;

            // Debit proportionally from all classes by population share.
            for region in &mut country.regions {
                for demo in region.class_demographics.rural_classes.values_mut() {
                    let share = demo.population as f64 / total_class_pop as f64;
                    let debit = (emigrant_wealth * share).min(demo.savings);
                    demo.savings -= debit;
                }
                for demo in region.class_demographics.urban_classes.values_mut() {
                    let share = demo.population as f64 / total_class_pop as f64;
                    let debit = (emigrant_wealth * share).min(demo.savings);
                    demo.savings -= debit;
                }
            }

            origin_wealth.insert(origin_name.clone(), emigrant_wealth);
        }
    }

    // Apply outflows (deduct from origin population)
    // Phase 36: Use bottom-up distribution instead of direct budget.population write.
    for (origin_name, total_out) in &origin_outflows {
        if let Some(country) = countries.get_mut(origin_name) {
            let delta = -(*total_out as i64);
            crate::economy::labor::labor::distribute_population_delta_and_reconcile(country, delta);
        }
    }

    // Compute total wealth arriving at each destination from all origins.
    // We distribute origin wealth proportionally to destination inflow shares.
    let mut dest_wealth: HashMap<String, f64> = HashMap::new();
    let total_inflow: u64 = flows.iter().map(|f| f.count as u64).sum();
    if total_inflow > 0 {
        for flow in flows {
            let origin_w = origin_wealth.get(&flow.origin_country).copied().unwrap_or(0.0);
            let origin_total_out = origin_outflows.get(&flow.origin_country).copied().unwrap_or(1);
            if origin_total_out == 0 {
                continue;
            }
            let flow_share = flow.count as f64 / origin_total_out as f64;
            *dest_wealth.entry(flow.dest_country.clone()).or_insert(0.0) += origin_w * flow_share;
        }
    }

    // Apply inflows (add to destination population + create immigrant cohorts)
    for (dest_name, inflows) in &dest_inflows {
        if let Some(country) = countries.get_mut(dest_name) {
            let total_in: u64 = inflows.iter().map(|(c, _)| *c).sum();
            if total_in == 0 {
                continue;
            }

            // Phase 36: Distribute population delta to class demographics bottom-up.
            crate::economy::labor::labor::distribute_population_delta_and_reconcile(
                country,
                total_in as i64,
            );

            // Phase F3: Credit emigrant wealth to destination class savings.
            let arriving_wealth = dest_wealth.get(dest_name).copied().unwrap_or(0.0);
            if arriving_wealth > 0.0 {
                let total_class_pop: i64 = country
                    .regions
                    .iter()
                    .flat_map(|r| {
                        r.class_demographics
                            .rural_classes
                            .values()
                            .chain(r.class_demographics.urban_classes.values())
                    })
                    .map(|d| d.population)
                    .sum();
                if total_class_pop > 0 {
                    for region in &mut country.regions {
                        for demo in region.class_demographics.rural_classes.values_mut() {
                            let share = demo.population as f64 / total_class_pop as f64;
                            demo.savings += arriving_wealth * share;
                        }
                        for demo in region.class_demographics.urban_classes.values_mut() {
                            let share = demo.population as f64 / total_class_pop as f64;
                            demo.savings += arriving_wealth * share;
                        }
                    }
                }
            }

            // Create immigrant cohort for total inflow
            // Phase 18A: Assign LegalStatus based on MigrationLaw
            let migration_law = country.politics.migration_law.as_ref();
            let open_borders = migration_law.map(|m| m.open_borders).unwrap_or(false);
            let visa_required = migration_law.map(|m| m.visa_required).unwrap_or(false);

            let legal_status = if open_borders {
                LegalStatus::Resident
            } else if visa_required {
                // Visa required but entered without one → Illegal
                // Refugees/asylum seekers get TemporaryWorker status
                let has_refugees = inflows.iter().any(|(_, r)| {
                    matches!(r, MigrationReason::Unrest | MigrationReason::Persecution)
                });
                if has_refugees {
                    LegalStatus::TemporaryWorker
                } else {
                    LegalStatus::Illegal
                }
            } else {
                // No visa required → legal temporary worker
                LegalStatus::TemporaryWorker
            };

            let remittance_rate = if legal_status == LegalStatus::TemporaryWorker {
                config.temporary_worker_remittance_rate
            } else {
                0.0
            };

            country
                .macro_indicators
                .demographics
                .immigrant_cohorts
                .push(ImmigrantCohort {
                    count: total_in as f64,
                    seniority: 0,
                    legal_status,
                    remittance_rate,
                    starting_savings: arriving_wealth,
                    extra: Map::new(),
                });

            // Update border state if present
            if let Some(_border_state) = &mut country.politics.border_state {
                for (count, reason) in inflows {
                    if matches!(reason, MigrationReason::Unrest)
                        || matches!(reason, MigrationReason::Persecution)
                    {
                        // These are refugees/asylum seekers, not illegal immigrants
                        continue;
                    }
                    // Economic migrants without visa are illegal if visa_required
                    let migration_law = country.politics.migration_law.as_ref();
                    let visa_required = migration_law.map(|m| m.visa_required).unwrap_or(false);
                    if visa_required {
                        country.macro_indicators.demographics.illegal_immigrants += *count as f64;
                    }
                }
            }
        }
    }
}

/// Process deportation based on deportation policy.
///
/// # Arguments
/// * `country` - Country to process deportations for.
/// * `border_capacity` - Border enforcement capacity.
///
/// # Returns
/// `(deported_count, deported_wealth)` — the number of illegal immigrants
/// deported and the total savings extracted from them. The caller must credit
/// `deported_wealth` to `foreign_sector_balance` (deportees take their money
/// out of the country).
///
/// # Rules
/// * Only deports if `DeportationPolicy` is not `None`.
/// * Deported population is removed (returns to origin or disappears).
/// * `MassDeportation` removes all illegal immigrants.
/// * `Selective` removes 10% per turn.
/// * Phase F4: Per-capita savings are extracted from classes with
///   `illegal_population` before population removal. This prevents phantom
///   wealth from remaining in the class savings pool after deportees leave.
pub fn process_deportations(
    country: &mut Country,
    border_capacity: f64,
    config: &MigrationConfig,
) -> (u64, f64) {
    let policy = country
        .politics
        .migration_law
        .as_ref()
        .map(|m| &m.deportation_policy)
        .cloned()
        .unwrap_or(DeportationPolicy::None);

    if matches!(policy, DeportationPolicy::None) {
        return (0, 0.0);
    }

    let illegal = country.macro_indicators.demographics.illegal_immigrants;
    if illegal <= 0.0 {
        return (0, 0.0);
    }

    // Border capacity limits how many can be deported per turn
    let capacity_factor = (border_capacity / config.border_capacity_normalization).min(1.0);

    let deport_count = match policy {
        DeportationPolicy::None => 0.0,
        DeportationPolicy::Selective => {
            (illegal * config.selective_deportation_fraction * capacity_factor).floor()
        }
        DeportationPolicy::MassDeportation => (illegal * capacity_factor).floor(),
    };

    let deport_count = deport_count as u64;
    if deport_count == 0 {
        return (0, 0.0);
    }

    // Phase F4: Extract per-capita savings from classes with illegal_population.
    // Distribute the deportation count proportionally across all classes that
    // have illegal_population, then extract per-capita savings from each.
    let mut total_deported_wealth: f64 = 0.0;
    let mut remaining_to_deport = deport_count as i64;

    for region in &mut country.regions {
        if remaining_to_deport <= 0 {
            break;
        }
        for demo in region.class_demographics.rural_classes.values_mut() {
            if remaining_to_deport <= 0 {
                break;
            }
            if demo.illegal_population <= 0 || demo.population <= 0 {
                continue;
            }
            let from_this_class = remaining_to_deport.min(demo.illegal_population);
            let per_capita = demo.savings / demo.population as f64;
            let wealth = (per_capita * from_this_class as f64).min(demo.savings);
            demo.savings -= wealth;
            demo.illegal_population -= from_this_class;
            total_deported_wealth += wealth;
            remaining_to_deport -= from_this_class;
        }
        for demo in region.class_demographics.urban_classes.values_mut() {
            if remaining_to_deport <= 0 {
                break;
            }
            if demo.illegal_population <= 0 || demo.population <= 0 {
                continue;
            }
            let from_this_class = remaining_to_deport.min(demo.illegal_population);
            let per_capita = demo.savings / demo.population as f64;
            let wealth = (per_capita * from_this_class as f64).min(demo.savings);
            demo.savings -= wealth;
            demo.illegal_population -= from_this_class;
            total_deported_wealth += wealth;
            remaining_to_deport -= from_this_class;
        }
    }

    // Remove from illegal immigrants
    country.macro_indicators.demographics.illegal_immigrants =
        (country.macro_indicators.demographics.illegal_immigrants - deport_count as f64).max(0.0);

    // Remove from population
    // Phase 36: Use bottom-up distribution instead of direct budget.population write.
    crate::economy::labor::labor::distribute_population_delta_and_reconcile(
        country,
        -(deport_count as i64),
    );

    // Record in border state
    if let Some(border_state) = &mut country.politics.border_state {
        border_state.deportations = deport_count as i64;
    }

    (deport_count, total_deported_wealth)
}

/// Helper: get nested f64 from serde Map.
fn get_nested_f64(map: &Map<String, serde_json::Value>, key1: &str, key2: &str) -> Option<f64> {
    map.get(key1)?.get(key2)?.as_f64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Building;
    use crate::politics::MigrationLaw;
    use crate::society::geography::{ClassDemographics, Region, RegionalClassDemographics, RuralClass};
    use crate::state::Country;

    /// Phase 36: Create a test region with class demographics matching the
    /// given population. This is needed because bottom-up reconciliation
    /// derives population from class demographics, not from budget.population.
    fn test_region_with_population(id: &str, country: &str, pop: i64) -> Region {
        let mut region = Region::default();
        region.id = id.to_string();
        region.owner_country = country.to_string();
        region.population = pop;
        // Put all population in a single rural class
        let mut rural = std::collections::BTreeMap::new();
        rural.insert(
            RuralClass::FreePeasant,
            ClassDemographics {
                population: pop,
                labor_participation: 0.55,
                ..Default::default()
            },
        );
        region.class_demographics = RegionalClassDemographics {
            rural_classes: rural,
            urban_classes: std::collections::BTreeMap::new(),
        };
        region
    }

    #[test]
    fn test_sum_border_capacity() {
        let mut b1 = Building::default();
        b1.last_production
            .insert(Commodity::BorderEnforcementCapacity, 10.0);
        let mut b2 = Building::default();
        b2.last_production
            .insert(Commodity::BorderEnforcementCapacity, 5.0);
        assert_eq!(sum_border_enforcement_capacity(&[b1, b2]), 15.0);
    }

    #[test]
    fn test_migration_pressure_zero_for_small_pop() {
        let mut country = Country::mock_for_tests();
        country.budget.population = 50;
        let buildings = vec![];
        let pressure = calculate_migration_pressure(&country, &buildings, 0, &MigrationConfig::default());
        assert_eq!(pressure, 0.0);
    }

    #[test]
    fn test_migration_pressure_high_unrest() {
        let mut country = Country::mock_for_tests();
        country.budget.population = 1_000_000;
        country.budget.gdp = 5_000_000_000.0;
        country.macro_indicators.average_wage = 1000.0;
        // Set low security index
        country.macro_indicators.extra.insert(
            "crime_rate".to_string(),
            serde_json::json!({"safety_index": 10.0}),
        );
        let buildings = vec![];
        let pressure = calculate_migration_pressure(&country, &buildings, 0, &MigrationConfig::default());
        // Phase 31: Weights were rebalanced to add unemployment and subsistence
        // components. With high unrest, poverty, and low wage, pressure should
        // still be significant (> 0.35).
        assert!(
            pressure > 0.35,
            "high unrest should produce high pressure: {}",
            pressure
        );
    }

    #[test]
    fn test_emigrants_respects_min_population() {
        let mut country = Country::mock_for_tests();
        country.budget.population = 200;
        let emigrants = calculate_emigrants(&country, 1.0, 0.0, &MigrationConfig::default());
        assert!(emigrants <= 100, "should not go below MIN_POPULATION");
    }

    #[test]
    fn test_emigrants_zero_with_full_enforcement() {
        let mut country = Country::mock_for_tests();
        country.budget.population = 1_000_000;
        country.politics.migration_law = Some(MigrationLaw {
            open_borders: false,
            visa_required: false,
            deportation_policy: DeportationPolicy::None,
        });
        let emigrants = calculate_emigrants(&country, 0.5, 1.0, &MigrationConfig::default());
        assert_eq!(emigrants, 0, "full enforcement should block all emigration");
    }

    #[test]
    fn test_open_borders_ignores_enforcement() {
        let mut country = Country::mock_for_tests();
        country.budget.population = 1_000_000;
        country.politics.migration_law = Some(MigrationLaw {
            open_borders: true,
            visa_required: false,
            deportation_policy: DeportationPolicy::None,
        });
        let emigrants = calculate_emigrants(&country, 0.5, 1.0, &MigrationConfig::default());
        assert!(
            emigrants > 0,
            "open borders should allow migration despite enforcement"
        );
    }

    #[test]
    fn test_collect_and_apply_flows_conservation() {
        let mut country_a = Country::mock_for_tests();
        country_a.name = "CountryA".to_string();
        country_a.budget.population = 1_000_000;
        country_a.budget.gdp = 1_000_000_000.0;
        country_a.macro_indicators.average_wage = 500.0;
        country_a.macro_indicators.extra.insert(
            "crime_rate".to_string(),
            serde_json::json!({"safety_index": 10.0}),
        );
        // Phase 36: Add a region with class demographics so bottom-up
        // reconciliation preserves the population instead of resetting to 0.
        country_a.regions = vec![test_region_with_population("REG-A", "CountryA", 1_000_000)];

        let mut country_b = Country::mock_for_tests();
        country_b.name = "CountryB".to_string();
        country_b.budget.population = 2_000_000;
        country_b.budget.gdp = 50_000_000_000.0;
        country_b.regions = vec![test_region_with_population("REG-B", "CountryB", 2_000_000)];
        country_b.macro_indicators.average_wage = 8000.0;
        country_b.macro_indicators.extra.insert(
            "crime_rate".to_string(),
            serde_json::json!({"safety_index": 90.0}),
        );

        let buildings_a: Vec<Building> = vec![Building {
            id: "transport-a".to_string(),
            name: "Bus Station".to_string(),
            sector: crate::registries::enums::Sector::TransportLogistics,
            inventory: {
                let mut inv = std::collections::BTreeMap::new();
                inv.insert(Commodity::PassengerTransport, 1_000_000.0);
                inv
            },
            ..Default::default()
        }];
        let buildings_b: Vec<Building> = vec![];

        let mut countries_ref: HashMap<String, (&Country, &[Building], u32)> = HashMap::new();
        countries_ref.insert("CountryA".to_string(), (&country_a, &buildings_a, 0));
        countries_ref.insert("CountryB".to_string(), (&country_b, &buildings_b, 0));

        let flows = collect_migration_flows(&countries_ref, 1, None, &MigrationConfig::default());
        assert!(!flows.is_empty(), "should produce migration flows");

        // Check conservation: total outflow = total inflow
        let total_out: i64 = flows.iter().map(|f| f.count).sum();
        assert!(total_out > 0, "should have migrants");

        // Apply flows
        let mut country_a_mut = country_a.clone();
        let mut country_b_mut = country_b.clone();
        let pop_a_before = country_a_mut.budget.population;
        let pop_b_before = country_b_mut.budget.population;

        let mut countries_mut: HashMap<String, &mut Country> = HashMap::new();
        countries_mut.insert("CountryA".to_string(), &mut country_a_mut);
        countries_mut.insert("CountryB".to_string(), &mut country_b_mut);

        let mut buildings_a: Vec<Building> = Vec::new();
        let mut buildings_b: Vec<Building> = Vec::new();
        let mut buildings_mut: HashMap<String, &mut [Building]> = HashMap::new();
        buildings_mut.insert("CountryA".to_string(), &mut buildings_a);
        buildings_mut.insert("CountryB".to_string(), &mut buildings_b);

        apply_migration_flows(&mut countries_mut, &mut buildings_mut, &flows, &MigrationConfig::default());

        let pop_a_after = country_a_mut.budget.population;
        let pop_b_after = country_b_mut.budget.population;

        // Conservation: A lost people, B gained people, total is conserved
        assert!(pop_a_after <= pop_a_before, "origin should lose population");
        assert!(
            pop_b_after >= pop_b_before,
            "destination should gain population"
        );
        assert_eq!(
            pop_a_before + pop_b_before,
            pop_a_after + pop_b_after,
            "total population must be conserved"
        );
    }

    #[test]
    fn test_deportation_none_policy() {
        let mut country = Country::mock_for_tests();
        country.budget.population = 1_000_000;
        country.macro_indicators.demographics.illegal_immigrants = 5000.0;
        country.politics.migration_law = Some(MigrationLaw {
            open_borders: false,
            visa_required: false,
            deportation_policy: DeportationPolicy::None,
        });
        let (deported, _wealth) = process_deportations(&mut country, 100.0, &MigrationConfig::default());
        assert_eq!(deported, 0);
    }

    #[test]
    fn test_deportation_mass() {
        let mut country = Country::mock_for_tests();
        country.budget.population = 1_000_000;
        country.macro_indicators.demographics.illegal_immigrants = 5000.0;
        country.politics.migration_law = Some(MigrationLaw {
            open_borders: false,
            visa_required: false,
            deportation_policy: DeportationPolicy::MassDeportation,
        });
        let (deported, _wealth) = process_deportations(&mut country, 100.0, &MigrationConfig::default());
        assert_eq!(deported, 5000);
        assert_eq!(
            country.macro_indicators.demographics.illegal_immigrants,
            0.0
        );
    }
}

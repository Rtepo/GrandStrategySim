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
use crate::politics::laws::{
    BorderState, DeportationPolicy, MigrationFlow, MigrationLaw, MigrationReason,
};
use crate::registries::enums::Commodity;
use crate::state::macro_data::ImmigrantCohort;
use crate::state::Country;
use serde_json::Map;
use std::collections::HashMap;

/// Phase 31: Maximum fraction of population that can emigrate per turn.
/// Corrected from 0.02 (48% annual at 24 turns/year) to 0.0002 (~0.5% annual).
const MAX_EMIGRATION_RATE: f64 = 0.0002;
/// Phase 31: Famine-level emigration rate cap (~2.4% annual at 24 turns/year).
const FAMINE_EMIGRATION_RATE: f64 = 0.001;
/// Minimum population to remain after emigration.
const MIN_POPULATION: u64 = 100;
/// Weight of unrest in migration pressure.
const UNREST_WEIGHT: f64 = 0.25;
/// Weight of poverty in migration pressure.
const POVERTY_WEIGHT: f64 = 0.20;
/// Weight of wage differential in migration pressure.
const WAGE_WEIGHT: f64 = 0.15;
/// Weight of disaster in migration pressure.
const DISASTER_WEIGHT: f64 = 0.05;
/// Phase 31: Weight of unemployment in migration pressure.
const UNEMPLOYMENT_WEIGHT: f64 = 0.15;
/// Phase 31: Weight of subsistence wage shortfall in migration pressure.
const SUBSISTENCE_WEIGHT: f64 = 0.20;

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
    buildings: &[Building],
    disaster_count: u32,
) -> f64 {
    let population = country.budget.population as f64;
    if population < 100.0 {
        return 0.0;
    }

    // Unrest: security index < 40 → high unrest
    let security_index = get_nested_f64(
        &country.macro_indicators.extra,
        "przestepczosc",
        "indeks_bezpieczenstwa",
    )
    .unwrap_or(80.0);
    let unrest = ((40.0 - security_index) / 40.0).max(0.0).min(1.0);

    // Poverty: GDP per capita below threshold → high poverty
    let gdp_pc = country.budget.gdp / population.max(1.0);
    let poverty = (1.0 - (gdp_pc / 10_000.0).min(1.0)).max(0.0);

    // Wage: low average wage → more likely to emigrate
    let avg_wage = country.macro_indicators.average_wage;
    let wage_pressure = (1.0 - (avg_wage / 5000.0).min(1.0)).max(0.0);

    // Disaster: recent disasters increase pressure
    let disaster_pressure = (disaster_count as f64 / 10.0).min(1.0);

    // Phase 31: Unemployment pressure (>10% unemployment increases pressure).
    let unemployment_rate = country.macro_indicators.labor_market.unemployment_rate / 100.0;
    let unemployment_pressure = (unemployment_rate - 0.10).max(0.0).min(1.0);

    // Phase 31: Subsistence wage pressure (avg wage below subsistence → pressure).
    let subsistence_wage = crate::politics::crisis_management::compute_subsistence_wage(
        &std::collections::HashMap::new(), // No market prices available here; use fallback
    );
    let subsistence_pressure = if subsistence_wage > 0.0 && avg_wage < subsistence_wage {
        (1.0 - avg_wage / subsistence_wage).min(1.0)
    } else {
        0.0
    };

    let pressure = UNREST_WEIGHT * unrest
        + POVERTY_WEIGHT * poverty
        + WAGE_WEIGHT * wage_pressure
        + DISASTER_WEIGHT * disaster_pressure
        + UNEMPLOYMENT_WEIGHT * unemployment_pressure
        + SUBSISTENCE_WEIGHT * subsistence_pressure;

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
) -> u64 {
    let migration_law = country
        .politics
        .migration_law
        .as_ref();

    let open_borders = migration_law.map(|m| m.open_borders).unwrap_or(false);

    let enforcement_factor = if open_borders {
        1.0 // Open borders: no enforcement reduction
    } else {
        1.0 - border_enforcement_ratio.clamp(0.0, 1.0)
    };

    // Phase 31: Use famine emigration rate when wage is below subsistence.
    let avg_wage = country.macro_indicators.average_wage;
    let subsistence_wage = crate::politics::crisis_management::compute_subsistence_wage(
        &std::collections::HashMap::new(),
    );
    let famine_mode = subsistence_wage > 0.0 && avg_wage < subsistence_wage;
    let rate = if famine_mode {
        FAMINE_EMIGRATION_RATE
    } else {
        MAX_EMIGRATION_RATE
    };

    let population = country.budget.population;
    let emigrants = (population as f64 * pressure * rate * enforcement_factor) as u64;

    // Ensure minimum population remains
    let max_emigrants = population.saturating_sub(MIN_POPULATION);
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
) -> Vec<MigrationFlow> {
    // Step 1: Calculate pressure and border enforcement for each country.
    let mut pressures: HashMap<String, f64> = HashMap::new();
    let mut border_capacities: HashMap<String, f64> = HashMap::new();

    for (name, (country, buildings, disaster_count)) in countries.iter() {
        let pressure = calculate_migration_pressure(country, buildings, *disaster_count);
        pressures.insert(name.clone(), pressure);

        let border_cap = sum_border_enforcement_capacity(buildings);
        border_capacities.insert(name.clone(), border_cap);
    }

    // Step 2: Calculate attractiveness (inverse of pressure, weighted by GDP).
    let mut attractors: Vec<(String, f64)> = countries
        .iter()
        .map(|(name, (country, _, _))| {
            let pressure = pressures[name];
            let gdp_pc = country.budget.gdp / (country.budget.population as f64).max(1.0);
            let attractiveness = (1.0 - pressure) * (gdp_pc / 10_000.0).min(1.0).max(0.01);
            (name.clone(), attractiveness)
        })
        .collect();
    attractors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let total_attractiveness: f64 = attractors.iter().map(|(_, a)| *a).sum();

    // Step 3: Generate flows.
    let mut flows = Vec::new();

    for (origin_name, (origin_country, _, _)) in countries.iter() {
        let pressure = pressures[origin_name];
        if pressure < 0.01 {
            continue;
        }

        let origin_border_cap = border_capacities[origin_name];
        // Border enforcement ratio: capacity relative to total border crossings (approx population)
        let origin_enforcement = (origin_border_cap
            / (origin_country.budget.population as f64).max(1.0))
        .min(1.0);

        let emigrants = calculate_emigrants(origin_country, pressure, origin_enforcement);
        if emigrants == 0 {
            continue;
        }

        // Determine reason based on dominant pressure factor
        let reason = determine_migration_reason(origin_country, pressure);

        // Distribute emigrants to attractive destinations
        let remaining = if total_attractiveness > 0.0 {
            for (dest_name, dest_attr) in &attractors {
                if dest_name == origin_name {
                    continue;
                }
                let share = dest_attr / total_attractiveness;
                let dest_emigrants = (emigrants as f64 * share) as u64;
                if dest_emigrants == 0 {
                    continue;
                }

                // Check destination border enforcement
                let dest_country = countries[dest_name].0;
                let dest_border_cap = border_capacities[dest_name];
                let dest_enforcement = (dest_border_cap
                    / (dest_country.budget.population as f64).max(1.0))
                .min(1.0);

                let migration_law = dest_country
                    .politics
                    .migration_law
                    .as_ref();
                let open_borders = migration_law.map(|m| m.open_borders).unwrap_or(false);

                let actual_migrants = if open_borders {
                    dest_emigrants
                } else {
                    (dest_emigrants as f64 * (1.0 - dest_enforcement * 0.5)) as u64
                };

                if actual_migrants > 0 {
                    flows.push(MigrationFlow {
                        origin_country: origin_name.clone(),
                        dest_country: dest_name.clone(),
                        count: actual_migrants as i64,
                        reason: reason.clone(),
                        turn,
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
                    flows.push(MigrationFlow {
                        origin_country: origin_name.clone(),
                        dest_country: top_dest.clone(),
                        count: blocked as i64,
                        reason: MigrationReason::Economic,
                        turn,
                    });
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
        "przestepczosc",
        "indeks_bezpieczenstwa",
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
/// * Destination population increases by flow count.
/// * ImmigrantCohort entries created in destination.
/// * Population is strictly conserved.
pub fn apply_migration_flows(
    countries: &mut HashMap<String, &mut Country>,
    flows: &[MigrationFlow],
) {
    // Aggregate by origin and destination to minimize mutations
    let mut origin_outflows: HashMap<String, u64> = HashMap::new();
    let mut dest_inflows: HashMap<String, Vec<(u64, &MigrationReason)>> = HashMap::new();

    for flow in flows {
        *origin_outflows.entry(flow.origin_country.clone()).or_insert(0) += flow.count as u64;
        dest_inflows
            .entry(flow.dest_country.clone())
            .or_insert_with(Vec::new)
            .push((flow.count as u64, &flow.reason));
    }

    // Apply outflows (deduct from origin population)
    // Phase 36: Use bottom-up distribution instead of direct budget.population write.
    for (origin_name, total_out) in &origin_outflows {
        if let Some(country) = countries.get_mut(origin_name) {
            let delta = -(*total_out as i64);
            crate::economy::labor::labor::distribute_population_delta_and_reconcile(country, delta);
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
            crate::economy::labor::labor::distribute_population_delta_and_reconcile(country, total_in as i64);

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
                let has_refugees = inflows.iter().any(|(_, r)| matches!(r, MigrationReason::Unrest | MigrationReason::Persecution));
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
                0.10
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
                    extra: Map::new(),
                });

            // Update border state if present
            if let Some(border_state) = &mut country.politics.border_state {
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
/// Number of illegal immigrants deported.
///
/// # Rules
/// * Only deports if `DeportationPolicy` is not `None`.
/// * Deported population is removed (returns to origin or disappears).
/// * `MassDeportation` removes all illegal immigrants.
/// * `Selective` removes 10% per turn.
pub fn process_deportations(country: &mut Country, border_capacity: f64) -> u64 {
    let policy = country
        .politics
        .migration_law
        .as_ref()
        .map(|m| &m.deportation_policy)
        .cloned()
        .unwrap_or(DeportationPolicy::None);

    if matches!(policy, DeportationPolicy::None) {
        return 0;
    }

    let illegal = country.macro_indicators.demographics.illegal_immigrants;
    if illegal <= 0.0 {
        return 0;
    }

    // Border capacity limits how many can be deported per turn
    let capacity_factor = (border_capacity / 100.0).min(1.0);

    let deport_count = match policy {
        DeportationPolicy::None => 0.0,
        DeportationPolicy::Selective => (illegal * 0.10 * capacity_factor).floor(),
        DeportationPolicy::MassDeportation => (illegal * capacity_factor).floor(),
    };

    let deport_count = deport_count as u64;
    if deport_count == 0 {
        return 0;
    }

    // Remove from illegal immigrants
    country.macro_indicators.demographics.illegal_immigrants =
        (country.macro_indicators.demographics.illegal_immigrants - deport_count as f64).max(0.0);

    // Remove from population
    // Phase 36: Use bottom-up distribution instead of direct budget.population write.
    crate::economy::labor::labor::distribute_population_delta_and_reconcile(country, -(deport_count as i64));

    // Record in border state
    if let Some(border_state) = &mut country.politics.border_state {
        border_state.deportations = deport_count as i64;
    }

    deport_count
}

/// Helper: get nested f64 from serde Map.
fn get_nested_f64(map: &Map<String, serde_json::Value>, key1: &str, key2: &str) -> Option<f64> {
    map.get(key1)?
        .get(key2)?
        .as_f64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Building;
    use crate::state::Country;
    use crate::society::geography::{Region, RegionalClassDemographics, ClassDemographics};

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
        rural.insert("FreePeasant".to_string(), ClassDemographics {
            population: pop,
            labor_participation: 0.55,
            ..Default::default()
        });
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
        assert_eq!(
            sum_border_enforcement_capacity(&[b1, b2]),
            15.0
        );
    }

    #[test]
    fn test_migration_pressure_zero_for_small_pop() {
        let mut country = Country::mock_for_tests();
        country.budget.population = 50;
        let buildings = vec![];
        let pressure = calculate_migration_pressure(&country, &buildings, 0);
        assert_eq!(pressure, 0.0);
    }

    #[test]
    fn test_migration_pressure_high_unrest() {
        let mut country = Country::mock_for_tests();
        country.budget.population = 1_000_000;
        country.budget.gdp = 5_000_000_000.0;
        country.macro_indicators.average_wage = 1000.0;
        // Set low security index
        country
            .macro_indicators
            .extra
            .insert(
                "przestepczosc".to_string(),
                serde_json::json!({"indeks_bezpieczenstwa": 10.0}),
            );
        let buildings = vec![];
        let pressure = calculate_migration_pressure(&country, &buildings, 0);
        // Phase 31: Weights were rebalanced to add unemployment and subsistence
        // components. With high unrest, poverty, and low wage, pressure should
        // still be significant (> 0.35).
        assert!(pressure > 0.35, "high unrest should produce high pressure: {}", pressure);
    }

    #[test]
    fn test_emigrants_respects_min_population() {
        let mut country = Country::mock_for_tests();
        country.budget.population = 200;
        let emigrants = calculate_emigrants(&country, 1.0, 0.0);
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
        let emigrants = calculate_emigrants(&country, 0.5, 1.0);
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
        let emigrants = calculate_emigrants(&country, 0.5, 1.0);
        assert!(emigrants > 0, "open borders should allow migration despite enforcement");
    }

    #[test]
    fn test_collect_and_apply_flows_conservation() {
        let mut country_a = Country::mock_for_tests();
        country_a.name = "CountryA".to_string();
        country_a.budget.population = 1_000_000;
        country_a.budget.gdp = 1_000_000_000.0;
        country_a.macro_indicators.average_wage = 500.0;
        country_a
            .macro_indicators
            .extra
            .insert(
                "przestepczosc".to_string(),
                serde_json::json!({"indeks_bezpieczenstwa": 10.0}),
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
        country_b
            .macro_indicators
            .extra
            .insert(
                "przestepczosc".to_string(),
                serde_json::json!({"indeks_bezpieczenstwa": 90.0}),
            );

        let buildings_a: Vec<Building> = vec![];
        let buildings_b: Vec<Building> = vec![];

        let mut countries_ref: HashMap<String, (&Country, &[Building], u32)> = HashMap::new();
        countries_ref.insert("CountryA".to_string(), (&country_a, &buildings_a, 0));
        countries_ref.insert("CountryB".to_string(), (&country_b, &buildings_b, 0));

        let flows = collect_migration_flows(&countries_ref, 1);
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

        apply_migration_flows(&mut countries_mut, &flows);

        let pop_a_after = country_a_mut.budget.population;
        let pop_b_after = country_b_mut.budget.population;

        // Conservation: A lost people, B gained people, total is conserved
        assert!(pop_a_after <= pop_a_before, "origin should lose population");
        assert!(pop_b_after >= pop_b_before, "destination should gain population");
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
        let deported = process_deportations(&mut country, 100.0);
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
        let deported = process_deportations(&mut country, 100.0);
        assert_eq!(deported, 5000);
        assert_eq!(country.macro_indicators.demographics.illegal_immigrants, 0.0);
    }
}

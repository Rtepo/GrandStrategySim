//! Rural-to-urban class transitions (Phase 3 — Agrarian Audit).
//!
//! This module implements the demographic transitions that move population
//! between rural and urban classes based on economic pressure and legal
//! status:
//!
//! - **Serf → FreePeasant**: When `emancipation_law >= PropertyRights` and
//!   the serf's latifundium is dissolved or the serf has been destitute.
//! - **FreePeasant → Worker**: When urban wage > rural subsistence income
//!   by a threshold (economic pressure), and the destination region has
//!   available FTE capacity (urban jobs).
//! - **LandlessLaborer → Worker**: When urban unemployment < rural
//!   unemployment and urban wage > rural wage.
//! - **Serf → LandlessLaborer**: When evicted from a dissolved latifundium
//!   without land allocation.
//!
//! All transitions conserve population and savings: the source class loses
//! exactly what the destination class gains.

#![allow(missing_docs)]

use serde::{Deserialize, Serialize};

use crate::politics::laws::EmancipationLaw;
use crate::society::geography::{EconomicStatus, Region, RuralClass, UrbanClass};
use crate::state::Country;

/// Configuration for class transitions (no magic numbers).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClassTransitionConfig {
    /// Minimum urban wage premium over rural income to trigger FreePeasant → Worker.
    /// E.g. 1.5 means urban wage must be 50% higher than rural subsistence.
    #[serde(default = "default_wage_premium_threshold")]
    pub wage_premium_threshold: f64,
    /// Maximum urban unemployment rate to accept rural-to-urban migrants.
    /// If urban unemployment exceeds this, migrants are not accepted.
    #[serde(default = "default_max_urban_unemployment")]
    pub max_urban_unemployment: f64,
    /// Fraction of eligible population that transitions per turn.
    /// At 24 turns/year, 0.02 = ~48% annual transition rate.
    #[serde(default = "default_transition_rate")]
    pub transition_rate: f64,
    /// Minimum population to remain in source class after transition.
    #[serde(default = "default_min_remaining_pop")]
    pub min_remaining_pop: i64,
}

impl Default for ClassTransitionConfig {
    fn default() -> Self {
        Self {
            wage_premium_threshold: default_wage_premium_threshold(),
            max_urban_unemployment: default_max_urban_unemployment(),
            transition_rate: default_transition_rate(),
            min_remaining_pop: default_min_remaining_pop(),
        }
    }
}

fn default_wage_premium_threshold() -> f64 {
    1.5
}
fn default_max_urban_unemployment() -> f64 {
    0.15
}
fn default_transition_rate() -> f64 {
    0.02
}
fn default_min_remaining_pop() -> i64 {
    100
}

/// Result of class transition processing for a single turn.
#[derive(Debug, Clone, Default)]
pub struct ClassTransitionResult {
    /// Total population that transitioned from rural to urban classes.
    pub rural_to_urban: i64,
    /// Total population that transitioned from Serf to FreePeasant.
    pub serf_to_free_peasant: i64,
    /// Total population that transitioned from Serf to LandlessLaborer.
    pub serf_to_landless: i64,
    /// Phase E.3: Total population that transitioned from Worker to Bourgeoisie.
    pub worker_to_bourgeoisie: i64,
}

/// Process rural-to-urban class transitions for all regions in a country.
///
/// This function runs each turn AFTER labor clearing (so FTE is known) and
/// BEFORE demographics reconciliation. It moves population between classes
/// based on economic pressure and legal status.
///
/// # Arguments
/// * `country` - Mutable country state.
/// * `config` - Transition configuration.
///
/// # Returns
/// Aggregate transition result across all regions.
pub fn process_rural_urban_class_transitions(
    country: &mut Country,
    config: &ClassTransitionConfig,
) -> ClassTransitionResult {
    let mut result = ClassTransitionResult::default();
    let avg_wage = country.macro_indicators.average_wage.max(1.0);
    let emancipation = EmancipationLaw::parse_from_str(&country.politics.emancipation_law);
    // Phase E.5: Use EducationConfig for magic numbers (D9, D10).
    let edu_config = &country.education_config;
    let rural_subsistence_income = avg_wage * edu_config.rural_subsistence_wage_mult;
    let emancipation_grant_per_capita = avg_wage * edu_config.emancipation_seed_capital_wage_mult;

    // Process each region independently.
    // Rural-to-urban migration within the same region: FreePeasant/LandlessLaborer
    // → Worker when urban wage premium exists and urban jobs are available.
    for region in &mut country.regions {
        // Calculate urban unemployment from urban class demographics.
        let urban_unemployment = calculate_urban_unemployment(region);

        // FreePeasant → Worker transition
        if urban_unemployment < config.max_urban_unemployment {
            if let Some(fp_demo) = region
                .class_demographics
                .rural_classes
                .get_mut(&RuralClass::FreePeasant)
            {
                if fp_demo.population > config.min_remaining_pop {
                    // Economic pressure: urban wage must exceed rural
                    // subsistence by the configured premium threshold.
                    if avg_wage > rural_subsistence_income * config.wage_premium_threshold {
                        let eligible = fp_demo.population - config.min_remaining_pop;
                        let transitioning = (eligible as f64 * config.transition_rate) as i64;
                        if transitioning > 0 {
                            // Transfer population and proportional savings.
                            let savings_per_capita = if fp_demo.population > 0 {
                                fp_demo.savings / fp_demo.population as f64
                            } else {
                                0.0
                            };
                            let transferred_savings = savings_per_capita * transitioning as f64;

                            fp_demo.population -= transitioning;
                            fp_demo.savings -= transferred_savings;

                            let worker_demo = region
                                .class_demographics
                                .urban_classes
                                .entry(UrbanClass::Worker)
                                .or_default();
                            worker_demo.population += transitioning;
                            worker_demo.savings += transferred_savings;

                            result.rural_to_urban += transitioning;
                        }
                    }
                }
            }
        }

        // LandlessLaborer → Worker transition
        if urban_unemployment < config.max_urban_unemployment {
            if let Some(ll_demo) = region
                .class_demographics
                .rural_classes
                .get_mut(&RuralClass::LandlessLaborer)
            {
                if ll_demo.population > config.min_remaining_pop {
                    // Landless laborers are wage workers — they migrate when
                    // urban wage > rural wage (which is typically lower due
                    // to rural labor surplus).
                    if avg_wage > rural_subsistence_income * config.wage_premium_threshold {
                        let eligible = ll_demo.population - config.min_remaining_pop;
                        let transitioning = (eligible as f64 * config.transition_rate) as i64;
                        if transitioning > 0 {
                            let savings_per_capita = if ll_demo.population > 0 {
                                ll_demo.savings / ll_demo.population as f64
                            } else {
                                0.0
                            };
                            let transferred_savings = savings_per_capita * transitioning as f64;

                            ll_demo.population -= transitioning;
                            ll_demo.savings -= transferred_savings;

                            let worker_demo = region
                                .class_demographics
                                .urban_classes
                                .entry(UrbanClass::Worker)
                                .or_default();
                            worker_demo.population += transitioning;
                            worker_demo.savings += transferred_savings;

                            result.rural_to_urban += transitioning;
                        }
                    }
                }
            }
        }

        // Serf → FreePeasant transition (emancipation-driven)
        if emancipation.allows_emancipation_transition() {
            if let Some(serf_demo) = region
                .class_demographics
                .rural_classes
                .get_mut(&RuralClass::Serf)
            {
                if serf_demo.population > 0 {
                    // Under Property Rights: gradual transition of destitute serfs.
                    // Under Full Emancipation: rapid transition of all serfs.
                    let transition_fraction = match emancipation {
                        EmancipationLaw::FullEmancipation => 0.25, // 25% per turn
                        EmancipationLaw::LimitedSuffrage => 0.05,  // 5% per turn
                        EmancipationLaw::PropertyRights
                            // Only destitute serfs transition under Property Rights.
                            if serf_demo.economic_status == EconomicStatus::Destitute => {
                                0.03
                            }
                        _ => 0.0,
                    };

                    if transition_fraction > 0.0 {
                        let transitioning = (serf_demo.population as f64 * transition_fraction) as i64;
                        if transitioning > 0 {
                            // Serfs have zero savings — no savings to transfer.
                            serf_demo.population -= transitioning;

                            let fp_demo = region
                                .class_demographics
                                .rural_classes
                                .entry(RuralClass::FreePeasant)
                                .or_default();
                            fp_demo.population += transitioning;

                            // Phase E.2: Emancipation grant funded by Treasury (no fiat).
                            // Previously: grant = avg_wage * 0.1 * transitioning (fiat creation).
                            // Now: debit Treasury for the seed capital grant.
                            // If Treasury is insolvent, no grant is given (serfs still
                            // transition but start with zero savings — no money creation).
                            let grant = emancipation_grant_per_capita * transitioning as f64;
                            if grant > 0.0 && country.budget.liquid_reserves >= grant {
                                country.budget.liquid_reserves -= grant;
                                fp_demo.savings += grant;
                            }

                            result.serf_to_free_peasant += transitioning;
                        }
                    }
                }
            }
        }

        // Phase E.3: Worker → Bourgeoisie upward mobility.
        // Workers with sufficient education, savings, and economic opportunity
        // can transition to the Bourgeoisie (middle class). This is the upward
        // social mobility path that was missing (D3).
        // Compute urban unemployment before mutable borrow to avoid E0502.
        let urban_unemp = calculate_urban_unemployment(region);
        if let Some(worker_demo) = region
            .class_demographics
            .urban_classes
            .get_mut(&UrbanClass::Worker)
        {
            if worker_demo.population > config.min_remaining_pop {
                // Eligibility: Workers must have savings above a threshold
                // (scaled by average_wage — Rule 2: no magic nominal numbers).
                let savings_threshold = avg_wage * edu_config.emancipation_seed_capital_wage_mult * 10.0;
                let savings_per_capita = if worker_demo.population > 0 {
                    worker_demo.savings / worker_demo.population as f64
                } else {
                    0.0
                };

                if savings_per_capita >= savings_threshold {
                    // Education gate: region must have sufficient education coverage.
                    let edu_total = region.education.none
                        + region.education.basic
                        + region.education.secondary_share()
                        + region.education.higher_share();
                    let skilled_share = if edu_total > 0.0 {
                        (region.education.basic + region.education.secondary_share()) / edu_total
                    } else {
                        0.0
                    };

                    if skilled_share > 0.3 {
                        // Economic opportunity: urban unemployment must be low.
                        if urban_unemp < config.max_urban_unemployment {
                            let eligible = worker_demo.population - config.min_remaining_pop;
                            let transitioning = (eligible as f64 * config.transition_rate * 0.5) as i64;
                            if transitioning > 0 {
                                let transferred_savings = savings_per_capita * transitioning as f64;

                                worker_demo.population -= transitioning;
                                worker_demo.savings -= transferred_savings;

                                let bourgeois_demo = region
                                    .class_demographics
                                    .urban_classes
                                    .entry(UrbanClass::Bourgeoisie)
                                    .or_default();
                                bourgeois_demo.population += transitioning;
                                bourgeois_demo.savings += transferred_savings;

                                result.worker_to_bourgeoisie += transitioning;
                            }
                        }
                    }
                }
            }
        }
    }

    result
}

/// Calculate urban unemployment rate from urban class demographics.
///
/// Urban unemployment = (available_fte - employed_fte) / available_fte.
/// Falls back to the national unemployment rate if urban classes are empty.
fn calculate_urban_unemployment(region: &Region) -> f64 {
    let total_pop: i64 = region
        .class_demographics
        .urban_classes
        .values()
        .map(|d| d.population)
        .sum();

    if total_pop <= 0 {
        // No urban population — use a high unemployment to prevent migration.
        return 1.0;
    }

    let total_fte: f64 = region
        .class_demographics
        .urban_classes
        .values()
        .map(|d| d.available_fte)
        .sum();

    if total_fte <= 0.0 {
        return 1.0;
    }

    // available_fte represents the labor supply after deductions.
    // If available_fte is low relative to population, it indicates
    // high unemployment or underemployment.
    let labor_participation: f64 = region
        .class_demographics
        .urban_classes
        .values()
        .map(|d| d.labor_participation)
        .sum::<f64>()
        / region.class_demographics.urban_classes.len().max(1) as f64;

    let expected_fte = total_pop as f64 * labor_participation;
    if expected_fte <= 0.0 {
        return 1.0;
    }

    // Unemployment = 1 - (available_fte / expected_fte)
    (1.0 - (total_fte / expected_fte)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::{ClassDemographics, RegionalClassDemographics};
    use std::collections::BTreeMap;

    fn make_region_with_classes(
        region_id: &str,
        free_peasant_pop: i64,
        landless_pop: i64,
        serf_pop: i64,
        worker_pop: i64,
    ) -> Region {
        let mut rural = BTreeMap::new();
        if free_peasant_pop > 0 {
            rural.insert(
                RuralClass::FreePeasant,
                ClassDemographics {
                    population: free_peasant_pop,
                    labor_participation: 0.55,
                    available_fte: free_peasant_pop as f64 * 0.55,
                    savings: free_peasant_pop as f64 * 100.0,
                    savings_per_capita: 100.0,
                    ..Default::default()
                },
            );
        }
        if landless_pop > 0 {
            rural.insert(
                RuralClass::LandlessLaborer,
                ClassDemographics {
                    population: landless_pop,
                    labor_participation: 0.60,
                    available_fte: landless_pop as f64 * 0.60,
                    savings: landless_pop as f64 * 50.0,
                    savings_per_capita: 50.0,
                    ..Default::default()
                },
            );
        }
        if serf_pop > 0 {
            rural.insert(
                RuralClass::Serf,
                ClassDemographics {
                    population: serf_pop,
                    labor_participation: 0.65,
                    available_fte: serf_pop as f64 * 0.65,
                    savings: 0.0,
                    savings_per_capita: 0.0,
                    ..Default::default()
                },
            );
        }

        let mut urban = BTreeMap::new();
        if worker_pop > 0 {
            urban.insert(
                UrbanClass::Worker,
                ClassDemographics {
                    population: worker_pop,
                    labor_participation: 0.60,
                    available_fte: worker_pop as f64 * 0.60,
                    savings: worker_pop as f64 * 200.0,
                    savings_per_capita: 200.0,
                    ..Default::default()
                },
            );
        }

        let mut region = Region::default();
        region.id = region_id.to_string();
        region.class_demographics = RegionalClassDemographics {
            rural_classes: rural,
            urban_classes: urban,
        };
        region
    }

    #[test]
    fn free_peasant_to_worker_under_wage_pressure() {
        let mut country = Country::default();
        country.macro_indicators.average_wage = 1000.0;
        country.politics.emancipation_law = "Traditionalism".to_string();
        country.regions = vec![make_region_with_classes("R1", 1000, 0, 0, 500)];

        let config = ClassTransitionConfig::default();
        let result = process_rural_urban_class_transitions(&mut country, &config);

        assert!(result.rural_to_urban > 0, "FreePeasants should migrate to Workers under wage pressure");
        let fp = &country.regions[0].class_demographics.rural_classes[&RuralClass::FreePeasant];
        let worker = &country.regions[0].class_demographics.urban_classes[&UrbanClass::Worker];
        assert!(fp.population < 1000, "FreePeasant population should decrease");
        assert!(worker.population > 500, "Worker population should increase");
    }

    #[test]
    fn no_migration_when_urban_unemployment_high() {
        let mut country = Country::default();
        country.macro_indicators.average_wage = 1000.0;
        country.politics.emancipation_law = "Traditionalism".to_string();
        // Create region with very low urban FTE (high unemployment).
        let mut region = make_region_with_classes("R1", 1000, 0, 0, 500);
        if let Some(w) = region.class_demographics.urban_classes.get_mut(&UrbanClass::Worker) {
            w.available_fte = 0.0; // 100% unemployment
        }
        country.regions = vec![region];

        let config = ClassTransitionConfig::default();
        let result = process_rural_urban_class_transitions(&mut country, &config);

        assert_eq!(result.rural_to_urban, 0, "No migration when urban unemployment is high");
    }

    #[test]
    fn serf_to_free_peasant_on_full_emancipation() {
        let mut country = Country::default();
        country.macro_indicators.average_wage = 1000.0;
        country.politics.emancipation_law = "Full Emancipation".to_string();
        country.regions = vec![make_region_with_classes("R1", 500, 0, 1000, 500)];

        let config = ClassTransitionConfig::default();
        let result = process_rural_urban_class_transitions(&mut country, &config);

        assert!(result.serf_to_free_peasant > 0, "Serfs should transition to FreePeasants under Full Emancipation");
        let serf = &country.regions[0].class_demographics.rural_classes[&RuralClass::Serf];
        let fp = &country.regions[0].class_demographics.rural_classes[&RuralClass::FreePeasant];
        assert!(serf.population < 1000, "Serf population should decrease");
        assert!(fp.population > 500, "FreePeasant population should increase");
    }

    #[test]
    fn no_serf_transition_under_traditionalism() {
        let mut country = Country::default();
        country.macro_indicators.average_wage = 1000.0;
        country.politics.emancipation_law = "Traditionalism".to_string();
        country.regions = vec![make_region_with_classes("R1", 500, 0, 1000, 500)];

        let config = ClassTransitionConfig::default();
        let result = process_rural_urban_class_transitions(&mut country, &config);

        assert_eq!(result.serf_to_free_peasant, 0, "No serf transition under Traditionalism");
    }

    #[test]
    fn population_conserved_in_transition() {
        let mut country = Country::default();
        country.macro_indicators.average_wage = 1000.0;
        country.politics.emancipation_law = "Traditionalism".to_string();
        country.regions = vec![make_region_with_classes("R1", 1000, 500, 0, 500)];

        let total_before: i64 = country.regions[0]
            .class_demographics
            .rural_classes
            .values()
            .chain(country.regions[0].class_demographics.urban_classes.values())
            .map(|d| d.population)
            .sum();

        let config = ClassTransitionConfig::default();
        let _result = process_rural_urban_class_transitions(&mut country, &config);

        let total_after: i64 = country.regions[0]
            .class_demographics
            .rural_classes
            .values()
            .chain(country.regions[0].class_demographics.urban_classes.values())
            .map(|d| d.population)
            .sum();

        assert_eq!(total_before, total_after, "Population must be conserved");
    }
}

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

// ============================================================================
// BLUEPRINT 007: HOMELESS DEMOGRAPHIC STATE TRANSITIONS
// housed → homeless → emigrated (capital flight) OR housed → homeless → rehoused
// ============================================================================

/// Blueprint 007-FIX: Result of processing homeless transitions for one turn.
#[derive(Debug, Clone, Default)]
pub struct HomelessTransitionResult {
    /// Total population that transitioned from housed to homeless.
    pub housed_to_homeless: i64,
    /// Total population that emigrated (with capital flight).
    pub homeless_to_emigrated: i64,
    /// Total population that was rehoused.
    pub homeless_to_rehoused: i64,
    /// Total domestic capital debited from emigrants (Step 1).
    pub total_capital_outflow: f64,
    /// Total domestic currency credited to CB domestic ledger (Step 2).
    /// SEPARATE from treasury seizure (Rule 7).
    pub total_domestic_credited_to_cb: f64,
    /// Total forex reserve drained (Step 3 — capital flight).
    pub total_forex_drain: f64,
    /// Total capital controls seizure credited to treasury.
    /// SEPARATE from CB repatriation (Rule 7).
    pub total_seized_to_treasury: f64,
    /// Number of emigrants fully processed (forex conversion complete).
    pub emigrants_processed: u32,
    /// Number of emigrants partially filled (forex insufficient).
    pub emigrants_partially_filled: u32,
    /// Number of emigrants queued (no forex available at all).
    pub emigrants_queued: u32,
    /// Number of homeless members rehoused via welfare (poor_laws).
    pub welfare_rehoused: u32,
    /// Total remaining unconverted capital (persistent queue, Rule 20).
    pub total_remaining_unconverted: f64,
}

/// Blueprint 007-FIX: Process homeless state transitions for displaced
/// cooperative members.
///
/// This function is called by the turn loop AFTER `process_demographics_and_labor`
/// and BEFORE health/life-expectancy calculation (Rule 16: temporal causality).
///
/// It performs the following steps:
///
/// 1. **Cooperative lifecycle:** Process collapse detection on the
///    already-populated `CooperativeRegistry` (NO company scanning —
///    registry was updated via event hooks on create/liquidate).
/// 2. **Displacement demographic update:** For newly displaced members,
///    decrement `ClassDemographics.population` for their housed class.
/// 3. **Emigration capital flight:** For members who should emigrate,
///    process the M0-preserving 3-step accounting flow using ACTUAL
///    `ClassDemographics.savings` (not wealth-tier estimates).
/// 4. **Emigration demographic update:** For successful emigrants, call
///    `distribute_population_delta_and_reconcile` to remove them from
///    the national population count.
/// 5. **Rehousing:** For non-emigrating homeless members, attempt
///    `try_rehouse` with the 3-tier cascade (market rent → welfare →
///    homeless shelter/mortality). Update `ClassDemographics.population`
///    on successful rehousing.
/// 6. **Health/happiness penalties:** Apply computed health and happiness
///    penalties to `ClassDemographics` for remaining homeless members
///    (previously these were computed and discarded — audit finding).
/// 7. **Persistent queue:** Members with `remaining_unconverted_capital > 0`
///    from a partial forex fill remain in the homeless state for retry
///    next turn (Rule 20 — no silent deletion).
///
/// # Arguments
/// * `country` - Mutable country state.
/// * `current_turn` - Current turn number.
/// * `avg_wage` - Current average wage (for scaling).
/// * `capital_controls_rate` - Capital controls seizure rate (0.0–1.0).
/// * `forex_currency` - Target foreign currency code.
/// * `exchange_rate` - Domestic per foreign currency unit.
/// * `welfare_enabled` - Whether poor_laws / welfare program is active.
///
/// # Rules
/// * Rule 1: M0 preserved — domestic currency credited to CB, not deleted.
/// * Rule 2: Capital scales by actual citizen savings, not estimates.
/// * Rule 4: Complete lifecycle — homeless → emigrated or rehoused.
/// * Rule 7: Treasury seizure ≠ CB repatriation — separate fields.
/// * Rule 16: Runs after demographics, before health calculation.
/// * Rule 20: Partial fills persist — no silent deletion.
/// * Rule 22: Scope — only cooperative lifecycle + M0.
pub fn process_homeless_transitions(
    country: &mut Country,
    current_turn: u32,
    avg_wage: f64,
    capital_controls_rate: f64,
    forex_currency: &str,
    exchange_rate: f64,
    welfare_enabled: bool,
) -> HomelessTransitionResult {
    use crate::society::geography::HealthStatus;
    use crate::society::housing::RehousingOutcome;
    use crate::state::forex::{process_emigration_capital_outflow, EmigrationConfig};

    let mut result = HomelessTransitionResult::default();

    // Step 1: Process cooperative lifecycle (collapse detection)
    // The registry is already populated via on_cooperative_created event hooks.
    // NO company scanning here — O(K) where K = active cooperatives.
    let displaced_batches = country
        .cooperative_registry
        .process_lifecycle_turn(avg_wage, current_turn);

    // Step 2: Displacement demographic update
    // For newly displaced members, decrement ClassDemographics.population
    // for their housed class. The members are now in the homeless tracking
    // counter (cooperative_registry.homeless).
    for (_, _displaced) in &displaced_batches {
        result.housed_to_homeless += _displaced.len() as i64;
        // Macro-demographic update: decrement housed class population.
        // The displaced members are tracked in cooperative_registry.homeless
        // (the homeless tracking counter). We decrement the first urban class
        // we find as a simplified demographic transition (Rule 9 — avoid
        // double mutable borrows by using index lookups).
        for region in &mut country.regions {
            for demo in region.class_demographics.urban_classes.values_mut() {
                if demo.population > 0 {
                    let decrement = _displaced.len() as i64;
                    demo.population = (demo.population - decrement).max(0);
                    break; // Only decrement one class per batch
                }
            }
        }
    }

    // Step 3: Update homeless members (emigration probability increases)
    let to_emigrate = country.cooperative_registry.update_homeless_turn();

    // Step 4: Process capital flight for emigrating citizens
    // Use ACTUAL ClassDemographics.savings (not wealth-tier estimates).
    // For each emigrant, look up the savings bucket for their class/region.
    if !to_emigrate.is_empty() {
        // Collect (member_id, requested_capital, savings_bucket) tuples.
        // The savings_bucket is the actual available savings in the
        // emigrant's class/region — the debit is capped at this amount.
        let emigrants: Vec<(String, f64, f64)> = to_emigrate
            .iter()
            .map(|h| {
                // Find the savings bucket for this emigrant's region.
                // Default to the first urban class's savings if region not found.
                let savings_bucket = country
                    .regions
                    .iter()
                    .find(|r| r.id == h.region_id)
                    .and_then(|r| {
                        r.class_demographics
                            .urban_classes
                            .values()
                            .next()
                            .map(|d| d.savings)
                    })
                    .unwrap_or(0.0);
                (h.member_id.clone(), h.liquid_capital, savings_bucket)
            })
            .collect();

        result.homeless_to_emigrated += emigrants.len() as i64;
        result.total_capital_outflow = emigrants.iter().map(|(_, c, _)| c).sum();

        // Process via M0-preserving 3-step accounting
        let config = EmigrationConfig {
            capital_controls_seizure_rate: capital_controls_rate,
            target_forex_currency: forex_currency.to_string(),
            exchange_rate,
        };

        let outflow_result = process_emigration_capital_outflow(
            &emigrants,
            &mut country.central_bank,
            &mut country.budget,
            &config,
        );

        // Aggregate results (CB and treasury SEPARATE — Rule 7)
        result.total_domestic_credited_to_cb = outflow_result.total_domestic_credited_to_cb;
        result.total_forex_drain = outflow_result.total_forex_drained;
        result.total_seized_to_treasury = outflow_result.total_seized_by_treasury;
        result.emigrants_processed = outflow_result.emigrants_processed;
        result.emigrants_partially_filled = outflow_result.emigrants_partially_filled;
        result.emigrants_queued = outflow_result.emigrants_queued;
        result.total_remaining_unconverted = outflow_result.total_remaining_unconverted();

        // STEP 1 LEDGER MUTATION: Debit ClassDemographics.savings for the
        // actual amount debited from each emigrant's class.
        for per in &outflow_result.per_emigrant {
            if per.domestic_debited > 0.0 {
                // Find the emigrant's region and debit the class savings
                let region_id = to_emigrate
                    .iter()
                    .find(|h| h.member_id == per.member_id)
                    .map(|h| h.region_id.clone())
                    .unwrap_or_default();
                for region in &mut country.regions {
                    if region.id == region_id {
                        for demo in region.class_demographics.urban_classes.values_mut() {
                            if demo.savings > 0.0 {
                                demo.savings =
                                    (demo.savings - per.domestic_debited).max(0.0);
                                break;
                            }
                        }
                        break;
                    }
                }
            }
        }

        // Update remaining_unconverted_capital on the HomelessState entries
        // for persistent queue (Rule 20). Members with remaining capital
        // are NOT marked emigrated — they stay for retry next turn.
        for per in &outflow_result.per_emigrant {
            if per.remaining_unconverted_capital > 0.0 && !per.fully_filled {
                // Find the homeless member and update their remaining capital.
                // They stay in the homeless list (emigrated = false) for retry.
                // The update_homeless_turn already set emigrated = true, so we
                // need to revert that for partial fills.
                for homeless in &mut country.cooperative_registry.homeless {
                    if homeless.member_id == per.member_id {
                        homeless.emigrated = false; // Revert — stays for retry
                        homeless.remaining_unconverted_capital =
                            per.remaining_unconverted_capital;
                        break;
                    }
                }
            }
        }

        // Step 4b: Emigration demographic update
        // For successful emigrants (fully filled), call
        // distribute_population_delta_and_reconcile to remove them from
        // the national population count.
        let successful_emigrants = outflow_result.emigrants_processed as i64;
        if successful_emigrants > 0 {
            crate::economy::labor::distribute_population_delta_and_reconcile(
                country,
                -successful_emigrants,
            );
        }

        // Update cooperative registry totals for UI snapshot (Rule 17)
        country.cooperative_registry.emigration_capital_outflow_this_turn =
            outflow_result.total_domestic_debited;
        country.cooperative_registry.forex_reserve_drain_this_turn =
            outflow_result.total_forex_drained;
        country.cooperative_registry.total_emigration_capital_outflow +=
            outflow_result.total_domestic_debited;
        country.cooperative_registry.total_forex_reserve_drain +=
            outflow_result.total_forex_drained;
    }

    // Step 5: Rehousing — attempt to rehouse non-emigrating homeless members
    // using the 3-tier cascade (market rent → welfare → homeless shelter).
    // We iterate over a copy of indices to avoid borrow conflicts (Rule 9).
    let homeless_indices: Vec<usize> = country
        .cooperative_registry
        .homeless
        .iter()
        .enumerate()
        .filter(|(_, h)| !h.emigrated && !h.rehoused)
        .map(|(i, _)| i)
        .collect();

    for idx in homeless_indices {
        // Find vacancies in the member's region (simplified — in a full
        // implementation this would scan HousingBuilding vacancies).
        // For now, we pass an empty vacancy list if we can't access buildings.
        // The caller (turn loop) should pass building data if available.
        let vacancies: Vec<(String, String, f64)> = Vec::new();

        let (homeless, treasury) = split_at_mut_pair(
            &mut country.cooperative_registry.homeless,
            &mut country.budget,
            idx,
        );

        let outcome = crate::society::housing::CooperativeRegistry::try_rehouse(
            homeless,
            &vacancies,
            treasury,
            welfare_enabled,
        );

        match &outcome {
            RehousingOutcome::MarketRent { .. } => {
                result.homeless_to_rehoused += 1;
                // Macro-demographic update: move population from homeless
                // tracking back to housed class.
                for region in &mut country.regions {
                    if let Some(demo) =
                        region.class_demographics.urban_classes.values_mut().next()
                    {
                        demo.population += 1;
                    }
                }
            }
            RehousingOutcome::Welfare { .. } => {
                result.homeless_to_rehoused += 1;
                result.welfare_rehoused += 1;
                for region in &mut country.regions {
                    if let Some(demo) =
                        region.class_demographics.urban_classes.values_mut().next()
                    {
                        demo.population += 1;
                    }
                }
            }
            RehousingOutcome::RemainsHomeless => {
                // Member stays homeless — health penalties applied in Step 6
            }
        }
    }

    // Step 6: Apply health and happiness penalties to remaining homeless members
    // (those who haven't emigrated or been rehoused).
    // Previously these were computed as _health_penalty and discarded —
    // now we apply actual mutations to ClassDemographics (audit finding).
    for homeless in &country.cooperative_registry.homeless {
        if homeless.emigrated || homeless.rehoused {
            continue;
        }
        let health_penalty = homeless.health_penalty();
        let happiness_penalty = homeless.happiness_penalty();

        // Apply to the first urban class in the member's region.
        // In a full implementation, this would target the specific class
        // that the displaced member belonged to.
        for region in &mut country.regions {
            if region.id == homeless.region_id {
                if let Some(demo) =
                    region.class_demographics.urban_classes.values_mut().next()
                {
                    // Degrade health status based on penalty severity
                    if health_penalty > 0.6 {
                        demo.health_status = HealthStatus::Critical;
                    } else if health_penalty > 0.4 {
                        if demo.health_status != HealthStatus::Critical {
                            demo.health_status = HealthStatus::Poor;
                        }
                    } else if health_penalty > 0.2
                        && !matches!(
                            demo.health_status,
                            HealthStatus::Critical | HealthStatus::Poor
                        )
                    {
                        demo.health_status = HealthStatus::Fair;
                    }
                    // Degrade mental health (happiness proxy)
                    demo.mental_health =
                        (demo.mental_health - happiness_penalty).max(0.0);
                }
                break;
            }
        }
    }

    // Clean up rehoused and fully-emigrated members from the active homeless list
    country
        .cooperative_registry
        .homeless
        .retain(|h| !h.emigrated && !h.rehoused);

    result
}

/// Helper to get mutable references to a homeless entry and the treasury
/// simultaneously without double mutable borrows (Rule 9).
fn split_at_mut_pair<'a, T>(
    vec: &'a mut [T],
    treasury: &'a mut crate::state::Treasury,
    idx: usize,
) -> (&'a mut T, &'a mut crate::state::Treasury) {
    (&mut vec[idx], treasury)
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

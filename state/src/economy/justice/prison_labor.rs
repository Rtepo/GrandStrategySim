//! Prison labor integration with the labor market (Phase 14 + 14.5).
//!
//! This module preprocesses prison labor BEFORE the labor market resolution.
//! For PrivateLaborCamps, prisoner FTEs are injected into mining/heavy industry
//! companies by reducing their `target_fte_demand`, and the State collects
//! a per-FTE fee from those companies into the Treasury.
//!
//! For IsolationCamp, targeted demographics are removed from the workforce
//! (their `available_fte` is zeroed) and their social unrest contribution
//! is neutralized. The State pays Food maintenance from the Treasury.
//! Inmates perform internal maintenance labor reducing OPEX by 15% (Phase 14.5).
//!
//! For VoluntaryLabor and StatePenalColony, prisoners produce goods internally
//! in the prison building via normal production cycles — no labor market hook.
//!
//! Phase 14.5 additions: cohort-based sentence tracking, rehabilitation on
//! release, prison security level calculation, and escape mechanics.

use crate::economy::sentencing::{
    determine_crime_category, generate_sentence, process_death_penalties, CrimeCategory,
    SentenceOutcome,
};
use crate::entities::{Building, Company};
use crate::politics::laws::PrisonType;
use crate::politics::system::{JusticeSystemState, PrisonSecurityLevel, PrisonerCohort};
use crate::registries::enums::Sector;
use crate::society::geography::HealthStatus;
use crate::state::Country;

/// Result of processing prison labor for one turn.
#[derive(Debug, Clone, Default)]
pub struct PrisonLaborTurnResult {
    /// FTEs injected into companies via private labor camps.
    pub allocated_fte: f64,
    /// Total fees collected from companies into Treasury.
    pub fees_collected: f64,
    /// Number of people isolated from workforce.
    pub isolated_count: i64,
    /// Food maintenance cost paid by Treasury for isolation camps.
    pub maintenance_cost: f64,
    /// Number of companies that received prison labor.
    pub companies_injected: usize,
    /// Phase 14.5: Number of prisoners released this turn (sentences expired).
    pub released_count: i64,
    /// Phase 14.5: Number of prisoners who escaped this turn.
    pub escaped_count: i64,
    /// Phase 14.5: Unrest spike from escapes and IsolationCamp releases.
    pub unrest_spike: f64,
}

/// Sectors eligible for private labor camp FTE injection.
fn is_eligible_sector(sector: Sector) -> bool {
    matches!(
        sector,
        Sector::Mining | Sector::HeavyIndustry | Sector::Construction | Sector::Energy
    )
}

/// Default sentence length (in turns) by prison type.
fn default_sentence_length(prison_type: PrisonType) -> u32 {
    match prison_type {
        PrisonType::VoluntaryLabor => 5,
        PrisonType::StatePenalColony => 10,
        PrisonType::PrivateLaborCamps => 8,
        PrisonType::IsolationCamp => 15,
    }
}

/// Escape security threshold by prison type. Below this, escapes occur.
fn escape_threshold(prison_type: PrisonType) -> f64 {
    match prison_type {
        PrisonType::VoluntaryLabor => 0.25,
        PrisonType::StatePenalColony => 0.35,
        PrisonType::PrivateLaborCamps => 0.30,
        PrisonType::IsolationCamp => 0.40,
    }
}

/// Degrades a HealthStatus by one tier.
fn degrade_health(h: HealthStatus) -> HealthStatus {
    match h {
        HealthStatus::Excellent => HealthStatus::Good,
        HealthStatus::Good => HealthStatus::Fair,
        HealthStatus::Fair => HealthStatus::Poor,
        HealthStatus::Poor => HealthStatus::Critical,
        HealthStatus::Critical => HealthStatus::Critical,
    }
}

/// Processes sentence decrements and releases expired cohorts back into demographics.
///
/// # Arguments
/// * `country` - Mutable country for demographic restoration and unrest updates
/// * `justice_state` - Mutable justice state containing prisoner cohorts
///
/// # Returns
/// Number of prisoners released this turn.
fn process_cohort_releases(country: &mut Country, justice_state: &mut JusticeSystemState) -> i64 {
    let mut released_total = 0_i64;
    let mut unrest_spike = 0.0_f64;

    // Decrement all cohort sentences (skip LifeImprisonment which never decrements)
    for cohort in &mut justice_state.prisoner_cohorts {
        match cohort.sentence_outcome {
            SentenceOutcome::LifeImprisonment => {
                // Life imprisonment: sentence never expires
            }
            _ => {
                if cohort.sentence_remaining > 0 {
                    cohort.sentence_remaining -= 1;
                }
            }
        }
    }

    // Phase 18B: Process death penalties first (executed cohorts are removed permanently)
    let (_executed, _death_unrest) = process_death_penalties(country, justice_state);

    // Partition into expired and remaining
    // Community service cohorts are released when their sentence expires (garnishment ends)
    let mut expired: Vec<PrisonerCohort> = Vec::new();
    let mut remaining: Vec<PrisonerCohort> = Vec::new();
    for cohort in justice_state.prisoner_cohorts.drain(..) {
        if cohort.sentence_remaining == 0
            && cohort.sentence_outcome != SentenceOutcome::LifeImprisonment
        {
            expired.push(cohort);
        } else {
            remaining.push(cohort);
        }
    }
    justice_state.prisoner_cohorts = remaining;

    // Release expired cohorts back into their origin demographics
    for cohort in &expired {
        released_total += cohort.count;

        // Phase 18B: Community service cohorts were never removed from demographics.
        // They stayed in the labor pool with garnished wages. On release, just
        // stop the garnishment (cohort is removed from justice_state). No population/FTE restoration needed.
        if matches!(
            cohort.sentence_outcome,
            SentenceOutcome::CommunityService(_)
        ) {
            continue;
        }

        // Find the origin region and class
        for region in &mut country.regions {
            if region.id != cohort.origin_region_id {
                continue;
            }
            let class_opt = if cohort.origin_is_urban {
                region
                    .class_demographics
                    .urban_classes
                    .get_mut(&cohort.origin_class_id)
            } else {
                region
                    .class_demographics
                    .rural_classes
                    .get_mut(&cohort.origin_class_id)
            };
            if let Some(class) = class_opt {
                // Restore population
                class.population += cohort.count;

                // Restore available_fte proportionally
                let fte_per_capita = if class.population > cohort.count {
                    class.available_fte / (class.population - cohort.count) as f64
                } else {
                    1.5 // default full-time + half-time
                };
                class.available_fte += fte_per_capita * cohort.count as f64;

                // Apply rehabilitation effects based on prison type
                match cohort.sentenced_under {
                    PrisonType::VoluntaryLabor => {
                        // No degradation — voluntary work maintains skills
                    }
                    PrisonType::StatePenalColony => {
                        // Skills rusted
                        class.labor_participation = (class.labor_participation - 0.05).max(0.0);
                        // Health degraded
                        class.health_status = degrade_health(cohort.intake_health);
                    }
                    PrisonType::PrivateLaborCamps => {
                        // Mild skill degradation
                        class.labor_participation = (class.labor_participation - 0.02).max(0.0);
                        // Radicalization
                        class.political_sentiment.radicals =
                            (class.political_sentiment.radicals + 0.03).min(1.0);
                        class.political_sentiment.loyalists =
                            (class.political_sentiment.loyalists - 0.03).max(0.0);
                        class.political_sentiment.normalize();
                    }
                    PrisonType::IsolationCamp => {
                        // Severe skill degradation
                        class.labor_participation = (class.labor_participation - 0.05).max(0.0);
                        // Strong radicalization
                        class.political_sentiment.radicals =
                            (class.political_sentiment.radicals + 0.05).min(1.0);
                        class.political_sentiment.loyalists =
                            (class.political_sentiment.loyalists - 0.05).max(0.0);
                        class.political_sentiment.normalize();
                        // Health degraded
                        class.health_status = degrade_health(cohort.intake_health);
                        // Unrest spike: political prisoners return angry
                        let total_pop = class.population.max(1) as f64;
                        unrest_spike += 2.0 * cohort.count as f64 / total_pop * 100.0;
                    }
                }
                break;
            }
        }
    }

    // Apply accumulated unrest spike
    if unrest_spike > 0.0 {
        country.macro_indicators.social_unrest += unrest_spike;
    }

    released_total
}

/// Calculates prison security levels and triggers escapes if security is too low.
///
/// # Arguments
/// * `country` - Mutable country for unrest updates
/// * `buildings` - All buildings (to find prison and assess security)
/// * `justice_state` - Mutable justice state for security level storage
///
/// # Returns
/// Number of prisoners who escaped this turn.
fn process_prison_escapes(
    country: &mut Country,
    buildings: &[Building],
    justice_state: &mut JusticeSystemState,
    prison_type: PrisonType,
) -> i64 {
    let threshold = escape_threshold(prison_type);

    let mut security_levels = Vec::new();
    let mut total_escaped = 0_i64;
    let mut unrest_from_escapes = 0.0_f64;

    for building in buildings.iter().filter(|b| b.name == "prison") {
        let guard_fte = building.current_employment as f64;
        let target_guard_fte = building.worker_capacity as f64;
        let guard_ratio = if target_guard_fte > 0.0 {
            (guard_fte / target_guard_fte).min(1.0)
        } else {
            0.0
        };
        let condition = building.condition;
        let security_score = guard_ratio * 0.6 + condition * 0.4;

        security_levels.push(PrisonSecurityLevel {
            building_id: building.id.clone(),
            security_score,
            guard_fte,
            target_guard_fte,
            condition,
        });

        // Check for escapes
        if security_score < threshold {
            let prisoners_here = building.worker_capacity as i64;
            let escape_fraction = (threshold - security_score) * 3.0;
            let escape_count = (prisoners_here as f64 * escape_fraction) as i64;

            if escape_count > 0 {
                total_escaped += escape_count;

                // Remove escapees from cohorts (largest first)
                let mut to_remove = escape_count;
                for cohort in &mut justice_state.prisoner_cohorts {
                    if to_remove <= 0 {
                        break;
                    }
                    let removed = cohort.count.min(to_remove);
                    cohort.count -= removed;
                    to_remove -= removed;
                }
                // Remove empty cohorts
                justice_state.prisoner_cohorts.retain(|c| c.count > 0);

                // Each escapee spikes unrest and crime demand
                let total_pop: f64 = country
                    .regions
                    .iter()
                    .flat_map(|r| {
                        r.class_demographics
                            .rural_classes
                            .values()
                            .chain(r.class_demographics.urban_classes.values())
                    })
                    .map(|c| c.population as f64)
                    .sum::<f64>()
                    .max(1.0);
                unrest_from_escapes += 5.0 * escape_count as f64 / total_pop * 100.0;
                justice_state.justice_demand += escape_count as f64 * 2.0;
            }
        }
    }

    if unrest_from_escapes > 0.0 {
        country.macro_indicators.social_unrest += unrest_from_escapes;
    }

    // Update active_prisoners after escapes
    justice_state.active_prisoners -= total_escaped;
    justice_state.prison_security_levels = security_levels;

    total_escaped
}

/// Generates new prisoner cohorts from crime demand overflow.
/// When justice coverage < 1.0, unresolved crime leads to arrests.
///
/// # Arguments
/// * `country` - Country state for demographics lookup
/// * `justice_state` - Mutable justice state to add cohorts to
/// * `total_prisoners` - Current total prisoner capacity from buildings
fn generate_new_cohorts(
    country: &Country,
    justice_state: &mut JusticeSystemState,
    total_prisoners: i64,
    prison_type: PrisonType,
) {
    // Calculate how many new prisoners to intake based on coverage gap
    let coverage = justice_state.justice_coverage;
    if coverage >= 1.0 {
        return;
    }

    // New arrests = coverage gap * small fraction of total population
    let total_pop: i64 = country
        .regions
        .iter()
        .flat_map(|r| {
            r.class_demographics
                .rural_classes
                .values()
                .chain(r.class_demographics.urban_classes.values())
        })
        .map(|c| c.population)
        .sum();

    if total_pop <= 0 {
        return;
    }

    let coverage_gap = 1.0 - coverage;
    let new_arrests = ((total_pop as f64 * coverage_gap * 0.001) as i64)
        .min((total_prisoners - justice_state.active_prisoners).max(0));

    if new_arrests <= 0 {
        return;
    }

    // Phase 18B: Dynamic sentencing — use SentencingLaw if available, else fall back to hardcoded.
    let sentencing_law = country.politics.sentencing_law.clone();
    let dominant_religion = country.macro_indicators.religion.clone();

    // Distribute arrests across regions and classes proportionally to radical population
    for region in &country.regions {
        for (class_id, class) in &region.class_demographics.rural_classes {
            let radicals = (class.population as f64 * class.political_sentiment.radicals) as i64;
            if radicals > 0 {
                let arrests = (new_arrests * radicals / total_pop.max(1))
                    .max(1)
                    .min(class.population);
                if arrests > 0 {
                    let (crime_category, sentence_outcome, sentence_turns) =
                        if let Some(ref law) = sentencing_law {
                            let radical_fraction = class.political_sentiment.radicals;
                            let category = determine_crime_category(coverage_gap, radical_fraction);
                            let is_minority_religion =
                                !class.religion.is_empty() && class.religion != dominant_religion;
                            let (outcome, turns) = generate_sentence(
                                category,
                                law,
                                class.legal_status,
                                is_minority_religion,
                                0.5, // deterministic mid-range sentence
                            );
                            (category, outcome, turns)
                        } else {
                            let sentence = default_sentence_length(prison_type);
                            (
                                CrimeCategory::Misdemeanor,
                                SentenceOutcome::Imprisonment(sentence),
                                sentence,
                            )
                        };

                    // For community service, don't remove from demographics
                    let is_community_service =
                        matches!(sentence_outcome, SentenceOutcome::CommunityService(_));

                    justice_state.prisoner_cohorts.push(PrisonerCohort {
                        origin_class_id: class_id.clone(),
                        origin_is_urban: false,
                        origin_region_id: region.id.clone(),
                        sentence_remaining: sentence_turns,
                        count: arrests,
                        intake_health: class.health_status,
                        sentenced_under: prison_type,
                        crime_category,
                        sentence_outcome,
                        legal_status: class.legal_status,
                    });

                    // Only deduct population for non-community-service cohorts
                    if !is_community_service {
                        // Population deduction happens in process_prison_labor_turn for isolation camps
                    }
                }
            }
        }
        for (class_id, class) in &region.class_demographics.urban_classes {
            let radicals = (class.population as f64 * class.political_sentiment.radicals) as i64;
            if radicals > 0 {
                let arrests = (new_arrests * radicals / total_pop.max(1))
                    .max(1)
                    .min(class.population);
                if arrests > 0 {
                    let (crime_category, sentence_outcome, sentence_turns) =
                        if let Some(ref law) = sentencing_law {
                            let radical_fraction = class.political_sentiment.radicals;
                            let category = determine_crime_category(coverage_gap, radical_fraction);
                            let is_minority_religion =
                                !class.religion.is_empty() && class.religion != dominant_religion;
                            let (outcome, turns) = generate_sentence(
                                category,
                                law,
                                class.legal_status,
                                is_minority_religion,
                                0.5,
                            );
                            (category, outcome, turns)
                        } else {
                            let sentence = default_sentence_length(prison_type);
                            (
                                CrimeCategory::Misdemeanor,
                                SentenceOutcome::Imprisonment(sentence),
                                sentence,
                            )
                        };

                    let is_community_service =
                        matches!(sentence_outcome, SentenceOutcome::CommunityService(_));

                    justice_state.prisoner_cohorts.push(PrisonerCohort {
                        origin_class_id: class_id.clone(),
                        origin_is_urban: true,
                        origin_region_id: region.id.clone(),
                        sentence_remaining: sentence_turns,
                        count: arrests,
                        intake_health: class.health_status,
                        sentenced_under: prison_type,
                        crime_category,
                        sentence_outcome,
                        legal_status: class.legal_status,
                    });

                    if !is_community_service {
                        // Population deduction happens in process_prison_labor_turn for isolation camps
                    }
                }
            }
        }
    }

    // Update active_prisoners to include new cohorts
    let cohort_total: i64 = justice_state.prisoner_cohorts.iter().map(|c| c.count).sum();
    justice_state.active_prisoners = cohort_total;
}

/// Processes prison labor for one turn.
///
/// Must be called BEFORE `resolve_regional_labor_market` in the turn loop.
///
/// # Arguments
/// * `country` - Mutable country state (for Treasury, demographics, justice_state)
/// * `buildings` - All buildings (to find prison buildings and count prisoners)
/// * `companies` - All companies (to inject FTEs for PrivateLaborCamps)
///
/// # Returns
/// `PrisonLaborTurnResult` with allocation statistics.
///
/// # Rules
/// * **PrivateLaborCamps**: Reduces `company.target_fte_demand` by injected FTEs.
///   Companies pay `private_transfer_fee * fte` to Treasury. Double-entry:
///   Debit company.available_cash, Credit budget.liquid_reserves.
/// * **IsolationCamp**: Zeroes `available_fte` for targeted demographic classes
///   across all regions. Reduces their `health_status` by degradation rate.
///   Treasury pays Food maintenance: `isolated_count * food_cost_per_capita`.
/// * **VoluntaryLabor / StatePenalColony**: No labor market hook needed —
///   these operate through normal building production cycles.
pub fn process_prison_labor_turn(
    country: &mut Country,
    buildings: &[Building],
    companies: &mut [Company],
) -> PrisonLaborTurnResult {
    let mut result = PrisonLaborTurnResult::default();

    // Get prison labor law; if none, return early
    let law = match country.politics.prison_labor_law.clone() {
        Some(l) => l,
        None => return result,
    };

    // Count total prisoners from prison buildings
    let total_prisoners: i64 = buildings
        .iter()
        .filter(|b| b.name == "prison")
        .map(|b| b.worker_capacity as i64)
        .sum();

    // Ensure justice_state exists, then extract it to avoid borrow conflicts
    if country.politics.justice_state.is_none() {
        country.politics.justice_state = Some(JusticeSystemState::default());
    }
    let mut justice_state = country.politics.justice_state.take().unwrap();

    // Phase 14.5: Process cohort releases (sentences expired)
    let released = process_cohort_releases(country, &mut justice_state);
    result.released_count = released;

    // Phase 14.5: Generate new cohorts from crime demand overflow
    generate_new_cohorts(
        country,
        &mut justice_state,
        total_prisoners,
        law.prison_type,
    );

    justice_state.active_prisoners = total_prisoners;

    match law.prison_type {
        PrisonType::VoluntaryLabor | PrisonType::StatePenalColony => {
            // No labor market hook — production happens via building PMs.
            // Prisoners accrue minor savings (VoluntaryLabor) or suffer
            // health degradation (StatePenalColony), handled in production cycle.
        }
        PrisonType::PrivateLaborCamps => {
            let eligible_companies: Vec<usize> = companies
                .iter()
                .enumerate()
                .filter(|(_, c)| is_eligible_sector(c.sector))
                .map(|(i, _)| i)
                .collect();

            if eligible_companies.is_empty() {
                country.politics.justice_state = Some(justice_state);
                return result;
            }

            // Distribute prisoner FTEs across eligible companies proportionally
            // to their target_fte_demand.
            let total_demand: f64 = eligible_companies
                .iter()
                .map(|&i| companies[i].target_fte_demand as f64)
                .sum();

            if total_demand <= 0.0 {
                country.politics.justice_state = Some(justice_state);
                return result;
            }

            // Prisoner FTE = total_prisoners * 0.8 (80% labor utilization rate)
            let prisoner_fte_pool = total_prisoners as f64 * 0.8;
            let mut allocated_total = 0.0_f64;
            let mut fees_total = 0.0_f64;
            let mut injected_count = 0_usize;

            for &idx in &eligible_companies {
                let share = companies[idx].target_fte_demand as f64 / total_demand;
                let injected_fte = prisoner_fte_pool * share;

                if injected_fte < 0.01 {
                    continue;
                }

                // Reduce company's labor demand by injected FTEs
                companies[idx].target_fte_demand = ((companies[idx].target_fte_demand as f64)
                    - injected_fte)
                    .max(0.0)
                    .round() as u32;

                // Company pays transfer fee to Treasury
                let fee = injected_fte * law.private_transfer_fee;
                let fee_clamped = fee.min(companies[idx].available_cash);
                companies[idx].available_cash -= fee_clamped;
                country.budget.liquid_reserves += fee_clamped;

                allocated_total += injected_fte;
                fees_total += fee_clamped;
                injected_count += 1;
            }

            result.allocated_fte = allocated_total;
            result.fees_collected = fees_total;
            result.companies_injected = injected_count;
            justice_state.prison_labor_allocated_fte = allocated_total;
        }
        PrisonType::IsolationCamp => {
            let target = law.target_demographic.as_deref().unwrap_or("");
            let capacity = law.isolation_capacity.min(total_prisoners);

            if capacity <= 0 || target.is_empty() {
                // Still process escapes even if no isolation capacity
                let escaped =
                    process_prison_escapes(country, buildings, &mut justice_state, law.prison_type);
                result.escaped_count = escaped;
                country.politics.justice_state = Some(justice_state);
                return result;
            }

            let mut isolated = 0_i64;
            let food_cost_per_capita = 5.0; // 5 currency units per prisoner per turn

            // Phase 14.5: Internal labor factor — 30% of inmates perform camp
            // maintenance labor, reducing OPEX by 15% (30% working × 50% offset).
            let internal_labor_fraction = 0.30;
            let opex_reduction = internal_labor_fraction * 0.50; // = 0.15 (15%)

            // Remove targeted demographics from workforce across all regions
            for region in &mut country.regions {
                // Check rural classes
                if let Some(class) = region.class_demographics.rural_classes.get_mut(target) {
                    let removable = class.population.min(capacity - isolated);
                    if removable > 0 {
                        // Zero out their available_fte contribution
                        let fte_removed = class.available_fte
                            * (removable as f64 / class.population.max(1) as f64);
                        class.available_fte -= fte_removed;
                        if class.available_fte < 0.0 {
                            class.available_fte = 0.0;
                        }

                        // Degrade health
                        class.health_status = degrade_health(class.health_status);

                        isolated += removable;
                    }
                }

                // Check urban classes
                if isolated < capacity {
                    if let Some(class) = region.class_demographics.urban_classes.get_mut(target) {
                        let removable = class.population.min(capacity - isolated);
                        if removable > 0 {
                            let fte_removed = class.available_fte
                                * (removable as f64 / class.population.max(1) as f64);
                            class.available_fte -= fte_removed;
                            if class.available_fte < 0.0 {
                                class.available_fte = 0.0;
                            }

                            class.health_status = degrade_health(class.health_status);

                            isolated += removable;
                        }
                    }
                }
            }

            // Treasury pays Food maintenance for isolated population
            // Phase 14.5: Reduced by internal labor OPEX offset (15% reduction)
            let maintenance = isolated as f64 * food_cost_per_capita * (1.0 - opex_reduction);
            let maintenance_clamped = maintenance.min(country.budget.liquid_reserves);
            country.budget.liquid_reserves -= maintenance_clamped;

            result.isolated_count = isolated;
            result.maintenance_cost = maintenance_clamped;
            justice_state.isolated_population = isolated;
        }
    }

    // Phase 14.5: Process prison security levels and escapes
    let escaped = process_prison_escapes(country, buildings, &mut justice_state, law.prison_type);
    result.escaped_count = escaped;
    if escaped > 0 {
        result.unrest_spike = escaped as f64 * 5.0;
    }

    // Restore justice_state
    country.politics.justice_state = Some(justice_state);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eligible_sectors() {
        assert!(is_eligible_sector(Sector::Mining));
        assert!(is_eligible_sector(Sector::HeavyIndustry));
        assert!(is_eligible_sector(Sector::Construction));
        assert!(is_eligible_sector(Sector::Energy));
        assert!(!is_eligible_sector(Sector::Agriculture));
        assert!(!is_eligible_sector(Sector::Banking));
    }

    #[test]
    fn test_no_law_returns_default() {
        let mut country = Country::mock_for_tests();
        let buildings: Vec<Building> = Vec::new();
        let mut companies: Vec<Company> = Vec::new();
        let result = process_prison_labor_turn(&mut country, &buildings, &mut companies);
        assert_eq!(result.allocated_fte, 0.0);
        assert_eq!(result.isolated_count, 0);
    }
}

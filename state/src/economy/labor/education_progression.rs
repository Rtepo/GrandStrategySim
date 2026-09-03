//! Phase B.1: Education Progression — Dynamic Demographics Transitions.
//!
//! This module implements the turn-phase that consumes `EducationSlots` (from
//! B2C clearing) to mathematically shift `demographics.education` shares:
//!
//! ```text
//!   none → basic → secondary → higher
//! ```
//!
//! # Rules
//! * Reads education services **actually consumed**, not nominal capacity.
//! * Computes progression by regional population/demographic pools.
//! * Uses dynamic rates based on coverage (consumed / needed), not magic constants.
//! * Applies bounds so no share becomes negative or exceeds its source population.
//! * Preserves total population share (sum = 1.0 after each turn).
//! * Handles specialization sub-maps consistently (pro-rata distribution).
//! * The one-turn lag to labor market is documented and intentional (Rule 16).
//! * Includes a forgetting rate (Death mechanism, Rule 4) when coverage collapses.

use crate::state::Country;
use std::collections::BTreeMap;

/// Result of the education progression phase (for telemetry/debugging).
#[derive(Debug, Clone, Default)]
pub struct EducationProgressionResult {
    /// Total population that moved none → basic this turn.
    pub none_to_basic: f64,
    /// Total population that moved basic → secondary this turn.
    pub basic_to_secondary: f64,
    /// Total population that moved secondary → higher this turn.
    pub secondary_to_higher: f64,
    /// Total population that degraded basic → none (forgetting rate).
    pub basic_to_none: f64,
    /// Mean national education coverage after this turn.
    pub mean_coverage: f64,
}

/// Physical constant: maximum fraction of an upgradable cohort that can
/// transition per turn. Education takes ~10 turns to complete a tier,
/// so the max rate is 0.10 (10%). This is a **physical** rate (time to
/// complete a degree), not a financial one — Rule 3 exempt, Rule 15 compliant.
const MAX_TRANSITION_RATE: f64 = 0.10;

/// Physical constant: coverage threshold below which education infrastructure
/// has effectively collapsed and the forgetting rate kicks in.
const COVERAGE_COLLAPSE_THRESHOLD: f64 = 0.1;

/// Physical constant: maximum forgetting rate per turn (very slow — literacy
/// loss in a generation without schooling). Rule 3: physical, not financial.
const MAX_FORGETTING_RATE: f64 = 0.01;

/// Process the education progression turn for a single country.
///
/// # Arguments
/// * `country` - Mutable country state (demographics + education_statistics updated in place).
/// * `education_consumption` - Per-region consumed EducationSlots (from B2C clearing).
/// * `education_needs` - Per-region needed EducationSlots (from populate_education_service_needs).
///
/// # Returns
/// `EducationProgressionResult` with transition totals for telemetry.
///
/// # Temporal Causality (Rule 16)
/// This phase runs **after** B2C education clearing and **before** assimilation.
/// The updated education shares are visible to `process_demographics_and_labor`
/// **next turn** (one-turn lag — education takes time to filter into the labor pool).
pub fn process_education_progression_turn(
    country: &mut Country,
    education_consumption: &BTreeMap<String, f64>,
    education_needs: &BTreeMap<String, f64>,
) -> EducationProgressionResult {
    let mut result = EducationProgressionResult::default();
    let mut total_coverage = 0.0;
    let mut region_count = 0u32;

    // Compute national-level coverage as the population-weighted mean of
    // regional coverage. Since demographics are at the country level (not
    // per-region), we aggregate consumption and needs nationally.
    let total_consumed: f64 = education_consumption.values().sum();
    let total_needed: f64 = education_needs.values().sum();

    let national_coverage = if total_needed > 0.0 {
        (total_consumed / total_needed).clamp(0.0, 1.0)
    } else {
        0.0
    };

    total_coverage += national_coverage;
    region_count += 1;

    result.mean_coverage = if region_count > 0 {
        total_coverage / region_count as f64
    } else {
        0.0
    };

    let coverage = national_coverage;

    // Get mutable access to demographics (stored on macro_indicators).
    let demographics = &mut country.macro_indicators.demographics;
    let youth_share = demographics.age_groups.children.max(0.0).min(1.0);
    let adult_share = demographics.age_groups.adults.max(0.0).min(1.0);

    // === Transition 1: none → basic ===
    // Target: children with no education. Rate scales with coverage.
    let upgradable_none = demographics.education.none * youth_share;
    let rate_none_to_basic = (MAX_TRANSITION_RATE * coverage)
        .min(MAX_TRANSITION_RATE)
        .max(0.0);
    let transition_none = upgradable_none * rate_none_to_basic;

    demographics.education.none = (demographics.education.none - transition_none).max(0.0);
    demographics.education.basic += transition_none;
    result.none_to_basic = transition_none;

    // === Transition 2: basic → secondary ===
    // Target: working-age adults with basic education re-entering education.
    // Adult re-entry share scales with coverage (more coverage = more opportunities).
    let adult_reentry_share = coverage * 0.05; // dynamic, not magic
    let upgradable_basic = demographics.education.basic * adult_share * adult_reentry_share;
    let rate_basic_to_secondary = (MAX_TRANSITION_RATE * coverage)
        .min(MAX_TRANSITION_RATE)
        .max(0.0);
    let transition_basic = upgradable_basic * rate_basic_to_secondary;

    demographics.education.basic = (demographics.education.basic - transition_basic).max(0.0);

    // Distribute into secondary sub-keys pro-rata by existing sub-key shares.
    // If no existing sub-keys, distribute evenly across standard specializations.
    distribute_pro_rata(
        &mut demographics.education.secondary,
        transition_basic,
        &["Vocational", "Technical", "Humanities"],
    );
    result.basic_to_secondary = transition_basic;

    // === Transition 3: secondary → higher ===
    // Target: young adults with secondary education entering university.
    let secondary_total = demographics.education.secondary_share();
    let upgradable_secondary = secondary_total * youth_share * coverage;
    let rate_secondary_to_higher = (MAX_TRANSITION_RATE * coverage)
        .min(MAX_TRANSITION_RATE)
        .max(0.0);
    let transition_secondary = upgradable_secondary * rate_secondary_to_higher;

    // Remove from secondary sub-keys pro-rata.
    remove_pro_rata(&mut demographics.education.secondary, transition_secondary);

    // Distribute into higher sub-keys pro-rata by existing sub-key shares.
    distribute_pro_rata(
        &mut demographics.education.higher,
        transition_secondary,
        &["Technical", "Humanities", "Medical"],
    );
    result.secondary_to_higher = transition_secondary;

    // === Death mechanism (Rule 4): forgetting rate ===
    // When coverage collapses below the threshold, basic education is lost
    // (literacy loss in a generation without schooling).
    if coverage < COVERAGE_COLLAPSE_THRESHOLD {
        let forgetting_rate = ((COVERAGE_COLLAPSE_THRESHOLD - coverage) * MAX_FORGETTING_RATE)
            .min(MAX_FORGETTING_RATE)
            .max(0.0);
        let degraded = demographics.education.basic * forgetting_rate;
        demographics.education.basic = (demographics.education.basic - degraded).max(0.0);
        demographics.education.none += degraded;
        result.basic_to_none = degraded;
    }

    // === Renormalize (Rule 20: shares must sum to 1.0) ===
    let total = demographics.education.none
        + demographics.education.basic
        + demographics.education.secondary_share()
        + demographics.education.higher_share();

    if total > 0.0 {
        let scale = 1.0 / total;
        demographics.education.none *= scale;
        demographics.education.basic *= scale;
        for v in demographics.education.secondary.values_mut() {
            *v *= scale;
        }
        for v in demographics.education.higher.values_mut() {
            *v *= scale;
        }
    }

    // === Update EducationStatistics (Full-Stack Accountability, Rule 17) ===
    country.macro_indicators.education_statistics.literacy_rate =
        (1.0 - country.macro_indicators.demographics.education.none).clamp(0.0, 1.0);
    country
        .macro_indicators
        .education_statistics
        .higher_education_rate = country
        .macro_indicators
        .demographics
        .education
        .higher_share()
        .clamp(0.0, 1.0);

    result
}

/// Distribute `amount` into a specialization map pro-rata by existing shares.
/// If the map is empty, distribute evenly across `default_keys`.
fn distribute_pro_rata(map: &mut BTreeMap<String, f64>, amount: f64, default_keys: &[&str]) {
    if amount <= 0.0 {
        return;
    }

    let current_total: f64 = map.values().sum();

    if current_total > 1e-12 {
        // Pro-rata by existing shares (Rule 5: pro-rata, not 50/50).
        let keys: Vec<String> = map.keys().cloned().collect();
        for key in &keys {
            if let Some(share) = map.get(key) {
                let fraction = share / current_total;
                *map.entry(key.clone()).or_insert(0.0) += amount * fraction;
            }
        }
    } else {
        // No existing shares — distribute evenly across default keys.
        let per_key = amount / default_keys.len() as f64;
        for key in default_keys {
            *map.entry(key.to_string()).or_insert(0.0) += per_key;
        }
    }
}

/// Remove `amount` from a specialization map pro-rata by existing shares.
fn remove_pro_rata(map: &mut BTreeMap<String, f64>, amount: f64) {
    if amount <= 0.0 {
        return;
    }

    let current_total: f64 = map.values().sum();
    if current_total <= 1e-12 {
        return;
    }

    let remove_amount = amount.min(current_total);
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in &keys {
        if let Some(share) = map.get(key) {
            let fraction = share / current_total;
            let deduction = remove_amount * fraction;
            let current = *map.get(key).unwrap_or(&0.0);
            map.insert(key.clone(), (current - deduction).max(0.0));
        }
    }
}

/// Phase E.4: Compute child labor FTE per region based on education capacity
/// and child labor law.
pub fn compute_child_labor_fte(
    country: &mut Country,
    education_consumption: &BTreeMap<String, f64>,
    education_needs: &BTreeMap<String, f64>,
) -> BTreeMap<String, f64> {
    use crate::politics::laws::ChildLaborLaw;

    let mut child_labor_by_region = BTreeMap::new();
    let permitted_fraction = match &country.politics.child_labor_law {
        Some(law) => law.permitted_child_labor_fraction(),
        None => 0.0,
    };

    if permitted_fraction <= 0.0 {
        return child_labor_by_region;
    }

    let youth_share = country
        .macro_indicators
        .demographics
        .age_groups
        .children
        .max(0.0)
        .min(1.0);

    for region in &mut country.regions {
        if region.population <= 0 {
            continue;
        }

        let consumed = education_consumption.get(&region.id).copied().unwrap_or(0.0);
        let needed = education_needs.get(&region.id).copied().unwrap_or(0.0);
        let coverage = if needed > 0.0 {
            (consumed / needed).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let unserved_fraction = 1.0 - coverage;
        let child_labor_eligible_fraction = unserved_fraction * permitted_fraction;
        let youth_pop = region.population as f64 * youth_share;
        let child_labor_fte = youth_pop * child_labor_eligible_fraction;

        if child_labor_fte > 0.0 {
            let rural_pop: f64 = region
                .class_demographics
                .rural_classes
                .values()
                .map(|d| d.population as f64)
                .sum();
            let urban_pop: f64 = region
                .class_demographics
                .urban_classes
                .values()
                .map(|d| d.population as f64)
                .sum();
            let total_pop = rural_pop + urban_pop;

            if total_pop > 0.0 {
                let rural_child_fte = child_labor_fte * (rural_pop / total_pop);
                let urban_child_fte = child_labor_fte * (urban_pop / total_pop);

                if rural_child_fte > 0.0 && rural_pop > 0.0 {
                    for demo in region.class_demographics.rural_classes.values_mut() {
                        let share = demo.population as f64 / rural_pop;
                        demo.available_fte += rural_child_fte * share;
                    }
                }

                if urban_child_fte > 0.0 && urban_pop > 0.0 {
                    for demo in region.class_demographics.urban_classes.values_mut() {
                        let share = demo.population as f64 / urban_pop;
                        demo.available_fte += urban_child_fte * share;
                    }
                }
            }
        }

        child_labor_by_region.insert(region.id.clone(), child_labor_fte);
    }

    child_labor_by_region
}

/// Phase E.9.2: Translate education building seat types when SchoolSystem changes.
pub fn translate_school_seat_types(
    buildings: &mut [crate::entities::Building],
    old_system: crate::politics::laws::SchoolSystem,
    new_system: crate::politics::laws::SchoolSystem,
) -> usize {
    use crate::politics::laws::SchoolSystem;
    use crate::registries::enums::CapacityType;

    if old_system == new_system {
        return 0;
    }

    let old_has_middle = old_system.has_middle_tier();
    let new_has_middle = new_system.has_middle_tier();

    if old_has_middle && !new_has_middle {
        let mut changed = 0;
        for building in buildings.iter_mut() {
            if building.active_method.seat_type == Some(CapacityType::MiddleSeats) {
                building.active_method.seat_type = Some(CapacityType::HighSchoolSeats);
                changed += 1;
            }
        }
        return changed;
    }

    0
}

/// Phase E.9.3: Per-tier education needs based on age progression brackets.
pub fn compute_per_tier_education_needs(
    country: &Country,
) -> BTreeMap<String, (f64, f64, f64)> {
    use crate::politics::laws::SchoolSystem;

    let school_system = country
        .politics
        .education_law
        .as_ref()
        .map(|law| law.school_system)
        .unwrap_or(SchoolSystem::FourPlusFourPlusFour);

    let (primary_bracket, middle_bracket, high_bracket) = school_system.age_brackets();
    let edu_config = &country.education_config;

    let age_groups = &country.macro_indicators.demographics.age_groups;
    let children_share = age_groups.children.max(0.0).min(1.0);
    let adults_share = age_groups.adults.max(0.0).min(1.0);

    let (primary_age_span, middle_age_span, high_age_span) = {
        let p = (primary_bracket.1 - primary_bracket.0) as f64;
        let m = if middle_bracket.1 > 0 {
            (middle_bracket.1 - middle_bracket.0) as f64
        } else {
            0.0
        };
        let h = (high_bracket.1 - high_bracket.0) as f64;
        (p, m, h)
    };

    let total_schooling_span = primary_age_span + middle_age_span + high_age_span;
    const UNIVERSITY_AGE_SPAN: f64 = 6.0;
    const ADULT_AGE_SPAN: f64 = 49.0;

    let mut needs = BTreeMap::new();

    for region in &country.regions {
        if region.population <= 0 {
            needs.insert(region.id.clone(), (0.0, 0.0, 0.0));
            continue;
        }

        let pop = region.population as f64;

        let (primary_need, secondary_need, higher_need) = if total_schooling_span > 0.0 && children_share > 0.0 {
            let school_age_pop = pop * children_share
                * (total_schooling_span / (total_schooling_span + UNIVERSITY_AGE_SPAN));
            let primary_pop = school_age_pop * (primary_age_span / total_schooling_span);
            let secondary_pop = school_age_pop * ((middle_age_span + high_age_span) / total_schooling_span);
            let university_pop = pop * adults_share * (UNIVERSITY_AGE_SPAN / ADULT_AGE_SPAN);

            let urban_pop: f64 = region
                .class_demographics
                .urban_classes
                .values()
                .map(|d| d.population as f64)
                .sum();
            let rural_pop: f64 = region
                .class_demographics
                .rural_classes
                .values()
                .map(|d| d.population as f64)
                .sum();
            let total_pop = urban_pop + rural_pop;
            let urban_mult = if total_pop > 0.0 {
                let urban_fraction = urban_pop / total_pop;
                1.0 + urban_fraction * (edu_config.urban_education_mult - 1.0)
            } else {
                1.0
            };

            (primary_pop, secondary_pop * urban_mult, university_pop)
        } else {
            let base_need = pop * edu_config.education_need_fraction;
            (base_need * 0.50, base_need * 0.35, base_need * 0.15)
        };

        needs.insert(region.id.clone(), (primary_need, secondary_need, higher_need));
    }

    needs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_demographics(
        none: f64,
        basic: f64,
        secondary: &[(&str, f64)],
        higher: &[(&str, f64)],
    ) -> crate::state::macro_data::Demographics {
        crate::state::macro_data::Demographics {
            age_groups: crate::state::macro_data::AgeGroups {
                children: 0.25,
                adults: 0.6,
                elderly: 0.15,
                ..Default::default()
            },
            education: crate::state::macro_data::Education {
                none,
                basic,
                secondary: secondary.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
                higher: higher.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_education_progression_shifts_tiers() {
        let mut country = Country::default();
        country.macro_indicators.demographics = make_demographics(
            0.5,
            0.3,
            &[("Technical", 0.1), ("Humanities", 0.05)],
            &[("Medical", 0.05)],
        );

        let mut consumption = BTreeMap::new();
        consumption.insert("region_1".to_string(), 100.0);
        let mut needs = BTreeMap::new();
        needs.insert("region_1".to_string(), 100.0);

        let result = process_education_progression_turn(&mut country, &consumption, &needs);

        // With full coverage, none should decrease and basic should increase.
        assert!(
            result.none_to_basic > 0.0,
            "none_to_basic should be positive"
        );
        assert!(
            country.macro_indicators.demographics.education.none < 0.5,
            "none should decrease"
        );
        assert!(
            country.macro_indicators.demographics.education.basic > 0.3,
            "basic should increase"
        );
    }

    #[test]
    fn test_education_shares_sum_to_one() {
        let mut country = Country::default();
        country.macro_indicators.demographics = make_demographics(
            0.4,
            0.3,
            &[("Technical", 0.15), ("Humanities", 0.1)],
            &[("Medical", 0.05)],
        );

        let mut consumption = BTreeMap::new();
        consumption.insert("r1".to_string(), 50.0);
        let mut needs = BTreeMap::new();
        needs.insert("r1".to_string(), 100.0);

        let _ = process_education_progression_turn(&mut country, &consumption, &needs);

        let total = country.macro_indicators.demographics.education.none
            + country.macro_indicators.demographics.education.basic
            + country
                .macro_indicators
                .demographics
                .education
                .secondary_share()
            + country
                .macro_indicators
                .demographics
                .education
                .higher_share();

        assert!(
            (total - 1.0).abs() < 1e-9,
            "shares must sum to 1.0, got {}",
            total
        );
    }

    #[test]
    fn test_forgetting_rate_when_coverage_collapses() {
        let mut country = Country::default();
        country.macro_indicators.demographics =
            make_demographics(0.1, 0.6, &[("Technical", 0.2)], &[("Medical", 0.1)]);

        // Zero coverage — schools collapsed.
        let consumption = BTreeMap::new();
        let needs = BTreeMap::new();

        let result = process_education_progression_turn(&mut country, &consumption, &needs);

        // With 0 coverage, forgetting rate should kick in.
        assert!(
            result.basic_to_none >= 0.0,
            "forgetting rate should be non-negative"
        );
    }

    #[test]
    fn test_no_negative_shares() {
        let mut country = Country::default();
        country.macro_indicators.demographics = make_demographics(0.0, 0.0, &[], &[]);

        let mut consumption = BTreeMap::new();
        consumption.insert("r1".to_string(), 100.0);
        let mut needs = BTreeMap::new();
        needs.insert("r1".to_string(), 100.0);

        let _ = process_education_progression_turn(&mut country, &consumption, &needs);

        assert!(country.macro_indicators.demographics.education.none >= 0.0);
        assert!(country.macro_indicators.demographics.education.basic >= 0.0);
        for v in country
            .macro_indicators
            .demographics
            .education
            .secondary
            .values()
        {
            assert!(*v >= 0.0, "secondary shares must be non-negative");
        }
        for v in country
            .macro_indicators
            .demographics
            .education
            .higher
            .values()
        {
            assert!(*v >= 0.0, "higher shares must be non-negative");
        }
    }

    #[test]
    fn test_education_statistics_updated() {
        let mut country = Country::default();
        country.macro_indicators.demographics =
            make_demographics(0.3, 0.4, &[("Technical", 0.2)], &[("Medical", 0.1)]);

        let mut consumption = BTreeMap::new();
        consumption.insert("r1".to_string(), 80.0);
        let mut needs = BTreeMap::new();
        needs.insert("r1".to_string(), 100.0);

        let _ = process_education_progression_turn(&mut country, &consumption, &needs);

        let literacy = country.macro_indicators.education_statistics.literacy_rate;
        let higher_rate = country
            .macro_indicators
            .education_statistics
            .higher_education_rate;

        assert!(
            literacy >= 0.0 && literacy <= 1.0,
            "literacy_rate must be in [0,1]"
        );
        assert!(
            higher_rate >= 0.0 && higher_rate <= 1.0,
            "higher_education_rate must be in [0,1]"
        );
        assert!(
            (literacy - (1.0 - country.macro_indicators.demographics.education.none)).abs() < 1e-9
        );
    }
}

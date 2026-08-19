//! Justice system coverage and consequences (Phase 14).
//!
//! This module implements the dynamic crime demand calculation and
//! justice/security coverage ratio mechanic. Crime demand scales with
//! demographic poverty, unemployment, social unrest, and health status —
//! NOT flat population. Insufficient coverage freezes company cash and
//! reduces worker efficiency.

use crate::entities::{Building, Company};
use crate::politics::ideology::Ideology;
use crate::politics::laws::{CourtWaitTime, JusticeLaw, PardonAuthority};
use crate::politics::system::JusticeSystemState;
use crate::registries::enums::Commodity;
use crate::society::geography::{ClassDemographics, HealthStatus};
use crate::state::Country;
use std::collections::BTreeMap;

/// Result of processing one justice turn.
#[derive(Debug, Clone, Default)]
pub struct JusticeTurnResult {
    /// Total justice capacity produced this turn.
    pub justice_capacity: f64,
    /// Total security capacity produced this turn.
    pub security_capacity: f64,
    /// Dynamic justice demand this turn.
    pub justice_demand: f64,
    /// Dynamic security demand this turn.
    pub security_demand: f64,
    /// Justice coverage ratio (0.0–1.0+).
    pub justice_coverage: f64,
    /// Security coverage ratio (0.0–1.0+).
    pub security_coverage: f64,
    /// Total cash frozen this turn across all companies.
    pub total_frozen: f64,
    /// Number of companies with frozen cash.
    pub companies_frozen: usize,
    /// Phase 14.5: Total fines collected this turn.
    pub fines_collected: f64,
}

/// Result of collecting fines for one turn (Phase 14.5).
#[derive(Debug, Clone, Default)]
pub struct FineCollectionResult {
    /// Total fines actually collected (strictly clamped to available cash).
    pub total_collected: f64,
    /// Number of companies fined.
    pub companies_fined: usize,
    /// Number of citizens fined.
    pub citizens_fined: i64,
    /// Theoretical fine amount (before clamping).
    pub theoretical_fines: f64,
    /// Uncollectible amount (theoretical - collected).
    pub uncollectible: f64,
}

/// Levies fines on companies and citizens based on justice/security coverage gaps.
///
/// Fine structure scales ideologically:
/// - Pro-business: flat range (10,000–50,000), capped at 5% of available cash.
/// - Pro-worker: percentage of available cash (2–5%), no cap.
/// - Ambiguous: hybrid max(flat 10,000, 3% of available cash).
///
/// **STRICT double-entry:** `actual_fine = min(fine_amount, available_cash)`.
/// Treasury receives exactly what was debited — no fiat printed.
///
/// # Arguments
/// * `country` - Mutable country (for Treasury and ideology lookup)
/// * `companies` - Mutable companies (to debit fines)
/// * `justice_coverage` - Justice coverage ratio (0.0–1.0)
/// * `security_coverage` - Security coverage ratio (0.0–1.0)
///
/// # Returns
/// `FineCollectionResult` with collection statistics.
pub fn levy_fines(
    country: &mut Country,
    companies: &mut [Company],
    justice_coverage: f64,
    security_coverage: f64,
) -> FineCollectionResult {
    let mut result = FineCollectionResult::default();

    // Determine ruling party ideology
    let ideology = country
        .politics
        .active_parties
        .get(&country.politics.ruling_party)
        .and_then(|p| Ideology::from_name(&p.ideology));

    // === COMPANY FINES ===
    // Trigger when justice coverage < 0.8
    if justice_coverage < 0.8 && !companies.is_empty() {
        let coverage_gap = 1.0 - justice_coverage;
        let num_to_fine = ((companies.len() as f64 * coverage_gap * 0.3) as usize).max(1);
        let num_to_fine = num_to_fine.min(companies.len());

        // Fine every Nth company to avoid fining the same ones each turn
        let step = (companies.len() / num_to_fine).max(1);
        let mut fined = 0_usize;

        for i in (0..companies.len()).step_by(step) {
            if fined >= num_to_fine {
                break;
            }

            let fine_amount = match ideology {
                Some(ideo) if ideo.is_pro_business() => {
                    // Flat range 10,000–50,000, capped at 5% of available cash
                    let base = 10_000.0 + (i as f64 % 40_000.0);
                    let capped = base.min(companies[i].available_cash * 0.05);
                    capped
                }
                Some(ideo) if ideo.is_pro_worker() => {
                    // Percentage of available cash: 2–5%
                    let pct = 0.02 + ((i as f64 % 3.0) / 100.0);
                    companies[i].available_cash * pct
                }
                _ => {
                    // Ambiguous: max(flat 10,000, 3% of available cash)
                    let pct_based = companies[i].available_cash * 0.03;
                    10_000.0_f64.max(pct_based)
                }
            };

            // STRICT double-entry: actual_fine = min(fine_amount, available_cash)
            let actual_fine = fine_amount.min(companies[i].available_cash);

            if actual_fine > 0.01 {
                companies[i].available_cash -= actual_fine;
                country.budget.liquid_reserves += actual_fine;
                result.total_collected += actual_fine;
                result.theoretical_fines += fine_amount;
                fined += 1;
            } else {
                result.theoretical_fines += fine_amount;
            }
        }
        result.companies_fined = fined;
    }

    // === CITIZEN FINES ===
    // Trigger when security coverage < 0.8
    if security_coverage < 0.8 {
        let coverage_gap = 1.0 - security_coverage;
        let fine_per_capita = 50.0 * coverage_gap; // up to 40 fiat per wealthy citizen

        for region in &mut country.regions {
            for class in region.class_demographics.rural_classes.values_mut() {
                if class.savings_per_capita > 200.0 {
                    let fined_count = (class.population as f64 * coverage_gap * 0.1) as i64;
                    if fined_count > 0 {
                        let theoretical = fine_per_capita * fined_count as f64;
                        // STRICT: clamp to available savings
                        let actual = theoretical.min(class.savings);
                        if actual > 0.01 {
                            class.savings -= actual;
                            if class.population > 0 {
                                class.savings_per_capita = class.savings / class.population as f64;
                            }
                            country.budget.liquid_reserves += actual;
                            result.total_collected += actual;
                            result.theoretical_fines += theoretical;
                            result.citizens_fined += fined_count;
                        } else {
                            result.theoretical_fines += theoretical;
                        }
                    }
                }
            }
            for class in region.class_demographics.urban_classes.values_mut() {
                if class.savings_per_capita > 200.0 {
                    let fined_count = (class.population as f64 * coverage_gap * 0.1) as i64;
                    if fined_count > 0 {
                        let theoretical = fine_per_capita * fined_count as f64;
                        let actual = theoretical.min(class.savings);
                        if actual > 0.01 {
                            class.savings -= actual;
                            if class.population > 0 {
                                class.savings_per_capita = class.savings / class.population as f64;
                            }
                            country.budget.liquid_reserves += actual;
                            result.total_collected += actual;
                            result.theoretical_fines += theoretical;
                            result.citizens_fined += fined_count;
                        } else {
                            result.theoretical_fines += theoretical;
                        }
                    }
                }
            }
        }
    }

    result.uncollectible = result.theoretical_fines - result.total_collected;
    result
}

/// Calculates per-class crime demand based on socio-economic factors.
///
/// # Arguments
/// * `class` - Demographic class data
/// * `class_pop` - Population of this class
/// * `unemployment_rate` - National unemployment rate (0–100)
/// * `social_unrest` - National social unrest level (0–100)
///
/// # Returns
/// Per-class crime demand in arbitrary units.
///
/// # Rules
/// * Base demand is 0.5 per capita.
/// * Poverty multiplier: lower savings_per_capita → higher crime.
/// * Unemployment multiplier: 1.0 + (rate/100 * 1.5).
/// * Unrest multiplier: 1.0 + (unrest/100 * 2.0).
/// * Subsistence rate adds desperation factor.
/// * Poor health increases crime propensity.
pub fn calculate_class_crime_demand(
    class: &ClassDemographics,
    class_pop: i64,
    unemployment_rate: f64,
    social_unrest: f64,
) -> f64 {
    let base: f64 = 0.5;

    let poverty_factor = if class.savings_per_capita < 10.0 {
        2.5
    } else if class.savings_per_capita < 50.0 {
        1.8
    } else if class.savings_per_capita < 200.0 {
        1.0
    } else {
        0.4
    };

    let unemployment_factor = 1.0 + (unemployment_rate / 100.0 * 1.5);
    let unrest_factor = 1.0 + (social_unrest / 100.0 * 2.0);
    let subsistence_factor = 1.0 + class.subsistence_rate;

    let health_factor = match class.health_status {
        HealthStatus::Critical => 1.8,
        HealthStatus::Poor => 1.3,
        HealthStatus::Fair => 1.0,
        HealthStatus::Good => 0.7,
        HealthStatus::Excellent => 0.5,
    };

    let per_capita_demand = base
        * poverty_factor
        * unemployment_factor
        * unrest_factor
        * subsistence_factor
        * health_factor;

    per_capita_demand * class_pop as f64
}

/// Calculates national justice and security demand from all regional demographics.
///
/// # Arguments
/// * `country` - Country with regions and macro indicators
/// * `company_count` - Number of active companies
///
/// # Returns
/// Tuple of (justice_demand, security_demand).
///
/// # Rules
/// * Iterates all rural and urban class demographics across all regions.
/// * Urban classes generate 2x justice and 3x security demand per capita vs rural.
/// * Each company adds 2.0 justice demand and 1.0 security demand.
pub fn calculate_national_demand(country: &Country, company_count: usize) -> (f64, f64) {
    let unemployment = country.macro_indicators.labor_market.unemployment_rate;
    let unrest = country.macro_indicators.social_unrest;

    let mut justice_demand = 0.0_f64;
    let mut security_demand = 0.0_f64;

    for region in &country.regions {
        for (_, class) in &region.class_demographics.rural_classes {
            let pop = class.population.max(1);
            let demand = calculate_class_crime_demand(class, pop, unemployment, unrest);
            justice_demand += demand;
            security_demand += demand * 1.5;
        }
        for (_, class) in &region.class_demographics.urban_classes {
            let pop = class.population.max(1);
            let demand = calculate_class_crime_demand(class, pop, unemployment, unrest);
            justice_demand += demand * 2.0;
            security_demand += demand * 3.0;
        }
    }

    justice_demand += company_count as f64 * 2.0;
    security_demand += company_count as f64 * 1.0;

    (justice_demand, security_demand)
}

/// Returns the court wait time freeze multiplier.
fn court_wait_multiplier(wait: CourtWaitTime) -> f64 {
    match wait {
        CourtWaitTime::Expedited => 0.5,
        CourtWaitTime::Normal => 1.0,
        CourtWaitTime::Backlogged => 1.5,
        CourtWaitTime::Paralyzed => 2.5,
    }
}

/// Processes the justice turn: sums capacity, calculates dynamic demand,
/// applies frozen cash and efficiency penalties.
///
/// # Arguments
/// * `country` - Mutable country state (for Treasury and justice_state)
/// * `buildings` - All buildings (to sum capacity from inventories)
/// * `companies` - All companies (to freeze cash)
/// * `building_inventories` - Building inventory map
///
/// # Returns
/// `JusticeTurnResult` with coverage statistics.
///
/// # Rules
/// * Sums JusticeCapacity and SecurityCapacity from building inventories.
/// * Calculates dynamic demand using `calculate_national_demand`.
/// * Coverage ratio = min(1.0, capacity / demand).
/// * Frozen cash = (1 - coverage) * 0.15 * company.available_cash * court_wait_multiplier.
/// * Double-entry: Debit company.available_cash, Credit justice_state.frozen_company_cash.
/// * Corruption index increases OPEX multiplier for all companies.
/// * Pardon authority reduces frozen cash by 5% per turn (President/HeadOfState).
pub fn process_justice_turn(
    country: &mut Country,
    buildings: &[Building],
    companies: &mut [Company],
    building_inventories: &BTreeMap<String, BTreeMap<Commodity, f64>>,
) -> JusticeTurnResult {
    // 1. Sum JusticeCapacity and SecurityCapacity from building inventories
    let mut justice_capacity = 0.0_f64;
    let mut security_capacity = 0.0_f64;

    for building in buildings {
        if let Some(inv) = building_inventories.get(&building.id) {
            if let Some(&jc) = inv.get(&Commodity::JusticeCapacity) {
                justice_capacity += jc;
            }
            if let Some(&sc) = inv.get(&Commodity::SecurityCapacity) {
                security_capacity += sc;
            }
        }
    }

    // 2. Calculate dynamic demand
    let (justice_demand, security_demand) =
        calculate_national_demand(country, companies.len());

    // 3. Coverage ratios
    let justice_coverage = if justice_demand > 0.0 {
        (justice_capacity / justice_demand).min(1.0)
    } else {
        1.0
    };
    let security_coverage = if security_demand > 0.0 {
        (security_capacity / security_demand).min(1.0)
    } else {
        1.0
    };

    // 4. Get justice law modifiers
    let (court_mult, corruption_index, pardon_authority) =
        if let Some(ref jl) = country.politics.justice_law {
            (
                court_wait_multiplier(jl.court_wait_time_target),
                jl.corruption_index,
                jl.pardon_authority,
            )
        } else {
            (1.0, 0.0, PardonAuthority::None)
        };

    // 5. Ensure justice_state exists
    if country.politics.justice_state.is_none() {
        country.politics.justice_state = Some(JusticeSystemState::default());
    }
    let justice_state = country.politics.justice_state.as_mut().unwrap();

    // 6. Apply pardon: reduce frozen cash by 5% for President/HeadOfState
    let pardon_reduction = match pardon_authority {
        PardonAuthority::President | PardonAuthority::HeadOfState => 0.05,
        _ => 0.0,
    };
    if pardon_reduction > 0.0 {
        let frozen = &mut justice_state.frozen_company_cash;
        let keys: Vec<String> = frozen.keys().cloned().collect();
        for key in keys {
            if let Some(amount) = frozen.get_mut(&key) {
                let reduction = *amount * pardon_reduction;
                *amount -= reduction;
                // Return unfrozen cash to company
                if let Some(company) = companies.iter_mut().find(|c| c.id == key) {
                    company.available_cash += reduction;
                }
                // Remove zero entries
                if *amount < 0.01 {
                    frozen.remove(&key);
                }
            }
        }
    }

    // 7. Apply frozen cash to companies based on coverage gap
    let freeze_base_ratio = (1.0 - justice_coverage) * 0.15;
    let freeze_ratio = freeze_base_ratio * court_mult;
    let mut total_frozen = 0.0_f64;
    let mut companies_frozen = 0_usize;

    if freeze_ratio > 0.0 {
        for company in companies.iter_mut() {
            let freeze_amount = company.available_cash * freeze_ratio;
            if freeze_amount > 0.01 {
                company.available_cash -= freeze_amount;
                *justice_state
                    .frozen_company_cash
                    .entry(company.id.clone())
                    .or_insert(0.0) += freeze_amount;
                total_frozen += freeze_amount;
                companies_frozen += 1;
            }
        }
    }

    // 8. Apply corruption OPEX multiplier
    let corruption_mult = 1.0 + corruption_index * 0.10;
    if corruption_mult > 1.0 {
        for company in companies.iter_mut() {
            // Increase debit_cash as OPEX overhead proxy
            let overhead = company.available_cash * (corruption_mult - 1.0) * 0.01;
            company.available_cash -= overhead;
            country.budget.liquid_reserves += overhead;
        }
    }

    // 9. Update justice state
    justice_state.total_justice_capacity = justice_capacity;
    justice_state.total_security_capacity = security_capacity;
    justice_state.justice_demand = justice_demand;
    justice_state.security_demand = security_demand;
    justice_state.justice_coverage = justice_coverage;
    justice_state.security_coverage = security_coverage;

    // 10. Phase 14.5: Levy fines (ideological scaling, strict double-entry)
    let fine_result = levy_fines(country, companies, justice_coverage, security_coverage);
    if let Some(js) = country.politics.justice_state.as_mut() {
        js.fines_collected = fine_result.total_collected;
    }

    // 11. Phase 14.5: Process intelligence capacity from Siedziba Służb buildings
    let intel_capacity: f64 = buildings
        .iter()
        .filter(|b| b.name == "Siedziba Służb")
        .map(|b| b.last_production.get(&Commodity::IntelligenceCapacity).copied().unwrap_or(0.0))
        .sum();

    if intel_capacity > 0.0 {
        // Calculate surveillance coverage based on capacity vs radical population
        let total_radicals: f64 = country
            .regions
            .iter()
            .flat_map(|r| {
                r.class_demographics.rural_classes.values()
                    .chain(r.class_demographics.urban_classes.values())
            })
            .map(|c| c.population as f64 * c.political_sentiment.radicals)
            .sum::<f64>()
            .max(1.0);

        let surveillance_coverage = (intel_capacity / total_radicals).min(1.0);

        // Update intelligence state
        if country.politics.intelligence_state.is_none() {
            country.politics.intelligence_state = Some(crate::politics::system::IntelligenceState::default());
        }
        if let Some(intel) = country.politics.intelligence_state.as_mut() {
            intel.total_capacity = intel_capacity;
            intel.surveillance_coverage = surveillance_coverage;
        }

        // Chilling effect: reduce social unrest passively
        country.macro_indicators.social_unrest = (country.macro_indicators.social_unrest - surveillance_coverage * 2.0).max(0.0);
    }

    JusticeTurnResult {
        justice_capacity,
        security_capacity,
        justice_demand,
        security_demand,
        justice_coverage,
        security_coverage,
        total_frozen,
        companies_frozen,
        fines_collected: fine_result.total_collected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::ClassDemographics;

    #[test]
    fn test_poverty_factor_destitute() {
        let class = ClassDemographics {
            population: 1000,
            savings_per_capita: 5.0,
            ..Default::default()
        };
        let demand = calculate_class_crime_demand(&class, 1000, 0.0, 0.0);
        // base 0.5 * poverty 2.5 * unemp 1.0 * unrest 1.0 * subsistence 1.0 * health 0.5 (default Excellent)
        // = 0.5 * 2.5 * 1.0 * 1.0 * 1.0 * 0.5 = 0.625
        // per_capita * 1000 = 625
        assert!((demand - 625.0).abs() < 0.1, "expected 625, got {demand}");
    }

    #[test]
    fn test_unemployment_multiplier() {
        let class = ClassDemographics {
            population: 1000,
            savings_per_capita: 100.0, // moderate poverty factor = 1.0
            ..Default::default()
        };
        let demand_low = calculate_class_crime_demand(&class, 1000, 0.0, 0.0);
        let demand_high = calculate_class_crime_demand(&class, 1000, 50.0, 0.0);
        // With 50% unemployment: factor = 1 + 0.5*1.5 = 1.75
        assert!(
            demand_high > demand_low * 1.5,
            "high unemployment should significantly increase demand"
        );
    }

    #[test]
    fn test_court_wait_multiplier() {
        assert_eq!(court_wait_multiplier(CourtWaitTime::Expedited), 0.5);
        assert_eq!(court_wait_multiplier(CourtWaitTime::Normal), 1.0);
        assert_eq!(court_wait_multiplier(CourtWaitTime::Backlogged), 1.5);
        assert_eq!(court_wait_multiplier(CourtWaitTime::Paralyzed), 2.5);
    }
}

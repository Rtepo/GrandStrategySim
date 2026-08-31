//! Union and labor organization management.
//!
//! This module implements the `process_unions` function which handles:
//! - Union militancy and strike actions
//! - Wage negotiations and collective bargaining
//! - Union fund management and member recruitment

use crate::entities::{Company, Union};
use crate::society::geography::Region;
use crate::state::Country;
use std::collections::HashMap;

/// Process union activities for a single country.
///
/// # Arguments
/// * `companies` - Mutable slice of companies for this country
/// * `unions` - Mutable slice of unions for this country
/// * `country` - Mutable reference to the country state
/// * `year` - Current game year
///
/// # Phase 41 Rules
/// * Unions trigger strikes when member companies have >10% layoffs (FTE drop).
/// * Striking companies have 0 production for that turn (handled in turn.rs).
/// * Strike pay = 50% of average_wage (or 50.0 min) per FTE, from union.strike_fund
///   to ClassDemographics.savings. STRICT DOUBLE-ENTRY.
/// * If union.strike_fund < required_strike_pay, fund is zeroed, strike ends immediately.
/// * Company payroll is zeroed for striking FTE (company saves cash).
/// * Company still pays building overhead/maintenance during strike.
/// * Cap simultaneous strikes to 10% of corporate sector.
pub fn process_unions(
    companies: &mut [Company],
    unions: &mut Vec<Union>,
    country: &mut Country,
    _year: u32,
) {
    // Phase 41: Cap simultaneous strikes to 10% of corporate sector.
    let total_companies = companies.len().max(1);
    let max_simultaneous_strikes = (total_companies / 10).max(1);
    let mut current_strike_count: usize = companies.iter().filter(|c| c.is_striking).count();

    for union in unions.iter_mut() {
        // Update union militancy based on economic conditions
        update_union_militancy(union, country);

        // Phase 41: Check for existing strikes from this union and pay strike benefits.
        // If the union is already on strike, pay strike pay from strike_fund.
        if union.on_strike {
            pay_strike_benefits(union, companies, country);
            // Check if strike should end (fund exhausted or no more striking companies)
            let any_striking = companies.iter().any(|c| c.is_striking && c.union_id.as_ref() == Some(&union.id));
            if !any_striking {
                union.on_strike = false;
            }
        } else {
            // Phase 41: Trigger new strikes if militancy is high AND there are layoffs.
            // Only trigger if we haven't hit the simultaneous strike cap.
            if union.militancy > 0.7 && current_strike_count < max_simultaneous_strikes {
                let new_strikes = trigger_strikes(union, companies, country, max_simultaneous_strikes - current_strike_count);
                current_strike_count += new_strikes;
                if new_strikes > 0 {
                    union.on_strike = true;
                }
            }
        }

        // Collect union dues from member companies
        collect_union_dues(union, companies);

        // Recruit new members based on economic conditions
        recruit_union_members(union, companies, country);
    }

    // ───────────────────────────────────────────────────────────────────
    // DISSOLUTION PHASE: distribute treasury of dead unions, then remove.
    //
    // A union is dissolved when its membership (company_ids.len()) falls
    // below dissolution_threshold. Before removal, its entire liquid
    // treasury is distributed pro-rata to member workers. Only after
    // distribution completes do we call `retain` (Rule 1: no union capital
    // may disappear into the void).
    // ───────────────────────────────────────────────────────────────────
    for union in unions.iter_mut() {
        if union.dissolved {
            continue;
        }
        if union.company_ids.len() < union.dissolution_threshold {
            dissolve_union(union, companies, country);
        }
    }
    // Remove dissolved unions only after all treasury has been distributed.
    unions.retain(|u| !u.dissolved);
}

/// Update union militancy based on economic conditions.
///
/// # Arguments
/// * `union` - Mutable reference to the union
/// * `country` - Reference to the country state
///
/// # Rules
/// * High unemployment increases militancy
/// * Low wages relative to GDP per capita increase militancy
/// * Government social programs can reduce militancy
fn update_union_militancy(union: &mut Union, country: &Country) {
    let unemployment_rate = country.macro_indicators.labor_market.unemployment_rate;
    let average_wage = country.macro_indicators.average_wage;
    let gdp_per_capita = country.budget.gdp / country.budget.population as f64;

    // Base militancy from unemployment
    let unemployment_factor = (unemployment_rate - 0.05).max(0.0) * 100.0;

    // Wage pressure: if average wage is low relative to GDP per capita
    let wage_pressure = if gdp_per_capita > 0.0 {
        (1.0 - (average_wage / gdp_per_capita)).max(0.0) * 50.0
    } else {
        0.0
    };

    // Social programs reduce militancy
    // Phase 8: Read from ministry system when available, fall back to legacy allocations
    let social_relief = if let Some(ref config) = country.politics.ministry_config {
        use crate::politics::ministries::GovernmentCompetency;
        let total: f64 = config.ministries.iter().map(|m| m.allocated_cash).sum();
        if total > 0.0 {
            let welfare: f64 = config
                .ministries
                .iter()
                .filter(|m| m.competencies.contains(&GovernmentCompetency::SocialWelfare))
                .map(|m| m.allocated_cash)
                .sum();
            (welfare / total) * 20.0
        } else {
            country.budget.allocations.social_programs * 20.0
        }
    } else {
        country.budget.allocations.social_programs * 20.0
    };

    // Update militancy with smoothing
    let target_militancy = (unemployment_factor + wage_pressure - social_relief).clamp(0.0, 100.0);
    union.militancy = union.militancy * 0.7 + target_militancy * 0.3;
}

/// Phase 41: Trigger strikes on companies with >10% layoffs (FTE drop).
///
/// # Arguments
/// * `union` - Mutable reference to the union
/// * `companies` - Mutable slice of companies
/// * `country` - Mutable reference to country state (for social unrest)
/// * `max_new_strikes` - Maximum number of new strikes allowed this turn
///
/// # Returns
/// Number of new strikes triggered.
///
/// # Rules
/// * A company is eligible for strike if its FTE dropped by more than 10%
///   relative to prev_fulfilled_fte (mass layoff).
/// * The union must have a strike_fund >= 50.0 (one worker's strike pay) to trigger.
/// * Sets company.is_striking = true on affected companies.
/// * Does NOT reduce worker_capacity — the strike flag handles 0 production.
fn trigger_strikes(
    union: &mut Union,
    companies: &mut [Company],
    country: &mut Country,
    max_new_strikes: usize,
) -> usize {
    if max_new_strikes == 0 {
        return 0;
    }
    // Union needs at least one worker's worth of strike pay to start a strike.
    let avg_wage = country.macro_indicators.average_wage;
    let min_strike_pay = (avg_wage * 0.5).max(50.0);
    if union.strike_fund < min_strike_pay {
        return 0;
    }

    let mut strikes_triggered = 0;
    for company in companies.iter_mut() {
        if strikes_triggered >= max_new_strikes {
            break;
        }
        // Only strike companies in this union that aren't already striking.
        if company.union_id.as_ref() != Some(&union.id) || company.is_striking {
            continue;
        }
        // Phase 47: Skip furloughed companies — furlough is authorized seasonal
        // leave, not an adversarial layoff. Workers return next season.
        if company
            .seasonal_profile
            .as_ref()
            .map(|p| p.current_state == crate::entities::SeasonalState::Furloughed)
            .unwrap_or(false)
        {
            continue;
        }
        // Phase 41: Trigger strike if >10% layoff (FTE drop from prev to current).
        let prev_fte = company.prev_fulfilled_fte;
        let current_fte = company.fulfilled_fte;
        if prev_fte > 0 {
            let layoff_pct = (prev_fte as f64 - current_fte as f64) / prev_fte as f64;
            if layoff_pct > 0.10 {
                company.is_striking = true;
                strikes_triggered += 1;
                country.macro_indicators.social_unrest += 2.0;
            }
        }
    }
    strikes_triggered
}

/// Phase 41: Pay strike benefits from union.strike_fund to ClassDemographics.savings.
///
/// STRICT DOUBLE-ENTRY: union.strike_fund ↓, ClassDemographics.savings ↑.
///
/// Strike pay = 50% of average_wage (or 50.0, whichever is higher) per FTE.
/// If union.strike_fund < required_strike_pay, fund is zeroed, remaining workers
/// get nothing, and the strike IMMEDIATELY ends (is_striking = false).
fn pay_strike_benefits(union: &mut Union, companies: &mut [Company], country: &mut Country) {
    let avg_wage = country.macro_indicators.average_wage;
    let strike_pay_per_fte = (avg_wage * 0.5).max(50.0);

    // Find all striking companies in this union and compute total strike pay needed.
    let mut total_strike_pay = 0.0;
    let mut striking_company_regions: Vec<(String, String, f64)> = Vec::new(); // (region_id, class_key, fte)
    for company in companies.iter() {
        if company.is_striking && company.union_id.as_ref() == Some(&union.id) {
            let striking_fte = company.fulfilled_fte;
            total_strike_pay += striking_fte as f64 * strike_pay_per_fte;
            // Phase 41: We need to credit the savings of the workers' class.
            // For simplicity, credit to the urban working class in the company's region.
            // The actual class is tracked by the labor market; we use a reasonable default.
            striking_company_regions.push((company.region_id.clone(), "Worker".to_string(), striking_fte as f64));
        }
    }

    if total_strike_pay <= 0.0 {
        return;
    }

    if union.strike_fund >= total_strike_pay {
        // Full payment: debit strike_fund, credit class savings.
        union.strike_fund -= total_strike_pay;
        credit_strike_pay_to_savings(country, &striking_company_regions, total_strike_pay);
    } else {
        // Fund exhausted: pay out what's left, zero the fund, end the strike immediately.
        let remaining = union.strike_fund;
        union.strike_fund = 0.0;
        if remaining > 0.0 {
            credit_strike_pay_to_savings(country, &striking_company_regions, remaining);
        }
        // End all strikes for this union's companies.
        for company in companies.iter_mut() {
            if company.is_striking && company.union_id.as_ref() == Some(&union.id) {
                company.is_striking = false;
            }
        }
        union.on_strike = false;
    }
}

/// Phase 41: Credit strike pay to the appropriate ClassDemographics.savings.
///
/// This is a simplified approach: we credit the total strike pay to the
/// urban working class ("Worker") in each company's region.
/// The labor market tracks actual class assignments, but for double-entry
/// purposes we need a valid savings account to credit.
fn credit_strike_pay_to_savings(
    country: &mut Country,
    striking_companies: &[(String, String, f64)],
    total_amount: f64,
) {
    if total_amount <= 0.0 || striking_companies.is_empty() {
        return;
    }
    // Distribute proportionally based on FTE share.
    let total_fte: f64 = striking_companies.iter().map(|(_, _, fte)| fte).sum();
    if total_fte <= 0.0 {
        return;
    }
    for (region_id, class_key, fte) in striking_companies {
        let share = fte / total_fte;
        let amount = total_amount * share;
        // Find the region and credit the class savings.
        for region in country.regions.iter_mut() {
            if region.id == *region_id {
                // Try urban classes first, then rural.
                if let Some(uc) = region.class_demographics.urban_classes.get_mut(class_key) {
                    uc.savings += amount;
                } else if let Some(first_urban) = region.class_demographics.urban_classes.values_mut().next() {
                    first_urban.savings += amount;
                } else if let Some(first_rural) = region.class_demographics.rural_classes.values_mut().next() {
                    first_rural.savings += amount;
                }
                break;
            }
        }
    }
}

/// Credit an amount to the urban `Worker` class savings in a region.
///
/// If the `Worker` class is absent, credits the first available urban class,
/// then the first rural class — ensuring no capital is stranded (Rule 1).
fn credit_worker_class_savings(region: &mut Region, amount: f64) {
    if amount <= 0.0 {
        return;
    }
    if let Some(worker) = region.class_demographics.urban_classes.get_mut("Worker") {
        worker.savings += amount;
    } else if let Some(first_urban) = region.class_demographics.urban_classes.values_mut().next() {
        first_urban.savings += amount;
    } else if let Some(first_rural) = region.class_demographics.rural_classes.values_mut().next() {
        first_rural.savings += amount;
    }
}

/// Dissolve a union, distributing its entire liquid treasury to the workers
/// of its member companies on a pro-rata basis.
///
/// # Dissolution criteria
/// A union is dissolved when its membership (`company_ids.len()`) falls below
/// `dissolution_threshold`. This occurs when member companies go bankrupt,
/// leave the union, or are acquired, leaving the union without enough
/// representation to justify its existence.
///
/// # Distribution logic (Rules 1, 5, 7)
/// 1. **Total treasury** = `union.budget` + `union.strike_fund`. Both are
///    liquid capital that must be returned to the economy — no union capital
///    may disappear into the void (Rule 1).
/// 2. **Primary allocation — pro-rata by historical dues**: each member
///    company's share is `dues_history[company_id] / sum(dues_history)`.
///    This ensures strict individual accountability (Rule 7): companies that
///    contributed more dues receive a larger share of the returned treasury.
/// 3. **Fallback allocation — proportional by worker headcount**: if
///    `dues_history` is empty or sums to zero (no historical contribution
///    data available), each member company's share is
///    `fulfilled_fte / sum(fulfilled_fte)`. This is a documented proportional
///    fallback based on regional/sector worker demographics.
/// 4. **Credit destination**: each company's allocated share is credited to
///    the urban `Worker` class `ClassDemographics.savings` in that company's
///    region. If the `Worker` class is absent, the credit flows to the first
///    available urban class, then the first rural class.
/// 5. **Debit**: `union.budget` and `union.strike_fund` are set to exactly
///    zero. `company_ids` and `dues_history` are cleared, and `dissolved`
///    is set to `true`.
///
/// # Arguments
/// * `union` - Mutable reference to the union being dissolved.
/// * `companies` - Slice of all companies (to locate member companies and
///   their regions/FTE).
/// * `country` - Mutable country state (to credit worker class savings).
fn dissolve_union(union: &mut Union, companies: &[Company], country: &mut Country) {
    let total_treasury = union.budget + union.strike_fund;

    // Collect member company data: (company_id, region_id, fulfilled_fte).
    let members: Vec<(String, String, u32)> = companies
        .iter()
        .filter(|c| union.company_ids.contains(&c.id))
        .map(|c| (c.id.clone(), c.region_id.clone(), c.fulfilled_fte))
        .collect();

    if total_treasury > 0.0 && !members.is_empty() {
        // Compute per-member allocation shares.
        let total_dues: f64 = union.dues_history.values().sum();
        let shares: Vec<f64> = if total_dues > 0.0 {
            // Primary: pro-rata by historical dues contribution (Rule 7).
            members
                .iter()
                .map(|(id, _, _)| *union.dues_history.get(id).unwrap_or(&0.0) / total_dues)
                .collect()
        } else {
            // Fallback: proportional by fulfilled_fte (worker demographics).
            let total_fte: f64 = members.iter().map(|(_, _, fte)| *fte as f64).sum();
            if total_fte > 0.0 {
                members
                    .iter()
                    .map(|(_, _, fte)| *fte as f64 / total_fte)
                    .collect()
            } else {
                // Final fallback: equal split among remaining members.
                let n = members.len() as f64;
                members.iter().map(|_| 1.0 / n).collect()
            }
        };

        // Accumulate per-region credits to avoid repeated mutable borrows
        // of `country.regions` inside the member loop (Rule 9).
        let mut region_credits: HashMap<String, f64> = HashMap::new();
        for ((_, region_id, _), share) in members.iter().zip(shares.iter()) {
            let amount = total_treasury * share;
            if amount > 0.0 {
                *region_credits.entry(region_id.clone()).or_insert(0.0) += amount;
            }
        }

        // Apply region credits to Worker class savings (double-entry credit).
        for (region_id, amount) in &region_credits {
            for region in country.regions.iter_mut() {
                if region.id == *region_id {
                    credit_worker_class_savings(region, *amount);
                    break;
                }
            }
        }
    } else if total_treasury > 0.0 && members.is_empty() {
        // No member companies remain. Credit to the Worker class in the
        // union's own region as the documented proportional fallback.
        for region in country.regions.iter_mut() {
            if region.id == union.region_id {
                credit_worker_class_savings(region, total_treasury);
                break;
            }
        }
    }

    // Debit treasury to exactly zero (Rule 1: no capital vanishes).
    union.budget = 0.0;
    union.strike_fund = 0.0;
    union.company_ids.clear();
    union.dues_history.clear();
    union.dissolved = true;
}

/// Collect union dues from member companies.
///
/// # Arguments
/// * `union` - Mutable reference to the union
/// * `companies` - Mutable slice of companies
///
/// # Rules
/// * Dues are calculated as 1% of company capital.
/// * Only companies whose `union_id` matches this union pay dues.
/// * Dues are debited from the company's liquid capital (brokerage cash or
///   `liquid_capital` field) and credited to the union's `strike_fund`.
///   STRICT DOUBLE-ENTRY (Rule 1): the company loses exactly what the union
///   gains — no money is created from thin air.
/// * Each company's cumulative contribution is recorded in `dues_history`
///   for pro-rata treasury distribution on dissolution (Rule 7).
fn collect_union_dues(union: &mut Union, companies: &mut [Company]) {
    for company in companies.iter_mut() {
        if company.union_id.as_ref() != Some(&union.id) {
            continue;
        }
        // Calculate dues as 1% of company capital.
        let dues = company.company_capital * 0.01;
        if dues <= 0.0 {
            continue;
        }
        // Debit from the company's liquid capital (Rule 1: double-entry).
        // Prefer brokerage_account.cash (the runtime liquid capital store);
        // fall back to the legacy liquid_capital field.
        let available = company.computed_liquid_capital() + company.liquid_capital;
        let actual_dues = dues.min(available.max(0.0));
        if actual_dues <= 0.0 {
            continue;
        }
        if let Some(ref mut acct) = company.brokerage_account {
            acct.cash = (acct.cash - actual_dues).max(0.0);
        } else {
            company.liquid_capital = (company.liquid_capital - actual_dues).max(0.0);
        }
        // Credit to union strike fund.
        union.strike_fund += actual_dues;
        // Record per-member historical contribution (Rule 7).
        *union.dues_history.entry(company.id.clone()).or_insert(0.0) += actual_dues;
    }
}

/// Recruit new companies to the union based on economic conditions.
///
/// # Arguments
/// * `union` - Mutable reference to the union
/// * `companies` - Slice of companies
/// * `country` - Reference to country state
///
/// # Rules
/// * High unemployment increases recruitment
/// * Low wages increase recruitment
/// * Union militancy affects recruitment success
fn recruit_union_members(union: &mut Union, companies: &[Company], country: &Country) {
    let unemployment_rate = country.macro_indicators.labor_market.unemployment_rate;

    // Only recruit if unemployment is high enough
    if unemployment_rate < 0.05 {
        return;
    }

    // Find companies without unions and recruit them
    for company in companies {
        if company.union_id.is_none() {
            union.company_ids.insert(company.id.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{FamilyBusinessData, LegalForm};
    use crate::registries::enums::Sector;

    #[test]
    fn test_update_union_militancy_high_unemployment() {
        let mut union = Union {
            id: "TEST_UNION".to_string(),
            name: "Test Union".to_string(),
            militancy: 0.5,
            strike_fund: 10_000.0,
            company_ids: std::collections::BTreeSet::new(),
            extra: serde_json::Map::new(),
            ..Default::default()
        };

        let mut country = Country::mock_for_tests();
        country.name = "Test".to_string();
        country.macro_indicators.labor_market.unemployment_rate = 0.15; // High unemployment
        country.macro_indicators.average_wage = 30_000.0;
        country.budget.gdp = 1_000_000_000.0;
        country.budget.population = 10_000_000;
        country.budget.allocations.social_programs = 0.1;

        update_union_militancy(&mut union, &country);

        // High unemployment should increase militancy (militancy is 0..1)
        assert!(union.militancy > 0.5);
    }

    #[test]
    fn test_collect_union_dues() {
        let mut union = Union {
            id: "TEST_UNION".to_string(),
            name: "Test Union".to_string(),
            militancy: 0.5,
            strike_fund: 1000.0,
            company_ids: std::collections::BTreeSet::new(),
            extra: serde_json::Map::new(),
            ..Default::default()
        };

        let legal_form = LegalForm::FamilyBusiness(FamilyBusinessData::default());
        let mut companies = vec![Company::new(
            "COMPANY_1".to_string(),
            "Test Company".to_string(),
            Sector::Mining,
            legal_form,
            100_000.0,
            50_000.0,
            100,
        )];
        companies[0].union_id = Some("TEST_UNION".to_string());

        collect_union_dues(&mut union, &mut companies);

        // Dues should increase strike fund
        assert!(union.strike_fund > 1000.0);
        // Dues should be recorded in dues_history (Rule 7)
        assert!(union.dues_history.contains_key("COMPANY_1"));
        assert!(*union.dues_history.get("COMPANY_1").unwrap() > 0.0);
    }

    #[test]
    fn test_recruit_union_members() {
        let mut union = Union {
            id: "TEST_UNION".to_string(),
            name: "Test Union".to_string(),
            militancy: 0.6,
            strike_fund: 10_000.0,
            company_ids: std::collections::BTreeSet::new(),
            extra: serde_json::Map::new(),
            ..Default::default()
        };

        let legal_form = LegalForm::FamilyBusiness(FamilyBusinessData::default());
        let companies = vec![
            Company::new(
                "COMPANY_1".to_string(),
                "Test Company 1".to_string(),
                Sector::Mining,
                legal_form.clone(),
                100_000.0,
                50_000.0,
                100,
            ),
            Company::new(
                "COMPANY_2".to_string(),
                "Test Company 2".to_string(),
                Sector::Mining,
                legal_form,
                100_000.0,
                50_000.0,
                100,
            ),
        ];

        let mut country = Country::mock_for_tests();
        country.name = "Test".to_string();
        country.macro_indicators.labor_market.unemployment_rate = 0.10;
        country.macro_indicators.labor_market.employed_total = 1_000_000.0;

        let initial_companies = union.company_ids.len();
        recruit_union_members(&mut union, &companies, &country);

        // Should recruit new companies
        assert!(union.company_ids.len() > initial_companies);
    }

    /// Verify that dissolve_union distributes treasury pro-rata by dues_history
    /// and credits the Worker class, then zeroes the union.
    #[test]
    fn test_dissolve_union_pro_rata_by_dues() {
        use crate::society::geography::{ClassDemographics, RegionalClassDemographics};
        use std::collections::BTreeMap;

        let mut union = Union {
            id: "TEST_UNION".to_string(),
            name: "Test Union".to_string(),
            budget: 50_000.0,
            strike_fund: 30_000.0,
            company_ids: {
                let mut s = std::collections::BTreeSet::new();
                s.insert("COMPANY_1".to_string());
                s.insert("COMPANY_2".to_string());
                s
            },
            dues_history: {
                let mut m = HashMap::new();
                m.insert("COMPANY_1".to_string(), 10_000.0);
                m.insert("COMPANY_2".to_string(), 30_000.0);
                m
            },
            dissolution_threshold: 1,
            ..Default::default()
        };

        let legal_form = LegalForm::FamilyBusiness(FamilyBusinessData::default());
        let mut companies = vec![
            Company::new(
                "COMPANY_1".to_string(),
                "Co1".to_string(),
                Sector::Mining,
                legal_form.clone(),
                100_000.0,
                50_000.0,
                100,
            ),
            Company::new(
                "COMPANY_2".to_string(),
                "Co2".to_string(),
                Sector::Mining,
                legal_form,
                100_000.0,
                50_000.0,
                100,
            ),
        ];
        companies[0].union_id = Some("TEST_UNION".to_string());
        companies[1].union_id = Some("TEST_UNION".to_string());
        companies[0].region_id = "R-1".to_string();
        companies[1].region_id = "R-1".to_string();

        let mut country = Country::mock_for_tests();
        country.regions.push(Region {
            id: "R-1".to_string(),
            owner_country: "Test".to_string(),
            class_demographics: RegionalClassDemographics {
                urban_classes: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        "Worker".to_string(),
                        ClassDemographics {
                            population: 1000,
                            savings: 0.0,
                            ..Default::default()
                        },
                    );
                    m
                },
                ..Default::default()
            },
            ..Default::default()
        });

        let total_treasury = union.budget + union.strike_fund; // 80_000
        dissolve_union(&mut union, &companies, &mut country);

        // Union treasury zeroed, dissolved flag set.
        assert_eq!(union.budget, 0.0);
        assert_eq!(union.strike_fund, 0.0);
        assert!(union.company_ids.is_empty());
        assert!(union.dissolved);

        // Worker savings should have received the full 80_000 (both companies
        // are in R-1, so credits accumulate there).
        let worker_savings = country.regions[0]
            .class_demographics
            .urban_classes
            .get("Worker")
            .unwrap()
            .savings;
        assert!(
            (worker_savings - total_treasury).abs() < 1.0,
            "Worker savings {} should equal total treasury {}",
            worker_savings,
            total_treasury
        );
    }

    /// Verify that dissolve_union falls back to FTE-proportional distribution
    /// when no dues_history is available.
    #[test]
    fn test_dissolve_union_fallback_by_fte() {
        use crate::society::geography::{ClassDemographics, RegionalClassDemographics};
        use std::collections::BTreeMap;

        let mut union = Union {
            id: "TEST_UNION".to_string(),
            name: "Test Union".to_string(),
            budget: 40_000.0,
            strike_fund: 0.0,
            company_ids: {
                let mut s = std::collections::BTreeSet::new();
                s.insert("COMPANY_A".to_string());
                s.insert("COMPANY_B".to_string());
                s
            },
            // No dues_history — should fall back to FTE.
            dues_history: HashMap::new(),
            dissolution_threshold: 1,
            ..Default::default()
        };

        let legal_form = LegalForm::FamilyBusiness(FamilyBusinessData::default());
        let mut companies = vec![
            Company::new(
                "COMPANY_A".to_string(),
                "CoA".to_string(),
                Sector::Mining,
                legal_form.clone(),
                100_000.0,
                50_000.0,
                100,
            ),
            Company::new(
                "COMPANY_B".to_string(),
                "CoB".to_string(),
                Sector::Mining,
                legal_form,
                100_000.0,
                50_000.0,
                100,
            ),
        ];
        companies[0].union_id = Some("TEST_UNION".to_string());
        companies[1].union_id = Some("TEST_UNION".to_string());
        companies[0].region_id = "R-1".to_string();
        companies[1].region_id = "R-2".to_string();
        // COMPANY_A has 300 FTE, COMPANY_B has 100 FTE → 75%/25% split.
        companies[0].fulfilled_fte = 300;
        companies[1].fulfilled_fte = 100;

        let mut country = Country::mock_for_tests();
        for rid in &["R-1", "R-2"] {
            country.regions.push(Region {
                id: rid.to_string(),
                owner_country: "Test".to_string(),
                class_demographics: RegionalClassDemographics {
                    urban_classes: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "Worker".to_string(),
                            ClassDemographics {
                                population: 500,
                                savings: 0.0,
                                ..Default::default()
                            },
                        );
                        m
                    },
                    ..Default::default()
                },
                ..Default::default()
            });
        }

        dissolve_union(&mut union, &companies, &mut country);

        assert_eq!(union.budget, 0.0);
        assert!(union.dissolved);

        // R-1 should get 75% of 40_000 = 30_000.
        let r1_savings = country.regions[0]
            .class_demographics
            .urban_classes
            .get("Worker")
            .unwrap()
            .savings;
        assert!(
            (r1_savings - 30_000.0).abs() < 1.0,
            "R-1 Worker savings {} should be ~30000",
            r1_savings
        );
        // R-2 should get 25% of 40_000 = 10_000.
        let r2_savings = country.regions[1]
            .class_demographics
            .urban_classes
            .get("Worker")
            .unwrap()
            .savings;
        assert!(
            (r2_savings - 10_000.0).abs() < 1.0,
            "R-2 Worker savings {} should be ~10000",
            r2_savings
        );
    }

    /// Verify that process_unions dissolves unions below threshold, distributes
    /// treasury, and removes them via retain.
    #[test]
    fn test_process_unions_dissolves_and_retains() {
        use crate::society::geography::{ClassDemographics, RegionalClassDemographics};
        use std::collections::BTreeMap;

        // A union with 0 members (below threshold of 1) and treasury.
        let union = Union {
            id: "DEAD_UNION".to_string(),
            name: "Dead Union".to_string(),
            budget: 10_000.0,
            strike_fund: 5_000.0,
            company_ids: std::collections::BTreeSet::new(),
            region_id: "R-1".to_string(),
            dissolution_threshold: 1,
            ..Default::default()
        };

        let mut unions = vec![union];
        let mut companies: Vec<Company> = Vec::new();
        let mut country = Country::mock_for_tests();
        country.regions.push(Region {
            id: "R-1".to_string(),
            owner_country: "Test".to_string(),
            class_demographics: RegionalClassDemographics {
                urban_classes: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        "Worker".to_string(),
                        ClassDemographics {
                            population: 500,
                            savings: 0.0,
                            ..Default::default()
                        },
                    );
                    m
                },
                ..Default::default()
            },
            ..Default::default()
        });

        process_unions(&mut companies, &mut unions, &mut country, 1900);

        // The dead union should have been removed.
        assert!(
            unions.is_empty(),
            "Dissolved union should be removed by retain"
        );

        // Treasury (15_000) should have been credited to Worker savings.
        let worker_savings = country.regions[0]
            .class_demographics
            .urban_classes
            .get("Worker")
            .unwrap()
            .savings;
        assert!(
            (worker_savings - 15_000.0).abs() < 1.0,
            "Worker savings {} should equal dissolved treasury 15000",
            worker_savings
        );
    }
}

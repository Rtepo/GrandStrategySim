//! Union and labor organization management.
//!
//! This module implements the `process_unions` function which handles:
//! - Union militancy and strike actions
//! - Wage negotiations and collective bargaining
//! - Union fund management and member recruitment

use crate::entities::{Company, Union};
use crate::state::Country;

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
    unions: &mut [Union],
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
            striking_company_regions.push((company.region_id.clone(), "Robotnicy".to_string(), striking_fte as f64));
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
/// urban working class ("Robotnicy") in each company's region.
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

/// Collect union dues from member companies.
///
/// # Arguments
/// * `union` - Mutable reference to the union
/// * `companies` - Slice of companies
///
/// # Rules
/// * Dues are calculated as a percentage of company profits
/// * Only profitable companies pay dues
/// * Dues replenish the strike fund
fn collect_union_dues(union: &mut Union, companies: &[Company]) {
    for company in companies {
        if company.union_id.as_ref() == Some(&union.id) {
            // Calculate dues as 1% of company capital
            let dues = company.company_capital * 0.01;
            if dues > 0.0 {
                union.strike_fund += dues;
            }
        }
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

        collect_union_dues(&mut union, &companies);

        // Dues should increase strike fund
        assert!(union.strike_fund > 1000.0);
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
}

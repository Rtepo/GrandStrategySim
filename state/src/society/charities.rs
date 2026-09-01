//! Third-Pillar charity mechanics: fundraising and relief distribution.
//!
//! Phase 13: Social Policy, NGOs & Religious Charities.
//!
//! Charities are standard `Company` entities with `Sector::NGO` or
//! `Sector::Religion`. They collect voluntary donations from wealthy
//! demographics (NGO) or co-religionists (Religion), and distribute
//! their available cash to the poorest classes. All transfers are
//! strict double-entry.

#![allow(missing_docs)]

use crate::entities::legal_form::LegalForm;
use crate::entities::Company;
use crate::registries::enums::Sector;
use crate::state::Country;

/// Solidarity factor by cultural group (proxy for charitable giving propensity).
fn solidarity_factor(cultural_group: &str) -> f64 {
    match cultural_group {
        "slavic" => 1.0,
        "germanic" => 0.8,
        "latin" => 0.9,
        "middle_eastern" => 1.2,
        "balkan" => 1.0,
        _ => 1.0,
    }
}

/// Devotion factor by cultural group (proxy for religious giving propensity).
fn devotion_factor(cultural_group: &str) -> f64 {
    match cultural_group {
        "slavic" => 1.0,
        "germanic" => 0.7,
        "latin" => 1.1,
        "middle_eastern" => 1.3,
        "balkan" => 1.0,
        _ => 1.0,
    }
}

/// Process charity fundraising: collect voluntary donations from demographics.
///
/// # Arguments
/// * `companies` - Mutable companies (charity available_cash credited).
/// * `country` - Mutable country (class savings debited).
/// * `_turn` - Current turn (unused for now, reserved for future logging).
///
/// # Double-Entry
/// * NGO: Debit ClassDemographics.savings (wealthy), Credit Company.available_cash
/// * Religion: Debit ClassDemographics.savings (co-religionists), Credit Company.available_cash
///
/// # Rules
/// * NGOs collect from classes with `savings_per_capita > 2 * average_wage`.
/// * Religious charities collect from classes matching their religion.
/// * Donation rate: 1% of wealthy savings (NGO), 0.5% of co-religionist savings (Religion).
/// * Factors scaled by cultural solidarity/devotion.
pub fn process_charity_fundraising(companies: &mut [Company], country: &mut Country, _turn: u32) {
    let avg_wage = country.macro_indicators.average_wage.max(1.0);
    let cultural_group = &country.macro_indicators.cultural_group;

    for company in companies.iter_mut() {
        let is_ngo = company.sector == Sector::NGO;
        let is_religion = company.sector == Sector::Religion;
        if !is_ngo && !is_religion {
            continue;
        }

        // Get religion from NonProfitData.
        let charity_religion = match &company.legal_form {
            LegalForm::NonProfit(data) => data.religion.clone(),
            _ => continue,
        };

        // Phase 78: Gate fundraising by staff capacity.
        // A charity with no fulfilled staff cannot run collection drives.
        // Staffing ratio = fulfilled_fte / worker_capacity (capped at 1.0).
        let staffing_ratio = if company.worker_capacity > 0 {
            (company.fulfilled_fte as f64 / company.worker_capacity as f64).min(1.0)
        } else {
            0.0
        };
        if staffing_ratio <= 0.0 {
            continue;
        }

        let mut total_collected = 0.0;

        for region in &mut country.regions {
            // Rural classes
            for demographics in region.class_demographics.rural_classes.values_mut() {
                if demographics.population <= 0 {
                    continue;
                }
                let per_capita = demographics.savings / demographics.population as f64;

                if is_ngo {
                    // Collect from wealthy classes.
                    if per_capita > avg_wage * 2.0 && demographics.savings > 0.0 {
                        let factor = solidarity_factor(cultural_group);
                        let donation = demographics.savings * 0.01 * factor;
                        let donation = (donation * staffing_ratio).min(demographics.savings);
                        demographics.savings -= donation;
                        total_collected += donation;
                    }
                } else if is_religion {
                    // Collect from co-religionists.
                    if !charity_religion.is_empty()
                        && !demographics.religion.is_empty()
                        && demographics.religion == charity_religion
                        && demographics.savings > 0.0
                    {
                        let factor = devotion_factor(cultural_group);
                        let donation = demographics.savings * 0.005 * factor;
                        let donation = (donation * staffing_ratio).min(demographics.savings);
                        demographics.savings -= donation;
                        total_collected += donation;
                    }
                }
            }
            // Urban classes
            for demographics in region.class_demographics.urban_classes.values_mut() {
                if demographics.population <= 0 {
                    continue;
                }
                let per_capita = demographics.savings / demographics.population as f64;

                if is_ngo {
                    if per_capita > avg_wage * 2.0 && demographics.savings > 0.0 {
                        let factor = solidarity_factor(cultural_group);
                        let donation = demographics.savings * 0.01 * factor;
                        let donation = (donation * staffing_ratio).min(demographics.savings);
                        demographics.savings -= donation;
                        total_collected += donation;
                    }
                } else if is_religion
                    && !charity_religion.is_empty()
                    && !demographics.religion.is_empty()
                    && demographics.religion == charity_religion
                    && demographics.savings > 0.0
                {
                    let factor = devotion_factor(cultural_group);
                    let donation = demographics.savings * 0.005 * factor;
                    let donation = (donation * staffing_ratio).min(demographics.savings);
                    demographics.savings -= donation;
                    total_collected += donation;
                }
            }
        }

        // Credit collected donations to charity's brokerage_account.cash (or available_cash fallback).
        // Must use brokerage_account.cash so it survives B2B sync overwrites.
        if let Some(ba) = &mut company.brokerage_account {
            ba.cash += total_collected;
        } else {
            company.available_cash += total_collected;
        }
        // Phase 35: Record this turn's donation in the rolling history.
        // Keep the last 12 turns (half a year) for smoothing.
        company.donation_history.push(total_collected);
        if company.donation_history.len() > 12 {
            company.donation_history.remove(0);
        }
    }
}

/// Process charity distribution: distribute relief to poorest classes.
///
/// # Arguments
/// * `companies` - Mutable companies (charity available_cash debited).
/// * `country` - Mutable country (class savings credited).
/// * `_turn` - Current turn (unused for now, reserved for future logging).
///
/// # Double-Entry
/// * Debit Company.available_cash, Credit ClassDemographics.savings (poor)
///
/// # Rules
/// * NGOs distribute to classes with lowest `savings_per_capita` across all religions.
/// * Religious charities distribute EXCLUSIVELY to classes matching their religion.
/// * Distribution is pro-rata by population among eligible classes.
/// * Threshold: classes with `savings_per_capita < average_wage` are eligible.
/// * Operational costs (rent, utilities, wages) are already deducted by the
///   standard company processing pipeline — all available_cash is distributable.
pub fn process_charity_distribution(companies: &mut [Company], country: &mut Country, _turn: u32) {
    let avg_wage = country.macro_indicators.average_wage.max(1.0);

    for company in companies.iter_mut() {
        let is_ngo = company.sector == Sector::NGO;
        let is_religion = company.sector == Sector::Religion;
        if !is_ngo && !is_religion {
            continue;
        }

        let charity_religion = match &company.legal_form {
            LegalForm::NonProfit(data) => data.religion.clone(),
            _ => continue,
        };

        // Phase 78: Gate distribution by staff capacity.
        // A charity with no fulfilled staff cannot distribute aid.
        let staffing_ratio = if company.worker_capacity > 0 {
            (company.fulfilled_fte as f64 / company.worker_capacity as f64).min(1.0)
        } else {
            0.0
        };
        if staffing_ratio <= 0.0 {
            continue;
        }

        let distributable = company
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(company.available_cash)
            * staffing_ratio;
        if distributable <= 0.0 {
            continue;
        }

        // Collect eligible classes: (region_idx, is_rural, class_key, population).
        let mut eligible: Vec<(usize, bool, String, i64)> = Vec::new();
        let mut total_eligible_pop: i64 = 0;

        for (r_idx, region) in country.regions.iter().enumerate() {
            for (class_key, demographics) in &region.class_demographics.rural_classes {
                if demographics.population <= 0 {
                    continue;
                }
                let per_capita = demographics.savings / demographics.population as f64;
                if per_capita >= avg_wage {
                    continue;
                }
                if is_religion
                    && (charity_religion.is_empty()
                        || demographics.religion.is_empty()
                        || demographics.religion != charity_religion)
                {
                    continue;
                }
                eligible.push((r_idx, true, class_key.clone(), demographics.population));
                total_eligible_pop += demographics.population;
            }
            for (class_key, demographics) in &region.class_demographics.urban_classes {
                if demographics.population <= 0 {
                    continue;
                }
                let per_capita = demographics.savings / demographics.population as f64;
                if per_capita >= avg_wage {
                    continue;
                }
                if is_religion
                    && (charity_religion.is_empty()
                        || demographics.religion.is_empty()
                        || demographics.religion != charity_religion)
                {
                    continue;
                }
                eligible.push((r_idx, false, class_key.clone(), demographics.population));
                total_eligible_pop += demographics.population;
            }
        }

        if total_eligible_pop <= 0 {
            continue;
        }

        // Distribute pro-rata by population.
        let per_capita_relief = distributable / total_eligible_pop as f64;
        let mut total_distributed = 0.0;

        for (r_idx, is_rural, class_key, pop) in &eligible {
            let amount = per_capita_relief * *pop as f64;
            if amount <= 0.0 {
                continue;
            }
            let region = &mut country.regions[*r_idx];
            let classes = if *is_rural {
                &mut region.class_demographics.rural_classes
            } else {
                &mut region.class_demographics.urban_classes
            };
            if let Some(demo) = classes.get_mut(class_key) {
                demo.savings += amount;
                total_distributed += amount;
            }
        }

        // Debit charity's brokerage_account.cash (or available_cash fallback).
        if let Some(ba) = &mut company.brokerage_account {
            ba.cash -= total_distributed;
        } else {
            company.available_cash -= total_distributed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::legal_form::NonProfitData;
    use crate::entities::Company;
    use crate::registries::enums::Sector;
    use crate::society::geography::{ClassDemographics, Region};
    use crate::state::Country;

    fn make_charity(sector: Sector, religion: &str, cash: f64) -> Company {
        let mut c = Company::default();
        c.sector = sector;
        c.legal_form = LegalForm::NonProfit(NonProfitData {
            religion: religion.to_string(),
            is_religious: sector == Sector::Religion,
        });
        c.brokerage_account = Some(crate::securities::brokerage::BrokerageAccount {
            cash,
            ..Default::default()
        });
        // Phase 78: Set fulfilled_fte = worker_capacity so tests reflect
        // a fully-staffed charity (staffing_ratio = 1.0).
        c.worker_capacity = 10;
        c.fulfilled_fte = 10;
        c
    }

    fn make_country_with_classes() -> Country {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.average_wage = 100.0;
        country.macro_indicators.cultural_group = "slavic".to_string();

        let mut region = Region::default();
        region.id = "R1".to_string();

        // Wealthy class (for NGO fundraising).
        let mut wealthy = ClassDemographics::default();
        wealthy.population = 100;
        wealthy.savings = 50000.0; // 500 per capita > 2 * 100 = 200
        wealthy.religion = "Catholicism".to_string();
        region
            .class_demographics
            .rural_classes
            .insert("aristocracy".to_string(), wealthy);

        // Poor class (for distribution).
        let mut poor = ClassDemographics::default();
        poor.population = 200;
        poor.savings = 2000.0; // 10 per capita < 100 = avg_wage
        poor.religion = "Catholicism".to_string();
        region
            .class_demographics
            .rural_classes
            .insert("free_peasant".to_string(), poor);

        // Poor class of different religion.
        let mut poor_other = ClassDemographics::default();
        poor_other.population = 100;
        poor_other.savings = 1000.0; // 10 per capita < 100
        poor_other.religion = "Islam".to_string();
        region
            .class_demographics
            .rural_classes
            .insert("landless_laborer".to_string(), poor_other);

        country.regions.push(region);
        country
    }

    #[test]
    fn test_ngo_fundraising_collects_from_wealthy() {
        let mut country = make_country_with_classes();
        let mut companies = vec![make_charity(Sector::NGO, "", 0.0)];

        process_charity_fundraising(&mut companies, &mut country, 1);

        // Should have collected 1% of 50000 * solidarity(1.0) = 500.
        let ba_cash = companies[0]
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(0.0);
        assert!(ba_cash > 400.0 && ba_cash < 600.0);
    }

    #[test]
    fn test_religion_fundraising_collects_from_co_religionists() {
        let mut country = make_country_with_classes();
        let mut companies = vec![make_charity(Sector::Religion, "Catholicism", 0.0)];

        process_charity_fundraising(&mut companies, &mut country, 1);

        // Should collect 0.5% from Catholicism classes (aristocracy: 50000, free_peasant: 2000).
        // Total = 0.005 * (50000 + 2000) * devotion(1.0) = 260.
        let ba_cash = companies[0]
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(0.0);
        assert!(ba_cash > 200.0 && ba_cash < 300.0);
    }

    #[test]
    fn test_religion_fundraising_excludes_other_religion() {
        let mut country = make_country_with_classes();
        let mut companies = vec![make_charity(Sector::Religion, "Islam", 0.0)];

        process_charity_fundraising(&mut companies, &mut country, 1);

        // Should only collect from landless_laborer (Islam): 0.005 * 1000 * 1.0 = 5.
        let ba_cash = companies[0]
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(0.0);
        assert!(ba_cash > 3.0 && ba_cash < 8.0);
    }

    #[test]
    fn test_ngo_distribution_to_poor() {
        let mut country = make_country_with_classes();
        let mut companies = vec![make_charity(Sector::NGO, "", 1000.0)];

        process_charity_distribution(&mut companies, &mut country, 1);

        // All 3 poor classes are eligible (per_capita < avg_wage).
        // Total pop = 200 + 100 = 300 (aristocracy is wealthy, excluded).
        // Wait — aristocracy has 500 per capita > 100, so excluded.
        // free_peasant: 10 < 100, landless_laborer: 10 < 100. Both eligible.
        // Total eligible pop = 300. Per capita relief = 1000/300 ≈ 3.33.
        let ba_cash = companies[0]
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(0.0);
        assert!(ba_cash < 1.0); // All distributed
    }

    #[test]
    fn test_religion_distribution_excludes_other_religion() {
        let mut country = make_country_with_classes();
        let mut companies = vec![make_charity(Sector::Religion, "Catholicism", 1000.0)];

        process_charity_distribution(&mut companies, &mut country, 1);

        // Only Catholicism poor classes: free_peasant (pop 200).
        // landless_laborer (Islam) excluded.
        // Per capita = 1000/200 = 5.0.
        // Check that landless_laborer did NOT receive anything.
        let region = &country.regions[0];
        let ll = region
            .class_demographics
            .rural_classes
            .get("landless_laborer")
            .unwrap();
        assert!((ll.savings - 1000.0).abs() < 1e-6); // Unchanged

        // free_peasant should have received 1000.
        let fp = region
            .class_demographics
            .rural_classes
            .get("free_peasant")
            .unwrap();
        assert!((fp.savings - 3000.0).abs() < 1e-6); // 2000 + 1000
    }
}

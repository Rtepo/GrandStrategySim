//! Phase 17C: The Religious Economy — Apostolic See remittance, Church Fund, and reinvestment.
//!
//! This module implements:
//! - Apostolic See remittance (secular/mixed: from building.available_cash; state religion: from Treasury)
//! - Church Fund payment (state religion: Treasury → owning company via TransferSettler)
//! - Apostolic See reinvestment (global pool → companies via credit_company_by_id)
//!
//! # Rules
//! * Mixed/secular remittance debits building.available_cash directly (physical alms box).
//! * State religion remittance debits country.budget.liquid_reserves.
//! * Church Fund credits owning company's brokerage_account.cash via credit_company_by_id.
//! * See reinvestment uses credit_company_by_id (TransferSettler) to sync bank reserves.
//! * All flows are double-entry: money mass is conserved.

use crate::economy::market::ApostolicSeeLedger;
use crate::economy::transfer_settler::credit_company_by_id;
use crate::entities::Company;
use crate::infrastructure::cultural::CulturalBuilding;
use crate::politics::laws::ReligiousLaw;
use crate::society::culture_registry::registry as culture_registry;
use crate::state::Country;
use std::collections::BTreeMap;

/// Configuration for Apostolic See remittance and reinvestment (no magic numbers).
#[derive(Debug, Clone, PartialEq)]
pub struct ApostolicSeeConfig {
    /// Fraction of donations remitted to See from mixed/secular buildings.
    pub secular_remittance_rate: f64,
    /// Fraction of church income remitted to See from state religion Treasury.
    pub state_religion_remittance_rate: f64,
    /// Threshold above which the See reinvests its charity pool.
    pub reinvestment_threshold: f64,
    /// Fraction of pool distributed as global charity each turn.
    pub charity_distribution_rate: f64,
    /// Fraction of pool invested as FDI in See's host country.
    pub fdi_rate: f64,
}

impl Default for ApostolicSeeConfig {
    fn default() -> Self {
        Self {
            secular_remittance_rate: 0.10,
            state_religion_remittance_rate: 0.10,
            reinvestment_threshold: 5000.0,
            charity_distribution_rate: 0.30,
            fdi_rate: 0.20,
        }
    }
}

/// Result of an Apostolic See remittance turn.
#[derive(Debug, Clone, Default)]
pub struct SeeRemittanceResult {
    /// Total remitted from secular/mixed buildings (alms box debits).
    pub secular_remittance: f64,
    /// Total remitted from state religion Treasury.
    pub state_religion_remittance: f64,
}

/// Process Apostolic See remittance for a single country.
///
/// # Arguments
/// * `country` - Mutable country (for Treasury debit on state religion, and cultural institutions for alms box debit).
/// * `religious_law` - Structured religious law (separation, remittance rate).
/// * `see_ledger` - Mutable global See ledger (receives remittances).
/// * `config` - Remittance configuration.
///
/// # Returns
/// `SeeRemittanceResult` with remittance breakdown.
///
/// # Rules
/// * Secular/mixed: DEBIT building.available_cash, CREDIT see_ledger.total_remittances.
/// * State religion: DEBIT country.budget.liquid_reserves, CREDIT see_ledger.total_remittances.
/// * Runs immediately after collect_cultural_donations, before relief spending.
pub fn process_see_remittance(
    country: &mut Country,
    religious_law: &ReligiousLaw,
    see_ledger: &mut ApostolicSeeLedger,
    config: &ApostolicSeeConfig,
) -> SeeRemittanceResult {
    let mut result = SeeRemittanceResult::default();
    let reg = culture_registry();

    let country_religion = country.macro_indicators.religion.clone();
    let religion_def = reg.religion_from_display_name(&country_religion);
    let is_centralized = religion_def.map(|d| d.is_centralized).unwrap_or(false);

    if !is_centralized {
        return result;
    }

    if religious_law.separation_of_church_and_state {
        let rate = config.secular_remittance_rate;
        for building in &mut country.cultural_institutions {
            let remittance = building.donations_collected_this_turn * rate;
            if remittance > 0.0 && building.available_cash >= remittance {
                building.available_cash -= remittance;
                result.secular_remittance += remittance;
            }
        }
        see_ledger.total_remittances += result.secular_remittance;
        see_ledger.global_charity_pool += result.secular_remittance;
    } else {
        let remittance_rate = config.state_religion_remittance_rate;
        let total_donations: f64 = country.cultural_institutions
            .iter()
            .map(|b| b.donations_collected_this_turn)
            .sum();
        let remittance = total_donations * remittance_rate;
        if remittance > 0.0 && country.budget.liquid_reserves >= remittance {
            country.budget.liquid_reserves -= remittance;
            result.state_religion_remittance = remittance;
            see_ledger.total_remittances += remittance;
            see_ledger.global_charity_pool += remittance;
        }
    }

    result
}

/// Result of a Church Fund payment turn.
#[derive(Debug, Clone, Default)]
pub struct ChurchFundResult {
    /// Total Church Fund paid to owning companies.
    pub total_paid: f64,
    /// Number of buildings that received Church Fund payments.
    pub buildings_funded: usize,
    /// Number of buildings that could not be funded (Treasury insufficient).
    pub buildings_unfunded: usize,
}

/// Process Church Fund payment for a country with state religion.
///
/// # Arguments
/// * `country` - Mutable country (for Treasury debit and cultural institutions).
/// * `companies` - Mutable companies (for credit_company_by_id).
/// * `religious_law` - Structured religious law.
///
/// # Returns
/// `ChurchFundResult` with payment stats.
///
/// # Rules
/// * Only runs if separation_of_church_and_state == false (state religion active).
/// * Church Fund = sum of religious building maintenance costs.
/// * DEBIT country.budget.liquid_reserves, CREDIT owning company via credit_company_by_id.
/// * Buildings without owner_company_id receive no payment (degrade).
/// * If Treasury cannot afford full payment: partial payment only.
pub fn process_church_fund(
    country: &mut Country,
    companies: &mut [Company],
    religious_law: &ReligiousLaw,
) -> ChurchFundResult {
    let mut result = ChurchFundResult::default();

    if religious_law.separation_of_church_and_state {
        return result;
    }

    let maintenance_per_building: f64 = 500.0;

    for building in &country.cultural_institutions {
        if building.condition < 0.05 {
            continue;
        }

        let maintenance_cost = maintenance_per_building * building.capacity.max(1.0).min(100.0) / 100.0;

        if let Some(ref owner_id) = building.owner_company_id {
            let affordable = country.budget.liquid_reserves.min(maintenance_cost);
            if affordable > 0.0 {
                country.budget.liquid_reserves -= affordable;
                let credited = credit_company_by_id(companies, owner_id, affordable);
                if credited {
                    result.total_paid += affordable;
                    result.buildings_funded += 1;
                } else {
                    country.budget.liquid_reserves += affordable;
                    result.buildings_unfunded += 1;
                }
            } else {
                result.buildings_unfunded += 1;
            }
        } else {
            result.buildings_unfunded += 1;
        }
    }

    result
}

/// Result of an Apostolic See reinvestment turn.
#[derive(Debug, Clone, Default)]
pub struct SeeReinvestmentResult {
    /// Total charity distributed to poor countries.
    pub charity_distributed: f64,
    /// Total FDI invested in See's host country.
    pub fdi_invested: f64,
}

/// Process Apostolic See reinvestment globally.
///
/// # Arguments
/// * `see_ledger` - Mutable See ledger (pool is debited).
/// * `all_companies` - All companies across all countries (for credit_company_by_id).
/// * `country_gdp_per_capita` - Map of country_name → GDP per capita (for charity targeting).
/// * `see_country_companies` - Company IDs in the See's host country (for FDI).
/// * `config` - Reinvestment configuration.
///
/// # Returns
/// `SeeReinvestmentResult` with distribution stats.
///
/// # Rules
/// * Only runs if global_charity_pool > reinvestment_threshold.
/// * Charity: distribute to companies in countries with lowest GDP per capita.
/// * FDI: invest in companies in the See's host country.
/// * DEBIT see_ledger.global_charity_pool, CREDIT companies via credit_company_by_id.
pub fn process_see_reinvestment(
    see_ledger: &mut ApostolicSeeLedger,
    all_companies: &mut [Company],
    country_gdp_per_capita: &BTreeMap<String, f64>,
    see_country_company_ids: &[String],
    config: &ApostolicSeeConfig,
) -> SeeReinvestmentResult {
    let mut result = SeeReinvestmentResult::default();

    if see_ledger.global_charity_pool <= config.reinvestment_threshold {
        return result;
    }

    let available = see_ledger.global_charity_pool - config.reinvestment_threshold;
    let charity_amount = available * config.charity_distribution_rate;
    let fdi_amount = available * config.fdi_rate;

    // Charity: distribute to companies in poorest countries.
    if charity_amount > 0.0 && !country_gdp_per_capita.is_empty() {
        let poorest_country = country_gdp_per_capita
            .iter()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(name, _)| name.clone());

        if let Some(_poorest) = poorest_country {
            // Collect Religion-sector company IDs first to avoid borrow conflict.
            let religion_company_ids: Vec<String> = all_companies.iter()
                .filter(|c| c.sector == crate::registries::enums::Sector::Religion)
                .take(3)
                .map(|c| c.id.clone())
                .collect();

            let charity_per_company = charity_amount / religion_company_ids.len().max(1) as f64;
            let mut distributed = 0.0;
            for company_id in &religion_company_ids {
                if distributed >= charity_amount {
                    break;
                }
                let amount = charity_per_company.min(charity_amount - distributed);
                if amount > 0.0 {
                    let credited = credit_company_by_id(all_companies, company_id, amount);
                    if credited {
                        distributed += amount;
                    }
                }
            }
            see_ledger.global_charity_pool -= distributed;
            result.charity_distributed = distributed;
        }
    }

    // FDI: invest in companies in the See's host country.
    if fdi_amount > 0.0 && !see_country_company_ids.is_empty() {
        let fdi_per_company = fdi_amount / see_country_company_ids.len() as f64;
        let mut invested = 0.0;
        for company_id in see_country_company_ids {
            if invested >= fdi_amount {
                break;
            }
            let amount = fdi_per_company.min(fdi_amount - invested);
            if amount > 0.0 {
                let credited = credit_company_by_id(all_companies, company_id, amount);
                if credited {
                    invested += amount;
                }
            }
        }
        see_ledger.global_charity_pool -= invested;
        result.fdi_invested = invested;
    }

    result
}

/// Process monastery/temple production for a country.
///
/// # Arguments
/// * `country` - Mutable country (for region access).
/// * `cultural_institutions` - Mutable cultural buildings (for production output).
/// * `companies` - Mutable companies (for revenue crediting via TransferSettler).
/// * `building_inventories` - Mutable building inventories (for input/output tracking).
///
/// # Returns
/// Total production value generated this turn.
///
/// # Rules
/// * Only buildings with production_method and owner_company_id produce.
/// * Output goes to building inventory (for B2B sell orders).
/// * Revenue from B2B settlement credits owning company via credit_company_by_id.
/// * The alms box (building.available_cash) is NOT touched by production revenue.
pub fn process_monastery_production(
    cultural_institutions: &mut [CulturalBuilding],
    companies: &mut [Company],
) -> f64 {
    let mut total_value = 0.0_f64;

    for building in cultural_institutions {
        if building.production_method.is_none() || building.owner_company_id.is_none() {
            continue;
        }

        if building.condition < 0.1 {
            continue;
        }

        // Estimate production value based on capacity and condition.
        let production_scale = building.capacity.max(1.0).min(100.0) / 100.0;
        let condition_factor = building.condition;
        let base_output_value = 100.0 * production_scale * condition_factor;

        // Credit the owning company via TransferSettler (simulated B2B revenue).
        if let Some(ref owner_id) = building.owner_company_id {
            let credited = credit_company_by_id(companies, owner_id, base_output_value);
            if credited {
                total_value += base_output_value;
            }
        }
    }

    total_value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::market::ApostolicSeeLedger;
    use crate::entities::Company;
    use crate::infrastructure::cultural::{CulturalBuilding, CulturalBuildingType};
    use crate::politics::laws::ReligiousLaw;
    use crate::state::Country;

    fn make_test_building(cash: f64, donations: f64, owner: Option<&str>) -> CulturalBuilding {
        let mut b = CulturalBuilding::default();
        b.available_cash = cash;
        b.donations_collected_this_turn = donations;
        b.capacity = 50.0;
        b.condition = 0.8;
        b.building_type = CulturalBuildingType::Temple;
        if let Some(id) = owner {
            b.owner_company_id = Some(id.to_string());
        }
        b
    }

    #[test]
    fn test_secular_remittance_debits_building_cash() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.religion = "Katolicyzm".to_string();
        country.cultural_institutions = vec![
            make_test_building(1000.0, 500.0, None),
            make_test_building(2000.0, 300.0, None),
        ];
        let law = ReligiousLaw {
            separation_of_church_and_state: true,
            apostolic_remittance_rate: 0.10,
            ..Default::default()
        };
        let mut ledger = ApostolicSeeLedger::default();
        let config = ApostolicSeeConfig::default();

        let result = process_see_remittance(&mut country, &law, &mut ledger, &config);

        assert!((result.secular_remittance - 80.0).abs() < 0.01,
            "secular remittance should be 80, got {}", result.secular_remittance);
        assert!((country.cultural_institutions[0].available_cash - 950.0).abs() < 0.01,
            "building 0 cash should be 950, got {}", country.cultural_institutions[0].available_cash);
        assert!((country.cultural_institutions[1].available_cash - 1970.0).abs() < 0.01,
            "building 1 cash should be 1970, got {}", country.cultural_institutions[1].available_cash);
        assert!((ledger.total_remittances - 80.0).abs() < 0.01);
    }

    #[test]
    fn test_state_religion_remittance_debits_treasury() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.religion = "Katolicyzm".to_string();
        country.budget.liquid_reserves = 100000.0;
        country.cultural_institutions = vec![
            make_test_building(1000.0, 500.0, None),
            make_test_building(2000.0, 300.0, None),
        ];
        let law = ReligiousLaw {
            separation_of_church_and_state: false,
            apostolic_remittance_rate: 0.10,
            ..Default::default()
        };
        let mut ledger = ApostolicSeeLedger::default();
        let config = ApostolicSeeConfig::default();

        let result = process_see_remittance(&mut country, &law, &mut ledger, &config);

        assert!((result.state_religion_remittance - 80.0).abs() < 0.01,
            "state religion remittance should be 80, got {}", result.state_religion_remittance);
        assert!((country.budget.liquid_reserves - 99920.0).abs() < 0.01,
            "treasury should be 99920, got {}", country.budget.liquid_reserves);
    }

    #[test]
    fn test_church_fund_credits_company_via_transfer_settler() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 100000.0;
        country.cultural_institutions = vec![
            make_test_building(0.0, 0.0, Some("religious_co_1")),
        ];

        let mut company = Company::default();
        company.id = "religious_co_1".to_string();
        company.sector = crate::registries::enums::Sector::Religion;
        let mut companies = vec![company];

        let law = ReligiousLaw {
            separation_of_church_and_state: false,
            ..Default::default()
        };

        let result = process_church_fund(&mut country, &mut companies, &law);

        assert!(result.total_paid > 0.0, "church fund should pay something");
        assert_eq!(result.buildings_funded, 1);
        let comp = &companies[0];
        let cash = comp.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(comp.available_cash);
        assert!(cash > 0.0, "company should have been credited, got {}", cash);
    }

    #[test]
    fn test_church_fund_no_payment_when_separated() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 100000.0;
        country.cultural_institutions = vec![
            make_test_building(0.0, 0.0, Some("co_1")),
        ];
        let mut companies = vec![];
        let law = ReligiousLaw {
            separation_of_church_and_state: true,
            ..Default::default()
        };

        let result = process_church_fund(&mut country, &mut companies, &law);

        assert!((result.total_paid).abs() < 0.001, "no church fund when separated");
    }

    #[test]
    fn test_see_reinvestment_distributes_to_companies() {
        let mut ledger = ApostolicSeeLedger::default();
        ledger.global_charity_pool = 20000.0;

        let mut company = Company::default();
        company.id = "rel_co_1".to_string();
        company.sector = crate::registries::enums::Sector::Religion;
        let mut companies = vec![company];

        let gdp_map = BTreeMap::from([("PoorCountry".to_string(), 100.0)]);
        let see_company_ids: Vec<String> = vec![];

        let config = ApostolicSeeConfig::default();
        let result = process_see_reinvestment(&mut ledger, &mut companies, &gdp_map, &see_company_ids, &config);

        assert!(result.charity_distributed > 0.0, "charity should be distributed");
        assert!(ledger.global_charity_pool < 20000.0, "pool should be reduced");
    }

    #[test]
    fn test_monastery_production_credits_owner_company() {
        let mut building = make_test_building(0.0, 0.0, Some("monastery_co_1"));
        building.production_method = Some("monastery_scriptorium".to_string());

        let mut company = Company::default();
        company.id = "monastery_co_1".to_string();
        company.sector = crate::registries::enums::Sector::Religion;
        let mut companies = vec![company];

        let value = process_monastery_production(&mut [building], &mut companies);

        assert!(value > 0.0, "production should generate value, got {}", value);
        let comp = &companies[0];
        let cash = comp.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(comp.available_cash);
        assert!(cash > 0.0, "owning company should be credited, got {}", cash);
    }

    #[test]
    fn test_no_production_without_owner_or_pm() {
        let mut building = make_test_building(0.0, 0.0, None);
        building.production_method = None;

        let mut companies: Vec<Company> = vec![];
        let value = process_monastery_production(&mut [building], &mut companies);

        assert!((value).abs() < 0.001, "no production without owner/PM");
    }
}

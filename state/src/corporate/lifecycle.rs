//! Company lifecycle management.
//!
//! This module implements the `CompanyLifecycle` service which handles:
//! - Spawning new companies in sectors with strong PMI and positive market signals
//! - Liquidating bankrupt companies via the Syndic (universal trustee)
//!
//! Phase 96: The crude "mark and remove" liquidation path has been replaced
//! with a call to `Syndic::execute_liquidation`, which is the SINGLE universal
//! liquidation path (Rule 14). The Syndic handles:
//! - FX seizure and conversion
//! - Cash seizure (brokerage + available_cash)
//! - Inventory fire-sale
//! - Building routing to auction pool with per-company creditor claims
//! - Full waterfall: wages → taxes → secured creditors → shareholders
//! - Cadastre parcel reassignment to Treasury

use crate::corporate::bankruptcy::{process_auction_turn, Syndic};
use crate::economy::market::MarketSignal;
use crate::entities::{Building, Company, FamilyBusinessData, LegalForm};
use crate::registries::enums::Sector;
use crate::state::Country;
use rustc_hash::FxHashMap;

#[allow(dead_code)]
type HashMap<K, V> = FxHashMap<K, V>;

/// CompanyLifecycle service manages organic birth and death of companies.
pub struct CompanyLifecycle;

impl CompanyLifecycle {
    /// Process company lifecycle for a single country.
    pub fn process_lifecycle(
        companies: &mut Vec<Company>,
        buildings: &mut Vec<Building>,
        country: &mut Country,
        year: u32,
        market_signal: &MarketSignal,
    ) {
        // 1. Liquidate bankrupt companies via the Syndic.
        Self::liquidate_bankrupt_companies(companies, buildings, country, year);

        // 2. Process the bankruptcy auction market (bidders buy, expired demolish).
        let policy = crate::state::BankruptcyPolicy::with_defaults();
        process_auction_turn(companies, buildings, country, &policy);

        // 3. Spawn new companies in promising sectors.
        Self::spawn_new_companies(companies, buildings, country, year, market_signal);
    }

    /// Identify and liquidate bankrupt companies via the Syndic.
    ///
    /// # Rules
    /// * Companies with negative equity (company_capital < 0) are bankrupt
    /// * Companies with sustained losses (3+ consecutive years) are bankrupt
    /// * Energy companies get receivership instead of liquidation
    /// * The Syndic handles all asset seizure, creditor payment, and cleanup
    fn liquidate_bankrupt_companies(
        companies: &mut Vec<Company>,
        buildings: &mut Vec<Building>,
        country: &mut Country,
        _year: u32,
    ) {
        let mut to_remove = Vec::new();

        for (idx, company) in companies.iter().enumerate() {
            // Check for negative equity.
            if company.company_capital < 0.0 {
                // Strategic Resolution: Energy companies are too critical to liquidate.
                if company.sector == Sector::Energy && !company.is_in_receivership {
                    continue;
                }
                to_remove.push(idx);
                continue;
            }

            // Check for sustained losses (3+ consecutive years).
            // Phase 33: Grace period for new companies.
            if company.financial_history.len() < 2 {
                continue;
            }
            let consecutive_losses = company
                .financial_history
                .iter()
                .rev()
                .take(3)
                .all(|record| {
                    record
                        .get("net_profit")
                        .and_then(|v| v.as_f64())
                        .is_some_and(|profit| profit < 0.0)
                });

            if consecutive_losses {
                if company.sector == Sector::Energy && !company.is_in_receivership {
                    continue;
                }
                to_remove.push(idx);
            }
        }

        // Phase 96: Use the Syndic as the universal liquidation path.
        let domestic_currency = country.macro_indicators.currency.clone();
        let policy = crate::state::BankruptcyPolicy::with_defaults();

        for idx in to_remove.into_iter().rev() {
            // We need to liquidate companies[idx] but also pass the rest of
            // companies as the bank/creditor slice. Split the vector.
            let mut company_to_liquidate = companies.remove(idx);

            // Get a mutable reference to the forex market from country.
            // The forex market is on GameState, not Country. We need to handle
            // this carefully — for now, create a dummy forex market since the
            // Syndic gracefully handles failed FX swaps.
            // TODO: Pass real forex_market from the turn task.
            let mut dummy_forex = crate::state::forex::ForexMarket::default();

            let mut syndic = Syndic::new(domestic_currency.clone());

            syndic.execute_liquidation(
                &mut company_to_liquidate,
                buildings,
                &mut dummy_forex,
                country,
                companies, // remaining companies (including banks)
                &policy,
            );

            // The Syndic marks is_liquidated = true. The company is already
            // removed from the vector. No need to push it back.
        }
    }

    /// Spawn new companies in sectors with strong PMI and positive market signals.
    fn spawn_new_companies(
        companies: &mut Vec<Company>,
        buildings: &mut Vec<Building>,
        country: &mut Country,
        year: u32,
        market_signal: &MarketSignal,
    ) {
        if market_signal.interest_rate > 0.15 {
            return;
        }

        let private_capital = country.budget.private_capital;
        if private_capital < 1000.0 {
            return;
        }

        let investment_fraction = 0.01 + (private_capital / 1_000_000.0).min(0.04);
        let investment = private_capital * investment_fraction;

        let promising_sectors: Vec<Sector> = country
            .budget
            .sectors
            .iter()
            .filter(|(_, sector_share)| {
                sector_share
                    .extra
                    .get("pmi")
                    .and_then(|v| v.as_f64())
                    .is_some_and(|pmi| pmi > 50.0)
            })
            .map(|(sector, _)| *sector)
            .collect();

        if promising_sectors.is_empty() {
            return;
        }

        let num_companies = ((investment / 10_000.0) as usize).min(5).max(1);

        for i in 0..num_companies {
            let sector = promising_sectors[i % promising_sectors.len()];
            let capital_per_company = investment / num_companies as f64;

            let avg_wage = country.macro_indicators.average_wage.max(1.0);
            let min_capital =
                crate::corporate::capital_intensity::minimum_capital_for_sector(&sector, avg_wage);
            if capital_per_company < min_capital {
                continue;
            }

            let company_id = format!("NEW_{}_{}_{}", country.name, year, i);
            let legal_form = LegalForm::FamilyBusiness(FamilyBusinessData {
                dynasty_id: None,
                successor_generation: 0,
                family_retained_share: 1.0,
                heir_vip_ids: Vec::new(),
                succession_crisis: false,
            });
            let new_company = Company::new(
                company_id.clone(),
                format!("New Company {}-{}", year, i),
                sector,
                legal_form,
                capital_per_company * 0.5,
                capital_per_company * 0.5,
                100,
            );

            let building_id = format!("BLD_{}_{}_{}", country.name, year, i);
            let new_building = Building::new(building_id, company_id, sector, 100);

            companies.push(new_company);
            buildings.push(new_building);

            country.budget.private_capital -= capital_per_company;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::market::MarketSignal;

    #[test]
    fn test_liquidate_bankrupt_negative_equity() {
        let legal_form = LegalForm::FamilyBusiness(FamilyBusinessData::default());
        let mut companies = vec![Company::new(
            "bankrupt".to_string(),
            "Bankrupt Co".to_string(),
            Sector::Mining,
            legal_form,
            1000.0,
            0.0,
            100,
        )];
        companies[0].liabilities = 2000.0;
        companies[0].company_capital = -1000.0;
        companies[0].building_ids.push("bld1".to_string());

        let mut buildings = vec![Building::new(
            "bld1".to_string(),
            "bankrupt".to_string(),
            Sector::Mining,
            100,
        )];
        buildings[0].current_employment = 50;

        let mut country = Country::mock_for_tests();
        country.name = "Test".to_string();

        CompanyLifecycle::liquidate_bankrupt_companies(
            &mut companies,
            &mut buildings,
            &mut country,
            2024,
        );

        // Company is removed (liquidated).
        assert!(companies.is_empty());
        // Building remains but is now in the auction pool (not demolished yet).
        // The Syndic routes buildings to the auction pool, not removes them.
        assert!(!buildings.is_empty());
    }

    #[test]
    fn test_no_spawn_high_interest() {
        let mut companies = Vec::new();
        let mut buildings = Vec::new();
        let mut country = Country::mock_for_tests();
        country.name = "Test".to_string();
        country.budget.private_capital = 100_000.0;

        let market_signal = MarketSignal {
            interest_rate: 0.20,
            sector_pmi: HashMap::default(),
            demand_surplus: HashMap::default(),
            global_surplus: HashMap::default(),
            prices: HashMap::default(),
            stock_confidence: 50.0,
            stock_index: 1000.0,
        };

        CompanyLifecycle::spawn_new_companies(
            &mut companies,
            &mut buildings,
            &mut country,
            2024,
            &market_signal,
        );

        assert!(companies.is_empty());
    }

    #[test]
    fn test_no_spawn_low_capital() {
        let mut companies = Vec::new();
        let mut buildings = Vec::new();
        let mut country = Country::mock_for_tests();
        country.name = "Test".to_string();
        country.budget.private_capital = 500.0;

        let market_signal = MarketSignal {
            interest_rate: 0.05,
            sector_pmi: HashMap::default(),
            demand_surplus: HashMap::default(),
            global_surplus: HashMap::default(),
            prices: HashMap::default(),
            stock_confidence: 50.0,
            stock_index: 1000.0,
        };

        CompanyLifecycle::spawn_new_companies(
            &mut companies,
            &mut buildings,
            &mut country,
            2024,
            &market_signal,
        );

        assert!(companies.is_empty());
    }
}

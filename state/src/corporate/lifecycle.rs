//! Company lifecycle management.
//!
//! This module implements the `CompanyLifecycle` service which handles:
//! - Spawning new companies in sectors with strong PMI and positive market signals
//! - Liquidating bankrupt companies with negative equity or sustained losses

use crate::economy::market::MarketSignal;
use crate::entities::{Building, Company, FamilyBusinessData, LegalForm};
use crate::registries::enums::Sector;
use crate::state::Country;
use rustc_hash::FxHashMap;

type HashMap<K, V> = FxHashMap<K, V>;

/// CompanyLifecycle service manages organic birth and death of companies.
///
/// This service is responsible for:
/// - Spawning new companies in sectors with strong PMI and positive market signals
/// - Liquidating bankrupt companies with negative equity or sustained losses
pub struct CompanyLifecycle;

impl CompanyLifecycle {
    /// Process company lifecycle for a single country.
    ///
    /// # Arguments
    /// * `companies` - Mutable slice of companies for this country
    /// * `buildings` - Mutable slice of buildings for this country
    /// * `country` - Mutable reference to the country state
    /// * `year` - Current game year
    /// * `market_signal` - Market conditions including PMI and interest rates
    ///
    /// # Rules
    /// * New companies are spawned in sectors with PMI > 50 and positive market signals
    /// * A fraction of private capital is converted into new companies
    /// * Bankrupt companies (negative equity or sustained losses) are liquidated
    /// * Buildings of liquidated companies are transferred or destroyed
    pub fn process_lifecycle(
        companies: &mut Vec<Company>,
        buildings: &mut Vec<Building>,
        country: &mut Country,
        year: u32,
        market_signal: &MarketSignal,
    ) {
        // 1. Liquidate bankrupt companies
        Self::liquidate_bankrupt_companies(companies, buildings, country, year);

        // 2. Spawn new companies in promising sectors
        Self::spawn_new_companies(companies, buildings, country, year, market_signal);
    }

    /// Identify and liquidate bankrupt companies.
    ///
    /// # Arguments
    /// * `companies` - Mutable slice of companies
    /// * `buildings` - Mutable slice of buildings
    /// * `country` - Mutable reference to country state
    /// * `year` - Current game year
    ///
    /// # Rules
    /// * Companies with negative equity (company_capital < 0) are bankrupt
    /// * Companies with sustained losses (3+ consecutive years of negative profit) are bankrupt
    /// * Bankrupt companies are removed from the companies vector
    /// * Their buildings are marked for liquidation (owner_id cleared, capacity zeroed)
    fn liquidate_bankrupt_companies(
        companies: &mut Vec<Company>,
        buildings: &mut Vec<Building>,
        country: &mut Country,
        _year: u32,
    ) {
        let mut to_remove = Vec::new();

        for (idx, company) in companies.iter().enumerate() {
            // Check for negative equity
            if company.company_capital < 0.0 {
                // Strategic Resolution: Energy companies are too critical to liquidate
                if company.sector == Sector::Energy && !company.is_in_receivership {
                    // Mark for receivership instead of liquidation
                    // The actual resolution processing happens in the turn loop
                    continue;
                }
                to_remove.push(idx);
                continue;
            }

            // Check for sustained losses (3+ consecutive years)
            // Phase 33: Grace period — companies with fewer than 2 financial history
            // entries are too new to be liquidated for sustained losses. They can still
            // be liquidated for negative equity (checked above). This prevents first-turn
            // mass bankruptcy when companies haven't had time to establish operations.
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
                        .get("zysk_netto")
                        .and_then(|v| v.as_f64())
                        .map_or(false, |profit| profit < 0.0)
                });

            if consecutive_losses {
                // Strategic Resolution for energy companies with sustained losses
                if company.sector == Sector::Energy && !company.is_in_receivership {
                    continue;
                }
                to_remove.push(idx);
            }
        }

        // Phase 24A.8: Remove bankrupt companies with full ghost reference cleanup.
        for idx in to_remove.into_iter().rev() {
            let company_id = companies[idx].id.clone();
            let company_fixed_capital = companies[idx].fixed_capital;
            let company_liquid_capital = companies[idx].liquid_capital;

            // 1. Mark loans to this company as Default in all banks
            for bank in companies.iter_mut() {
                if let Some(ref mut bs) = bank.balance_sheet {
                    for loan in &mut bs.loans_issued {
                        if loan.borrower_id == company_id && loan.status != crate::state::banking::LoanStatus::Repaid {
                            loan.status = crate::state::banking::LoanStatus::Default;
                        }
                    }
                }
            }

            // 2. Liquidate buildings owned by this company
            for building in buildings.iter_mut() {
                if building.owner_id == company_id {
                    // Heritage buildings are protected from demolition
                    if building.is_heritage_site {
                        building.owner_id.clear();
                        building.current_employment = 0;
                        // Preserve capacity for potential state takeover
                        continue;
                    }
                    // Phase 24A.8: Route building assets to auction pool
                    country.bankruptcy_auction_pool.add_asset(
                        format!("building_{}", building.id),
                        company_fixed_capital,
                        company_id.clone(),
                        std::collections::HashMap::new(),
                        &crate::state::BankruptcyPolicy::with_defaults(),
                    );
                    building.owner_id.clear();
                    building.current_employment = 0;
                    building.worker_capacity = 0;
                }
            }

            // 3. Remove frozen company cash from justice state
            if let Some(ref mut justice_state) = country.politics.justice_state {
                justice_state.frozen_company_cash.remove(&company_id);
            }

            // 4. Remove any exchange listings for this company
            country.stock_exchange.liquidity_pools.remove(&format!("EQUITY:{}", company_id));

            // 5. Add recovered cash to auction pool
            country.bankruptcy_auction_pool.cash_collected += company_liquid_capital.max(0.0);

            // Remove the company
            companies.remove(idx);
        }
    }

    /// Spawn new companies in sectors with strong PMI and positive market signals.
    ///
    /// # Arguments
    /// * `companies` - Mutable slice of companies (new companies will be pushed)
    /// * `buildings` - Mutable slice of buildings (new buildings will be pushed)
    /// * `country` - Mutable reference to country state
    /// * `year` - Current game year
    /// * `market_signal` - Market conditions
    ///
    /// # Rules
    /// * Only spawn in sectors with PMI > 50
    /// * Only spawn if interest rate is reasonable (< 0.15)
    /// * Convert 1-5% of private capital into new companies
    /// * New companies start as family businesses or cooperatives
    /// * Each new company gets one building with base capacity
    fn spawn_new_companies(
        companies: &mut Vec<Company>,
        buildings: &mut Vec<Building>,
        country: &mut Country,
        year: u32,
        market_signal: &MarketSignal,
    ) {
        // Don't spawn if interest rates are too high
        if market_signal.interest_rate > 0.15 {
            return;
        }

        let private_capital = country.budget.private_capital;
        if private_capital < 1000.0 {
            return; // Not enough capital to spawn
        }

        // Calculate how much capital to invest in new companies (1-5%)
        let investment_fraction = 0.01 + (private_capital / 1_000_000.0).min(0.04);
        let investment = private_capital * investment_fraction;

        // Find promising sectors (PMI > 50)
        // PMI is stored in the extra Map as a Value
        let promising_sectors: Vec<Sector> = country
            .budget
            .sectors
            .iter()
            .filter(|(_, sector_share)| {
                sector_share
                    .extra
                    .get("pmi")
                    .and_then(|v| v.as_f64())
                    .map_or(false, |pmi| pmi > 50.0)
            })
            .map(|(sector, _)| *sector)
            .collect();

        if promising_sectors.is_empty() {
            return;
        }

        // Determine number of new companies to spawn
        // Base on investment amount and sector count
        let num_companies = ((investment / 10_000.0) as usize).min(5).max(1);

        for i in 0..num_companies {
            let sector = promising_sectors[i % promising_sectors.len()];
            let capital_per_company = investment / num_companies as f64;

            // Phase 24C.8: Enforce sector-aware entry barriers.
            // Companies must meet the minimum capital requirement for their sector,
            // scaled by average wage (inflation-indexed). Skip spawning if capital
            // is insufficient for the chosen sector.
            let avg_wage = country.macro_indicators.average_wage.max(1.0);
            let min_capital = crate::corporate::capital_intensity::minimum_capital_for_sector(&sector, avg_wage);
            if capital_per_company < min_capital {
                continue; // Insufficient capital for this sector's entry barrier
            }

            // Create new company - use FamilyBusiness as default for startups
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

            // Create initial building for the company
            let building_id = format!("BLD_{}_{}_{}", country.name, year, i);
            let new_building = Building::new(building_id, company_id, sector, 100);

            companies.push(new_company);
            buildings.push(new_building);

            // Deduct from private capital
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

        CompanyLifecycle::liquidate_bankrupt_companies(&mut companies, &mut buildings, &mut country, 2024);

        assert!(companies.is_empty());
        assert_eq!(buildings[0].owner_id, "");
        assert_eq!(buildings[0].current_employment, 0);
        assert_eq!(buildings[0].worker_capacity, 0);
    }

    #[test]
    fn test_no_spawn_high_interest() {
        let mut companies = Vec::new();
        let mut buildings = Vec::new();
        let mut country = Country::mock_for_tests();
        country.name = "Test".to_string();
        country.budget.private_capital = 100_000.0;

        let market_signal = MarketSignal {
            interest_rate: 0.20, // Too high
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
        country.budget.private_capital = 500.0; // Too low

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

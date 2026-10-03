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
        current_turn: u32,
        registries: &crate::registries::Registries,
    ) {
        // Phase 94: M0 conservation diagnostic — compute simplified M0 sum
        // before and after each lifecycle step to identify M0 leaks.
        #[cfg(feature = "diagnostic")]
        let m0_before = Self::compute_m0_sum(companies, country);

        // 1. Liquidate bankrupt companies via the Syndic.
        Self::liquidate_bankrupt_companies(companies, buildings, country, year, current_turn);
        #[cfg(feature = "diagnostic")]
        {
            let m0_after = Self::compute_m0_sum(companies, country);
            eprintln!("LIFECYCLE_POST_LIQ: country={} cb_inj={:.0} m0_delta={:.0}", country.name, country.central_bank.liquidity_injected, m0_after - m0_before);
        }

        // 2. Process the bankruptcy auction market (bidders buy, expired demolish).
        let policy = crate::state::BankruptcyPolicy::with_defaults();
        #[cfg(feature = "diagnostic")]
        let m0_pre_auc = Self::compute_m0_sum(companies, country);
        process_auction_turn(companies, buildings, country, &policy);
        #[cfg(feature = "diagnostic")]
        {
            let m0_after = Self::compute_m0_sum(companies, country);
            eprintln!("LIFECYCLE_POST_AUC: country={} cb_inj={:.0} m0_delta={:.0}", country.name, country.central_bank.liquidity_injected, m0_after - m0_pre_auc);
        }

        // 3. Spawn new companies in promising sectors.
        #[cfg(feature = "diagnostic")]
        let m0_pre_spawn = Self::compute_m0_sum(companies, country);
        Self::spawn_new_companies(
            companies,
            buildings,
            country,
            year,
            market_signal,
            current_turn,
            registries,
        );
        #[cfg(feature = "diagnostic")]
        {
            let m0_after = Self::compute_m0_sum(companies, country);
            eprintln!("LIFECYCLE_POST_SPAWN: country={} cb_inj={:.0} m0_delta={:.0}", country.name, country.central_bank.liquidity_injected, m0_after - m0_pre_spawn);
        }
    }

    /// Phase 94: Compute a simplified M0 sum for diagnostic tracing.
    /// Sums treasury, bank reserves, corporate (unbanked), citizen savings,
    /// and ministry cash. Excludes foreign, charity, etc. for brevity.
    #[cfg(feature = "diagnostic")]
    fn compute_m0_sum(companies: &[Company], country: &Country) -> f64 {
        let mut total: f64 = 0.0;
        // Treasury
        total += country.budget.liquid_reserves;
        for region in &country.regions {
            if let Some(ref gov) = region.governance {
                total += gov.budget.liquid_reserves;
            }
        }
        for megaregion in &country.megaregions {
            if let Some(ref gov) = megaregion.governance {
                total += gov.budget.liquid_reserves;
            }
        }
        // Bank reserves
        for c in companies {
            if c.sector == Sector::Banking {
                if let Some(ref bs) = c.balance_sheet {
                    total += bs.reserves_at_central_bank + bs.cb_deposit_facility_balance;
                }
            }
        }
        total += country.bfg_fund.reserves;
        total += country.sobk_scheme.pool;
        // Corporate (unbanked only)
        for c in companies {
            if c.sector != Sector::Banking && c.primary_bank_id.is_none() {
                total += c.available_cash.max(0.0);
                total += c.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(0.0).max(0.0);
                total += c.rd_budget;
                total += c.debit_cash;
            }
        }
        // Citizen savings
        for region in &country.regions {
            for d in region.class_demographics.rural_classes.values() {
                total += d.savings;
            }
            for d in region.class_demographics.urban_classes.values() {
                total += d.savings;
            }
        }
        // Ministry
        if let Some(ref config) = country.politics.ministry_config {
            for ministry in &config.ministries {
                total += ministry.ministry_cash;
            }
        }
        total += country.ministry_public_service_pool;
        // Arbitration escrow (on country)
        total += country.arbitration_court.unclaimed_arbitration_funds;
        // Black-ops budget (on country)
        total += country.budget.black_ops_budget;
        // Frozen cash (on country)
        if let Some(ref justice) = country.politics.justice_state {
            total += justice.frozen_company_cash.values().sum::<f64>();
        }
        total
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
        current_turn: u32,
    ) {
        // Phase 94: Fix stale primary_bank_id references from banks liquidated
        // in previous turns (before the lifecycle fix was added). These
        // companies have primary_bank_id pointing to a non-existent bank.
        // Their available_cash is M1 (not in M0) but should be M0 (unbanked).
        // Clear primary_bank_id and create CB injection to cover the deposit.
        let valid_bank_ids: std::collections::HashSet<String> = companies
            .iter()
            .filter(|c| c.sector == Sector::Banking)
            .map(|c| c.id.clone())
            .collect();
        let mut stale_deposit_guarantee: f64 = 0.0;
        for company in companies.iter_mut() {
            if company.sector != Sector::Banking {
                if let Some(ref bid) = company.primary_bank_id {
                    if !valid_bank_ids.contains(bid) {
                        let deposit = company.available_cash
                            + company
                                .brokerage_account
                                .as_ref()
                                .map(|ba| ba.cash)
                                .unwrap_or(0.0)
                            + company.rd_budget
                            + company.debit_cash;
                        if deposit > 0.0 {
                            stale_deposit_guarantee += deposit;
                        }
                        company.primary_bank_id = None;
                    }
                }
            }
        }
        if stale_deposit_guarantee > 0.0 {
            country.central_bank.liquidity_injected += stale_deposit_guarantee;
        }

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
            // Phase 33: Grace period for new companies — a firm must first
            // operate a full year before a loss streak can liquidate it.
            // Without this, the per-turn records would execute companies for
            // ~6 weeks of startup losses during market ramp-up, which
            // mass-liquidates half the corporate sector and destroys its
            // labor demand before recovery can occur.
            const LOSS_STREAK_GRACE_TURNS: u32 =
                crate::state::macro_data::TURNS_PER_YEAR as u32;
            if current_turn.saturating_sub(company.founded_turn) < LOSS_STREAK_GRACE_TURNS {
                continue;
            }
            if company.financial_history.len() < 3 {
                continue;
            }
            // Macro-Remediation: banks are supervised through the reserve
            // floor + ELA path, not industrial P&L. A bank's interest/fee
            // income is not booked through `last_profit`, so its recorded
            // net_profit is structurally negative and the 3-strike rule
            // would liquidate the entire banking sector every ~3 turns.
            // Insolvency still applies via the company_capital < 0 check.
            if company.sector == Sector::Banking {
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

        #[cfg(feature = "diagnostic")]
        {
            let neg_equity = companies
                .iter()
                .filter(|c| c.company_capital < 0.0)
                .count();
            eprintln!(
                "LIQCYCLE: year={} total={} to_remove={} neg_equity={}",
                _year,
                companies.len(),
                to_remove.len(),
                neg_equity
            );
            let mut by_sector: std::collections::BTreeMap<String, (usize, f64, f64, f64, f64)> =
                std::collections::BTreeMap::new();
            for &idx in &to_remove {
                let c = &companies[idx];
                let e = by_sector
                    .entry(format!("{:?}", c.sector))
                    .or_default();
                e.0 += 1;
                e.1 += c.target_fte_demand as f64;
                e.2 += c.fulfilled_fte as f64;
                e.3 += c
                    .financial_history
                    .last()
                    .and_then(|r| r.get("net_profit"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                e.4 += c
                    .financial_history
                    .last()
                    .and_then(|r| r.get("revenue"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
            }
            for (s, (n, d, f, np, rev)) in &by_sector {
                eprintln!(
                    "  LIQCYCLE_SECTOR: {} n={} demand={:.0} fulfilled={:.0} last_profit={:.0} last_rev={:.0}",
                    s, n, d, f, np, rev
                );
            }
        }

        // Phase 96: Use the Syndic as the universal liquidation path.
        let domestic_currency = country.macro_indicators.currency.clone();
        let policy = crate::state::BankruptcyPolicy::with_defaults();

        for idx in to_remove.into_iter().rev() {
            // We need to liquidate companies[idx] but also pass the rest of
            // companies as the bank/creditor slice. Split the vector.
            let mut company_to_liquidate = companies.remove(idx);

            // Blueprint 007-FIX: Fire on_cooperative_liquidated event hook
            // BEFORE the Syndic executes liquidation. This transitions the
            // cooperative to Liquidated stage in the CooperativeRegistry and
            // collects displaced members for homeless state assignment.
            // The event-based cache is updated ONLY on create/liquidate —
            // no per-turn O(N) company scanning (Rule: PERFORMANCE).
            if company_to_liquidate
                .legal_form
                .is_housing_legal_form()
            {
                let current_turn = _year;
                let displaced = crate::entities::legal_form::on_cooperative_liquidated(
                    &mut country.cooperative_registry,
                    &company_to_liquidate.id,
                    current_turn,
                );
                // Create HomelessState entries for each displaced member.
                // Liquid asset estimates are stocks sized in annual-wage
                // units — annualize the per-turn average_wage.
                let avg_wage = country.macro_indicators.average_wage
                    * crate::state::macro_data::TURNS_PER_YEAR as f64;
                for (member_id, wealth_tier) in &displaced {
                    let liquid = match wealth_tier {
                        crate::society::housing::WealthTier::Upper => avg_wage * 100.0,
                        crate::society::housing::WealthTier::Middle => avg_wage * 20.0,
                        crate::society::housing::WealthTier::Working => avg_wage * 5.0,
                        crate::society::housing::WealthTier::Destitute => avg_wage * 0.5,
                    };
                    let mut homeless = crate::society::housing::HomelessState::new(
                        member_id.clone(),
                        company_to_liquidate.id.clone(),
                        current_turn,
                        *wealth_tier,
                        liquid,
                    );
                    // Set region_id for rehousing vacancy search
                    homeless.region_id = company_to_liquidate.region_id.clone();
                    country.cooperative_registry.homeless.push(homeless);
                }
            }

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

            // Phase 94: When a bank is liquidated, clear primary_bank_id for
            // all remaining companies that used this bank. Their deposits are
            // no longer backed by bank reserves — they become unbanked and
            // their available_cash is M0 (physical fiat). Without this,
            // stale primary_bank_id references cause M0 conservation violations
            // in justice system freezes, tax collection, and other operations
            // that skip bank reserve debits for "banked" companies whose bank
            // no longer exists.
            // Since the available_cash was M1 (backed by bank reserves, not
            // in M0) and is now M0 (unbanked, in M0), we must create CB
            // injection equal to the deposit amount to keep M0 conservation
            // balanced. The bank's reserves were already seized and
            // distributed to creditors during liquidation — the deposits
            // are now backed by CB liquidity (lender of last resort).
            if company_to_liquidate.sector == crate::registries::enums::Sector::Banking {
                let liquidated_bank_id = &company_to_liquidate.id;
                let mut total_deposit_guarantee: f64 = 0.0;
                for remaining in companies.iter_mut() {
                    if remaining.primary_bank_id.as_ref() == Some(liquidated_bank_id) {
                        let deposit = remaining.available_cash
                            + remaining
                                .brokerage_account
                                .as_ref()
                                .map(|ba| ba.cash)
                                .unwrap_or(0.0)
                            + remaining.rd_budget
                            + remaining.debit_cash;
                        if deposit > 0.0 {
                            total_deposit_guarantee += deposit;
                        }
                        remaining.primary_bank_id = None;
                    }
                }
                if total_deposit_guarantee > 0.0 {
                    country.central_bank.liquidity_injected += total_deposit_guarantee;
                }
            }

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
        current_turn: u32,
        registries: &crate::registries::Registries,
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

        // Phase 94: M0 conservation — debit actual citizen savings (in M0 walk)
        // instead of the private_capital tracking field (NOT in M0 walk).
        // New company cash is unbanked M0, so the funding source must also be
        // M0. Pro-rata debit across all class demographics by savings share.
        let total_citizen_savings: f64 = country
            .regions
            .iter()
            .flat_map(|r| {
                r.class_demographics
                    .rural_classes
                    .values()
                    .chain(r.class_demographics.urban_classes.values())
            })
            .map(|d| d.savings)
            .sum();
        if total_citizen_savings < investment {
            return;
        }

        // Anchor lifecycle startups to the region with the largest idle labor
        // pool. `Company::new`/`Building::new` leave `region_id` empty, and
        // labor-market clearing filters strictly on
        // `company.region_id == region.id` — a regionless company can never
        // hire and bleeds its capital as a zero-FTE shell.
        let Some(spawn_region_id) = country
            .regions
            .iter()
            .map(|r| {
                let idle_fte: f64 = r
                    .class_demographics
                    .rural_classes
                    .values()
                    .chain(r.class_demographics.urban_classes.values())
                    .map(|d| (d.available_fte - d.allocated_fte - d.guild_fte_allocated).max(0.0))
                    .sum();
                (idle_fte, r.id.clone())
            })
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(_, id)| id)
        else {
            return;
        };

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
            let building_id = format!("BLD_{}_{}_{}", country.name, year, i);
            let legal_form = LegalForm::FamilyBusiness(FamilyBusinessData {
                dynasty_id: None,
                successor_generation: 0,
                family_retained_share: 1.0,
                heir_vip_ids: Vec::new(),
                succession_crisis: false,
            });
            let mut new_company = Company::new(
                company_id.clone(),
                format!("New Company {}-{}", year, i),
                sector,
                legal_form,
                capital_per_company * 0.5,
                capital_per_company * 0.5,
                100,
            );
            new_company.region_id = spawn_region_id.clone();
            new_company.building_ids.push(building_id.clone());
            new_company.founded_turn = current_turn;

            let mut new_building = Building::new(building_id, company_id, sector, 100);
            new_building.region_id = spawn_region_id.clone();
            new_building.cluster_info.region_id = spawn_region_id.clone();
            new_building.year_built = year;

            // W7: A lifecycle spawn is born with `Building::new`'s empty
            // active_method — resolve_active_method assigns one lazily at the
            // first production pass, but nothing ever stocks the inputs, so
            // the plant binds at ratio 0 and immediately furloughs the
            // workers it just hired. Resolve the era-legal sector method and
            // seed the standard input buffer here so the building is viable
            // from birth (formation capital, same convention as worldgen).
            let sector_key =
                crate::engine::generator::corporate::sector_json_name(sector);
            if let Some(pm) = registries
                .production_methods
                .get(&sector_key)
                .and_then(|m| {
                    m.iter_production_slots()
                        .filter(|pm| pm.year <= year)
                        .max_by_key(|pm| pm.year)
                })
            {
                new_building.active_method.year = pm.year;
                new_building.active_method.experts_ratio = pm.experts_ratio;
                new_building.active_method.skilled_ratio = pm.skilled_ratio;
                new_building.active_method.basic_ratio = pm.basic_ratio;
                new_building.active_method.efficiency = pm.efficiency;
                new_building.active_method.inputs =
                    pm.inputs.iter().map(|(&k, &v)| (k, v)).collect();
                new_building.active_method.outputs =
                    pm.outputs.iter().map(|(&k, &v)| (k, v)).collect();
                new_building.active_method.thermal_efficiency = pm.thermal_efficiency;
                new_building.active_method.storage_efficiency = pm.storage_efficiency;
                new_building.active_method.emission_factor = pm.emission_factor;
                new_building.active_method.biohazard_factor = pm.biohazard_factor;
                new_building.active_method.output_water_quality = pm.output_water_quality;
                new_building.active_method.discharge_quality = pm.discharge_quality;
                new_building.active_method.seat_type = pm.seat_type;
                let (seeded, _cost) =
                    crate::engine::generator::corporate::seed_inventory(
                        &new_building.active_method,
                        new_building.worker_capacity,
                        sector,
                    );
                let mut seed_total = 0.0;
                for (commodity, qty) in seeded {
                    seed_total += qty;
                    *new_building.inventory.entry(commodity).or_insert(0.0) += qty;
                }
                // The seed must fit — otherwise the hard-cap overflow
                // fallback destroys it on the first production pass.
                new_building.inventory_capacity =
                    new_building.inventory_capacity.max(seed_total * 2.0);
            }

            companies.push(new_company);
            buildings.push(new_building);

            country.budget.private_capital -= capital_per_company;

            // Phase 94: Pro-rata debit citizen savings to fund the new company.
            // This preserves M0: citizen savings (M0) → unbanked company cash (M0).
            // Only debit the liquid portion (50% of capital_per_company) because
            // only liquid_capital goes to brokerage_account.cash (counted in M0
            // for unbanked companies). The fixed_capital portion goes to
            // company_capital, which is NOT in the M0 walk.
            let liquid_portion = capital_per_company * 0.5;
            let mut remaining = liquid_portion;
            for region in &mut country.regions {
                if remaining <= 0.0 {
                    break;
                }
                let classes: Vec<&mut crate::society::geography::ClassDemographics> = region
                    .class_demographics
                    .rural_classes
                    .values_mut()
                    .chain(region.class_demographics.urban_classes.values_mut())
                    .collect();
                for cls in classes {
                    if remaining <= 0.0 {
                        break;
                    }
                    let share = cls.savings / total_citizen_savings;
                    let debit = (liquid_portion * share).min(cls.savings).min(remaining);
                    if debit > 0.0 {
                        cls.savings -= debit;
                        remaining -= debit;
                    }
                }
            }
            // If any residual remains (rounding), debit from the first class
            // with sufficient savings.
            if remaining > 0.0 {
                for region in &mut country.regions {
                    if remaining <= 0.0 {
                        break;
                    }
                    let classes: Vec<&mut crate::society::geography::ClassDemographics> = region
                        .class_demographics
                        .rural_classes
                        .values_mut()
                        .chain(region.class_demographics.urban_classes.values_mut())
                        .collect();
                    for cls in classes {
                        if remaining <= 0.0 {
                            break;
                        }
                        let debit = cls.savings.min(remaining);
                        if debit > 0.0 {
                            cls.savings -= debit;
                            remaining -= debit;
                        }
                    }
                }
            }
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
            0,
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
            0,
            &crate::registries::Registries::native_only(),
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
            0,
            &crate::registries::Registries::native_only(),
        );

        assert!(companies.is_empty());
    }

    /// Regression: lifecycle-spawned companies must be anchored to a region.
    /// `Company::new`/`Building::new` leave `region_id` empty and labor-market
    /// clearing filters strictly on `company.region_id == region.id`, so a
    /// regionless startup can never hire. The spawn must pick the region with
    /// the largest idle labor pool and link building ↔ company both ways.
    #[test]
    fn test_spawn_assigns_region_and_links_building() {
        use crate::society::geography::{
            ClassDemographics, Region, RegionalClassDemographics, UrbanClass,
        };
        use crate::state::treasury::SectorShare;
        use serde_json::{Map, Value};
        use std::collections::BTreeMap;

        let mut companies = Vec::new();
        let mut buildings = Vec::new();
        let mut country = Country::mock_for_tests();
        country.name = "Test".to_string();
        country.budget.private_capital = 1_000_000.0;

        // One promising sector (PMI > 50) so the spawn path proceeds.
        let mut extra = Map::new();
        extra.insert("pmi".to_string(), Value::from(55.0));
        country.budget.sectors.insert(
            Sector::Agriculture,
            SectorShare {
                gdp_share: 0.1,
                crisis_vulnerability: None,
                active_method: None,
                extra,
            },
        );

        // Two regions: R-1 nearly fully employed, R-2 mostly idle. Startups
        // must choose R-2 (the largest idle labor pool).
        let make_region = |id: &str, idle_fte: f64, savings: f64| Region {
            id: id.to_string(),
            owner_country: "Test".to_string(),
            class_demographics: RegionalClassDemographics {
                urban_classes: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        UrbanClass::Worker,
                        ClassDemographics {
                            population: 1000,
                            savings,
                            available_fte: idle_fte,
                            allocated_fte: 0.0,
                            ..Default::default()
                        },
                    );
                    m
                },
                ..Default::default()
            },
            ..Default::default()
        };
        country.regions.push(make_region("R-1", 10.0, 500_000.0));
        country.regions.push(make_region("R-2", 5_000.0, 500_000.0));

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
            0,
            &crate::registries::Registries::native_only(),
        );

        assert!(!companies.is_empty(), "expected lifecycle spawn");
        assert_eq!(companies.len(), buildings.len());
        for company in &companies {
            assert!(company.id.starts_with("NEW_"));
            assert_eq!(company.region_id, "R-2");
            assert_eq!(company.building_ids.len(), 1);
        }
        for building in &buildings {
            assert_eq!(building.region_id, "R-2");
            assert_eq!(building.cluster_info.region_id, "R-2");
            assert_eq!(building.year_built, 2024);
            let owner = companies
                .iter()
                .find(|c| c.id == building.owner_id)
                .unwrap();
            assert!(owner.building_ids.contains(&building.id));
        }
    }

    /// Regression: lifecycle-spawned companies must stamp `founded_turn` with
    /// the simulation turn, not the calendar `year`. Stamping the year (e.g.
    /// 1925) makes `current_turn - founded_turn` underflow/saturate and marks
    /// every spawn as either newborn or impossibly old, defeating the
    /// loss-streak grace gate.
    #[test]
    fn test_spawn_founded_turn_uses_current_turn() {
        use crate::society::geography::{
            ClassDemographics, Region, RegionalClassDemographics, UrbanClass,
        };
        use crate::state::treasury::SectorShare;
        use serde_json::{Map, Value};
        use std::collections::BTreeMap;

        let mut companies = Vec::new();
        let mut buildings = Vec::new();
        let mut country = Country::mock_for_tests();
        country.name = "Test".to_string();
        country.budget.private_capital = 1_000_000.0;

        let mut extra = Map::new();
        extra.insert("pmi".to_string(), Value::from(55.0));
        country.budget.sectors.insert(
            Sector::Agriculture,
            SectorShare {
                gdp_share: 0.1,
                crisis_vulnerability: None,
                active_method: None,
                extra,
            },
        );

        country.regions.push(Region {
            id: "R-1".to_string(),
            owner_country: "Test".to_string(),
            class_demographics: RegionalClassDemographics {
                urban_classes: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        UrbanClass::Worker,
                        ClassDemographics {
                            population: 1000,
                            savings: 500_000.0,
                            available_fte: 5_000.0,
                            allocated_fte: 0.0,
                            ..Default::default()
                        },
                    );
                    m
                },
                ..Default::default()
            },
            ..Default::default()
        });

        let market_signal = MarketSignal {
            interest_rate: 0.05,
            sector_pmi: HashMap::default(),
            demand_surplus: HashMap::default(),
            global_surplus: HashMap::default(),
            prices: HashMap::default(),
            stock_confidence: 50.0,
            stock_index: 1000.0,
        };

        let spawn_turn = 7u32;
        CompanyLifecycle::spawn_new_companies(
            &mut companies,
            &mut buildings,
            &mut country,
            2024,
            &market_signal,
            spawn_turn,
            &crate::registries::Registries::native_only(),
        );

        assert!(!companies.is_empty(), "expected lifecycle spawn");
        for company in &companies {
            assert_eq!(company.founded_turn, spawn_turn);
        }
    }

    /// Regression: a young company inside the startup grace window must not be
    /// liquidated by the sustained-loss rule, even with a full history of
    /// consecutive losses. Without the age gate the rule fired after ~2-3
    /// startup turns and mass-liquidated half the corporate sector.
    #[test]
    fn test_loss_streak_grace_protects_young_company() {
        let legal_form = LegalForm::FamilyBusiness(FamilyBusinessData::default());
        let mut companies = vec![Company::new(
            "young_loss".to_string(),
            "Young Loss Co".to_string(),
            Sector::LocalServices,
            legal_form,
            1000.0,
            0.0,
            100,
        )];
        companies[0].founded_turn = 10;
        companies[0].company_capital = 500.0;
        companies[0].financial_history = (0..5)
            .map(|_| serde_json::json!({"net_profit": -100.0, "revenue": 0.0}))
            .collect();

        let mut buildings = Vec::new();
        let mut country = Country::mock_for_tests();
        country.name = "Test".to_string();

        // Age = 17 turns < TURNS_PER_YEAR: inside the grace window.
        CompanyLifecycle::liquidate_bankrupt_companies(
            &mut companies,
            &mut buildings,
            &mut country,
            2024,
            27,
        );

        assert_eq!(companies.len(), 1, "young company must survive loss streak");
    }

    /// Regression: once a company is older than the grace window, three
    /// consecutive loss records still trigger liquidation.
    #[test]
    fn test_loss_streak_liquidates_old_company() {
        let legal_form = LegalForm::FamilyBusiness(FamilyBusinessData::default());
        let mut companies = vec![Company::new(
            "old_loss".to_string(),
            "Old Loss Co".to_string(),
            Sector::LocalServices,
            legal_form,
            1000.0,
            0.0,
            100,
        )];
        companies[0].founded_turn = 0;
        companies[0].company_capital = 500.0;
        companies[0].financial_history = (0..3)
            .map(|_| serde_json::json!({"net_profit": -100.0, "revenue": 0.0}))
            .collect();
        companies[0].building_ids.push("bld_old".to_string());

        let mut buildings = vec![Building::new(
            "bld_old".to_string(),
            "old_loss".to_string(),
            Sector::LocalServices,
            100,
        )];
        buildings[0].current_employment = 10;

        let mut country = Country::mock_for_tests();
        country.name = "Test".to_string();

        let grace_turn = crate::state::macro_data::TURNS_PER_YEAR as u32;
        CompanyLifecycle::liquidate_bankrupt_companies(
            &mut companies,
            &mut buildings,
            &mut country,
            2024,
            grace_turn,
        );

        assert!(
            companies.is_empty(),
            "company past grace with 3 loss records must liquidate"
        );
    }
}

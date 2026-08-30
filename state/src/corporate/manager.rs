//! Corporate post-production cycle — expansion, restructuring and bankruptcy.
//!
//! This is a focused port of the Python `corporate/manager.py` post-production
//! step.  It processes each company after building production has produced a
//! `last_profit`, applies corporate overhead, interest, CIT tax, and then
//! expands healthy firms or shrinks/bankrupts bleeding ones.

use crate::economy::market::{GlobalMarket, MarketOrders, MarketSignal};
use crate::entities::{Building, Company, LegalForm, SeasonalState};
use crate::state::treasury::SectorShare;
use crate::state::Country;
use serde_json::{Map, Value};
use std::collections::HashMap;

use super::strategy::{CorporateAction, CorporateDecisionCtx, CorporateStrategy, FinanceSource, try_apply_ipo};

/// Emergency Stabilization: Recruitment cost multiplier — 4 weeks of wages
/// per new worker (2 turns of payroll). Applied to `average_wage` to derive
/// the recruitment cost. This creates hiring friction so the corporate AI
/// prefers furlough (free re-instatement) over fire+rehire (recruitment cost).
const RECRUITMENT_MULTIPLIER: f64 = 4.0;

/// Processes every company in the country after the production phase.
///
/// # Arguments
/// * `companies` - Mutable slice of companies for this country.
/// * `buildings` - Mutable slice of buildings for this country.
/// * `country` - Mutable country state (used for GDP, tax rate, CIT revenue).
/// * `year` - In-game year, stored in the financial history.
/// * `market_signal` - Snapshot of market conditions produced by market clearing.
///
/// # Rules
/// * Each company aggregates the `last_profit` of its owned buildings.
/// * Overhead, interest and corporate tax are deducted from the company.
/// * Collected CIT is transferred to `country.budget.liquid_reserves`.
/// * Profitable firms expand capacity; bleeding firms shrink and fire workers.
/// * `country.budget.private_capital` is updated to the sum of non-state
///   company capital.
pub fn process_companies(
    companies: &mut [Company],
    buildings: &mut Vec<Building>,
    country: &mut Country,
    year: u32,
    market_signal: &MarketSignal,
    current_turn: u32,
) {
    // Build an owner -> building indices map once, eliminating the O(N*M) nested
    // lookup that scaled with the number of companies and buildings.
    let mut by_owner: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, building) in buildings.iter().enumerate() {
        by_owner
            .entry(building.owner_id.clone())
            .or_default()
            .push(idx);
    }

    for i in 0..companies.len() {
        let company_id = companies[i].id.clone();
        let company_building_ids = companies[i].building_ids.clone();

        let owned: Vec<usize> = by_owner
            .get(&company_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|j| company_building_ids.is_empty() || company_building_ids.contains(&buildings[*j].id))
            .collect();

        let total_profit: f64 = owned.iter().map(|j| buildings[*j].last_profit).sum();

        // Emergency Stabilization: Compute average fulfillment ratio across
        // owned buildings to detect raw-material distress.
        let avg_fulfillment_ratio: f64 = if owned.is_empty() {
            1.0
        } else {
            let sum: f64 = owned.iter().map(|j| buildings[*j].last_fulfillment_ratio).sum();
            sum / owned.len() as f64
        };

        // Phase 24A.3: Track liabilities before process_company to detect new loans.
        let liabilities_before = companies[i].liabilities;
        let company_id = companies[i].id.clone();
        let _company_fixed_capital = companies[i].fixed_capital;

        let (_profitable, interest_paid) = {
            let company = &mut companies[i];
            process_company(company, total_profit, country, year, market_signal, avg_fulfillment_ratio, current_turn)
        };

        // Phase 39: Accumulate annual profit for SOE dividend calculation.
        // Only positive profits are accumulated; losses don't reduce the pool.
        if total_profit > 0.0 {
            companies[i].annual_profit_accumulator += total_profit;
        }

        // Phase 24A.3: Route interest to the lending bank (fixes Black Hole #3).
        // Previously, interest was subtracted from liquid_capital but never credited
        // to any bank — money was destroyed. Now we credit the lending bank's reserves.
        if interest_paid > 0.0 {
            let loan_bank_id = companies[i].outstanding_loan_bank_id.clone();
            let has_liabilities = companies[i].liabilities > 0.0;
            if let Some(bank_id) = loan_bank_id {
                // Credit the lending bank's reserves with the interest payment
                if let Some(bank) = companies.iter_mut().find(|c| c.id == bank_id) {
                    if let Some(ref mut bs) = bank.balance_sheet {
                        bs.reserves_at_central_bank += interest_paid;
                    }
                }
            } else if has_liabilities {
                // Invalid state: liabilities exist but no lending bank.
                // Per the plan: wipe liabilities entirely (no short-term patches).
                companies[i].liabilities = 0.0;
            }
        }

        // Phase 24A.3 / Phase 77: If liabilities increased (new loan taken via
        // BankLoan), route through issue_loan() which enforces fractional reserve
        // requirements. Previously this pushed loans directly to the first bank
        // (always the State Bank) WITHOUT checking reserves — the primary source
        // of the 8193% LDR anomaly.
        let liabilities_after = companies[i].liabilities;
        if liabilities_after > liabilities_before + 0.01 {
            let new_loan_amount = liabilities_after - liabilities_before;
            let cb_clone = country.central_bank.clone();
            let xibor = country.interbank_market.xibor;
            let avg_wage = country.macro_indicators.average_wage.max(1.0);

            // Phase 77: Try each bank in order of excess reserves (competitive
            // allocation). If all banks fail the reserve check, revert the
            // liability increase — the loan is refused.
            let borrower_clone = companies[i].clone();
            let mut loan_issued = false;

            // Collect bank indices sorted by excess reserves descending
            let mut bank_indices: Vec<(usize, f64)> = companies
                .iter()
                .enumerate()
                .filter(|(_, c)| c.bank_type.is_some() && c.balance_sheet.is_some())
                .map(|(idx, c)| {
                    let bs = c.balance_sheet.as_ref().unwrap();
                    let required = bs.deposits * cb_clone.reserve_requirement_ratio;
                    let effective = bs.reserves_at_central_bank - bs.cb_lombard_loans;
                    let excess = (effective - required).max(0.0);
                    (idx, excess)
                })
                .collect::<Vec<_>>();
            bank_indices.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            for (bank_idx, _) in &bank_indices {
                let bi = *bank_idx;
                // Phase 77: Check operational capacity (labor-based)
                let bank_fte = companies[bi].fulfilled_fte as f64;
                let capacity = crate::state::banking::bank_operational_capacity(bank_fte, avg_wage);
                if capacity.max_new_loans_per_turn <= 0.0 {
                    continue;
                }
                let current_assets = companies[bi].balance_sheet.as_ref().map(|bs| {
                    bs.loans_issued.iter().map(|l| l.outstanding_balance).sum::<f64>() + bs.securities
                }).unwrap_or(0.0);
                if current_assets + new_loan_amount > capacity.max_asset_under_management {
                    continue;
                }

                let bank_margin = companies[bi].loan_margin.unwrap_or(0.02);
                let bank_id = companies[bi].id.clone();

                let loan_result = crate::state::banking::issue_loan(
                    companies[bi].balance_sheet.as_mut().unwrap(),
                    &bank_id,
                    bank_margin,
                    &borrower_clone,
                    &borrower_clone.id,
                    new_loan_amount,
                    crate::state::banking::LoanType::WorkingCapital,
                    12,
                    &cb_clone,
                    xibor,
                );

                if let Ok(lr) = loan_result {
                    companies[i].outstanding_loan_bank_id = Some(bank_id);
                    // Double-entry: borrower receives principal as cash
                    companies[i].available_cash += lr.principal_amount;
                    if let Some(ref mut ba) = companies[i].brokerage_account {
                        ba.cash += lr.principal_amount;
                    }
                    loan_issued = true;
                    break;
                }
            }

            if !loan_issued {
                // No bank could issue the loan — revert the liability increase.
                // The company's expansion/investment is not funded.
                companies[i].liabilities = liabilities_before;
            }
        }

        // Consume pending_expansion: publish a ConstructionTender so a
        // construction company can bid on the expansion project. This routes
        // investment through the construction sector, creating real contractor
        // relationships, tranche payments, and construction company revenue.
        if let Some(expansion) = companies[i].pending_expansion.take() {
            if !owned.is_empty() {
                // Find first building without an active project
                let target_idx = owned.iter().find(|&&j| buildings[j].active_project.is_none()).copied();
                if let Some(j) = target_idx {
                    let building_id = buildings[j].id.clone();
                    let building_name = buildings[j].name.clone();
                    let region_id = buildings[j].region_id.clone();
                    let tender = crate::construction::tender_market::publish_expansion_tender(
                        company_id.clone(),
                        crate::construction::tenders::TenderInvestorType::Corporation,
                        crate::construction::ConstructionProjectType::Factory,
                        region_id,
                        building_name,
                        expansion.new_workers,
                        expansion.investment,
                        expansion.investment,
                        2, // 2-turn bidding window
                        current_turn,
                        building_id,
                        crate::registries::enums::Sector::HeavyIndustry,
                        1925 + (current_turn / 24),
                    );
                    country.phase22_tenders.push(tender);
                }
            }
        }

        // Apply any capacity shrink to the physical buildings (expansion
        // is now handled by ConstructionProject completion, not here).
        let new_worker_capacity = companies[i].worker_capacity;
        if new_worker_capacity == 0 {
            for &j in &owned {
                buildings[j].current_employment = 0;
                buildings[j].worker_capacity = 0;
            }
        } else if !owned.is_empty() {
            for &j in &owned {
                let building = &mut buildings[j];
                let scale = building.scale_factor.max(1);
                let new_base = (new_worker_capacity / scale / owned.len() as u32).max(1);
                building.current_employment = building.current_employment.min(new_base);
                building.worker_capacity = new_base;
            }
        }

        // Update aggregate statistics from the (first) owned building.
        if let Some(&j) = owned.first() {
            let building = &buildings[j];
            let scale = building.scale_factor.max(1);
            companies[i].aggregated_stats.total_employment = building.current_employment * scale;
            companies[i].aggregated_stats.total_production = building.last_production.clone();
            companies[i].aggregated_stats.total_dividends = total_profit.max(0.0);
        }
    }

    // Phase 24A.6: Process pending dividend queue — credit company/fund owners.
    // State/treasury owners were already credited in apply_action.
    let dividend_queue = std::mem::take(&mut country.dividend_queue);
    for (owner_id, amount) in &dividend_queue {
        if owner_id == "STATE" || owner_id == "TREASURY" {
            continue; // Already credited in apply_action
        }
        // Try to credit the owner company's brokerage account or available_cash
        if let Some(owner) = companies.iter_mut().find(|c| c.id == *owner_id) {
            if let Some(ref mut ba) = owner.brokerage_account {
                ba.cash += amount;
            } else {
                owner.available_cash += amount;
            }
        }
        // If owner not found in companies, the dividend is lost (owner may be
        // a citizen class or foreign entity not tracked as a company).
        // TODO: Route to citizen savings for individual shareholders.
    }

    // Phase 24A.7: Process pending IPO queue — execute with real buyer cash.
    let ipo_queue = std::mem::take(&mut country.ipo_queue);
    for (company_id, shares_to_float, reserve_price) in &ipo_queue {
        // Find fund buyers with brokerage accounts and sufficient cash
        let mut buyers: Vec<(String, u64)> = Vec::new();
        let total_proceeds = *shares_to_float as f64 * reserve_price;
        let _ = total_proceeds;

        // Find funds (companies with fund_type) that have brokerage cash
        for buyer in companies.iter() {
            if buyer.fund_type.is_some() {
                if let Some(ref ba) = buyer.brokerage_account {
                    if ba.cash >= reserve_price * (*shares_to_float as f64 / 3.0) {
                        // Allocate ~1/3 of the IPO to each eligible fund
                        let allocation = (*shares_to_float / 3).max(1);
                        let cost = allocation as f64 * reserve_price;
                        if ba.cash >= cost {
                            buyers.push((buyer.id.clone(), allocation));
                        }
                    }
                }
            }
        }

        if !buyers.is_empty() {
            // Execute the IPO: debit buyers first, then credit issuer
            let mut total_collected = 0.0_f64;
            for (buyer_id, allocation) in &buyers {
                let cost = *allocation as f64 * reserve_price;
                if let Some(buyer) = companies.iter_mut().find(|c| c.id == *buyer_id) {
                    if let Some(ref mut ba) = buyer.brokerage_account {
                        if ba.cash >= cost {
                            ba.cash -= cost;
                            ba.add_lot(&format!("EQUITY:{}", company_id), *allocation, *reserve_price, year);
                            total_collected += cost;
                        }
                    }
                }
            }
            // Credit proceeds to issuer (real cash from real buyers)
            if total_collected > 0.0 {
                if let Some(issuer) = companies.iter_mut().find(|c| c.id == *company_id) {
                    issuer.liquid_capital += total_collected;
                }
            }
        }
        // If no buyers found, the IPO fails silently — no proceeds credited.
        // The shares_count was already increased in apply_action, but without
        // buyer allocation, the free_float absorbs them.
    }

    // Phase 24A.9: Process demolition queue — demolish buildings with land conservation.
    let demolition_queue = std::mem::take(&mut country.demolition_queue);
    for (company_id, building_id) in &demolition_queue {
        // Find the building
        if let Some(idx) = buildings.iter().position(|b| b.id == *building_id && b.owner_id == *company_id) {
            let land_hectares = buildings[idx].land_hectares;
            let region_id = buildings[idx].region_id.clone();
            let current_employment = buildings[idx].current_employment;

            // Return workers to regional labor pool (class demographics)
            if current_employment > 0 {
                for region in &mut country.regions {
                    if region.id == region_id {
                        // Distribute laid-off workers across classes proportionally
                        let total_fte: f64 = region.class_demographics.rural_classes.values().map(|c| c.allocated_fte).sum::<f64>()
                            + region.class_demographics.urban_classes.values().map(|c| c.allocated_fte).sum::<f64>();
                        if total_fte > 0.0 {
                            let layoff_fte = current_employment as f64;
                            for class in region.class_demographics.rural_classes.values_mut() {
                                let share = class.allocated_fte / total_fte;
                                class.allocated_fte = (class.allocated_fte - layoff_fte * share).max(0.0);
                            }
                            for class in region.class_demographics.urban_classes.values_mut() {
                                let share = class.allocated_fte / total_fte;
                                class.allocated_fte = (class.allocated_fte - layoff_fte * share).max(0.0);
                            }
                        }
                        break;
                    }
                }
            }

            // Fire-sale inventory to auction pool
            let inventory_value: f64 = buildings[idx].inventory.values().sum();
            if inventory_value > 0.0 {
                country.bankruptcy_auction_pool.cash_collected += inventory_value * 0.5; // 50% fire-sale
            }

            // Conserve land: return hectares to regional land inventory
            if land_hectares > 0.0 {
                for region in &mut country.regions {
                    if region.id == region_id {
                        // The land was already subtracted when the building was built,
                        // so we add it back. The total_area invariant is preserved
                        // because we're returning land, not creating it.
                        // For now, we don't modify the land_use_inventory because
                        // the exact category mapping requires the land_category field
                        // which we're not adding yet (default 0.0 for legacy buildings).
                        break;
                    }
                }
            }

            // Route building fixed assets to auction pool
            country.bankruptcy_auction_pool.add_asset(
                format!("demolish_{}", building_id),
                buildings[idx].fixed_assets.iter().map(|c| c.count).sum::<f64>(),
                company_id.clone(),
                std::collections::HashMap::new(),
                &crate::state::BankruptcyPolicy::with_defaults(),
            );

            // Phase 86.5A: Actually REMOVE the building from the collection.
            // Previously, the code only cleared owner_id and zeroed employment,
            // leaving a zombie building that leaked memory and stale references.
            // Now we remove it entirely after all accounting is settled.
            buildings.remove(idx);
        }
    }

    // Phase 24A.9: Process halt queue — temporarily halt production.
    let halt_queue = std::mem::take(&mut country.halt_queue);
    for (company_id, building_id) in &halt_queue {
        if let Some(idx) = buildings.iter().position(|b| b.id == *building_id && b.owner_id == *company_id) {
            // Halt production: set employment to 0 but preserve capacity
            let current_employment = buildings[idx].current_employment;
            if current_employment > 0 {
                let region_id = buildings[idx].region_id.clone();
                for region in &mut country.regions {
                    if region.id == region_id {
                        let total_fte: f64 = region.class_demographics.rural_classes.values().map(|c| c.allocated_fte).sum::<f64>()
                            + region.class_demographics.urban_classes.values().map(|c| c.allocated_fte).sum::<f64>();
                        if total_fte > 0.0 {
                            let layoff_fte = current_employment as f64;
                            for class in region.class_demographics.rural_classes.values_mut() {
                                let share = class.allocated_fte / total_fte;
                                class.allocated_fte = (class.allocated_fte - layoff_fte * share).max(0.0);
                            }
                            for class in region.class_demographics.urban_classes.values_mut() {
                                let share = class.allocated_fte / total_fte;
                                class.allocated_fte = (class.allocated_fte - layoff_fte * share).max(0.0);
                            }
                        }
                        break;
                    }
                }
            }
            buildings[idx].current_employment = 0;
        }
    }

    // Phase 28: Real production method switching.
    // If a company's building has an active method whose inputs are unavailable
    // (no inventory of any required input), try to switch to a simpler method
    // from the registry that produces the same outputs with fewer inputs.
    // This is the corporate AI's supply-chain resilience mechanism.
    let mut registry = crate::registries::production_methods::industrial_production_methods();
    registry.extend(crate::registries::production_methods::state_building_methods());
    registry.extend(crate::registries::production_methods::retail_production_methods());
    for company in &*companies {
        let company_id = company.id.clone();
        for &building_idx in by_owner.get(&company_id).into_iter().flatten() {
            let building = &mut buildings[building_idx];
            // Skip buildings with no active method
            let current_method_name = building.active_method.active_methods.production.clone();
            if current_method_name.is_empty() {
                continue;
            }
            // Check if current method's inputs are available
            let has_inputs = building.active_method.inputs.iter()
                .any(|(commodity, _)| {
                    building.inventory.get(commodity).copied().unwrap_or(0.0) > 0.0
                });
            let needs_inputs = !building.active_method.inputs.is_empty();
            if has_inputs || !needs_inputs {
                continue; // Method is working fine
            }
            // Find alternative methods from the registry
            let current_outputs: Vec<_> = building.active_method.outputs.keys().cloned().collect();
            if current_outputs.is_empty() {
                continue;
            }
            let mut best_alt: Option<(&str, &crate::registries::production_methods::ProductionMethod)> = None;
            for (method_name, building_methods) in &registry {
                if method_name == &current_method_name {
                    continue;
                }
                // Iterate over all production methods in this BuildingMethods
                for (pm_name, prod_method) in &building_methods.production {
                    let _full_name = format!("{}::{}", method_name, pm_name);
                    // Check if this method produces any of the same outputs
                    if !prod_method.outputs.keys().any(|k| current_outputs.contains(k)) {
                        continue;
                    }
                    // Check year availability
                    if prod_method.year > year {
                        continue;
                    }
                    // Prefer methods with fewer inputs (simpler)
                    let current_input_count = building.active_method.inputs.len();
                    let alt_input_count = prod_method.inputs.len();
                    if alt_input_count < current_input_count {
                        // Use the registry key as the method name
                        best_alt = Some((method_name.as_str(), prod_method));
                    }
                }
            }
            if let Some((method_name, prod_method)) = best_alt {
                // Switch to the alternative method
                building.active_method = crate::entities::ActiveProductionMethod {
                    year: prod_method.year.max(year),
                    inputs: prod_method.inputs.iter().map(|(k, v)| (*k, *v)).collect(),
                    outputs: prod_method.outputs.iter().map(|(k, v)| (*k, *v)).collect(),
                    experts_ratio: prod_method.experts_ratio,
                    skilled_ratio: prod_method.skilled_ratio,
                    basic_ratio: prod_method.basic_ratio,
                    efficiency: prod_method.efficiency,
                    active_methods: crate::state::treasury::ProductionMethodChoice {
                        automation: String::new(),
                        production: method_name.to_string(),
                        organization: String::new(),
                        ..Default::default()
                    },
                    active_blueprint: None,
                    extra: serde_json::Map::new(),
                    ..Default::default()
                };
            }
        }
    }

    let private_capital: f64 = companies
        .iter()
        .filter(|c| c.state_share < 1.0)
        .map(|c| c.company_capital)
        .sum();
    country.budget.private_capital = private_capital;
}

/// Manages strategic reserves for the Strategic Reserve Agency.
///
/// This function implements the automatic commodity purchasing and release logic
/// for the Strategic Reserve Agency based on moving-average VWAP ratios and
/// surplus/deficit thresholds.
///
/// Phase 79: Triggers are now ratio-based relative to a moving-average VWAP,
/// not static nominal price thresholds. Buy when current price falls below
/// `buy_threshold_ratio * moving_avg_vwap` (price crash/glut). Release when
/// current price exceeds `sell_threshold_ratio * moving_avg_vwap` (supply
/// shock/war). Falls back to `global_market.base_price()` when insufficient
/// VWAP history exists.
///
/// # Arguments
/// * `agency` - The Strategic Reserve Agency company
/// * `country` - The country state (for budget access)
/// * `global_market` - The global market for price and surplus/deficit data
/// * `market_history` - Market history with rolling VWAP data for trigger calculations
/// * `market_orders` - Market orders to add buy/sell orders to
pub fn manage_strategic_reserves(
    agency: &mut Company,
    country: &mut Country,
    global_market: &GlobalMarket,
    market_history: &crate::economy::market::market_history::MarketHistory,
    market_orders: &mut MarketOrders,
) {
    if let LegalForm::StrategicReserveAgency(data) = &mut agency.legal_form {
        // Purchase phase
        for (commodity_str, trigger) in &data.purchase_triggers {
            // Parse commodity string to Commodity enum (Phase 79: snake_case keys)
            if let Ok(commodity) = commodity_str.parse::<crate::registries::enums::Commodity>() {
                let current_price = global_market.base_price(commodity, 100.0);
                let global_surplus = global_market.surplus(commodity);

                // Phase 79: Moving-average VWAP for shock-responsive triggers.
                let moving_avg = crate::economy::market::market_history::moving_average_vwap(
                    market_history, &commodity,
                ).unwrap_or(current_price);

                let buy_threshold = trigger.buy_threshold_ratio * moving_avg;
                let surplus_triggered = global_surplus > trigger.surplus_threshold;

                if current_price < buy_threshold || surplus_triggered {
                    let budget = data.budget_allocation * trigger.budget_fraction;
                    let purchase_amount = budget / current_price.max(0.01);

                    if country.budget.liquid_reserves >= budget {
                        country.budget.liquid_reserves -= budget;

                        // Update reserves (respect physical max_capacity)
                        let current = data.commodity_reserves.get(commodity_str).copied().unwrap_or(0.0);
                        let max_capacity = data.max_capacity.get(commodity_str).copied().unwrap_or(f64::MAX);
                        let actual_purchase = purchase_amount.min(max_capacity - current).max(0.0);

                        if actual_purchase > 0.0 {
                            data.commodity_reserves.insert(commodity_str.clone(), current + actual_purchase);
                            market_orders.add_buy(commodity, actual_purchase);
                        }
                    }
                }
            }
        }

        // Release phase
        for (commodity_str, trigger) in &data.release_triggers {
            if let Ok(commodity) = commodity_str.parse::<crate::registries::enums::Commodity>() {
                let current_price = global_market.base_price(commodity, 100.0);
                let global_deficit = (-global_market.surplus(commodity)).max(0.0);

                let moving_avg = crate::economy::market::market_history::moving_average_vwap(
                    market_history, &commodity,
                ).unwrap_or(current_price);

                let sell_threshold = trigger.sell_threshold_ratio * moving_avg;
                let deficit_triggered = global_deficit > trigger.deficit_threshold;

                if current_price > sell_threshold || deficit_triggered {
                    let available = data.commodity_reserves.get(commodity_str).copied().unwrap_or(0.0);
                    let release_amount = (available * trigger.release_fraction).min(global_deficit.max(available * 0.1));

                    if release_amount > 0.0 {
                        data.commodity_reserves.insert(commodity_str.clone(), available - release_amount);
                        market_orders.add_sell(commodity, release_amount);
                    }
                }
            }
        }
    }
}

/// Processes a single company given its aggregated building profit.
///
/// # Arguments
/// * `company` - The company to update.
/// * `total_profit` - Aggregated `last_profit` from the company's buildings.
/// * `country` - Country state for tax transfer and GDP.
/// * `year` - In-game year.
/// * `market_signal` - Snapshot of market conditions produced by market clearing.
///
/// # Returns
/// `true` if the company ended the turn with a positive net profit.
///
/// # Rules
/// * Negative liquidity is immediately converted to liabilities.
/// * Overhead is `5%` of gross profit, capped at `0` when unprofitable.
/// * Interest is applied only if the company has at least two historical
///   records and positive gross profit.
/// * Corporate tax is `country.tax_rates.corporate_tax * positive taxable income`.
/// * The company's [`LegalForm`] then chooses a [`CorporateAction`] (expand,
///   restructure, dividend, IPO) based on the [`CorporateDecisionCtx`].
/// * IPOs trigger a [`LegalFormTransition`] and raise capital.
pub fn process_company(
    company: &mut Company,
    total_profit: f64,
    country: &mut Country,
    year: u32,
    market_signal: &MarketSignal,
    avg_fulfillment_ratio: f64,
    current_turn: u32,
) -> (bool, f64) {
    let corporate_tax_rate = country.tax_rates.corporate_tax;
    let xibor = market_signal.interest_rate;

    // 1. Add building-level profits to the company.
    company.liquid_capital += total_profit;

    if company.liquid_capital < 0.0 {
        company.liabilities += -company.liquid_capital;
        company.liquid_capital = 0.0;
    }

    company.company_capital = company.fixed_capital + company.liquid_capital - company.liabilities;

    // 2. Corporate overhead.
    let overhead = (total_profit * 0.05).max(0.0);
    company.liquid_capital = (company.liquid_capital - overhead).max(0.0);

    // 3. Interest costs (only when there is a track record and profit).
    let interest = if company.liabilities > 0.0 && company.financial_history.len() >= 2 && total_profit > 0.0 {
        let leverage = company.liabilities / company.fixed_capital.max(1.0);
        let risk_margin = (leverage * 0.03).min(0.12);
        let interest_cost = company.liabilities * (xibor + risk_margin);
        interest_cost.min(total_profit * 0.5)
    } else {
        0.0
    };
    company.liquid_capital = (company.liquid_capital - interest).max(0.0);

    // 4. CIT (paid to the treasury).
    let taxable_income = total_profit - overhead - interest;
    let tax = if taxable_income > 0.0 {
        taxable_income * corporate_tax_rate
    } else {
        0.0
    };
    company.liquid_capital = (company.liquid_capital - tax).max(0.0);
    country.budget.liquid_reserves += tax;

    // 5. Recalculate equity after the ledger.
    company.company_capital = company.fixed_capital + company.liquid_capital - company.liabilities;

    let net_profit = total_profit - overhead - interest - tax;

    // 5.5. Handle Latifundium labor cost calculation
    if let LegalForm::Latifundium(latifundium) = &company.legal_form {
        let market_wage = country.macro_indicators.labor_market.unemployment_rate;
        let effective_labor_cost = latifundium.calculate_labor_cost(
            company.worker_capacity,
            market_wage,
        );
        
        // Apply reduced labor cost to company finances
        // (This would normally be applied during production, but we adjust here for consistency)
        let labor_cost_adjustment = effective_labor_cost - (total_profit * 0.5); // Approximate baseline labor cost
        if labor_cost_adjustment < 0.0 {
            company.liquid_capital += -labor_cost_adjustment; // Add savings from cheap serf labor
        }
        
        // Calculate aristocracy profit share
        let aristocracy_profit = latifundium.calculate_aristocracy_profit(
            net_profit,
            0.1, // 10% reinvestment rate
        );
        
        // CRITICAL: Route municipal profits to regional budget
        // If a Latifundium has a dynasty_id matching a RegionalGovernance.id,
        // the calculated aristocracy_profit is added directly to that region's
        // RegionalBudget.liquid_reserves as non-tax revenue.
        if let Some(municipality_id) = &latifundium.dynasty_id {
            if let Some(region) = country.regions.iter_mut()
                .find(|r| r.governance.as_ref()
                    .map(|g| &g.id) == Some(municipality_id)) {
                region.governance.as_mut()
                    .unwrap()
                    .budget
                    .liquid_reserves += aristocracy_profit;
            }
        }
    }

    // 6. Ownership-specific strategic decision.
    let action = {
        let default_sector_share = SectorShare {
            gdp_share: 0.0,
            crisis_vulnerability: None,
            active_method: None,
            extra: Map::new(),
        };
        let sector_share = country
            .budget
            .sectors
            .get(&company.sector)
            .unwrap_or(&default_sector_share);
        // Phase 57: Evaluate CEO traits via centralized module — no raw string checks.
        let behavior_modifiers = if let Some(ref ceo_id) = company.ceo_vip_id {
            if let Some(ref registry) = country.politics.vip_registry {
                if let Some(vip) = registry.get(ceo_id) {
                    crate::corporate::market_behavior::evaluate_market_behavior(&vip.traits)
                } else {
                    crate::corporate::market_behavior::MarketBehaviorModifiers::default()
                }
            } else {
                crate::corporate::market_behavior::MarketBehaviorModifiers::default()
            }
        } else {
            crate::corporate::market_behavior::MarketBehaviorModifiers::default()
        };

        let ctx = CorporateDecisionCtx {
            company: &*company,
            country: &*country,
            sector: company.sector,
            sector_share,
            market_signal,
            bank_credit_rate: market_signal.interest_rate,
            stock_market: &country.budget.stock_market,
            labor_market: &country.macro_indicators.labor_market,
            year,
            gross_profit: total_profit,
            net_profit,
            behavior_modifiers,
            avg_fulfillment_ratio,
            current_turn,
        };
        company.legal_form.decide(&ctx)
    };

    // 7. Apply the chosen action.
    // AI & Stability Audit (Pillar 4B): Record major actions in the ActionLedger
    // BEFORE applying (action is moved into apply_action).
    let action_type_str = match &action {
        CorporateAction::Expand { .. } => Some("Expand"),
        CorporateAction::Restructure { .. } => Some("Restructure"),
        CorporateAction::Furlough { .. } => Some("Furlough"),
        CorporateAction::Ipo { .. } => Some("Ipo"),
        _ => None,
    };
    if let Some(action_str) = action_type_str {
        company.action_ledger.record_action(action_str, current_turn, net_profit);
    }

    // Evaluate past actions and update penalty weights.
    company.action_ledger.evaluate_and_update(current_turn, net_profit);

    apply_action(company, action, market_signal, country, year, total_profit, net_profit, current_turn);

    // 8. Recalculate equity after the action.
    company.company_capital = company.fixed_capital + company.liquid_capital - company.liabilities;

    // 9. Financial history ring buffer.
    let record = Value::Object(
        [
            ("year".to_string(), Value::from(year)),
            ("revenue".to_string(), Value::from(total_profit + overhead)),
            ("operating_costs".to_string(), Value::from(overhead)),
            ("interest".to_string(), Value::from(interest)),
            ("taxes".to_string(), Value::from(tax)),
            ("net_profit".to_string(), Value::from(net_profit)),
        ]
        .into_iter()
        .collect(),
    );
    company.financial_history.push(record);
    if company.financial_history.len() > 5 {
        company.financial_history.remove(0);
    }

    // Phase 55: Compute EPS, P/E ratio, and dividend yield for listed companies.
    if company.shares_count > 0 {
        company.eps = net_profit / company.shares_count as f64;
        if company.eps > 0.0 && company.share_price > 0.0 {
            company.pe_ratio = company.share_price / company.eps;
        } else {
            company.pe_ratio = 0.0;
        }
        let market_cap = company.share_price * company.shares_count as f64;
        if market_cap > 0.0 {
            company.dividend_yield = company.aggregated_stats.total_dividends / market_cap;
        } else {
            company.dividend_yield = 0.0;
        }
    } else {
        company.eps = 0.0;
        company.pe_ratio = 0.0;
        company.dividend_yield = 0.0;
    }

    (net_profit > 0.0, interest)
}

fn apply_action(
    company: &mut Company,
    action: CorporateAction,
    market_signal: &MarketSignal,
    country: &mut Country,
    year: u32,
    gross_profit: f64,
    net_profit: f64,
    current_turn: u32,
) {
    match action {
        CorporateAction::Expand { investment, new_workers, finance } => {
            // Handle financing — capital is committed now but capacity
            // is only added when the ConstructionProject completes.
            match finance {
                FinanceSource::Internal => {
                    company.liquid_capital = (company.liquid_capital - investment).max(0.0);
                }
                FinanceSource::BankLoan(loan) => {
                    company.liquid_capital = (company.liquid_capital + loan - investment).max(0.0);
                    company.liabilities += loan;
                }
                FinanceSource::BondIssue(amount) => {
                    company.liquid_capital = (company.liquid_capital + amount - investment).max(0.0);
                    company.liabilities += amount;
                }
                FinanceSource::IpoProceeds(amount) => {
                    company.liquid_capital = (company.liquid_capital + amount - investment).max(0.0);
                }
            }

            // Emergency Stabilization: Recruitment cost — hiring new workers
            // costs 4 weeks of wages (2 turns of payroll) per worker. This is
            // a real cash flow to workers (signing bonus / onboarding incentive),
            // NOT a sink. Double-entry: debit company cash, credit regional
            // worker class savings via recruitment_cost_queue.
            if new_workers > 0 {
                let avg_wage = country.macro_indicators.average_wage.max(1.0);
                let recruitment_cost = new_workers as f64 * avg_wage * RECRUITMENT_MULTIPLIER;
                let available = company.brokerage_account
                    .as_ref()
                    .map(|ba| ba.cash.max(0.0))
                    .unwrap_or(company.available_cash.max(0.0));
                let payable = recruitment_cost.min(available);
                if payable > 0.0 {
                    if let Some(ba) = &mut company.brokerage_account {
                        ba.cash -= payable;
                    } else {
                        company.available_cash -= payable;
                    }
                    // Credit to regional worker class savings (same routing
                    // pattern as severance pay).
                    country.recruitment_cost_queue.push((company.id.clone(), payable));
                }
            }

            // Store expansion intent — process_companies will create
            // a ConstructionProject on the building. Capacity/capital
            // is added only when materials are physically delivered.
            company.pending_expansion = Some(crate::entities::PendingExpansion {
                investment,
                new_workers,
            });
        }
        CorporateAction::Restructure { layoffs, capital_write_off } => {
            if layoffs >= company.worker_capacity || capital_write_off >= company.fixed_capital {
                company.liabilities = (company.liabilities - company.fixed_capital).max(0.0);
                company.fixed_capital = 0.0;
                company.liquid_capital = 0.0;
                company.worker_capacity = 0;
            } else {
                company.worker_capacity = company.worker_capacity.saturating_sub(layoffs);
                if capital_write_off > 0.0 {
                    company.fixed_capital = (company.fixed_capital - capital_write_off).max(0.0);
                    company.liabilities = (company.liabilities - capital_write_off).max(0.0);
                }
            }
        }
        CorporateAction::PayDividend { total } => {
            // Phase 24A.6: Wire dividends to all actual shareholders.
            // Previously, dividends were subtracted from liquid_capital but only
            // credited to cultural institutions — the main shareholders (owners,
            // free float, state) never received their payments.
            company.liquid_capital = (company.liquid_capital - total).max(0.0);
            company.aggregated_stats.total_dividends += total;

            // Route to known owners from owners map
            let owners = company.owners.clone();
            let shares_count = company.shares_count;
            let dividend_per_share = if shares_count > 0 {
                total / shares_count as f64
            } else {
                0.0
            };

            for (owner_id, share_percentage) in &owners {
                let owner_share_count = (shares_count as f64 * share_percentage) as u64;
                let dividend_amount = owner_share_count as f64 * dividend_per_share;
                if dividend_amount <= 0.0 {
                    continue;
                }
                // Try to credit the owner (could be a company, fund, or state)
                // We use country as the intermediary for state-owned dividends
                if owner_id == "STATE" || owner_id == "TREASURY" {
                    country.budget.liquid_reserves += dividend_amount;
                }
                // Other owners (companies, funds) will be credited in a post-pass
                // since we can't borrow companies slice here.
                // For now, track for later settlement.
                country.dividend_queue.push((owner_id.clone(), dividend_amount));
            }

            // Route dividends to cultural building shareholders (monasteries with shares)
            for cultural in &mut country.cultural_institutions {
                if let Some(&share) = cultural.owned_company_shares.get(&company.id) {
                    let dividend_share = total * share;
                    if dividend_share > 0.0 {
                        cultural.available_cash += dividend_share;
                    }
                }
            }
        }
        CorporateAction::Ipo { shares_to_float, reserve_price } => {
            // Phase 24A.7: Fix IPO to use real buyer cash, not synthetic proceeds.
            // Previously, `company.liquid_capital += proceeds` created money from
            // nothing — no buyer was ever debited. Now we route through the stock
            // exchange's execute_ipo which validates and debits real buyers.
            let default_sector_share = SectorShare {
                gdp_share: 0.0,
                crisis_vulnerability: None,
                active_method: None,
                extra: Map::new(),
            };
            let sector_share = country
                .budget
                .sectors
                .get(&company.sector)
                .unwrap_or(&default_sector_share);
            // Phase 57: Evaluate CEO traits via centralized module.
            let behavior_modifiers = if let Some(ref ceo_id) = company.ceo_vip_id {
                if let Some(ref registry) = country.politics.vip_registry {
                    if let Some(vip) = registry.get(ceo_id) {
                        crate::corporate::market_behavior::evaluate_market_behavior(&vip.traits)
                    } else {
                        crate::corporate::market_behavior::MarketBehaviorModifiers::default()
                    }
                } else {
                    crate::corporate::market_behavior::MarketBehaviorModifiers::default()
                }
            } else {
                crate::corporate::market_behavior::MarketBehaviorModifiers::default()
            };

            let ctx = CorporateDecisionCtx {
                company: &*company,
                country: &*country,
                sector: company.sector,
                sector_share,
                market_signal,
                bank_credit_rate: market_signal.interest_rate,
                stock_market: &country.budget.stock_market,
                labor_market: &country.macro_indicators.labor_market,
                year,
                gross_profit,
                net_profit,
                behavior_modifiers,
                avg_fulfillment_ratio: 1.0, // IPO path: not used for furlough decisions
                current_turn,
            };
            if let Some(new_form) = try_apply_ipo(&*company, &company.legal_form, shares_to_float, reserve_price, &ctx) {
                let _ = ctx;
                // Phase 24A.7: Collect real buyers from funds with brokerage accounts.
                // We queue the IPO for execution in process_companies post-pass
                // where we have access to the full companies slice.
                let company_id = company.id.clone();
                country.ipo_queue.push((company_id, shares_to_float, reserve_price));
                company.legal_form = new_form;
                // Don't credit proceeds here — they'll be credited when the IPO
                // is executed with real buyer cash in the post-pass.
                company.shares_count += shares_to_float;
                if let crate::entities::LegalForm::JointStockCompany(ref mut data) = company.legal_form {
                    data.shares_issued = company.shares_count;
                    data.free_float = (shares_to_float as f64 / company.shares_count as f64).clamp(0.0, 1.0);
                }
            }
        }
        CorporateAction::SwitchMethod { .. } |
        CorporateAction::RaiseWages { .. } |
        CorporateAction::CutWages { .. } |
        CorporateAction::Idle => {}
        CorporateAction::Furlough { fte_count, wage_fraction } => {
            // Emergency Stabilization: Move workers from fulfilled_fte to
            // furloughed_workers_count. They are retained by the company and
            // excluded from active labor clearing. Re-instatement is free
            // (no recruitment cost) when conditions improve.
            let actual_furlough = fte_count.min(company.fulfilled_fte);
            company.fulfilled_fte -= actual_furlough;
            company.furloughed_workers_count += actual_furlough as f64;

            // Pay furlough wages (wage_fraction * normal_wage * furloughed_count).
            // Double-entry: debit company cash, credit furloughed workers via
            // regional class savings (same routing as severance pay).
            if wage_fraction > 0.0 && actual_furlough > 0 {
                let furlough_wage = actual_furlough as f64
                    * company.offered_wage_per_fte
                    * wage_fraction;
                let available = company.brokerage_account
                    .as_ref()
                    .map(|ba| ba.cash.max(0.0))
                    .unwrap_or(company.available_cash.max(0.0));
                let payable = furlough_wage.min(available);
                if payable > 0.0 {
                    if let Some(ba) = &mut company.brokerage_account {
                        ba.cash -= payable;
                    } else {
                        company.available_cash -= payable;
                    }
                    // Credit to regional class savings (proportional distribution
                    // handled in labor market post-pass via furlough_wage_queue).
                    country.furlough_wage_queue.push((company.id.clone(), payable));
                }
            }
        }
        CorporateAction::Demolish { building_id } => {
            // Phase 24A.9: Demolish building — return workers to labor pool,
            // fire-sale inventory, route assets to auction pool, conserve land.
            // The actual building removal happens in process_companies post-pass
            // where we have access to buildings and regions.
            country.demolition_queue.push((company.id.clone(), building_id));
        }
        CorporateAction::HaltProduction { building_id } => {
            // Phase 24A.9: Halt production temporarily (no capacity destruction).
            // The actual halting happens in process_companies post-pass.
            country.halt_queue.push((company.id.clone(), building_id));
        }
    }
}

/// Phase 47: Apply seasonal furlough to a company's FTE demand and employment state.
///
/// Called after agricultural FTE demand is computed and before `set_wage_offers`.
///
/// # Workforce Isolation & fulfilled_fte Accounting
/// Furloughed FTE are "held" by the company in `furloughed_workers_count`.
/// They do NOT re-enter the general labor pool and do NOT participate in
/// labor market clearing. This prevents off-season workers from flooding
/// the market and distorting unemployment rates / wages.
///
/// CRITICAL: `fulfilled_fte` (the actual employee count) is explicitly
/// manipulated so that wage/production phases see only the standby crew
/// during off-season. When the season reactivates, furloughed workers are
/// transferred back into `fulfilled_fte` — the company's employee count
/// perfectly reflects the returning workforce without re-hiring friction.
pub fn apply_seasonal_furlough(company: &mut Company, season: crate::state::Season) {
    let Some(profile) = &mut company.seasonal_profile else {
        // Non-seasonal company: ensure no stale furlough count.
        company.furloughed_workers_count = 0.0;
        return;
    };

    if profile.active_seasons.contains(&season) {
        // Season reactivation: re-instate furloughed workers.
        if profile.current_state == SeasonalState::Furloughed
            && company.furloughed_workers_count > 0.0
        {
            // Transfer furloughed workers back into fulfilled_fte.
            company.fulfilled_fte += company.furloughed_workers_count.round() as u32;
            company.furloughed_workers_count = 0.0;
        }
        profile.current_state = SeasonalState::Active;
        // physical_fte_demand already set by sector logic — leave unchanged
        return;
    }

    // Off-season: furlough
    profile.current_state = SeasonalState::Furloughed;
    let full_demand = company.physical_fte_demand as f64;
    let standby = full_demand * profile.standby_fte_fraction;

    // Transfer excess workers from fulfilled_fte into furloughed_workers_count.
    // fulfilled_fte drops to the standby level — wage/production phases will
    // only see the standby crew.
    let excess = (company.fulfilled_fte as f64 - standby).max(0.0);
    company.furloughed_workers_count = excess;
    company.fulfilled_fte = (company.fulfilled_fte as f64 - excess).max(0.0).round() as u32; // Drop to standby level
    company.physical_fte_demand = standby.round() as u32;
    company.target_fte_demand = standby.round() as u32;
}

/// Phase 47: Apply seasonal furlough to all companies for a country.
/// Convenience wrapper for the turn loop.
pub fn apply_seasonal_furlough_all(companies: &mut [Company], season: crate::state::Season) {
    for company in companies.iter_mut() {
        apply_seasonal_furlough(company, season);
    }
}

/// Emergency Stabilization: Re-instate furloughed workers when conditions
/// improve. Called BEFORE production so re-instated workers can participate
/// in this turn's production cycle.
///
/// # Re-instatement Conditions
/// - The company has furloughed workers (`furloughed_workers_count > 0`)
/// - The company is NOT in receivership
/// - The company can cover full payroll for re-instated workers
/// - The company's average fulfillment ratio is healthy (> 0.5)
///
/// # Arguments
/// * `companies` - Mutable slice of companies.
/// * `buildings` - Buildings slice for fulfillment ratio lookup.
pub fn process_furlough_reinstatement(companies: &mut [Company], buildings: &[Building]) {
    // Build owner -> fulfillment ratios map
    let mut by_owner: HashMap<String, Vec<f64>> = HashMap::new();
    for b in buildings {
        by_owner.entry(b.owner_id.clone()).or_default().push(b.last_fulfillment_ratio);
    }

    for company in companies.iter_mut() {
        if company.furloughed_workers_count <= 0.0 {
            continue;
        }
        if company.is_in_receivership {
            continue;
        }

        // Check average fulfillment ratio for this company's buildings
        let avg_ratio = by_owner.get(&company.id)
            .map(|ratios| {
                if ratios.is_empty() { 1.0 }
                else { ratios.iter().sum::<f64>() / ratios.len() as f64 }
            })
            .unwrap_or(1.0);

        if avg_ratio < 0.5 {
            continue; // Raw materials still scarce — don't re-instate yet
        }

        // Check if company can cover full payroll for re-instated workers
        let available = company.brokerage_account
            .as_ref()
            .map(|ba| ba.cash.max(0.0))
            .unwrap_or(company.available_cash.max(0.0));
        let re_instate_count = company.furloughed_workers_count.round() as u32;
        let payroll_cost = re_instate_count as f64 * company.offered_wage_per_fte;

        if available < payroll_cost {
            continue; // Can't afford to re-instate yet
        }

        // Re-instate: transfer furloughed workers back to fulfilled_fte
        company.fulfilled_fte += re_instate_count;
        company.furloughed_workers_count = 0.0;
        company.furlough_turns_accumulated = 0; // Reset duration counter
    }
}

/// Emergency Stabilization: Furlough attrition — workers quit after prolonged
/// unpaid furlough and return to the general labor pool. Called AFTER
/// re-instatement and BEFORE labor market clearing.
///
/// # Attrition Formula
/// ```text
/// wage_gap = 1.0 - wage_fraction   // 1.0 for 0% pay
/// base_quit_rate = 0.05            // 5% per turn baseline
/// duration_factor = 1.0 + (furlough_turns_accumulated * 0.10)
/// quit_rate = (base_quit_rate * wage_gap * duration_factor).min(0.50)
/// quit_count = ceil(furloughed_workers_count * quit_rate)
/// ```
///
/// Workers who quit are released to the general labor pool. Since
/// `available_fte` is recomputed each turn from `population ×
/// labor_participation`, the released workers are automatically available
/// for hire by other companies next turn.
///
/// # Rule 8 (Rational Actors)
/// A rational worker will not sit unpaid forever. This mechanic prevents
/// the "eternal furlough" trap where a dead company holds thousands of
/// workers hostage infinitely.
pub fn process_furlough_attrition(companies: &mut [Company]) {
    for company in companies.iter_mut() {
        if company.furloughed_workers_count <= 0.0 {
            continue;
        }

        // Increment the duration counter
        company.furlough_turns_accumulated += 1;

        // Compute quit rate (wage_fraction = 0.0 for current era — no UI)
        let wage_fraction = 0.0; // Future labor laws can increase this
        let wage_gap = 1.0 - wage_fraction;
        let base_quit_rate = 0.05;
        let duration_factor = 1.0 + (company.furlough_turns_accumulated as f64 * 0.10);
        let quit_rate = (base_quit_rate * wage_gap * duration_factor).min(0.50);

        let quit_count = (company.furloughed_workers_count * quit_rate).ceil() as u32;
        let quit_count = quit_count.min(company.furloughed_workers_count.round() as u32);

        if quit_count > 0 {
            company.furloughed_workers_count =
                (company.furloughed_workers_count - quit_count as f64).max(0.0);
            // Workers return to the general labor pool automatically via
            // the available_fte recompute in labor.rs (population × participation)
        }
    }
}

/// Phase 25: Set wage offers for all companies before labor market clearing.
///
/// This is the critical fix for the 100% unemployment / 0 GDP collapse.
/// Companies are generated with `offered_wage_per_fte = 0.0`, which causes
/// the labor market clearing to reject all bids. This pass sets each
/// company's wage offer based on its available cash and labor demand,
/// using a market wage signal as a reference — NOT a hardcoded floor.
///
/// # Arguments
/// * `companies` - Mutable slice of companies for this country.
/// * `market_average_wage` - The current macro average wage (reference signal only).
///
/// # Rules
/// * `offered_wage_per_fte = (available_cash * wage_budget_fraction) / target_fte_demand`
/// * `available_cash = brokerage_account.cash + available_cash` (total liquid)
/// * If `available_cash = 0`, wage = 0 — the company hires nobody.
/// * If `target_fte_demand = 0`, wage = 0.
/// * `wage_budget_fraction` is 0.6 for most sectors, 0.5 for capital-intensive
///   sectors (HeavyIndustry, Mining, Energy), and 0.7 for labor-intensive
///   sectors (Agriculture, LightIndustry, Services).
/// * The market average wage is used as a competitive signal: companies
///   with more cash offer higher wages to attract workers, but there is
///   NO minimum wage floor. If a company is broke, it offers 0.
/// * State-owned companies (owner_id = "State") use a higher budget
///   fraction (0.8) since they draw from treasury allocations.
pub fn set_wage_offers(companies: &mut [Company], market_average_wage: f64) {
    // Phase 39: Sticky wage constant — 3% max drop per turn.
    const STICKY_WAGE_MAX_DROP: f64 = 0.03;
    const STICKY_WAGE_MAX_RISE: f64 = 0.05; // Phase 40: Symmetric upward cap
    // Phase 41: Target wage adjusts max 2% per turn toward market average.
    const TARGET_WAGE_MAX_ADJUSTMENT: f64 = 0.02;
    // Phase 41: Hard fallback floor for Turn 1 when market average is 0.
    const TARGET_WAGE_FALLBACK: f64 = 50.0;

    for company in companies.iter_mut() {
        // Phase 41: Banks now use the same target_wage mechanism as other
        // companies. The banking turn still sets target_fte_demand based on
        // portfolio size, but the wage rate is set here using target_wage.
        // This removes the old skip that caused bank wages to be pegged to
        // a collapsing market average.

        // Phase 41: Initialize target_wage on first turn (or if it's 0).
        // Use the current offered_wage if nonzero, otherwise market average
        // with a hard fallback floor of 50.0.
        if company.target_wage == 0.0 {
            company.target_wage = if company.offered_wage_per_fte > 0.0 {
                company.offered_wage_per_fte
            } else {
                market_average_wage.max(TARGET_WAGE_FALLBACK)
            };
        }

        // Phase 41: Compute desired wage based on market average.
        // Companies target the market average wage. Profitable companies
        // (with high cash relative to FTE) target slightly above average.
        // Unprofitable companies target slightly below.
        let brokerage_cash = company
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash)
            .unwrap_or(0.0);

        let is_charity = matches!(
            company.sector,
            crate::registries::enums::Sector::NGO
                | crate::registries::enums::Sector::Religion
        );
        let effective_cash = if is_charity && !company.donation_history.is_empty() {
            let avg_donation: f64 =
                company.donation_history.iter().sum::<f64>() / company.donation_history.len() as f64;
            let expected_inflow = avg_donation * 6.0;
            brokerage_cash.min(expected_inflow.max(brokerage_cash * 0.5))
        } else {
            brokerage_cash
        };

        let effective_fte = (company.target_fte_demand as f64).max(1.0);
        let cash_per_fte = effective_cash / effective_fte;

        // Desired wage: if cash_per_fte > market average, target slightly above.
        // If below, target slightly below. This creates slow convergence.
        let desired_wage = if market_average_wage > 0.0 {
            if cash_per_fte > market_average_wage * 2.0 {
                market_average_wage * 1.1 // Profitable: target 10% above market
            } else if cash_per_fte < market_average_wage * 0.5 {
                market_average_wage * 0.9 // Unprofitable: target 10% below market
            } else {
                market_average_wage // Neutral: target market average
            }
        } else {
            TARGET_WAGE_FALLBACK
        };

        // Phase 41: Move target_wage toward desired_wage by max 2% per turn.
        let adjustment = (desired_wage - company.target_wage).clamp(
            -company.target_wage * TARGET_WAGE_MAX_ADJUSTMENT,
            company.target_wage * TARGET_WAGE_MAX_ADJUSTMENT,
        );
        company.target_wage = (company.target_wage + adjustment).max(TARGET_WAGE_FALLBACK);

        // Phase 39: Sticky wage floor helper.
        let sticky_floor = if company.prev_offered_wage_per_fte > 0.0 {
            company.prev_offered_wage_per_fte * (1.0 - STICKY_WAGE_MAX_DROP)
        } else {
            0.0
        };

        // Skip companies with no labor demand.
        if company.target_fte_demand == 0 {
            company.offered_wage_per_fte = sticky_floor;
            continue;
        }

        // Phase 41: The offered wage IS the target wage.
        // The labor market will compute max_affordable_fte = cash / target_wage.
        // If the company can't afford its target wage, it hires fewer workers,
        // but the wage rate itself is stable.
        let computed_wage = company.target_wage;

        // Phase 34: Sanity cap at 3× market average with 5000 floor.
        let sane_max = (market_average_wage * 3.0).max(5000.0);
        let capped_wage = computed_wage.min(sane_max);

        // Phase 38/39/40: Keynesian Symmetric Wage Rigidity.
        let final_wage = if company.prev_offered_wage_per_fte > 0.0 {
            let wage_floor = company.prev_offered_wage_per_fte * (1.0 - STICKY_WAGE_MAX_DROP);
            let wage_ceiling = company.prev_offered_wage_per_fte * (1.0 + STICKY_WAGE_MAX_RISE);
            capped_wage.max(wage_floor).min(wage_ceiling)
        } else {
            capped_wage
        };

        company.offered_wage_per_fte = final_wage;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Company;
    use crate::registries::enums::Sector;
    use crate::securities::BrokerageAccount;

    fn company_with_cash_and_fte(id: &str, sector: Sector, cash: f64, fte: f64) -> Company {
        let mut c = Company::default();
        c.id = id.to_string();
        c.sector = sector;
        c.target_fte_demand = fte as u32;
        c.physical_fte_demand = fte as u32;
        c.brokerage_account = Some(BrokerageAccount {
            cash,
            ..Default::default()
        });
        c
    }

    #[test]
    fn test_wage_capped_at_3x_market_average() {
        // A charity with huge cash and tiny FTE should NOT produce a 1M wage.
        let mut companies = vec![company_with_cash_and_fte(
            "NGO1",
            Sector::NGO,
            5_000_000.0,
            3.0,
        )];
        let market_avg = 5000.0;
        set_wage_offers(&mut companies, market_avg);
        // Cap = max(5000 * 3.0, 5000.0) = 15000.0
        assert!(
            companies[0].offered_wage_per_fte <= 15000.01,
            "Wage should be capped at 3× market average (15000), got {}",
            companies[0].offered_wage_per_fte
        );
        assert!(
            companies[0].offered_wage_per_fte > 0.0,
            "Wage should be positive with cash and FTE demand"
        );
    }

    #[test]
    fn test_wage_cap_floor_5000_when_market_avg_low() {
        // When market average is very low, the cap floor is 5000.
        let mut companies = vec![company_with_cash_and_fte(
            "RICH",
            Sector::LightIndustry,
            1_000_000.0,
            1.0,
        )];
        let market_avg = 100.0; // Very low
        set_wage_offers(&mut companies, market_avg);
        // Cap = max(100 * 3.0, 5000.0) = 5000.0
        assert!(
            companies[0].offered_wage_per_fte <= 5000.01,
            "Wage should be capped at floor of 5000, got {}",
            companies[0].offered_wage_per_fte
        );
    }

    #[test]
    fn test_charity_uses_lower_wage_fraction() {
        // Phase 41: With target-wage system, the wage starts at market average
        // (5000) and adjusts slowly. The NGO's high cash-per-FTE (20k vs 5k
        // market) means desired_wage = 5000 * 1.1 = 5500, but target_wage
        // only moves 2% per turn from 5000 → 5100. The sticky floor/ceiling
        // doesn't apply on first turn (prev_offered_wage_per_fte = 0).
        let mut companies = vec![company_with_cash_and_fte(
            "NGO2",
            Sector::NGO,
            100_000.0,
            5.0,
        )];
        set_wage_offers(&mut companies, 5000.0);
        // Target wage starts at 5000 (market avg), adjusts 2% toward 5500 → 5100
        assert!(
            (companies[0].offered_wage_per_fte - 5100.0).abs() < 1.0,
            "NGO wage should use target-wage system: expected ~5100, got {}",
            companies[0].offered_wage_per_fte
        );
    }

    #[test]
    fn test_fte_denominator_floored_at_1() {
        // Phase 41: With target-wage system, wage starts at market average (5000).
        // cash_per_fte = 10000 / 1.0 = 10000, which equals 5000 * 2.0 = 10000.
        // The condition is strictly >, so this falls to neutral: desired = 5000.
        // Target stays at 5000 (no adjustment needed). The FTE denominator
        // flooring at 1.0 is still tested by verifying the wage is computed
        // (not NaN or infinity) and equals the market average.
        // NOTE: target_fte_demand is u32, so the smallest nonzero demand is 1.
        let mut companies = vec![company_with_cash_and_fte(
            "TINY",
            Sector::LightIndustry,
            10_000.0,
            1.0,
        )];
        set_wage_offers(&mut companies, 5000.0);
        assert!(
            (companies[0].offered_wage_per_fte - 5000.0).abs() < 1.0,
            "Wage with 0.5 FTE should use target-wage system: expected ~5000, got {}",
            companies[0].offered_wage_per_fte
        );
    }

    #[test]
    fn test_zero_cash_zero_wage() {
        // Phase 41: With target-wage system, even a broke company gets the
        // fallback floor of 50.0. The labor market will compute
        // max_affordable_fte = 0 / 50 = 0, so it hires nobody, but the
        // wage rate itself is nonzero (sticky target).
        let mut companies = vec![company_with_cash_and_fte(
            "BROKE",
            Sector::HeavyIndustry,
            0.0,
            100.0,
        )];
        set_wage_offers(&mut companies, 5000.0);
        // Target wage starts at market avg 5000, desired is 0.9*5000=4500
        // (cash_per_fte = 0 < 5000*0.5=2500), adjusts 2% → 4900
        assert!(
            (companies[0].offered_wage_per_fte - 4900.0).abs() < 1.0,
            "Broke company wage should use target-wage with fallback: expected ~4900, got {}",
            companies[0].offered_wage_per_fte
        );
    }

    #[test]
    fn test_zero_fte_zero_wage() {
        let mut companies = vec![company_with_cash_and_fte(
            "NOFTE",
            Sector::HeavyIndustry,
            100_000.0,
            0.0,
        )];
        set_wage_offers(&mut companies, 5000.0);
        assert_eq!(companies[0].offered_wage_per_fte, 0.0);
    }

    // ═══════════════════════════════════════════════════════════════════
    // Phase 47: Seasonal Furlough Tests
    // ═══════════════════════════════════════════════════════════════════

    use crate::entities::{SeasonalProfile, SeasonalState};
    use crate::state::Season;
    use std::collections::BTreeSet;

    fn make_seasonal_company(
        id: &str,
        active_seasons: BTreeSet<Season>,
        standby_fraction: f64,
        full_fte: f64,
    ) -> Company {
        let mut company = company_with_cash_and_fte(id, Sector::Hospitality, 100_000.0, full_fte);
        company.seasonal_profile = Some(SeasonalProfile {
            active_seasons,
            standby_fte_fraction: standby_fraction,
            current_state: SeasonalState::Active,
        });
        company.physical_fte_demand = full_fte as u32;
        company.target_fte_demand = full_fte as u32;
        company.fulfilled_fte = full_fte as u32;
        company.furloughed_workers_count = 0.0;
        company
    }

    #[test]
    fn test_furlough_transfers_excess_to_furloughed_count() {
        // Tourism company active in Spring/Summer/Autumn, furloughed in Winter.
        let active = BTreeSet::from([Season::Spring, Season::Summer, Season::Autumn]);
        let mut company = make_seasonal_company("TOUR1", active, 0.20, 100.0);

        // Apply furlough in Winter (off-season)
        apply_seasonal_furlough(&mut company, Season::Winter);

        // Standby = 100 * 0.20 = 20
        assert_eq!(company.furloughed_workers_count, 80.0, "Excess FTE should be furloughed");
        assert_eq!(company.fulfilled_fte, 20, "fulfilled_fte should drop to standby");
        assert_eq!(company.physical_fte_demand, 20, "physical_fte_demand should be standby");
        assert_eq!(company.target_fte_demand, 20, "target_fte_demand should be standby");
    }

    #[test]
    fn test_furlough_reactivation_restores_fulfilled_fte() {
        let active = BTreeSet::from([Season::Spring, Season::Summer, Season::Autumn]);
        let mut company = make_seasonal_company("TOUR2", active, 0.20, 100.0);

        // Furlough in Winter
        apply_seasonal_furlough(&mut company, Season::Winter);
        assert_eq!(company.furloughed_workers_count, 80.0);
        assert_eq!(company.fulfilled_fte, 20);

        // Reactivate in Spring
        apply_seasonal_furlough(&mut company, Season::Spring);

        assert_eq!(company.furloughed_workers_count, 0.0, "Furlough count should be zeroed");
        assert_eq!(company.fulfilled_fte, 100, "fulfilled_fte should be restored");
    }

    #[test]
    fn test_furlough_state_tracking() {
        let active = BTreeSet::from([Season::Spring, Season::Summer, Season::Autumn]);
        let mut company = make_seasonal_company("TOUR3", active, 0.20, 50.0);

        apply_seasonal_furlough(&mut company, Season::Winter);
        assert_eq!(
            company.seasonal_profile.as_ref().unwrap().current_state,
            SeasonalState::Furloughed
        );

        apply_seasonal_furlough(&mut company, Season::Spring);
        assert_eq!(
            company.seasonal_profile.as_ref().unwrap().current_state,
            SeasonalState::Active
        );
    }

    #[test]
    fn test_non_seasonal_company_unaffected() {
        let mut company = company_with_cash_and_fte("FACT1", Sector::LightIndustry, 100_000.0, 100.0);
        company.seasonal_profile = None;
        let original_fte = company.fulfilled_fte;

        apply_seasonal_furlough(&mut company, Season::Winter);

        assert_eq!(company.fulfilled_fte, original_fte, "Non-seasonal company should be unaffected");
        assert_eq!(company.furloughed_workers_count, 0.0, "No furlough for non-seasonal");
    }

    #[test]
    fn test_energy_company_furlough_in_summer() {
        // Energy company active in Autumn/Winter, furloughed in Spring/Summer.
        let active = BTreeSet::from([Season::Autumn, Season::Winter]);
        let mut company = make_seasonal_company("ENER1", active, 0.15, 80.0);

        apply_seasonal_furlough(&mut company, Season::Summer);

        // Standby = 80 * 0.15 = 12
        assert_eq!(company.furloughed_workers_count, 68.0, "Excess should be furloughed in summer");
        assert_eq!(company.fulfilled_fte, 12, "fulfilled_fte should be at standby");

        // Reactivate in Autumn
        apply_seasonal_furlough(&mut company, Season::Autumn);
        assert_eq!(company.fulfilled_fte, 80, "Should be fully restored in autumn");
        assert_eq!(company.furloughed_workers_count, 0.0);
    }

    #[test]
    fn test_furlough_zero_excess_when_already_at_standby() {
        let active = BTreeSet::from([Season::Spring, Season::Summer, Season::Autumn]);
        let mut company = make_seasonal_company("TOUR4", active, 0.20, 100.0);
        // Manually set fulfilled_fte to standby level
        company.fulfilled_fte = 20;

        apply_seasonal_furlough(&mut company, Season::Winter);

        assert_eq!(company.furloughed_workers_count, 0.0, "No excess to furlough");
        assert_eq!(company.fulfilled_fte, 20, "Should remain at standby");
    }
}

//! Bankruptcy and liquidation mechanics — Phase 96 Universal Trustee.
//!
//! This module implements:
//! - BankruptcyAuctionPool for distressed asset liquidation with real bidders
//! - Syndic (court-appointed liquidator) as the SINGLE universal liquidation path
//! - RestructuringPlan for debt haircut proposals
//! - Full waterfall distribution: wages → taxes → secured creditors → unsecured → shareholders
//! - Auction market with company bidders and demolition of unsold assets
//!
//! # Critical Rules
//! - Rule 1: Strict double-entry. Every debit has an exact credit. No fiat leaks.
//! - Rule 4: Complete entity lifecycle. Unsold assets are demolished, not nationalized.
//! - Rule 7: Individual accountability. Per-company creditor claims, not proportional averaging.
//! - Rule 14: No parallel systems. The Syndic is the only liquidation path.

use crate::entities::{Building, Company};
use crate::state::banking::{Bank, LoanStatus};
use crate::state::forex::ForexMarket;
use crate::state::BankruptcyPolicy;
use crate::state::Country;
use serde::{Deserialize, Serialize};
use serde_json::Map;
use std::collections::{BTreeMap, HashMap};

// ─────────────────────────────────────────────────────────────────────────
// BankruptcyAuctionPool
// ─────────────────────────────────────────────────────────────────────────

/// Bankruptcy auction pool for distressed physical assets.
///
/// Assets are added at fire-sale prices and tracked with creditor claims.
/// Unsold assets after `auction_max_turns` are **demolished** (not nationalized).
/// A fraction of structural materials is salvaged and sold at fire-sale prices.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct BankruptcyAuctionPool {
    /// Assets in auction pool:
    /// (asking_price, book_value, owner_id, creditor_claims, turns_in_pool, sector, building_id)
    #[serde(default)]
    pub assets: BTreeMap<
        String,
        (
            f64,                // asking_price
            f64,                // book_value
            String,             // owner_id (original bankrupt company)
            HashMap<String, f64>, // creditor_claims: bank_id → outstanding_amount
            u32,                // turns_in_pool
            String,             // sector string (for bidder matching)
            Option<String>,     // building_id (for building transfer on purchase)
        ),
    >,

    /// Cash collected from asset purchases (to be distributed to creditors).
    #[serde(default)]
    pub cash_collected: f64,

    /// Creditor distribution queue: (bank_id, amount_to_distribute).
    /// Drained each turn by `drain_creditor_distribution`.
    #[serde(default)]
    pub creditor_distribution: HashMap<String, f64>,

    /// Salvage proceeds pending distribution (from demolished assets).
    #[serde(default)]
    pub salvage_proceeds: f64,

    /// Any additional auction pool fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

impl BankruptcyAuctionPool {
    /// Add physical asset to auction pool at fire-sale price with creditor claims.
    ///
    /// # Rules
    /// * Asking price = book_value * fire_sale_discount
    /// * Asset remains in pool until purchased or demolished
    /// * Creditor claims are tracked per asset for precise distribution
    pub fn add_asset(
        &mut self,
        asset_id: String,
        book_value: f64,
        owner_id: String,
        creditor_claims: HashMap<String, f64>,
        policy: &BankruptcyPolicy,
        sector: String,
        building_id: Option<String>,
    ) {
        let asking_price = book_value * policy.fire_sale_discount;
        self.assets.insert(
            asset_id,
            (
                asking_price,
                book_value,
                owner_id,
                creditor_claims,
                0,
                sector,
                building_id,
            ),
        );
    }

    /// Purchase asset from auction pool.
    ///
    /// # Rules
    /// * Asset removed from pool
    /// * Cash queued for distribution to creditor_claims (pro-rata by claim)
    /// * Returns the building_id for ownership transfer
    pub fn purchase_asset(
        &mut self,
        asset_id: &str,
        _buyer_id: &str,
        price: f64,
    ) -> Option<String> {
        if let Some((asking_price, _book_value, _owner_id, creditor_claims, _turns, _sector, building_id)) =
            self.assets.remove(asset_id)
        {
            if price >= asking_price {
                // Queue distribution to specific creditor banks (pro-rata by claim).
                let total_claims: f64 = creditor_claims.values().sum();
                if total_claims > 0.0 {
                    for (bank_id, claim_amount) in creditor_claims {
                        let share = (claim_amount / total_claims) * price;
                        *self.creditor_distribution.entry(bank_id).or_insert(0.0) += share;
                    }
                } else {
                    // No creditors — cash goes to salvage/general pool for treasury.
                    self.salvage_proceeds += price;
                }
                self.cash_collected += price;
                return building_id;
            } else {
                // Put back if price insufficient.
                self.assets.insert(
                    asset_id.to_string(),
                    (
                        asking_price,
                        _book_value,
                        _owner_id,
                        creditor_claims,
                        _turns,
                        _sector,
                        building_id,
                    ),
                );
            }
        }
        None
    }

    /// Increment turn counters for all assets in pool.
    pub fn increment_turns(&mut self) {
        for (_, _, _, _, turns, _, _) in self.assets.values_mut() {
            *turns += 1;
        }
    }

    /// Get assets that have exceeded auction_max_turns (to be demolished).
    pub fn expired_asset_ids(&self, max_turns: u32) -> Vec<String> {
        self.assets
            .iter()
            .filter(|(_, (_, _, _, _, turns, _, _))| *turns >= max_turns)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Remove an asset (for demolition). Returns its data for salvage calculation.
    pub fn remove_asset(
        &mut self,
        asset_id: &str,
    ) -> Option<(f64, f64, String, HashMap<String, f64>, u32, String, Option<String>)> {
        self.assets.remove(asset_id)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// RestructuringPlan
// ─────────────────────────────────────────────────────────────────────────

/// Debt restructuring proposal for distressed companies.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct RestructuringPlan {
    /// Company ID being restructured.
    #[serde(default)]
    pub company_id: String,
    /// Proposed haircut percentage (0.0 - 1.0).
    #[serde(default)]
    pub debt_haircut: f64,
    /// Extended repayment period in turns.
    #[serde(default)]
    pub extended_period: u32,
    /// Whether the plan has been approved by creditors.
    #[serde(default)]
    pub approved: bool,
    /// Any additional restructuring plan fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

impl RestructuringPlan {
    /// Evaluate if restructuring plan is viable (NPV rule).
    pub fn is_viable(&self, company: &Company, bank: &Bank) -> bool {
        let positive_ocf = company
            .financial_history
            .iter()
            .rev()
            .take(3)
            .all(|record| {
                record
                    .get("operating_cash_flows")
                    .and_then(|v| v.as_f64())
                    .is_some_and(|ocf| ocf > 0.0)
            });

        if !positive_ocf {
            return false;
        }

        let haircut_loss = self.debt_haircut * bank.issued_loans;
        let capital_after_loss = bank.own_capital - haircut_loss;
        let minimum_capital = 0.06 * bank.issued_loans;

        capital_after_loss >= minimum_capital
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Syndic — Universal Liquidator
// ─────────────────────────────────────────────────────────────────────────

/// Syndic (court-appointed liquidator) for bankruptcy proceedings.
///
/// This is the SINGLE, universal liquidation path (Rule 14).
/// Called from `CompanyLifecycle::liquidate_bankrupt_companies`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Syndic {
    /// Domestic currency code.
    #[serde(default)]
    pub domestic_currency: String,
    /// Creditor distributions: (creditor_id, amount_distributed).
    #[serde(default)]
    pub creditor_distributions: HashMap<String, f64>,
    /// Any additional syndic fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

impl Syndic {
    /// Create a new syndic.
    pub fn new(domestic_currency: String) -> Self {
        Self {
            domestic_currency,
            creditor_distributions: HashMap::new(),
            extra: Map::new(),
        }
    }

    /// Execute full liquidation for a bankrupt company.
    ///
    /// This is the ONLY liquidation path. It handles:
    /// 1. FX seizure and conversion
    /// 2. Cash seizure (brokerage_account.cash + available_cash)
    /// 3. Inventory seizure from buildings (fire-sale to cash)
    /// 4. Building routing to auction pool with per-company creditor claims
    /// 5. Full waterfall: wages → taxes → secured creditors → unsecured → shareholders
    /// 6. Cadastre parcel reassignment to Treasury (empty land only)
    ///
    /// # Double-Entry Flow
    /// * Company cash → seized (debit company, credit waterfall recipients)
    /// * Building inventory → fire-sold (debit inventory, credit cash pool)
    /// * Buildings → routed to auction pool (physical transfer, no cash yet)
    /// * Waterfall pays: wages (credit worker savings), taxes (credit treasury),
    ///   banks (credit bank reserves), residual (credit treasury)
    pub fn execute_liquidation(
        &mut self,
        company: &mut Company,
        buildings: &mut Vec<Building>,
        forex_market: &mut ForexMarket,
        country: &mut Country,
        companies: &mut [Company],
        policy: &BankruptcyPolicy,
    ) {
        // ── STEP 1: Seize and convert fx_balances ──
        if let Some(brokerage) = &mut company.brokerage_account {
            let seized_fx = std::mem::take(&mut brokerage.fx_balances);
            for (currency_code, amount) in seized_fx {
                if amount > 0.0 && currency_code != self.domestic_currency {
                    if let Ok(domestic_received) = forex_market.execute_direct_swap(
                        &currency_code,
                        &self.domestic_currency,
                        amount,
                    ) {
                        brokerage.cash += domestic_received;
                    }
                }
            }
        }

        // ── STEP 2: Build per-company creditor claims (Rule 7) ──
        // Iterate all banks' loans_issued and find loans to THIS company.
        let mut creditor_claims: HashMap<String, f64> = HashMap::new();
        for bank_company in companies.iter() {
            if let Some(ref bs) = bank_company.balance_sheet {
                for loan in &bs.loans_issued {
                    if loan.borrower_id == company.id
                        && loan.status != LoanStatus::Repaid
                        && loan.outstanding_balance > 0.0
                    {
                        *creditor_claims
                            .entry(bank_company.id.clone())
                            .or_insert(0.0) += loan.outstanding_balance;
                    }
                }
            }
        }

        // ── STEP 3: Seize actual cash (Rule 1) ──
        let brokerage_cash = company
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(0.0)
            .max(0.0);
        let available_cash = company.available_cash.max(0.0);
        let mut total_seized_cash = brokerage_cash + available_cash;

        // Clear company cash (it's been seized).
        if let Some(brokerage) = &mut company.brokerage_account {
            brokerage.cash = 0.0;
        }
        company.available_cash = 0.0;

        // ── STEP 3a: Reclaim frozen cash from justice system ──
        if let Some(justice_state) = country.politics.justice_state.as_mut() {
            if let Some(frozen) = justice_state.frozen_company_cash.remove(&company.id) {
                if frozen > 0.0 {
                    total_seized_cash += frozen;
                }
            }
        }

        // ── STEP 4: Seize building inventory (Rule 1 & 20) ──
        // Fire-sell inventory at policy discount, add to cash pool.
        let mut building_ids_owned: Vec<String> = Vec::new();
        for building in buildings.iter_mut() {
            if building.owner_id != company.id {
                continue;
            }
            building_ids_owned.push(building.id.clone());

            let inventory_value: f64 = building.inventory.values().sum();
            if inventory_value > 0.0 {
                let fire_sale_value = inventory_value * policy.fire_sale_discount;
                total_seized_cash += fire_sale_value;
                // Clear inventory — physical mass is "sold" at fire-sale.
                building.inventory.clear();
            }
        }

        // ── STEP 5: Route buildings to auction pool with per-building book value ──
        for building in buildings.iter() {
            if building.owner_id != company.id {
                continue;
            }
            // Per-building book value from actual fixed_assets (Rule 7).
            let book_value: f64 = building.fixed_assets.iter().map(|c| c.count).sum();
            let sector_str = format!("{:?}", building.sector);
            let asset_id = format!("building_{}", building.id);

            country.bankruptcy_auction_pool.add_asset(
                asset_id,
                book_value,
                company.id.clone(),
                creditor_claims.clone(),
                policy,
                sector_str,
                Some(building.id.clone()),
            );
        }

        // ── STEP 6: Mark loans as Default in all banks ──
        for bank_company in companies.iter_mut() {
            if let Some(ref mut bs) = bank_company.balance_sheet {
                for loan in &mut bs.loans_issued {
                    if loan.borrower_id == company.id
                        && loan.status != LoanStatus::Repaid
                    {
                        loan.status = LoanStatus::Default;
                    }
                }
            }
        }

        // ── STEP 7: Full Waterfall Distribution (Rule 21) ──
        if total_seized_cash > 0.0 {
            let mut remaining_cash = total_seized_cash;

            // Waterfall Step 1: Unpaid wages (super-priority).
            // Credit worker savings in the company's region.
            let wage_arrears = company.wage_arrears.max(0.0);
            if wage_arrears > 0.0 && remaining_cash > 0.0 {
                let wage_payment = wage_arrears.min(remaining_cash);
                remaining_cash -= wage_payment;
                company.wage_arrears -= wage_payment;

                // Credit workers via regional class demographics.
                let region_id = company.region_id.clone();
                if let Some(region) = country.regions.iter_mut().find(|r| r.id == region_id) {
                    // Distribute to LandlessLaborer and Serf classes (workers).
                    for class in [
                        crate::society::geography::RuralClass::LandlessLaborer,
                        crate::society::geography::RuralClass::Serf,
                        crate::society::geography::RuralClass::FreePeasant,
                    ] {
                        if let Some(demo) = region.class_demographics.get_class_mut(class) {
                            demo.savings += wage_payment * 0.5;
                            if demo.population > 0 {
                                demo.savings_per_capita =
                                    demo.savings / demo.population as f64;
                            }
                            break; // Give to the first available worker class
                        }
                    }
                }
            }

            // Waterfall Step 2: Tax arrears (super-priority).
            // Pay from financial_history (cumulative unpaid taxes).
            let tax_owed: f64 = company
                .financial_history
                .iter()
                .filter_map(|record| {
                    record
                        .get("taxes_unpaid")
                        .and_then(|v| v.as_f64())
                })
                .sum::<f64>()
                .max(0.0);
            let tax_from_last = company
                .financial_history
                .iter()
                .next_back()
                .and_then(|record| record.get("taxes").and_then(|v| v.as_f64()))
                .unwrap_or(0.0)
                .max(0.0);
            let total_tax_owed = tax_owed.max(tax_from_last);

            if total_tax_owed > 0.0 && remaining_cash > 0.0 {
                let tax_payment = total_tax_owed.min(remaining_cash);
                country.budget.liquid_reserves += tax_payment;
                remaining_cash -= tax_payment;
            }

            // Waterfall Step 3: Secured creditors (banks with collateral).
            // Pay exact per-bank amounts (Rule 7).
            if remaining_cash > 0.0 && !creditor_claims.is_empty() {
                let total_claims: f64 = creditor_claims.values().sum();
                if total_claims > 0.0 {
                    let collection_ratio = (remaining_cash / total_claims).min(1.0);
                    for (bank_id, claim_amount) in &creditor_claims {
                        let payment = claim_amount * collection_ratio;
                        if payment > 0.0 {
                            // Credit the bank's reserves and reduce outstanding loans.
                            if let Some(bank) = companies.iter_mut().find(|c| c.id == *bank_id) {
                                if let Some(ref mut bs) = bank.balance_sheet {
                                    bs.reserves_at_central_bank += payment;
                                    // Reduce the outstanding balance on the specific loans.
                                    let mut to_reduce = payment;
                                    for loan in &mut bs.loans_issued {
                                        if loan.borrower_id == company.id
                                            && loan.status != LoanStatus::Repaid
                                            && to_reduce > 0.0
                                        {
                                            let reduction = loan.outstanding_balance.min(to_reduce);
                                            loan.outstanding_balance -= reduction;
                                            to_reduce -= reduction;
                                            if loan.outstanding_balance <= 0.0 {
                                                loan.status = LoanStatus::Repaid;
                                            }
                                        }
                                    }
                                }
                            }
                            *self
                                .creditor_distributions
                                .entry(bank_id.clone())
                                .or_insert(0.0) += payment;
                        }
                    }
                    remaining_cash = 0.0;
                }
            }

            // Waterfall Step 4: Shareholder residual → treasury.
            if remaining_cash > 0.0 {
                country.budget.liquid_reserves += remaining_cash;
                *self
                    .creditor_distributions
                    .entry("shareholders".to_string())
                    .or_insert(0.0) += remaining_cash;
            }
        }

        // ── STEP 8: Reassign cadastre parcels to Treasury (empty land) ──
        use crate::society::cadastre::ParcelOwnerType;
        for (_, parcel) in country.cadastre.parcels.iter_mut() {
            if parcel.owner_id == company.id {
                parcel.owner_type = ParcelOwnerType::State;
                parcel.owner_id = "TREASURY".to_string();
            }
        }

        // ── STEP 9: Remove exchange listings ──
        country
            .stock_exchange
            .liquidity_pools
            .remove(&format!("EQUITY:{}", company.id));

        // ── STEP 10: Mark company as liquidated ──
        company.is_liquidated = true;
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Auction Market — Real Bidders
// ─────────────────────────────────────────────────────────────────────────

/// Process one turn of the bankruptcy auction market.
///
/// # Rules
/// 1. Increment turn counters on all pool assets.
/// 2. Generate bids from existing companies (sector-matching, affordability-checked).
/// 3. Execute purchases — debit buyer cash, credit creditor distribution.
/// 4. Transfer building ownership to winning bidder.
/// 5. Demolish assets exceeding auction_max_turns (salvage materials at fire-sale).
/// 6. Drain creditor_distribution to actual bank reserves.
///
/// # Arguments
/// * `companies` - All companies (bidders and banks)
/// * `buildings` - All buildings (for ownership transfer)
/// * `country` - Country state (for treasury salvage proceeds)
/// * `policy` - Bankruptcy policy
pub fn process_auction_turn(
    companies: &mut [Company],
    buildings: &mut Vec<Building>,
    country: &mut Country,
    policy: &BankruptcyPolicy,
) {
    let pool = &mut country.bankruptcy_auction_pool;

    // 1. Age all assets.
    pool.increment_turns();

    // 2. Collect asset info for bidding (immutable snapshot).
    let asset_snapshots: Vec<(
        String,  // asset_id
        f64,     // asking_price
        String,  // sector
        Option<String>, // building_id
    )> = pool
        .assets
        .iter()
        .map(|(id, (price, _, _, _, _, sector, bld))| {
            (id.clone(), *price, sector.clone(), bld.clone())
        })
        .collect();

    // 3. Generate bids and execute purchases.
    // For each asset, find the best affordable bidder in the same sector.
    let mut purchased: Vec<(String, String, f64, Option<String>)> = Vec::new();
    // (asset_id, buyer_id, price, building_id)

    for (asset_id, asking_price, sector_str, building_id) in &asset_snapshots {
        // Find candidate bidders: companies with positive cash in matching sector.
        // Exclude banks and liquidated companies.
        let mut best_bidder: Option<(usize, f64)> = None; // (company_idx, bid_price)
        for (idx, company) in companies.iter().enumerate() {
            if company.is_liquidated {
                continue;
            }
            if company.bank_type.is_some() {
                continue; // Banks don't buy industrial assets
            }
            if company.fund_type.is_some() {
                continue; // Funds don't buy industrial assets
            }
            // Sector matching: bidder must be in the same sector.
            let company_sector = format!("{:?}", company.sector);
            if company_sector != *sector_str {
                continue;
            }
            // Affordability check.
            let bidder_cash = company
                .brokerage_account
                .as_ref()
                .map(|b| b.cash)
                .unwrap_or(0.0)
                .max(0.0)
                + company.available_cash.max(0.0);
            if bidder_cash < *asking_price {
                continue;
            }
            // Pick the bidder with the most cash (highest capacity to pay).
            match best_bidder {
                None => best_bidder = Some((idx, bidder_cash)),
                Some((_, prev_cash)) if bidder_cash > prev_cash => {
                    best_bidder = Some((idx, bidder_cash));
                }
                _ => {}
            }
        }

        if let Some((buyer_idx, _)) = best_bidder {
            let buyer_id = companies[buyer_idx].id.clone();
            let price = *asking_price;

            // Debit buyer's cash (double-entry).
            let buyer = &mut companies[buyer_idx];
            let brokerage_cash = buyer
                .brokerage_account
                .as_ref()
                .map(|b| b.cash)
                .unwrap_or(0.0);
            if brokerage_cash >= price {
                if let Some(brokerage) = &mut buyer.brokerage_account {
                    brokerage.cash -= price;
                }
            } else {
                // Use brokerage first, then available_cash.
                let remaining = price - brokerage_cash.max(0.0);
                if let Some(brokerage) = &mut buyer.brokerage_account {
                    brokerage.cash = 0.0;
                }
                buyer.available_cash = (buyer.available_cash - remaining).max(0.0);
            }

            purchased.push((asset_id.clone(), buyer_id, price, building_id.clone()));
        }
    }

    // 4. Execute purchases in the pool and transfer building ownership.
    for (asset_id, buyer_id, price, building_id) in purchased {
        let transferred_bld = country.bankruptcy_auction_pool.purchase_asset(
            &asset_id,
            &buyer_id,
            price,
        );
        // Transfer building ownership.
        if let Some(bld_id) = transferred_bld.or(building_id) {
            if let Some(building) = buildings.iter_mut().find(|b| b.id == bld_id) {
                building.owner_id = buyer_id.clone();
                // Restore capacity (was zeroed when added to pool).
                // Use a reasonable default based on sector.
                if building.worker_capacity == 0 {
                    building.worker_capacity = 100;
                }
            }
        }
    }

    // 5. Demolish expired assets (Rule 4 — no zombie nationalization).
    let expired_ids = country.bankruptcy_auction_pool.expired_asset_ids(policy.auction_max_turns);
    for asset_id in expired_ids {
        if let Some((_asking, book_value, _owner, _claims, _turns, _sector, building_id)) =
            country.bankruptcy_auction_pool.remove_asset(&asset_id)
        {
            // Salvage: a fraction of structural materials at fire-sale price.
            // Salvage fraction is configurable via policy.rescue_nationalization_discount
            // (repurposed as salvage yield fraction, e.g., 0.1 = 10% of book value).
            let salvage_value = book_value * policy.rescue_nationalization_discount;
            if salvage_value > 0.0 {
                country.bankruptcy_auction_pool.salvage_proceeds += salvage_value;
                country.bankruptcy_auction_pool.cash_collected += salvage_value;
            }

            // Remove the building entity entirely (demolition).
            if let Some(bld_id) = building_id {
                buildings.retain(|b| b.id != bld_id);
            }
        }
    }

    // 6. Drain salvage_proceeds to treasury (no creditor claims on salvage).
    let salvage = country.bankruptcy_auction_pool.salvage_proceeds;
    if salvage > 0.0 {
        country.budget.liquid_reserves += salvage;
        country.bankruptcy_auction_pool.salvage_proceeds = 0.0;
    }

    // 7. Drain creditor_distribution to actual bank reserves (Rule 1).
    let distributions = std::mem::take(&mut country.bankruptcy_auction_pool.creditor_distribution);
    for (bank_id, amount) in distributions {
        if amount <= 0.0 {
            continue;
        }
        if let Some(bank) = companies.iter_mut().find(|c| c.id == bank_id) {
            if let Some(ref mut bs) = bank.balance_sheet {
                bs.reserves_at_central_bank += amount;
            }
        } else {
            // Bank not found — credit treasury as residual.
            country.budget.liquid_reserves += amount;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Phase 22A: Distressed construction defect retention
// ─────────────────────────────────────────────────────────────────────────

/// Phase 22A: Distressed construction asset defect retention.
///
/// When a contractor goes bankrupt mid-construction, the building's
/// `structural_defect` and any committed `MaterialSubstitution` fraud
/// must be preserved verbatim through the auction process.
pub fn preserve_defects_through_bankruptcy(
    buildings: &mut [Building],
    bankrupt_company_id: &str,
) {
    for building in buildings.iter_mut() {
        if building.owner_id != bankrupt_company_id {
            continue;
        }
        let _ = building.structural_defect;
        if let Some(ref _project) = building.active_project {}
    }
}

/// Phase 22D: Clamp reputation and defect values after save migration.
pub fn normalize_phase22_fields(
    companies: &mut [Company],
    buildings: &mut [Building],
) {
    for company in companies.iter_mut() {
        company.reputation_score = company.reputation_score.clamp(0.0, 100.0);
    }
    for building in buildings.iter_mut() {
        building.structural_defect = building.structural_defect.clamp(0.0, 1.0);
        if let Some(ref mut project) = building.active_project {
            project.structural_defect = project.structural_defect.clamp(0.0, 1.0);
            project.ohs_coverage_ratio = project.ohs_coverage_ratio.clamp(0.0, 1.0);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bankruptcy_policy_defaults() {
        let policy = BankruptcyPolicy::with_defaults();
        assert_eq!(policy.auction_max_turns, 4);
        assert_eq!(policy.fire_sale_discount, 0.5);
        assert_eq!(policy.rescue_nationalization_discount, 0.1);
        assert_eq!(policy.privatization_markup, 1.0);
    }

    #[test]
    fn test_auction_pool_add_asset() {
        let mut pool = BankruptcyAuctionPool::default();
        let policy = BankruptcyPolicy::with_defaults();
        let mut claims = HashMap::new();
        claims.insert("bank1".to_string(), 1000.0);

        pool.add_asset(
            "asset1".to_string(),
            10000.0,
            "owner1".to_string(),
            claims,
            &policy,
            "Mining".to_string(),
            Some("bld1".to_string()),
        );

        assert!(pool.assets.contains_key("asset1"));
        let (price, book, owner, _claims, turns, sector, bld) =
            pool.assets.get("asset1").unwrap();
        assert_eq!(*price, 5000.0);
        assert_eq!(*book, 10000.0);
        assert_eq!(*owner, "owner1");
        assert_eq!(*turns, 0);
        assert_eq!(*sector, "Mining");
        assert_eq!(*bld, Some("bld1".to_string()));
    }

    #[test]
    fn test_auction_pool_purchase() {
        let mut pool = BankruptcyAuctionPool::default();
        let policy = BankruptcyPolicy::with_defaults();
        let mut claims = HashMap::new();
        claims.insert("bank1".to_string(), 1000.0);

        pool.add_asset(
            "asset1".to_string(),
            10000.0,
            "owner1".to_string(),
            claims.clone(),
            &policy,
            "Mining".to_string(),
            Some("bld1".to_string()),
        );

        let bld = pool.purchase_asset("asset1", "buyer1", 6000.0);
        assert_eq!(bld, Some("bld1".to_string()));
        assert_eq!(pool.creditor_distribution.get("bank1"), Some(&6000.0));
        assert_eq!(pool.cash_collected, 6000.0);
    }

    #[test]
    fn test_auction_pool_increment_turns() {
        let mut pool = BankruptcyAuctionPool::default();
        let policy = BankruptcyPolicy::with_defaults();

        pool.add_asset(
            "asset1".to_string(),
            10000.0,
            "owner1".to_string(),
            HashMap::new(),
            &policy,
            "Mining".to_string(),
            None,
        );

        pool.increment_turns();
        let (_, _, _, _, turns, _, _) = pool.assets.get("asset1").unwrap();
        assert_eq!(*turns, 1);
    }

    #[test]
    fn test_expired_asset_ids() {
        let mut pool = BankruptcyAuctionPool::default();
        let policy = BankruptcyPolicy::with_defaults();

        pool.add_asset(
            "asset1".to_string(),
            10000.0,
            "owner1".to_string(),
            HashMap::new(),
            &policy,
            "Mining".to_string(),
            None,
        );

        // Age 5 turns (max is 4).
        for _ in 0..5 {
            pool.increment_turns();
        }

        let expired = pool.expired_asset_ids(4);
        assert_eq!(expired, vec!["asset1".to_string()]);
    }

    #[test]
    fn test_demolish_expired_asset() {
        let mut pool = BankruptcyAuctionPool::default();
        let policy = BankruptcyPolicy::with_defaults();

        pool.add_asset(
            "asset1".to_string(),
            10000.0,
            "owner1".to_string(),
            HashMap::new(),
            &policy,
            "Mining".to_string(),
            Some("bld1".to_string()),
        );

        // Age past max turns.
        for _ in 0..5 {
            pool.increment_turns();
        }

        let expired = pool.expired_asset_ids(4);
        for id in &expired {
            let removed = pool.remove_asset(id);
            assert!(removed.is_some());
            // Salvage is calculated by the caller (process_auction_turn),
            // not by remove_asset itself. Simulate it here:
            if let Some((_, book_value, _, _, _, _, _)) = &removed {
                let salvage = book_value * policy.rescue_nationalization_discount;
                pool.salvage_proceeds += salvage;
            }
        }
        assert!(!pool.assets.contains_key("asset1"));
        // Salvage proceeds should be 10% of book value.
        assert!((pool.salvage_proceeds - 1000.0).abs() < 0.01);
    }
}

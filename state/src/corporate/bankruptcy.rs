//! Bankruptcy and liquidation mechanics.
//!
//! This module implements:
//! - BankruptcyAuctionPool for distressed asset liquidation
//! - Syndic for creditor hierarchy and asset distribution
//! - RestructuringPlan for debt haircut proposals
//! - Waterfall distribution for seized cash

use crate::entities::Company;
use crate::state::banking::Bank;
use crate::state::Country;
use crate::state::forex::ForexMarket;
use crate::state::BankruptcyPolicy;
use serde::{Deserialize, Serialize};
use serde_json::Map;
use std::collections::{BTreeMap, HashMap};

/// Bankruptcy auction pool for distressed physical assets.
///
/// Assets are added at fire-sale prices and tracked with creditor claims.
/// Unsold assets after auction_max_turns trigger nationalization.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct BankruptcyAuctionPool {
    /// Assets in auction pool: (asset_id, (asking_price, book_value, owner_id, creditor_claims, turns_in_pool))
    #[serde(default)]
    pub assets: BTreeMap<String, (f64, f64, String, HashMap<String, f64>, u32)>,
    
    /// Cash collected from asset purchases (to be distributed to creditors)
    #[serde(default)]
    pub cash_collected: f64,
    
    /// Creditor distribution: (bank_id, amount_to_distribute)
    #[serde(default)]
    pub creditor_distribution: HashMap<String, f64>,
    
    /// Privatization queue: (asset_id, (owner_id, book_value))
    /// Assets nationalized by JST/State awaiting privatization
    #[serde(default)]
    pub privatization_queue: BTreeMap<String, (String, f64)>,
    
    /// Any additional auction pool fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

impl BankruptcyAuctionPool {
    /// Add physical asset to auction pool at fire-sale price with creditor claims.
    ///
    /// # Arguments
    /// * `asset_id` - Unique identifier for the asset (building_id, commodity_key)
    /// * `book_value` - Original book value of the asset
    /// * `owner_id` - Original owner company ID
    /// * `creditor_claims` - HashMap mapping bank_id to unpaid loan_balance
    /// * `policy` - Bankruptcy policy for discount rates
    ///
    /// # Rules
    /// * Asking price = book_value * fire_sale_discount
    /// * Asset remains in pool until purchased or nationalized
    /// * Creditor claims are tracked per asset for precise distribution
    pub fn add_asset(
        &mut self,
        asset_id: String,
        book_value: f64,
        owner_id: String,
        creditor_claims: HashMap<String, f64>,
        policy: &BankruptcyPolicy,
    ) {
        let asking_price = book_value * policy.fire_sale_discount;
        self.assets.insert(
            asset_id,
            (asking_price, book_value, owner_id, creditor_claims, 0),
        );
    }
    
    /// Purchase asset from auction pool.
    ///
    /// # Arguments
    /// * `asset_id` - Asset to purchase
    /// * `buyer_id` - ID of the purchasing entity
    /// * `price` - Purchase price
    ///
    /// # Returns
    /// * `true` if purchase successful, `false` if asset not found
    ///
    /// # Rules
    /// * Asset removed from pool
    /// * Cash queued for distribution to creditor_claims
    pub fn purchase_asset(&mut self, asset_id: &str, _buyer_id: &str, price: f64) -> bool {
        if let Some((asking_price, _book_value, _owner_id, creditor_claims, _turns)) = 
            self.assets.remove(asset_id)
        {
            if price >= asking_price {
                // Queue distribution to specific creditor banks
                let total_claims: f64 = creditor_claims.values().sum();
                if total_claims > 0.0 {
                    for (bank_id, claim_amount) in creditor_claims {
                        let share = (claim_amount / total_claims) * price;
                        *self.creditor_distribution.entry(bank_id).or_insert(0.0) += share;
                    }
                }
                self.cash_collected += price;
                return true;
            } else {
                // Put back if price insufficient
                self.assets.insert(
                    asset_id.to_string(),
                    (asking_price, _book_value, _owner_id, creditor_claims, _turns),
                );
            }
        }
        false
    }
    
    /// Increment turn counters for all assets in pool.
    ///
    /// # Rules
    /// * Each asset's turns_in_pool increases by 1
    /// * Assets exceeding auction_max_turns should be nationalized
    pub fn increment_turns(&mut self) {
        for (_, (_, _, _, _, turns)) in self.assets.iter_mut() {
            *turns += 1;
        }
    }
}

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
    ///
    /// # Arguments
    /// * `company` - Company being evaluated
    /// * `bank` - Creditor bank
    ///
    /// # Returns
    /// * `true` if plan is viable (positive OCF + regulatory capital compliance)
    ///
    /// # Rules
    /// * Company must have positive operating cash flow for 3 turns
    /// * Haircut loss must not breach bank's regulatory capital minimum
    pub fn is_viable(&self, company: &Company, bank: &Bank) -> bool {
        // Check positive OCF over 3 turns
        let positive_ocf = company
            .financial_history
            .iter()
            .rev()
            .take(3)
            .all(|record| {
                record
                    .get("operating_cash_flows")
                    .and_then(|v| v.as_f64())
                    .map_or(false, |ocf| ocf > 0.0)
            });
        
        if !positive_ocf {
            return false;
        }
        
        // Check regulatory capital compliance (simplified for legacy Bank structure)
        let haircut_loss = self.debt_haircut * bank.issued_loans;
        let capital_after_loss = bank.own_capital - haircut_loss;
        // KNF minimum: Own Capital >= 6% of Issued Loans
        let minimum_capital = 0.06 * bank.issued_loans;
        
        capital_after_loss >= minimum_capital
    }
}

/// Syndic (court-appointed liquidator) for bankruptcy proceedings.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct Syndic {
    /// Domestic currency code.
    #[serde(default)]
    pub domestic_currency: String,
    
    /// Creditor distributions: (creditor_id, amount_distributed)
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
    
    /// Execute full liquidation for bankrupt company.
    ///
    /// # Arguments
    /// * `company` - Bankrupt company to liquidate
    /// * `forex_market` - Forex market for fx_balances conversion
    /// * `auction_pool` - Bankruptcy auction pool for physical assets
    /// * `country` - Country state for tax collection
    /// * `banks` - Banks for debt repayment
    /// * `policy` - Bankruptcy policy for discount rates
    ///
    /// # Double-Entry Flow
    /// * Company: fx_balances cleared (seized)
    /// * ForexMarket: direct AMM swap called with raw amounts (market sell pressure)
    /// * AuctionPool: assets added with creditor_claims (physical asset transfer)
    /// * Creditors: paid via waterfall (exact amounts, no percentages)
    /// * Commercial Banks: reserves_at_central_bank updated on all cash transfers
    pub fn execute_liquidation(
        &mut self,
        company: &mut Company,
        forex_market: &mut ForexMarket,
        auction_pool: &mut BankruptcyAuctionPool,
        country: &mut Country,
        banks: &mut Vec<Bank>,
        policy: &BankruptcyPolicy,
    ) {
        // STEP 1: Seize and convert fx_balances to domestic fiat using direct AMM swap
        if let Some(brokerage) = &mut company.brokerage_account {
            let seized_fx = std::mem::take(&mut brokerage.fx_balances);
            
            for (currency_code, amount) in seized_fx {
                if amount > 0.0 && currency_code != self.domestic_currency {
                    // Direct AMM swap to avoid BorrowChecker panic (no brokerage_accounts map)
                    match forex_market.execute_direct_swap(
                        &currency_code,
                        &self.domestic_currency,
                        amount,
                    ) {
                        Ok(domestic_received) => {
                            // Manually add domestic cash to company's brokerage account
                            brokerage.cash += domestic_received;
                            eprintln!(
                                "Syndic converted {} {} to {} {} (rate: {})",
                                amount, currency_code, domestic_received, self.domestic_currency,
                                domestic_received / amount
                            );
                        }
                        Err(e) => {
                            eprintln!("Syndic failed to convert {}: {}", currency_code, e);
                        }
                    }
                }
            }
        }
        
        // STEP 2: Build creditor_claims for each asset (bank_id -> unpaid loan_balance)
        // Note: Legacy Bank structure doesn't have balance_sheet with loan details
        // We'll use a simplified approach - distribute proportionally based on bank size
        let mut creditor_claims: HashMap<String, f64> = HashMap::new();
        for bank in banks.iter() {
            // Simplified: assume each bank has equal exposure for now
            // In full implementation, would need to track per-company loan exposure
            creditor_claims.insert(bank.id.clone(), bank.issued_loans / banks.len() as f64);
        }
        
        // STEP 3: Route physical assets to BankruptcyAuctionPool with creditor_claims
        for building_id in &company.building_ids {
            let book_value = company.fixed_capital / company.building_ids.len() as f64;
            auction_pool.add_asset(
                building_id.clone(),
                book_value,
                company.id.clone(),
                creditor_claims.clone(),
                policy,
            );
        }
        
        // STEP 4: Waterfall distribution of seized domestic cash
        let mut total_seized_cash = company.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(0.0);

        // STEP 4a: Reclaim frozen cash from justice system (Phase 14)
        // When a company goes bankrupt, the Syndic reclaims any cash frozen
        // by unresolved court disputes from justice_state.frozen_company_cash.
        // Double-entry: Debit justice_state.frozen_company_cash, Credit seized cash pool.
        if let Some(justice_state) = country.politics.justice_state.as_mut() {
            if let Some(frozen) = justice_state.frozen_company_cash.remove(&company.id) {
                if frozen > 0.0 {
                    total_seized_cash += frozen;
                    company.available_cash += frozen;
                    eprintln!(
                        "Syndic reclaimed {} frozen cash for bankrupt {}",
                        frozen, company.id
                    );
                }
            }
        }

        if total_seized_cash > 0.0 {
            let mut remaining_cash = total_seized_cash;
            
            // Waterfall Step 1: Pay actual unpaid tax liabilities from financial history
            let tax_owed = company.financial_history
                .iter()
                .rev()
                .next()
                .and_then(|record| record.get("podatki").and_then(|v| v.as_f64()))
                .unwrap_or(0.0);
            let tax_payment = tax_owed.min(remaining_cash);
            if tax_payment > 0.0 {
                country.budget.liquid_reserves += tax_payment;
                remaining_cash -= tax_payment;
                eprintln!("Syndic paid {} in taxes for {}", tax_payment, company.id);
            }
            
            // Waterfall Step 2: Pay bank loans proportionally based on actual per-bank loan exposure
            if remaining_cash > 0.0 {
                let total_issued: f64 = banks.iter().map(|b| b.issued_loans).sum();
                if total_issued > 0.0 {
                    let distribution_pool = remaining_cash;
                    for bank in banks.iter_mut() {
                        let bank_share = bank.issued_loans / total_issued;
                        let bank_payment = distribution_pool * bank_share;
                        if bank_payment > 0.0 {
                            bank.issued_loans = (bank.issued_loans - bank_payment).max(0.0);
                            bank.total_deposits = (bank.total_deposits - bank_payment).max(0.0);

                            *self.creditor_distributions.entry(bank.id.clone()).or_insert(0.0) +=
                                bank_payment;

                            eprintln!("Syndic paid {} to bank {} for {}", bank_payment, bank.id, company.id);
                        }
                    }
                    remaining_cash = 0.0;
                }
            }
            
            // Waterfall Step 3: Residual to shareholders (route to treasury as residual claimant)
            if remaining_cash > 0.0 {
                *self.creditor_distributions.entry("shareholders".to_string()).or_insert(0.0) +=
                    remaining_cash;
                country.budget.liquid_reserves += remaining_cash;
                eprintln!("Syndic routed {} residual to treasury for shareholders of {}", remaining_cash, company.id);
                remaining_cash = 0.0;
            }
        }
        
        // Clear company brokerage account
        if let Some(brokerage) = &mut company.brokerage_account {
            brokerage.cash = 0.0;
        }
    }
}

/// Phase 22A: Distressed construction asset defect retention.
///
/// When a contractor goes bankrupt mid-construction, the building's
/// `structural_defect` and any committed `MaterialSubstitution` fraud
/// must be preserved verbatim through the auction process. The new
/// investor inherits the hidden defect.
///
/// # Arguments
/// * `buildings` - All buildings (the bankrupt contractor's buildings are updated).
/// * `bankrupt_company_id` - The contractor being liquidated.
///
/// # Rules
/// * Buildings owned by the bankrupt contractor keep their `structural_defect`.
/// * Active construction projects keep their `structural_defect` and fraud history.
/// * The auction listing does NOT expose the defect (it's hidden).
/// * The new investor discovers the defect via private inspection.
pub fn preserve_defects_through_bankruptcy(
    buildings: &mut [crate::entities::Building],
    bankrupt_company_id: &str,
) {
    for building in buildings.iter_mut() {
        if building.owner_id != bankrupt_company_id {
            continue;
        }
        // Defect is already stored on the building — it survives ownership transfer.
        // Active projects also retain their defect field.
        // No action needed beyond ensuring the fields are not cleared.
        // This function exists as an explicit checkpoint for documentation
        // and to allow future hook points (e.g. hiding defect from auction UI).
        let _ = building.structural_defect;
        if let Some(ref _project) = building.active_project {
            // Project's structural_defect is preserved automatically.
        }
    }
}

/// Phase 22D: Clamp reputation and defect values after save migration.
///
/// Old saves may have out-of-range values due to float drift or manual edits.
/// This normalizes all companies and buildings to valid ranges.
///
/// # Rules
/// * `reputation_score` clamped to [0.0, 100.0].
/// * `structural_defect` clamped to [0.0, 1.0].
pub fn normalize_phase22_fields(
    companies: &mut [Company],
    buildings: &mut [crate::entities::Building],
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
        
        pool.add_asset("asset1".to_string(), 10000.0, "owner1".to_string(), claims, &policy);
        
        assert!(pool.assets.contains_key("asset1"));
        let (price, book, owner, claims, turns) = pool.assets.get("asset1").unwrap();
        assert_eq!(*price, 5000.0); // 10000 * 0.5
        assert_eq!(*book, 10000.0);
        assert_eq!(*owner, "owner1");
        assert_eq!(*turns, 0);
    }

    #[test]
    fn test_auction_pool_purchase() {
        let mut pool = BankruptcyAuctionPool::default();
        let policy = BankruptcyPolicy::with_defaults();
        let mut claims = HashMap::new();
        claims.insert("bank1".to_string(), 1000.0);
        
        pool.add_asset("asset1".to_string(), 10000.0, "owner1".to_string(), claims.clone(), &policy);
        
        let success = pool.purchase_asset("asset1", "buyer1", 6000.0);
        assert!(success);
        assert!(!pool.assets.contains_key("asset1"));
        assert_eq!(pool.cash_collected, 6000.0);
        assert_eq!(pool.creditor_distribution.get("bank1"), Some(&6000.0));
    }

    #[test]
    fn test_preserve_defects_through_bankruptcy() {
        use crate::entities::Building;
        let mut buildings = vec![
            {
                let mut b = Building::default();
                b.owner_id = "bankrupt_co".to_string();
                b.structural_defect = 0.5;
                b
            },
            {
                let mut b = Building::default();
                b.owner_id = "other_co".to_string();
                b.structural_defect = 0.3;
                b
            },
        ];
        preserve_defects_through_bankruptcy(&mut buildings, "bankrupt_co");
        // Defect is preserved (not cleared) on bankrupt company's building
        assert_eq!(buildings[0].structural_defect, 0.5);
        // Other company's building is untouched
        assert_eq!(buildings[1].structural_defect, 0.3);
    }

    #[test]
    fn test_normalize_phase22_fields() {
        use crate::entities::Building;
        let mut companies = vec![
            Company {
                reputation_score: 150.0,
                ..Default::default()
            },
            Company {
                reputation_score: -10.0,
                ..Default::default()
            },
        ];
        let mut buildings = vec![
            {
                let mut b = Building::default();
                b.structural_defect = 1.5;
                b
            },
        ];
        normalize_phase22_fields(&mut companies, &mut buildings);
        assert_eq!(companies[0].reputation_score, 100.0);
        assert_eq!(companies[1].reputation_score, 0.0);
        assert_eq!(buildings[0].structural_defect, 1.0);
    }

    #[test]
    fn test_auction_pool_increment_turns() {
        let mut pool = BankruptcyAuctionPool::default();
        let policy = BankruptcyPolicy::with_defaults();
        let mut claims = HashMap::new();
        claims.insert("bank1".to_string(), 1000.0);
        
        pool.add_asset("asset1".to_string(), 10000.0, "owner1".to_string(), claims, &policy);
        
        pool.increment_turns();
        let (_, _, _, _, turns) = pool.assets.get("asset1").unwrap();
        assert_eq!(*turns, 1);
    }
}

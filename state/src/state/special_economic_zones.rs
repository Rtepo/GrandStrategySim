//! Special Economic Zones (SSE) for Phase 4 Economic Bridge.
//!
//! This module defines territorial legal constructs (Special Economic Zones) that
//! politicians create to reward capital, with tax benefits, investment subventions,
//! and clawback mechanics.

#![allow(missing_docs)]

use crate::entities::Company;
use crate::securities::BrokerageAccount;
use crate::state::Treasury;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Special Economic Zone attached to a specific Region
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SpecialEconomicZone {
    /// Zone identifier (e.g., "[SSE-WAR-001]")
    #[serde(default)]
    pub id: String,
    
    /// Zone name (e.g., "Warsaw Technology Park")
    #[serde(default)]
    pub name: String,
    
    /// Region where zone is located
    #[serde(default)]
    pub region_id: String,
    
    /// Zone type (technology, manufacturing, logistics)
    #[serde(default)]
    pub zone_type: SpecialEconomicZoneType,
    
    /// Corporate income tax discount (e.g., 0.5 = 50% reduction)
    #[serde(default)]
    pub corporate_income_tax_discount: f64,
    
    /// Property tax discount (e.g., 0.8 = 80% reduction)
    #[serde(default)]
    pub property_tax_discount: f64,
    
    /// VAT exemption (true = exempt from VAT)
    #[serde(default)]
    pub vat_exemption: bool,
    
    /// Minimum fixed capital requirement for eligibility
    #[serde(default)]
    pub minimum_fixed_capital: f64,
    
    /// Minimum employment requirement
    #[serde(default)]
    pub minimum_employment: u32,
    
    /// Eligible companies (by company_id)
    #[serde(default)]
    pub eligible_companies: Vec<String>,
    
    /// Turn when zone was established
    #[serde(default)]
    pub establishment_turn: u32,
    
    /// Turn when zone expires (0 = permanent)
    #[serde(default)]
    pub expiration_turn: u32,
    
    /// PHASE 4 ADVANCED: Annual budget funded by Treasury
    #[serde(default)]
    pub budget: f64,
    
    /// PHASE 4 ADVANCED: Investment subventions granted to companies
    #[serde(default)]
    pub investment_subventions: Vec<InvestmentSubvention>,
    
    /// PHASE 4 ADVANCED: Minimum operation turns before clawback
    #[serde(default)]
    pub minimum_operation_turns: u32,
    
    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Investment subvention granted by SSE to company
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InvestmentSubvention {
    /// Subvention ID
    #[serde(default)]
    pub id: String,
    
    /// Receiving company ID
    #[serde(default)]
    pub company_id: String,
    
    /// Amount granted
    #[serde(default)]
    pub amount: f64,
    
    /// Turn when subvention was granted
    #[serde(default)]
    pub granted_turn: u32,
    
    /// Turn when company must convert to fixed_capital
    #[serde(default)]
    pub conversion_deadline: u32,
    
    /// Whether subvention was converted to fixed_capital
    #[serde(default)]
    pub converted: bool,
    
    /// Whether clawback was triggered
    #[serde(default)]
    pub clawed_back: bool,
    
    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum SpecialEconomicZoneType {
    #[default]

    Technology,  // Technology park / innovation hub
    

    Industrial,  // Manufacturing zone
    

    Logistics,  // Logistics / transport hub
    

    Financial,  // Financial services zone
    

    Tourism,  // Tourism / entertainment zone
}

#[derive(Debug, Clone, PartialEq)]
pub enum SSEError {
    InsufficientBudget,
    CompanyNotEligible,
}

/// Get SSE tax multiplier for a specific company (safe, no global mutation)
pub fn get_sse_tax_multiplier(
    company: &Company,
    special_economic_zones: &[SpecialEconomicZone],
    current_turn: u32,
) -> f64 {
    let applicable_zone = special_economic_zones.iter()
        .find(|z| z.region_id == company.region_id && z.eligible_companies.contains(&company.id));
    
    match applicable_zone {
        Some(zone) => {
            // Check eligibility requirements
            if company.fixed_capital < zone.minimum_fixed_capital {
                return 1.0;  // No discount
            }
            
            // Calculate vesting period bonus (tax discounts scale with time in zone)
            let turns_in_zone = current_turn.saturating_sub(zone.establishment_turn);
            let vesting_factor = (turns_in_zone as f64 / 10.0).min(1.0);  // Full discount after 10 turns
            
            // Apply vesting-scaled discount
            let base_discount = zone.corporate_income_tax_discount;
            let vested_discount = base_discount * vesting_factor;
            
            1.0 - vested_discount  // Return multiplier (e.g., 0.5 = 50% of base rate)
        }
        None => 1.0  // No discount
    }
}

/// Apply SSE property tax rebate to company
pub fn apply_sse_property_tax_rebate(
    company: &mut Company,
    special_economic_zones: &[SpecialEconomicZone],
    treasury: &mut Treasury,
) -> f64 {
    let applicable_zone = special_economic_zones.iter()
        .find(|z| z.region_id == company.region_id && z.eligible_companies.contains(&company.id));
    
    match applicable_zone {
        Some(zone) => {
            if zone.property_tax_discount <= 0.0 {
                return 0.0;
            }
            
            // Calculate proxy property tax based on fixed capital
            // Property tax rate is typically 0.5-2% of fixed capital annually
            let proxy_property_tax_rate = 0.01;  // 1% of fixed capital
            let proxy_tax = company.fixed_capital * proxy_property_tax_rate;
            let rebate = proxy_tax * zone.property_tax_discount;
            
            // Deduct from Treasury (double-entry compliance)
            treasury.liquid_reserves -= rebate;
            
            // Credit rebate directly to company brokerage account
            if company.brokerage_account.is_none() {
                company.brokerage_account = Some(BrokerageAccount {
                    cash: 0.0,
                    fx_balances: HashMap::new(),
                    portfolio: std::collections::BTreeMap::new(),
                    pending_orders: std::collections::BTreeMap::new(),
                    frozen_cash: 0.0,
                    is_frozen: false,
                    margin_account: None,
                    extra: HashMap::new(),
                });
            }
            
            company.brokerage_account.as_mut().unwrap().cash += rebate;
            rebate
        }
        None => 0.0
    }
}

/// Check if company is VAT exempt due to SSE
pub fn apply_sse_vat_exemption(
    company: &Company,
    special_economic_zones: &[SpecialEconomicZone],
) -> bool {
    special_economic_zones.iter()
        .any(|z| z.region_id == company.region_id 
            && z.eligible_companies.contains(&company.id) 
            && z.vat_exemption)
}

/// Calculate corporate tax with SSE discount
pub fn calculate_corporate_tax_with_sse(
    company: &Company,
    base_tax_rate: f64,
    special_economic_zones: &[SpecialEconomicZone],
    current_turn: u32,
) -> f64 {
    let multiplier = get_sse_tax_multiplier(company, special_economic_zones, current_turn);
    base_tax_rate * multiplier
}

/// Grant investment subvention to company
pub fn grant_investment_subvention(
    zone: &mut SpecialEconomicZone,
    company: &mut Company,
    treasury: &mut Treasury,
    amount: f64,
    current_turn: u32,
    conversion_deadline_turns: u32,
) -> Result<(), SSEError> {
    // Check SSE budget allocation limit
    if zone.budget < amount {
        return Err(SSEError::InsufficientBudget);
    }
    
    // Check company eligibility
    if !zone.eligible_companies.contains(&company.id) {
        return Err(SSEError::CompanyNotEligible);
    }
    
    // Deduct from SSE budget allocation limit
    zone.budget -= amount;
    
    // Deduct from Treasury (THIS is the actual cash transfer - double-entry)
    treasury.liquid_reserves -= amount;
    
    // Credit to company brokerage account
    if company.brokerage_account.is_none() {
        company.brokerage_account = Some(BrokerageAccount {
            cash: 0.0,
            fx_balances: HashMap::new(),
            portfolio: std::collections::BTreeMap::new(),
            pending_orders: std::collections::BTreeMap::new(),
            frozen_cash: 0.0,
            is_frozen: false,
            margin_account: None,
            extra: HashMap::new(),
        });
    }
    
    company.brokerage_account.as_mut().unwrap().cash += amount;
    
    // Record subvention
    zone.investment_subventions.push(InvestmentSubvention {
        id: format!("[SUBV-{}]", zone.investment_subventions.len()),
        company_id: company.id.clone(),
        amount,
        granted_turn: current_turn,
        conversion_deadline: current_turn + conversion_deadline_turns,
        converted: false,
        clawed_back: false,
        extra: Map::new(),
    });
    
    Ok(())
}

/// Process subvention conversions (borrow-checker safe)
pub fn process_subvention_conversions(
    zone: &mut SpecialEconomicZone,
    companies: &mut [Company],
    treasury: &mut Treasury,
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();
    let mut companies_to_remove: Vec<String> = Vec::new();
    
    for subvention in zone.investment_subventions.iter_mut() {
        if subvention.converted || subvention.clawed_back {
            continue;
        }
        
        // Find the company
        if let Some(company) = companies.iter_mut().find(|c| c.id == subvention.company_id) {
            // Check if conversion deadline has passed
            if current_turn >= subvention.conversion_deadline {
                // Auto-convert if company has sufficient cash
                let company_cash = company.brokerage_account.as_ref()
                    .map(|a| a.cash)
                    .unwrap_or(0.0);
                
                if company_cash >= subvention.amount {
                    // Convert to fixed_capital
                    company.brokerage_account.as_mut().unwrap().cash -= subvention.amount;
                    company.fixed_capital += subvention.amount;
                    subvention.converted = true;
                    
                    messages.push(format!(
                        "[SEZ] Company {} converted subsidy {} to fixed capital",
                        company.id, subvention.id
                    ));
                } else {
                    // Trigger clawback for non-conversion
                    execute_clawback(company, subvention, treasury);  // NO zone parameter
                    companies_to_remove.push(company.id.clone());
                    messages.push(format!(
                        "[SEZ] Company {} did not convert subsidy {} - funds recovered",
                        company.id, subvention.id
                    ));
                }
            }
        }
    }
    
    // AFTER the loop (borrow checker satisfied), remove companies from eligible list
    zone.eligible_companies.retain(|id| !companies_to_remove.contains(id));
    
    messages
}

/// Execute clawback (borrow-checker safe - does NOT take zone)
pub fn execute_clawback(
    company: &mut Company,
    subvention: &mut InvestmentSubvention,
    treasury: &mut Treasury,
) {
    // Convert subvention to company debt
    company.liabilities += subvention.amount;
    
    // Record as Treasury receivable (double-entry: liability ↔ receivable)
    *treasury.outstanding_corporate_debts
        .entry(company.id.clone())
        .or_insert(0.0) += subvention.amount;
    
    subvention.clawed_back = true;
    // NOTE: Does NOT take &mut zone to avoid borrow checker violation
    // Zone.eligible_companies removal handled by caller after loop
}

/// Check zone eligibility for clawback (borrow-checker safe)
pub fn check_zone_eligibility_for_clawback(
    zone: &mut SpecialEconomicZone,
    companies: &mut [Company],
    treasury: &mut Treasury,
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();
    let mut companies_to_remove: Vec<String> = Vec::new();
    
    for subvention in zone.investment_subventions.iter_mut() {
        if subvention.converted || subvention.clawed_back {
            continue;
        }
        
        // Check if company is still in the zone
        let company_still_in_zone = companies.iter()
            .any(|c| c.id == subvention.company_id && c.region_id == zone.region_id);
        
        if !company_still_in_zone {
            // Company left the zone - trigger clawback
            if let Some(company) = companies.iter_mut().find(|c| c.id == subvention.company_id) {
                execute_clawback(company, subvention, treasury);  // NO zone parameter
                companies_to_remove.push(company.id.clone());
                messages.push(format!(
                    "[SEZ] Company {} left zone before minimum period - subsidy {} recovered",
                    company.id, subvention.id
                ));
            }
        }
        
        // Check minimum operation turns
        let turns_since_grant = current_turn.saturating_sub(subvention.granted_turn);
        if turns_since_grant < zone.minimum_operation_turns && !subvention.converted {
            // Company hasn't met minimum operation period - trigger clawback
            if let Some(company) = companies.iter_mut().find(|c| c.id == subvention.company_id) {
                execute_clawback(company, subvention, treasury);  // NO zone parameter
                companies_to_remove.push(company.id.clone());
                messages.push(format!(
                    "[SEZ] Company {} did not meet minimum operating period - subsidy {} recovered",
                    company.id, subvention.id
                ));
            }
        }
    }
    
    // AFTER the loop (borrow checker satisfied), remove companies from eligible list
    zone.eligible_companies.retain(|id| !companies_to_remove.contains(id));
    
    messages
}

/// Fund SSE budgets (allocation only, no cash transfer)
pub fn fund_sse_budgets(
    special_economic_zones: &mut [SpecialEconomicZone],
    total_budget_allocation: f64,
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();
    
    if special_economic_zones.is_empty() {
        return messages;
    }
    
    // Divide budget equally among active zones
    let budget_per_zone = total_budget_allocation / special_economic_zones.len() as f64;
    
    for zone in special_economic_zones.iter_mut() {
        // Check if zone is still active (not expired)
        if zone.expiration_turn > 0 && current_turn >= zone.expiration_turn {
            continue;  // Zone has expired, skip funding
        }
        
        // Allocate budget as an allowance limit (NOT a cash transfer)
        // No money moves here - this is just setting the spending limit
        zone.budget += budget_per_zone;
        
        messages.push(format!(
            "[SEZ] Zone {} received allocation limit: {:.2}",
            zone.name, budget_per_zone
        ));
    }
    
    messages
}

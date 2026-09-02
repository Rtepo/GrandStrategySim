//! Transfer Settler (Phase 16A — Plugging the Black Holes).
//!
//! Canonical double-entry settlement for all fiat transfers between entities.
//! Every deduction from `brokerage_account.cash` that sends money to a non-bank
//! entity MUST route through `settle_transfer()` to ensure bank balance sheets
//! stay synchronized with reality.
//!
//! ## Double-Entry Rules
//!
//! When a company pays money out:
//! 1. `company.brokerage_account.cash -= amount` (or `available_cash` if no brokerage)
//! 2. `bank.balance_sheet.deposits -= amount` (bank liability decreases — deposit extinguished)
//! 3. `bank.balance_sheet.reserves_at_central_bank -= amount` (bank asset decreases — reserves transfer out)
//! 4. Recipient receives `amount` (Treasury, citizen savings, or another company)
//!
//! When a company receives money (e.g. B2C revenue):
//! 1. `company.brokerage_account.cash += amount` (or `available_cash` if no brokerage)
//! 2. `bank.balance_sheet.deposits += amount` (bank liability increases — new deposit)
//! 3. `bank.balance_sheet.reserves_at_central_bank += amount` (bank asset increases — reserves transfer in)

use crate::entities::Company;
use crate::society::geography::{Region, RuralClass, UrbanClass};
use crate::state::Country;
use std::collections::HashMap;

/// What kind of recipient receives the transfer.
#[derive(Debug, Clone)]
pub enum TransferRecipient {
    /// Credit to `country.budget.liquid_reserves`.
    Treasury,
    /// Credit to a specific class demographics savings in a region.
    CitizenSavings {
        /// Index into `country.regions`.
        region_idx: usize,
        /// True for rural classes, false for urban.
        is_rural: bool,
        /// Class key in `rural_classes` or `urban_classes`.
        class_key: String,
    },
    /// Credit to another company's brokerage account (inter-bank or intra-bank).
    OtherCompany {
        /// Index into the `companies` slice.
        recipient_idx: usize,
    },
    /// Credit to a foreign entity (FX outflow — money leaves the system).
    ForeignEntity,
    /// Credit to the Central Bank (Lombard repayment, reserve deposit).
    CentralBank,
}

/// Error types for transfer settlement.
#[derive(Debug, Clone, PartialEq)]
pub enum TransferError {
    /// Payer does not have enough cash for the transfer.
    InsufficientCash,
    /// Payer has no `primary_bank_id` — cash-only transfer, no bank sync.
    NoPrimaryBank,
    /// Bank company not found in the companies slice.
    BankNotFound,
    /// Amount must be positive (> 0.0).
    InvalidAmount,
    /// Recipient company index out of bounds.
    RecipientNotFound,
    /// Phase 46: Bank has insufficient reserves for this transfer.
    /// Transfer rejected to prevent negative reserves (no silent clamping).
    InsufficientReserves,
    /// Phase 46: Central Bank has insufficient FX reserves for this foreign transfer.
    /// Convertibility is suspended (capital controls). Transfer rejected entirely.
    InsufficientFxReserves,
}

/// Result of a successful transfer.
#[derive(Debug, Clone, Default)]
pub struct TransferResult {
    /// Actual amount transferred (may be less than requested if clamped).
    pub amount_transferred: f64,
    /// Bank ID of the payer (if any).
    pub payer_bank_id: Option<String>,
    /// Bank ID of the recipient (if any).
    pub recipient_bank_id: Option<String>,
    /// True if this was an inter-bank transfer (different banks).
    pub inter_bank: bool,
    /// Phase 41: VAT amount collected from this B2C transaction (for treasury credit).
    pub vat_amount: f64,
}

/// Internal helper: find a bank company by ID and adjust its balance sheet.
///
/// # Returns
/// `true` if the bank was found and adjusted, `false` otherwise.
///
/// Phase 46: Silent clamping of negative reserves removed. Callers must use
/// `would_cause_negative_reserves` to pre-check before calling this function.
/// Internal helper: find a bank company by ID and adjust its balance sheet.
/// Mapped version: uses a pre-computed `id -> idx` table for O(1) lookup.
fn adjust_bank_balance<S: std::hash::BuildHasher>(
    companies: &mut [Company],
    id_to_idx: &HashMap<String, usize, S>,
    bank_id: &str,
    deposit_delta: f64,
    reserve_delta: f64,
) -> bool {
    if let Some(&idx) = id_to_idx.get(bank_id) {
        if let Some(ref mut bs) = companies[idx].balance_sheet {
            bs.deposits += deposit_delta;
            bs.reserves_at_central_bank += reserve_delta;
            return true;
        }
    }
    false
}

/// Unmapped fallback for helper functions that are called without an index map.
/// Searches linearly for the bank; prefer `adjust_bank_balance` when a map exists.
fn adjust_bank_balance_unmapped(
    companies: &mut [Company],
    bank_id: &str,
    deposit_delta: f64,
    reserve_delta: f64,
) -> bool {
    if let Some(bank) = companies.iter_mut().find(|c| c.id == bank_id) {
        if let Some(ref mut bs) = bank.balance_sheet {
            bs.deposits += deposit_delta;
            bs.reserves_at_central_bank += reserve_delta;
            return true;
        }
    }
    false
}

/// Phase 46: Pre-check whether a bank balance adjustment would cause negative reserves.
/// Mapped version: uses a pre-computed `id -> idx` table for O(1) lookup.
fn would_cause_negative_reserves<S: std::hash::BuildHasher>(
    companies: &[Company],
    id_to_idx: &HashMap<String, usize, S>,
    bank_id: &str,
    reserve_delta: f64,
) -> bool {
    if reserve_delta >= 0.0 {
        return false;
    }
    if let Some(&idx) = id_to_idx.get(bank_id) {
        if let Some(ref bs) = companies[idx].balance_sheet {
            return bs.reserves_at_central_bank + reserve_delta < 0.0;
        }
    }
    false
}

/// Settle a fiat transfer from a company to a recipient outside the banking system.
///
/// # Arguments
/// * `companies` - Mutable slice of all companies (banks and non-banks).
/// * `payer_idx` - Index of the paying company in `companies`.
/// * `amount` - Amount to transfer (must be > 0.0).
/// * `recipient` - What kind of recipient receives the transfer.
/// * `country` - Mutable country state (for Treasury, citizen savings, CB).
///
/// # Returns
/// `Ok(TransferResult)` on success, `Err(TransferError)` on failure.
///
/// # Rules
/// - If company has no `primary_bank_id`, the transfer is cash-only (no bank balance sheet impact).
/// - If bank has no `balance_sheet`, proceed with cash-only transfer.
/// - If payer's cash < amount, returns `InsufficientCash`.
/// - For `OtherCompany` recipient: if same bank, deposits/reserves unchanged (intra-bank).
/// - For `OtherCompany` recipient: if different bank, inter-bank reserve transfer occurs.
/// - For `ForeignEntity`: money leaves the system (no recipient credit).
/// - For `CentralBank`: money is extinguished (deposit destroyed, reserves returned to CB).
pub fn settle_transfer(
    companies: &mut [Company],
    payer_idx: usize,
    amount: f64,
    recipient: &TransferRecipient,
    country: &mut Country,
) -> Result<TransferResult, TransferError> {
    let id_to_idx: HashMap<String, usize> = companies
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), i))
        .collect();
    settle_transfer_mapped(companies, &id_to_idx, payer_idx, amount, recipient, country)
}

/// O(1) mapped variant of `settle_transfer` for callers that already have an `id -> idx` map.
pub fn settle_transfer_mapped<S: std::hash::BuildHasher>(
    companies: &mut [Company],
    id_to_idx: &HashMap<String, usize, S>,
    payer_idx: usize,
    amount: f64,
    recipient: &TransferRecipient,
    country: &mut Country,
) -> Result<TransferResult, TransferError> {
    if amount <= 0.0 {
        return Err(TransferError::InvalidAmount);
    }
    if payer_idx >= companies.len() {
        return Err(TransferError::RecipientNotFound);
    }

    // Phase 46: FX reserve pre-check for foreign transfers.
    // Must happen BEFORE any debits to ensure atomicity (no state mutated on rejection).
    // If the Central Bank cannot cover the FX conversion, convertibility is suspended.
    if matches!(recipient, TransferRecipient::ForeignEntity) {
        let total_fx: f64 = country.central_bank.fx_reserves.values().sum();
        if total_fx < amount {
            return Err(TransferError::InsufficientFxReserves);
        }
    }

    // Read payer's bank ID before mutating
    let payer_bank_id = companies[payer_idx].primary_bank_id.clone();

    // For OtherCompany recipients, check if intra-bank (same bank) — if so, skip bank adjustments
    let is_intra_bank = match recipient {
        TransferRecipient::OtherCompany { recipient_idx } => {
            if *recipient_idx >= companies.len() {
                return Err(TransferError::RecipientNotFound);
            }
            let r_bank = companies[*recipient_idx].primary_bank_id.clone();
            payer_bank_id.is_some() && payer_bank_id == r_bank
        }
        _ => false,
    };

    // Phase 46: Pre-check bank reserves before debiting.
    // If the bank's reserves would go negative, reject the transfer entirely.
    // This replaces the old silent clamping behavior.
    if !is_intra_bank {
        if let Some(ref bank_id) = payer_bank_id {
            if would_cause_negative_reserves(companies, id_to_idx, bank_id, -amount) {
                return Err(TransferError::InsufficientReserves);
            }
        }
    }

    // Check and debit payer's cash
    let has_brokerage = companies[payer_idx].brokerage_account.is_some();
    if has_brokerage {
        let cash = companies[payer_idx]
            .brokerage_account
            .as_ref()
            .unwrap()
            .cash;
        if cash < amount {
            return Err(TransferError::InsufficientCash);
        }
        companies[payer_idx]
            .brokerage_account
            .as_mut()
            .unwrap()
            .cash -= amount;
    } else {
        if companies[payer_idx].available_cash < amount {
            return Err(TransferError::InsufficientCash);
        }
        companies[payer_idx].available_cash -= amount;
    }

    // Debit payer's bank only if money leaves the bank (not intra-bank)
    if !is_intra_bank {
        if let Some(ref bank_id) = payer_bank_id {
            adjust_bank_balance(companies, id_to_idx, bank_id, -amount, -amount);
        }
    }

    let mut result = TransferResult {
        amount_transferred: amount,
        payer_bank_id: payer_bank_id.clone(),
        ..Default::default()
    };

    // Credit recipient
    match recipient {
        TransferRecipient::Treasury => {
            country.budget.liquid_reserves += amount;
        }
        TransferRecipient::CitizenSavings {
            region_idx,
            is_rural,
            class_key,
        } => {
            if let Some(region) = country.regions.get_mut(*region_idx) {
                if *is_rural {
                    if let Some(rural_key) = RuralClass::from_str(class_key) {
                        if let Some(demo) = region.class_demographics.rural_classes.get_mut(&rural_key) {
                            demo.savings += amount;
                        }
                    }
                } else {
                    if let Some(urban_key) = UrbanClass::from_str(class_key) {
                        if let Some(demo) = region.class_demographics.urban_classes.get_mut(&urban_key) {
                            demo.savings += amount;
                        }
                    }
                }
            }
        }
        TransferRecipient::OtherCompany { recipient_idx } => {
            let recipient_bank_id = companies[*recipient_idx].primary_bank_id.clone();
            result.recipient_bank_id = recipient_bank_id.clone();

            // Credit recipient's cash
            if let Some(ref mut ba) = companies[*recipient_idx].brokerage_account {
                ba.cash += amount;
            } else {
                companies[*recipient_idx].available_cash += amount;
            }

            // Credit recipient's bank only if inter-bank transfer (different banks)
            if !is_intra_bank {
                if let Some(ref r_bank_id) = recipient_bank_id {
                    result.inter_bank = true;
                    adjust_bank_balance(companies, id_to_idx, r_bank_id, amount, amount);
                }
            }
        }
        TransferRecipient::ForeignEntity => {
            // Phase 46: Domestic deposit extinguished (bank debit already applied above).
            // Central Bank loses FX reserves equal to the converted value.
            // Debit proportionally from all FX reserve holdings (basket drawdown).
            // The pre-check above guarantees total_fx >= amount.
            let total_fx: f64 = country.central_bank.fx_reserves.values().sum();
            if total_fx > 0.0 {
                let ratio = amount / total_fx;
                for balance in country.central_bank.fx_reserves.values_mut() {
                    *balance -= *balance * ratio;
                }
            }
        }
        TransferRecipient::CentralBank => {
            // Money is extinguished — deposit destroyed, reserves returned to CB
            // The bank debit above already reduced deposits and reserves.
            // No further action needed.
        }
    }

    Ok(result)
}

/// Settle a B2C purchase: citizens pay a retail company.
///
/// This is the reverse flow — citizens are the payer, company is the recipient.
/// Citizen savings are debited; the company's brokerage account is credited;
/// the company's bank balance sheet is synced (deposits and reserves increase).
///
/// # Arguments
/// * `companies` - Mutable slice of all companies.
/// * `recipient_company_idx` - Index of the retail company receiving payment.
/// * `amount` - Amount to transfer (must be > 0.0).
/// * `region` - Mutable region containing class demographics.
/// * `is_rural` - True for rural class, false for urban.
/// * `class_key` - Class key in `rural_classes` or `urban_classes`.
///
/// # Returns
/// `Ok(TransferResult)` on success. The `amount_transferred` may be less than
/// requested if citizen savings are insufficient (clamped to available).
pub fn settle_b2c_purchase(
    companies: &mut [Company],
    recipient_company_idx: usize,
    amount: f64,
    region: &mut Region,
    is_rural: bool,
    class_key: &str,
    vat_amount: f64,
) -> Result<TransferResult, TransferError> {
    if amount <= 0.0 {
        return Err(TransferError::InvalidAmount);
    }
    if recipient_company_idx >= companies.len() {
        return Err(TransferError::RecipientNotFound);
    }

    // Phase 41: Debit citizen savings by the TOTAL amount (base + VAT).
    // The company gets the base amount, the treasury gets the VAT.
    let total_debit = amount + vat_amount;

    let actual_total = {
        if is_rural {
            RuralClass::from_str(class_key)
                .and_then(|k| region.class_demographics.rural_classes.get_mut(&k))
                .map(|demo| {
                    let affordable = total_debit.min(demo.savings);
                    demo.savings -= affordable;
                    affordable
                })
                .unwrap_or(0.0)
        } else {
            UrbanClass::from_str(class_key)
                .and_then(|k| region.class_demographics.urban_classes.get_mut(&k))
                .map(|demo| {
                    let affordable = total_debit.min(demo.savings);
                    demo.savings -= affordable;
                    affordable
                })
                .unwrap_or(0.0)
        }
    };

    if actual_total <= 0.0 {
        return Ok(TransferResult {
            amount_transferred: 0.0,
            vat_amount: 0.0,
            ..Default::default()
        });
    }

    // Phase 41: Split the actual debited amount into base and VAT proportionally.
    let actual_base = if total_debit > 0.0 {
        actual_total * (amount / total_debit)
    } else {
        actual_total
    };
    let actual_vat = actual_total - actual_base;

    // Credit company with the base amount only
    if let Some(ref mut ba) = companies[recipient_company_idx].brokerage_account {
        ba.cash += actual_base;
    } else {
        companies[recipient_company_idx].available_cash += actual_base;
    }

    // Sync recipient's bank: deposits and reserves increase by base amount only
    // (VAT goes to treasury, not bank deposits)
    let recipient_bank_id = companies[recipient_company_idx].primary_bank_id.clone();
    if let Some(ref bank_id) = recipient_bank_id {
        adjust_bank_balance_unmapped(companies, bank_id, actual_base, actual_base);
    }

    Ok(TransferResult {
        amount_transferred: actual_base,
        vat_amount: actual_vat,
        recipient_bank_id,
        ..Default::default()
    })
}

/// Convenience: settle a transfer from a company to the Treasury (taxes, fines, remittances).
///
/// # Arguments
/// * `companies` - Mutable slice of all companies.
/// * `payer_idx` - Index of the paying company.
/// * `amount` - Amount to transfer.
/// * `country` - Mutable country state.
///
/// # Returns
/// `Ok(TransferResult)` on success, `Err(TransferError)` on failure.
pub fn settle_transfer_to_treasury(
    companies: &mut [Company],
    payer_idx: usize,
    amount: f64,
    country: &mut Country,
) -> Result<TransferResult, TransferError> {
    settle_transfer(
        companies,
        payer_idx,
        amount,
        &TransferRecipient::Treasury,
        country,
    )
}

/// Convenience: settle a wage payment from a company to citizen savings.
///
/// # Arguments
/// * `companies` - Mutable slice of all companies.
/// * `payer_idx` - Index of the paying company.
/// * `amount` - Amount to transfer.
/// * `country` - Mutable country state.
/// * `region_idx` - Index into `country.regions`.
/// * `is_rural` - True for rural class, false for urban.
/// * `class_key` - Class key in `rural_classes` or `urban_classes`.
pub fn settle_wage_payment(
    companies: &mut [Company],
    payer_idx: usize,
    amount: f64,
    country: &mut Country,
    region_idx: usize,
    is_rural: bool,
    class_key: &str,
) -> Result<TransferResult, TransferError> {
    settle_transfer(
        companies,
        payer_idx,
        amount,
        &TransferRecipient::CitizenSavings {
            region_idx,
            is_rural,
            class_key: class_key.to_string(),
        },
        country,
    )
}

/// Convenience: settle a transfer between two companies (B2B, B2C revenue).
///
/// # Arguments
/// * `companies` - Mutable slice of all companies.
/// * `payer_idx` - Index of the paying company.
/// * `recipient_idx` - Index of the receiving company.
/// * `amount` - Amount to transfer.
/// * `country` - Mutable country state.
pub fn settle_company_to_company(
    companies: &mut [Company],
    payer_idx: usize,
    recipient_idx: usize,
    amount: f64,
    country: &mut Country,
) -> Result<TransferResult, TransferError> {
    settle_transfer(
        companies,
        payer_idx,
        amount,
        &TransferRecipient::OtherCompany { recipient_idx },
        country,
    )
}

/// Phase 22A: Convenience: settle a transfer from the State Treasury to a company.
///
/// Used for state-funded tender tranche payments. The Treasury is not a
/// `Company`, so this helper debits `country.budget.liquid_reserves` directly
/// and credits the recipient company's cash, syncing the recipient's bank
/// balance sheet (deposits + reserves increase).
///
/// # Arguments
/// * `companies` - Mutable slice of all companies.
/// * `recipient_idx` - Index of the receiving company.
/// * `amount` - Amount to transfer (must be > 0.0).
/// * `country` - Mutable country state (Treasury debited).
///
/// # Returns
/// `Ok(TransferResult)` on success, `Err(TransferError)` on failure.
///
/// # Rules
/// * If Treasury reserves < amount, returns `InsufficientCash`.
/// * Recipient's bank balance sheet is synced (deposits + reserves increase).
/// * No payer bank debit (Treasury is not a bank depositor).
pub fn settle_treasury_to_company(
    companies: &mut [Company],
    recipient_idx: usize,
    amount: f64,
    country: &mut Country,
) -> Result<TransferResult, TransferError> {
    if amount <= 0.0 {
        return Err(TransferError::InvalidAmount);
    }
    if recipient_idx >= companies.len() {
        return Err(TransferError::RecipientNotFound);
    }
    if country.budget.liquid_reserves < amount {
        return Err(TransferError::InsufficientCash);
    }

    // Debit Treasury
    country.budget.liquid_reserves -= amount;

    // Credit recipient's cash
    if let Some(ref mut ba) = companies[recipient_idx].brokerage_account {
        ba.cash += amount;
    } else {
        companies[recipient_idx].available_cash += amount;
    }

    // Sync recipient's bank: deposits and reserves increase
    let recipient_bank_id = companies[recipient_idx].primary_bank_id.clone();
    if let Some(ref bank_id) = recipient_bank_id {
        adjust_bank_balance_unmapped(companies, bank_id, amount, amount);
    }

    Ok(TransferResult {
        amount_transferred: amount,
        recipient_bank_id,
        ..Default::default()
    })
}

/// Debit citizen savings from all classes in a region proportionally to their savings.
///
/// Returns the actual amount debited (may be less than requested if savings are insufficient).
pub fn debit_citizen_savings_region(region: &mut Region, amount: f64) -> f64 {
    if amount <= 0.0 {
        return 0.0;
    }

    let mut class_info: Vec<(bool, String, f64)> = Vec::new();
    let mut total_savings = 0.0;
    for (key, demo) in &region.class_demographics.rural_classes {
        if demo.savings > 0.0 {
            class_info.push((true, key.to_string(), demo.savings));
            total_savings += demo.savings;
        }
    }
    for (key, demo) in &region.class_demographics.urban_classes {
        if demo.savings > 0.0 {
            class_info.push((false, key.to_string(), demo.savings));
            total_savings += demo.savings;
        }
    }

    if total_savings <= 0.0 {
        return 0.0;
    }

    let actual = amount.min(total_savings);
    for (is_rural, key, class_savings) in &class_info {
        let share = class_savings / total_savings;
        let debit = actual * share;
        if *is_rural {
            if let Some(rk) = RuralClass::from_str(key) {
                if let Some(demo) = region.class_demographics.rural_classes.get_mut(&rk) {
                    demo.savings -= debit;
                }
            }
        } else {
            if let Some(uk) = UrbanClass::from_str(key) {
                if let Some(demo) = region.class_demographics.urban_classes.get_mut(&uk) {
                    demo.savings -= debit;
                }
            }
        }
    }

    actual
}

/// Credit citizen savings across all classes in a region, distributed proportionally by population.
///
/// Returns the actual amount credited (equal to `amount` if there are citizens, 0 otherwise).
pub fn credit_citizen_savings_region(region: &mut Region, amount: f64) -> f64 {
    if amount <= 0.0 {
        return 0.0;
    }

    let mut class_info: Vec<(bool, String, f64)> = Vec::new();
    let mut total_pop = 0.0;
    for (key, demo) in &region.class_demographics.rural_classes {
        if demo.population > 0 {
            class_info.push((true, key.to_string(), demo.population as f64));
            total_pop += demo.population as f64;
        }
    }
    for (key, demo) in &region.class_demographics.urban_classes {
        if demo.population > 0 {
            class_info.push((false, key.to_string(), demo.population as f64));
            total_pop += demo.population as f64;
        }
    }

    if total_pop <= 0.0 {
        return 0.0;
    }

    for (is_rural, key, pop) in &class_info {
        let share = pop / total_pop;
        let credit = amount * share;
        if *is_rural {
            if let Some(rk) = RuralClass::from_str(key) {
                if let Some(demo) = region.class_demographics.rural_classes.get_mut(&rk) {
                    demo.savings += credit;
                }
            }
        } else {
            if let Some(uk) = UrbanClass::from_str(key) {
                if let Some(demo) = region.class_demographics.urban_classes.get_mut(&uk) {
                    demo.savings += credit;
                }
            }
        }
    }

    amount
}

/// Credit a company by ID: credits brokerage_account.cash (or available_cash) and syncs bank.
///
/// Returns `true` if the company was found and credited, `false` otherwise.
pub fn credit_company_by_id(companies: &mut [Company], company_id: &str, amount: f64) -> bool {
    if amount <= 0.0 {
        return false;
    }

    let bank_id = if let Some(company) = companies.iter_mut().find(|c| c.id == company_id) {
        if let Some(ba) = &mut company.brokerage_account {
            ba.cash += amount;
        } else {
            company.available_cash += amount;
        }
        company.primary_bank_id.clone()
    } else {
        return false;
    };

    if let Some(ref bank_id) = bank_id {
        adjust_bank_balance_unmapped(companies, bank_id, amount, amount);
    }
    true
}

/// Debit a company by ID: debits brokerage_account.cash (or available_cash) and syncs bank.
///
/// Returns the actual amount debited (may be less than requested if cash is insufficient).
pub fn debit_company_by_id(companies: &mut [Company], company_id: &str, amount: f64) -> f64 {
    if amount <= 0.0 {
        return 0.0;
    }

    let (actual, bank_id) = if let Some(company) = companies.iter_mut().find(|c| c.id == company_id)
    {
        if let Some(ba) = &mut company.brokerage_account {
            let affordable = amount.min(ba.cash);
            ba.cash -= affordable;
            (affordable, company.primary_bank_id.clone())
        } else {
            let affordable = amount.min(company.available_cash);
            company.available_cash -= affordable;
            (affordable, company.primary_bank_id.clone())
        }
    } else {
        return 0.0;
    };

    if actual > 0.0 {
        if let Some(ref bank_id) = bank_id {
            adjust_bank_balance_unmapped(companies, bank_id, -actual, -actual);
        }
    }
    actual
}

/// Sync a company's bank balance sheet for a credit already applied to the company's cash.
///
/// This is used when a company's cash was credited directly (e.g. by `settle_trades`)
/// but the bank's deposits/reserves were not adjusted. This function brings the bank
/// balance sheet in line with the company's increased cash.
///
/// Returns `true` if the bank was found and adjusted, `false` otherwise.
pub fn sync_bank_credit_by_company_id(
    companies: &mut [Company],
    company_id: &str,
    amount: f64,
) -> bool {
    if amount <= 0.0 {
        return false;
    }
    let bank_id = companies
        .iter()
        .find(|c| c.id == company_id)
        .and_then(|c| c.primary_bank_id.clone());

    if let Some(ref bank_id) = bank_id {
        adjust_bank_balance_unmapped(companies, bank_id, amount, amount);
        true
    } else {
        false
    }
}

/// Get the total liquid cash available to a company (brokerage_account.cash + available_cash).
pub fn company_liquid_cash(company: &Company) -> f64 {
    company
        .brokerage_account
        .as_ref()
        .map(|ba| ba.cash)
        .unwrap_or(0.0)
        + company.available_cash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::legal_form::{JointStockData, LegalForm};
    use crate::entities::Company;
    use crate::registries::enums::Sector;
    use crate::society::geography::{ClassDemographics, Region, RegionalClassDemographics};
    use crate::state::banking::BankBalanceSheet;
    use crate::state::{Country, Treasury};
    use std::collections::BTreeMap;

    fn make_test_company(id: &str, cash: f64) -> Company {
        Company::new(
            id.to_string(),
            id.to_string(),
            Sector::LightIndustry,
            LegalForm::JointStockCompany(JointStockData::default()),
            100_000.0,
            cash,
            10,
        )
    }

    fn make_test_bank(id: &str, reserves: f64, deposits: f64) -> Company {
        let mut bank = Company::new(
            id.to_string(),
            id.to_string(),
            Sector::Banking,
            LegalForm::JointStockCompany(JointStockData::default()),
            1_000_000.0,
            0.0,
            5,
        );
        bank.balance_sheet = Some(BankBalanceSheet {
            reserves_at_central_bank: reserves,
            deposits,
            ..Default::default()
        });
        bank.bank_type = Some(crate::state::banking::BankType::Commercial);
        bank
    }

    fn make_test_country() -> Country {
        let mut country = Country::default();
        country.budget = Treasury {
            gdp: 1_000_000.0,
            population: 1000,
            nominal_budget: 500_000.0,
            liquid_reserves: 100_000.0,
            ..Default::default()
        };
        country.regions = vec![Region {
            id: "region_0".to_string(),
            class_demographics: RegionalClassDemographics {
                rural_classes: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        RuralClass::FreePeasant,
                        ClassDemographics {
                            population: 100,
                            savings: 50_000.0,
                            ..Default::default()
                        },
                    );
                    m
                },
                urban_classes: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        UrbanClass::Bourgeoisie,
                        ClassDemographics {
                            population: 200,
                            savings: 80_000.0,
                            ..Default::default()
                        },
                    );
                    m
                },
            },
            ..Default::default()
        }];
        country
    }

    #[test]
    fn test_settle_transfer_to_treasury() {
        let mut companies = vec![
            make_test_company("comp_0", 10_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let mut country = make_test_country();
        let initial_treasury = country.budget.liquid_reserves;
        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;
        let initial_reserves = companies[1]
            .balance_sheet
            .as_ref()
            .unwrap()
            .reserves_at_central_bank;

        let result = settle_transfer_to_treasury(&mut companies, 0, 1_000.0, &mut country);
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.amount_transferred, 1_000.0);

        // Payer cash decreased
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash - 1_000.0
        );
        // Treasury increased
        assert_eq!(country.budget.liquid_reserves, initial_treasury + 1_000.0);
        // Bank deposits decreased
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_deposits - 1_000.0
        );
        // Bank reserves decreased
        assert_eq!(
            companies[1]
                .balance_sheet
                .as_ref()
                .unwrap()
                .reserves_at_central_bank,
            initial_reserves - 1_000.0
        );
    }

    #[test]
    fn test_settle_transfer_insufficient_cash() {
        let mut companies = vec![make_test_company("comp_0", 100.0)];
        let mut country = make_test_country();

        let result = settle_transfer_to_treasury(&mut companies, 0, 1_000.0, &mut country);
        assert!(matches!(result, Err(TransferError::InsufficientCash)));
    }

    #[test]
    fn test_settle_transfer_no_bank() {
        let mut companies = vec![make_test_company("comp_0", 10_000.0)];
        let mut country = make_test_country();
        let initial_treasury = country.budget.liquid_reserves;
        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;

        let result = settle_transfer_to_treasury(&mut companies, 0, 1_000.0, &mut country);
        assert!(result.is_ok());
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash - 1_000.0
        );
        assert_eq!(country.budget.liquid_reserves, initial_treasury + 1_000.0);
    }

    #[test]
    fn test_settle_transfer_inter_bank() {
        let mut companies = vec![
            make_test_company("comp_0", 10_000.0),
            make_test_bank("bank_x", 500_000.0, 400_000.0),
            make_test_company("comp_1", 1.0),
            make_test_bank("bank_y", 300_000.0, 250_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_x".to_string());
        companies[2].primary_bank_id = Some("bank_y".to_string());

        let mut country = make_test_country();
        let initial_payer_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_bank_x_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;
        let initial_bank_x_reserves = companies[1]
            .balance_sheet
            .as_ref()
            .unwrap()
            .reserves_at_central_bank;
        let initial_bank_y_deposits = companies[3].balance_sheet.as_ref().unwrap().deposits;
        let initial_bank_y_reserves = companies[3]
            .balance_sheet
            .as_ref()
            .unwrap()
            .reserves_at_central_bank;

        let result = settle_company_to_company(&mut companies, 0, 2, 1_000.0, &mut country);
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.inter_bank);

        // Payer cash decreased
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_payer_cash - 1_000.0
        );
        // Recipient cash increased (1.0 initial + 1_000.0 received)
        assert_eq!(
            companies[2].brokerage_account.as_ref().unwrap().cash,
            1_001.0
        );
        // Bank X: deposits and reserves decreased
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_bank_x_deposits - 1_000.0
        );
        assert_eq!(
            companies[1]
                .balance_sheet
                .as_ref()
                .unwrap()
                .reserves_at_central_bank,
            initial_bank_x_reserves - 1_000.0
        );
        // Bank Y: deposits and reserves increased
        assert_eq!(
            companies[3].balance_sheet.as_ref().unwrap().deposits,
            initial_bank_y_deposits + 1_000.0
        );
        assert_eq!(
            companies[3]
                .balance_sheet
                .as_ref()
                .unwrap()
                .reserves_at_central_bank,
            initial_bank_y_reserves + 1_000.0
        );
    }

    #[test]
    fn test_settle_transfer_intra_bank() {
        let mut companies = vec![
            make_test_company("comp_0", 10_000.0),
            make_test_bank("bank_x", 500_000.0, 400_000.0),
            make_test_company("comp_1", 1.0),
        ];
        companies[0].primary_bank_id = Some("bank_x".to_string());
        companies[2].primary_bank_id = Some("bank_x".to_string());

        let mut country = make_test_country();
        let initial_bank_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;
        let initial_bank_reserves = companies[1]
            .balance_sheet
            .as_ref()
            .unwrap()
            .reserves_at_central_bank;

        let result = settle_company_to_company(&mut companies, 0, 2, 1_000.0, &mut country);
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(!r.inter_bank);

        // Bank deposits and reserves unchanged (intra-bank)
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_bank_deposits
        );
        assert_eq!(
            companies[1]
                .balance_sheet
                .as_ref()
                .unwrap()
                .reserves_at_central_bank,
            initial_bank_reserves
        );
    }

    #[test]
    fn test_settle_wage_payment() {
        let mut companies = vec![
            make_test_company("comp_0", 10_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let mut country = make_test_country();
        let initial_savings = country.regions[0]
            .class_demographics
            .rural_classes
            .get(&RuralClass::FreePeasant)
            .unwrap()
            .savings;

        let result = settle_wage_payment(
            &mut companies,
            0,
            1_000.0,
            &mut country,
            0,
            true,
            "FreePeasant",
        );
        assert!(result.is_ok());

        // Citizen savings increased
        let new_savings = country.regions[0]
            .class_demographics
            .rural_classes
            .get(&RuralClass::FreePeasant)
            .unwrap()
            .savings;
        assert_eq!(new_savings, initial_savings + 1_000.0);
    }

    #[test]
    fn test_settle_b2c_purchase() {
        let mut companies = vec![
            make_test_company("store_co", 1.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let mut region = Region {
            id: "region_0".to_string(),
            class_demographics: RegionalClassDemographics {
                rural_classes: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        RuralClass::FreePeasant,
                        ClassDemographics {
                            population: 100,
                            savings: 5_000.0,
                            ..Default::default()
                        },
                    );
                    m
                },
                urban_classes: BTreeMap::new(),
            },
            ..Default::default()
        };

        let initial_savings = region
            .class_demographics
            .rural_classes
            .get(&RuralClass::FreePeasant)
            .unwrap()
            .savings;
        let initial_bank_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;
        let initial_bank_reserves = companies[1]
            .balance_sheet
            .as_ref()
            .unwrap()
            .reserves_at_central_bank;

        let result = settle_b2c_purchase(
            &mut companies,
            0,
            1_000.0,
            &mut region,
            true,
            "FreePeasant",
            0.0,
        );
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.amount_transferred, 1_000.0);

        // Citizen savings decreased
        assert_eq!(
            region
                .class_demographics
                .rural_classes
                .get(&RuralClass::FreePeasant)
                .unwrap()
                .savings,
            initial_savings - 1_000.0
        );
        // Company cash increased (1.0 initial + 1_000.0 received)
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            1_001.0
        );
        // Bank deposits increased
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_bank_deposits + 1_000.0
        );
        // Bank reserves increased
        assert_eq!(
            companies[1]
                .balance_sheet
                .as_ref()
                .unwrap()
                .reserves_at_central_bank,
            initial_bank_reserves + 1_000.0
        );
    }

    #[test]
    fn test_settle_b2c_purchase_clamped() {
        let mut companies = vec![make_test_company("store_co", 1.0)];
        let mut region = Region {
            id: "region_0".to_string(),
            class_demographics: RegionalClassDemographics {
                rural_classes: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        RuralClass::FreePeasant,
                        ClassDemographics {
                            population: 100,
                            savings: 500.0,
                            ..Default::default()
                        },
                    );
                    m
                },
                urban_classes: BTreeMap::new(),
            },
            ..Default::default()
        };

        // Request 1_000 but only 500 available
        let result = settle_b2c_purchase(
            &mut companies,
            0,
            1_000.0,
            &mut region,
            true,
            "FreePeasant",
            0.0,
        );
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.amount_transferred, 500.0);
        assert_eq!(companies[0].brokerage_account.as_ref().unwrap().cash, 501.0);
        assert_eq!(
            region
                .class_demographics
                .rural_classes
                .get(&RuralClass::FreePeasant)
                .unwrap()
                .savings,
            0.0
        );
    }

    #[test]
    fn test_settle_transfer_invalid_amount() {
        let mut companies = vec![make_test_company("comp_0", 10_000.0)];
        let mut country = make_test_country();

        let result = settle_transfer_to_treasury(&mut companies, 0, 0.0, &mut country);
        assert!(matches!(result, Err(TransferError::InvalidAmount)));

        let result = settle_transfer_to_treasury(&mut companies, 0, -100.0, &mut country);
        assert!(matches!(result, Err(TransferError::InvalidAmount)));
    }

    #[test]
    fn test_credit_company_by_id_with_bank() {
        let mut companies = vec![
            make_test_company("seller_0", 1_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;
        let initial_reserves = companies[1]
            .balance_sheet
            .as_ref()
            .unwrap()
            .reserves_at_central_bank;

        let ok = credit_company_by_id(&mut companies, "seller_0", 2_000.0);
        assert!(ok);

        // Cash increased
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash + 2_000.0
        );
        // Bank deposits increased
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_deposits + 2_000.0
        );
        // Bank reserves increased
        assert_eq!(
            companies[1]
                .balance_sheet
                .as_ref()
                .unwrap()
                .reserves_at_central_bank,
            initial_reserves + 2_000.0
        );
    }

    #[test]
    fn test_credit_company_by_id_no_bank() {
        let mut companies = vec![make_test_company("seller_0", 1_000.0)];

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;

        let ok = credit_company_by_id(&mut companies, "seller_0", 500.0);
        assert!(ok);
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash + 500.0
        );
    }

    #[test]
    fn test_credit_company_by_id_not_found() {
        let mut companies = vec![make_test_company("seller_0", 1_000.0)];
        let ok = credit_company_by_id(&mut companies, "nonexistent", 500.0);
        assert!(!ok);
    }

    #[test]
    fn test_credit_company_by_id_invalid_amount() {
        let mut companies = vec![make_test_company("seller_0", 1_000.0)];
        let ok = credit_company_by_id(&mut companies, "seller_0", 0.0);
        assert!(!ok);
        let ok = credit_company_by_id(&mut companies, "seller_0", -100.0);
        assert!(!ok);
    }

    #[test]
    fn test_settle_transfer_foreign_entity() {
        let mut companies = vec![
            make_test_company("comp_0", 10_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let mut country = make_test_country();
        // Phase 46: Add FX reserves so the transfer can succeed
        country
            .central_bank
            .fx_reserves
            .insert("USD".to_string(), 5_000.0);
        country
            .central_bank
            .fx_reserves
            .insert("EUR".to_string(), 3_000.0);
        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_treasury = country.budget.liquid_reserves;
        let initial_total_fx: f64 = country.central_bank.fx_reserves.values().sum();

        let result = settle_transfer(
            &mut companies,
            0,
            1_000.0,
            &TransferRecipient::ForeignEntity,
            &mut country,
        );
        assert!(result.is_ok());

        // Cash decreased
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash - 1_000.0
        );
        // Treasury unchanged (money left the system)
        assert_eq!(country.budget.liquid_reserves, initial_treasury);
        // Phase 46: FX reserves decreased by transfer amount (proportional drawdown)
        let final_total_fx: f64 = country.central_bank.fx_reserves.values().sum();
        assert!((final_total_fx - (initial_total_fx - 1_000.0)).abs() < 1e-6);
    }

    #[test]
    fn test_settle_transfer_foreign_entity_rejected_insufficient_fx() {
        let mut companies = vec![
            make_test_company("comp_0", 10_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let mut country = make_test_country();
        // Insufficient FX reserves: only 500 available, transfer requests 1_000
        country
            .central_bank
            .fx_reserves
            .insert("USD".to_string(), 500.0);
        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;
        let initial_reserves = companies[1]
            .balance_sheet
            .as_ref()
            .unwrap()
            .reserves_at_central_bank;

        let result = settle_transfer(
            &mut companies,
            0,
            1_000.0,
            &TransferRecipient::ForeignEntity,
            &mut country,
        );
        // Phase 46: Transfer rejected — capital controls
        assert!(matches!(result, Err(TransferError::InsufficientFxReserves)));

        // No state mutated (atomicity)
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash
        );
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_deposits
        );
        assert_eq!(
            companies[1]
                .balance_sheet
                .as_ref()
                .unwrap()
                .reserves_at_central_bank,
            initial_reserves
        );
        assert_eq!(country.central_bank.fx_reserves.get("USD"), Some(&500.0));
    }

    #[test]
    fn test_settle_transfer_rejected_insufficient_bank_reserves() {
        let mut companies = vec![
            make_test_company("comp_0", 10_000.0),
            make_test_bank("bank_0", 100.0, 400_000.0), // Bank has only 100 reserves
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let mut country = make_test_country();
        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;
        let initial_reserves = companies[1]
            .balance_sheet
            .as_ref()
            .unwrap()
            .reserves_at_central_bank;

        // Transfer 1_000 would cause bank reserves to go negative (100 - 1000 = -900)
        let result = settle_transfer_to_treasury(&mut companies, 0, 1_000.0, &mut country);
        assert!(matches!(result, Err(TransferError::InsufficientReserves)));

        // No state mutated (atomicity)
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash
        );
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_deposits
        );
        assert_eq!(
            companies[1]
                .balance_sheet
                .as_ref()
                .unwrap()
                .reserves_at_central_bank,
            initial_reserves
        );
    }

    #[test]
    fn test_no_silent_clamp_on_negative_reserves() {
        let mut companies = vec![
            make_test_company("comp_0", 10_000.0),
            make_test_bank("bank_0", 50.0, 400_000.0), // Bank has only 50 reserves
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let mut country = make_test_country();

        // Transfer 100 would cause bank reserves to go negative (50 - 100 = -50)
        let result = settle_transfer_to_treasury(&mut companies, 0, 100.0, &mut country);
        assert!(matches!(result, Err(TransferError::InsufficientReserves)));

        // Verify reserves were NOT silently clamped — they remain at original value
        assert_eq!(
            companies[1]
                .balance_sheet
                .as_ref()
                .unwrap()
                .reserves_at_central_bank,
            50.0
        );
    }
}

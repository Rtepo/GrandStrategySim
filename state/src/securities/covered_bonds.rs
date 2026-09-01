//! Covered bonds module for bank debt securities.
//!
//! This module implements CoveredBond (List Zastawny) for bank-issued
//! bonds backed by mortgage assets, with proper asset classification.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// List Zastawny - Covered Bond issued by banks backed by mortgage/investment assets.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct CoveredBond {
    /// Bond ID.
    pub id: String,

    /// Issuing bank ID.
    pub issuer_id: String,

    /// Current holder ID (investor).
    pub holder_id: String,

    /// Principal amount.
    pub principal: f64,

    /// Coupon rate (annual interest).
    pub coupon_rate: f64,

    /// Maturity turn.
    pub maturity_turn: u32,

    /// Backing asset pool (mortgage/investment IDs).
    pub backing_pool: Vec<String>,

    /// Coverage ratio (backing assets / principal).
    pub coverage_ratio: f64,

    /// Any additional bond fields.
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

// Re-export BankBalanceSheet from banking module
pub use crate::state::banking::BankBalanceSheet;

/// Extension trait for covered bond functionality on BankBalanceSheet.
pub trait CoveredBondExtension {
    /// Issue covered bond backed by mortgage assets.
    fn issue_covered_bond(
        &mut self,
        bank_id: &str,
        principal: f64,
        coupon_rate: f64,
        maturity_turn: u32,
        backing_mortgages: Vec<String>,
    ) -> Result<CoveredBond, String>;

    /// Calculate mortgage pool value for coverage ratio.
    fn calculate_mortgage_pool_value(&self, backing_mortgages: &[String]) -> f64;
}

impl CoveredBondExtension for BankBalanceSheet {
    fn issue_covered_bond(
        &mut self,
        bank_id: &str,
        principal: f64,
        coupon_rate: f64,
        maturity_turn: u32,
        backing_mortgages: Vec<String>,
    ) -> Result<CoveredBond, String> {
        // Verify coverage ratio >= 1.0 (100% backing)
        let backing_value = self.calculate_mortgage_pool_value(&backing_mortgages);
        if backing_value < principal {
            return Err("Insufficient backing assets".to_string());
        }

        let bond = CoveredBond {
            id: format!(
                "CB-{}-{}",
                bank_id,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
            issuer_id: bank_id.to_string(),
            holder_id: String::new(),
            principal,
            coupon_rate,
            maturity_turn,
            backing_pool: backing_mortgages,
            coverage_ratio: backing_value / principal,
            extra: HashMap::new(),
        };

        self.reserves_at_central_bank += principal; // Receive cash (correct asset class)
        self.issued_bonds += principal; // Record liability (double-entry compliance)
        Ok(bond)
    }

    fn calculate_mortgage_pool_value(&self, backing_mortgages: &[String]) -> f64 {
        // Sum outstanding_balance of all pledged loans matching the backing pool IDs
        self.loans_issued
            .iter()
            .filter(|l| backing_mortgages.contains(&l.id) && l.pledged_to_covered_bond.is_none())
            .map(|l| l.outstanding_balance)
            .sum()
    }
}

/// Create a covered bond backed by mortgage assets from a bank's balance sheet.
///
/// # Arguments
/// * `bank` - Mutable issuing bank company
/// * `covered_bonds` - Mutable vector of all covered bonds (new bond appended here)
/// * `exchange` - Mutable stock exchange (Ask order submitted here)
/// * `config` - Securities market config with minimum coverage ratio
/// * `principal` - Principal amount to raise
/// * `coupon_rate` - Annual coupon rate
/// * `maturity_turn` - Turn when bond matures
/// * `current_turn` - Current turn number
///
/// # Returns
/// `Ok(bond_id)` if successful, `Err(reason)` if coverage insufficient
///
/// # Rules
/// * NO MAGIC CASH: bank receives reserves from investor who buys the bond
/// * Coverage ratio = sum of pledged loan outstanding_balance / principal
/// * Coverage must be >= config.covered_bond_min_coverage (e.g., 1.0 = 100%)
/// * Pledged loans are marked with pledged_to_covered_bond = bond_id
/// * Bank's issued_bonds increased by principal (liability side)
/// * Bond submitted as Ask order on exchange for investor purchase
/// * When investor buys: investor cash -> bank reserves (double-entry)
pub fn create_covered_bond(
    bank: &mut crate::entities::Company,
    covered_bonds: &mut Vec<CoveredBond>,
    exchange: &mut crate::securities::exchange::StockExchange,
    config: &crate::securities::config::SecuritiesMarketConfig,
    principal: f64,
    coupon_rate: f64,
    maturity_turn: u32,
    current_turn: u32,
) -> Result<String, String> {
    let balance_sheet = bank
        .balance_sheet
        .as_mut()
        .ok_or("Bank has no balance sheet")?;

    // Find eligible loans to pledge (non-securitized, non-pledged, outstanding > 0)
    let eligible: Vec<(String, f64)> = balance_sheet
        .loans_issued
        .iter()
        .filter(|l| {
            !l.securitized && l.pledged_to_covered_bond.is_none() && l.outstanding_balance > 0.0
        })
        .map(|l| (l.id.clone(), l.outstanding_balance))
        .collect();

    // Select loans to pledge (smallest set covering principal * min_coverage)
    let required_backing = principal * config.covered_bond_min_coverage;
    let mut pledged: Vec<String> = Vec::new();
    let mut pledged_value = 0.0;
    for (loan_id, outstanding) in &eligible {
        if pledged_value >= required_backing {
            break;
        }
        pledged.push(loan_id.clone());
        pledged_value += outstanding;
    }

    if pledged_value < required_backing {
        return Err(format!(
            "Insufficient backing: have {:.2}, need {:.2}",
            pledged_value, required_backing
        ));
    }

    let coverage_ratio = pledged_value / principal;
    let bond_id = format!("CB-{}-{}", bank.id, current_turn);

    // Mark loans as pledged to this covered bond
    for loan_id in &pledged {
        if let Some(loan) = balance_sheet
            .loans_issued
            .iter_mut()
            .find(|l| &l.id == loan_id)
        {
            loan.pledged_to_covered_bond = Some(bond_id.clone());
        }
    }

    // Record liability on balance sheet (double-entry: bond is a liability)
    balance_sheet.issued_bonds += principal;

    let bond = CoveredBond {
        id: bond_id.clone(),
        issuer_id: bank.id.clone(),
        holder_id: String::new(), // Available for purchase
        principal,
        coupon_rate,
        maturity_turn,
        backing_pool: pledged,
        coverage_ratio,
        extra: HashMap::new(),
    };

    // Submit Ask order on exchange for investor purchase
    let instrument_id = format!("BOND:{}", bond_id);
    let ask_order = crate::securities::exchange::Order::new_sell(
        format!("CB-ASK-{}", bond_id),
        bank.id.clone(),
        instrument_id.clone(),
        crate::securities::exchange::InstrumentType::CoveredBond,
        1,
        principal,
        current_turn + 20,
    );
    let book = exchange.order_book.entry(instrument_id).or_default();
    if let Some(pos) = book.asks.iter().position(|(p, _)| *p == principal) {
        book.asks[pos].1.push(ask_order);
    } else {
        book.asks.push((principal, vec![ask_order]));
        book.asks
            .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }
    book.best_ask = book.asks.first().map(|(p, _)| *p).unwrap_or(0.0);

    covered_bonds.push(bond);
    Ok(bond_id)
}

/// Process covered bonds turn: pay coupons from issuing bank to bond holders.
///
/// # Arguments
/// * `covered_bonds` - Mutable slice of all covered bonds
/// * `companies` - Mutable slice of all companies (for bank debit and holder credit)
/// * `current_turn` - Current turn number
///
/// # Rules
/// * Coupon = principal * coupon_rate (per turn)
/// * Issuing bank is DEBITED (reserves_at_central_bank -= coupon)
/// * Bond holder is CREDITED (brokerage cash += coupon)
/// * Coupon payments do NOT reduce principal (principal stays until maturity)
/// * At maturity: principal repaid from bank reserves to holder
/// * If bank cannot pay, pays what it can (partial default)
pub fn process_covered_bonds_turn(
    covered_bonds: &mut Vec<CoveredBond>,
    companies: &mut [crate::entities::Company],
    current_turn: u32,
) {
    let mut matured_indices = Vec::new();

    for (idx, bond) in covered_bonds.iter_mut().enumerate() {
        if bond.holder_id.is_empty() {
            continue; // Not yet purchased
        }

        // Pay coupon
        let coupon = bond.principal * bond.coupon_rate;
        if coupon > 0.0 {
            let issuer_id = bond.issuer_id.clone();
            let mut actual_coupon = coupon;
            if let Some(bank) = companies.iter_mut().find(|c| c.id == issuer_id) {
                if let Some(ref mut bs) = bank.balance_sheet {
                    let available = bs.reserves_at_central_bank;
                    actual_coupon = coupon.min(available);
                    bs.reserves_at_central_bank -= actual_coupon;
                }
            }

            if actual_coupon > 0.0 {
                let holder_id = bond.holder_id.clone();
                if let Some(holder) = companies.iter_mut().find(|c| c.id == holder_id) {
                    if let Some(ref mut acct) = holder.brokerage_account {
                        acct.cash += actual_coupon;
                    }
                }
            }
        }

        // Check maturity
        if current_turn >= bond.maturity_turn {
            let issuer_id = bond.issuer_id.clone();
            let holder_id = bond.holder_id.clone();
            let principal = bond.principal;

            // Repay principal from bank reserves to holder
            if let Some(bank) = companies.iter_mut().find(|c| c.id == issuer_id) {
                if let Some(ref mut bs) = bank.balance_sheet {
                    let available = bs.reserves_at_central_bank;
                    let actual_repayment = principal.min(available);
                    bs.reserves_at_central_bank -= actual_repayment;
                    bs.issued_bonds -= actual_repayment;

                    if actual_repayment > 0.0 {
                        if let Some(holder) = companies.iter_mut().find(|c| c.id == holder_id) {
                            if let Some(ref mut acct) = holder.brokerage_account {
                                acct.cash += actual_repayment;
                            }
                        }
                    }
                }
            }

            matured_indices.push(idx);
        }
    }

    // Remove matured bonds (in reverse order to preserve indices)
    for idx in matured_indices.into_iter().rev() {
        covered_bonds.remove(idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_covered_bond_serialization() {
        let bond = CoveredBond {
            id: "CB-001".to_string(),
            issuer_id: "BANK-001".to_string(),
            holder_id: "INV-001".to_string(),
            principal: 1000000.0,
            coupon_rate: 0.05,
            maturity_turn: 120,
            backing_pool: vec!["MORT-001".to_string()],
            coverage_ratio: 1.2,
            extra: HashMap::new(),
        };

        let serialized = serde_json::to_string(&bond).unwrap();
        assert!(serialized.contains("backing_pool"));
    }

    #[test]
    fn test_issue_covered_bond_insufficient_backing() {
        let mut balance_sheet = BankBalanceSheet {
            reserves_at_central_bank: 0.0,
            issued_bonds: 0.0,
            ..Default::default()
        };

        let result = balance_sheet.issue_covered_bond(
            "BANK-001",
            3_000_000.0, // Exceeds backing value (no loans = 0 backing)
            0.05,
            120,
            vec!["MORT-001".to_string()],
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_issue_covered_bond_double_entry_compliance() {
        let principal = 1_000_000.0;
        let mut balance_sheet = BankBalanceSheet {
            reserves_at_central_bank: 0.0,
            issued_bonds: 0.0,
            loans_issued: vec![crate::state::banking::Loan {
                id: "MORT-001".to_string(),
                outstanding_balance: 2_000_000.0,
                ..Default::default()
            }],
            ..Default::default()
        };

        let initial_reserves = balance_sheet.reserves_at_central_bank;
        let initial_bonds = balance_sheet.issued_bonds;

        let result = balance_sheet.issue_covered_bond(
            "BANK-001",
            principal,
            0.05,
            120,
            vec!["MORT-001".to_string()],
        );

        assert!(result.is_ok());

        // Double-entry verification: Assets = Liabilities
        assert_eq!(
            balance_sheet.reserves_at_central_bank - initial_reserves,
            principal,
            "Asset side (reserves) must increase by principal"
        );
        assert_eq!(
            balance_sheet.issued_bonds - initial_bonds,
            principal,
            "Liability side (issued_bonds) must increase by principal"
        );
    }
}

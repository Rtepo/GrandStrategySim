//! Phase 77: Banking Realism Tests
//!
//! Tests for the Phase 77 capital markets and banking realism fixes:
//! - issue_loan rejects when excess reserves are insufficient
//! - issue_loan subtracts Lombard loans from effective reserves
//! - Bank operational capacity limits loan issuance
//! - A bank with 0 FTE cannot issue loans
//! - No bank can lend more than its excess reserves (LDR stays near 100%)

use sim_engine::state::banking::{
    bank_operational_capacity, issue_loan, BankBalanceSheet, LoanType,
};
use sim_engine::state::CentralBank;

/// A minimal borrower for testing.
struct TestBorrower {
    id: String,
    liquid: f64,
    fixed: f64,
    liabilities: f64,
}

impl sim_engine::state::banking::Borrower for TestBorrower {
    fn id(&self) -> &str { &self.id }
    fn liquid_capital(&self) -> f64 { self.liquid }
    fn fixed_capital(&self) -> f64 { self.fixed }
    fn liabilities(&self) -> f64 { self.liabilities }
    fn computed_liquid_capital(&self) -> f64 { self.liquid }
}

fn test_central_bank() -> CentralBank {
    let mut cb = CentralBank::default();
    cb.reserve_requirement_ratio = 0.10;
    cb.interest_rates.reference_rate = 0.05;
    cb
}

fn test_balance_sheet(deposits: f64, reserves: f64, lombard: f64) -> BankBalanceSheet {
    BankBalanceSheet {
        reserves_at_central_bank: reserves,
        loans_issued: Vec::new(),
        interbank_loans_given: std::collections::HashMap::new(),
        securities: 0.0,
        mbs_holdings: Vec::new(),
        real_estate: 0.0,
        deposits,
        cb_lombard_loans: lombard,
        cb_deposit_facility_balance: 0.0,
        interbank_loans_taken: std::collections::HashMap::new(),
        issued_bonds: 0.0,
        tier_1_capital: deposits * 0.10,
        extra: serde_json::Map::new(),
    }
}

#[test]
fn issue_loan_rejects_insufficient_excess_reserves() {
    let cb = test_central_bank();
    // Bank has 100 deposits, 10 reserves (exactly required), 0 lombard.
    // No excess reserves — cannot lend.
    let mut bs = test_balance_sheet(100.0, 10.0, 0.0);
    let borrower = TestBorrower {
        id: "TEST-BORROWER".to_string(),
        liquid: 0.0,
        fixed: 1000.0,
        liabilities: 0.0,
    };
    let result = issue_loan(
        &mut bs,
        "BANK-001",
        0.02,
        &borrower,
        &borrower.id,
        50.0,
        LoanType::WorkingCapital,
        12,
        &cb,
        0.05,
    );
    assert!(result.is_err(), "Should reject loan when no excess reserves");
}

#[test]
fn issue_loan_succeeds_with_excess_reserves() {
    let cb = test_central_bank();
    // Bank has 100 deposits, 20 reserves, 0 lombard.
    // Required = 10, excess = 10. Can lend up to ~90 (deposit expansion).
    let mut bs = test_balance_sheet(100.0, 20.0, 0.0);
    let borrower = TestBorrower {
        id: "TEST-BORROWER".to_string(),
        liquid: 0.0,
        fixed: 1000.0,
        liabilities: 0.0,
    };
    let result = issue_loan(
        &mut bs,
        "BANK-001",
        0.02,
        &borrower,
        &borrower.id,
        10.0,
        LoanType::WorkingCapital,
        12,
        &cb,
        0.05,
    );
    assert!(result.is_ok(), "Should accept loan with sufficient excess reserves");
}

#[test]
fn issue_loan_subtracts_lombard_from_effective_reserves() {
    let cb = test_central_bank();
    // Bank has 100 deposits, 20 reserves, 15 lombard.
    // Effective reserves = 20 - 15 = 5. Required = 10. Cannot lend.
    let mut bs = test_balance_sheet(100.0, 20.0, 15.0);
    let borrower = TestBorrower {
        id: "TEST-BORROWER".to_string(),
        liquid: 0.0,
        fixed: 1000.0,
        liabilities: 0.0,
    };
    let result = issue_loan(
        &mut bs,
        "BANK-001",
        0.02,
        &borrower,
        &borrower.id,
        5.0,
        LoanType::WorkingCapital,
        12,
        &cb,
        0.05,
    );
    assert!(result.is_err(), "Should reject when effective reserves (after Lombard) are insufficient");
}

#[test]
fn bank_operational_capacity_zero_fte_returns_zero() {
    let capacity = bank_operational_capacity(0.0, 5000.0);
    assert_eq!(capacity.max_asset_under_management, 0.0);
    assert_eq!(capacity.max_new_loans_per_turn, 0.0);
    assert_eq!(capacity.max_deposit_handling, 0.0);
}

#[test]
fn bank_operational_capacity_scales_with_fte() {
    let capacity_100 = bank_operational_capacity(100.0, 5000.0);
    let capacity_200 = bank_operational_capacity(200.0, 5000.0);
    // Doubling FTE should double capacity
    assert!((capacity_200.max_asset_under_management / capacity_100.max_asset_under_management - 2.0).abs() < 0.01);
    assert!((capacity_200.max_new_loans_per_turn / capacity_100.max_new_loans_per_turn - 2.0).abs() < 0.01);
}

#[test]
fn bank_operational_capacity_scales_with_wage() {
    let capacity_low_wage = bank_operational_capacity(100.0, 2000.0);
    let capacity_high_wage = bank_operational_capacity(100.0, 10000.0);
    // 5x wage should 5x capacity
    assert!((capacity_high_wage.max_asset_under_management / capacity_low_wage.max_asset_under_management - 5.0).abs() < 0.01);
}

#[test]
fn bank_operational_capacity_zero_wage_returns_zero() {
    let capacity = bank_operational_capacity(100.0, 0.0);
    assert_eq!(capacity.max_asset_under_management, 0.0);
}

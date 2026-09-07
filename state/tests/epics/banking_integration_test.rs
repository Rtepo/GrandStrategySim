//! Integration tests for the banking sector.
//!
//! These tests verify end-to-end workflows including:
//! - Complete loan issuance from credit scoring to balance sheet updates
//! - Interbank market clearing with multiple banks
//! - Central Bank parameter integration
//! - Full banking sector workflow

use sim_engine::entities::{Company, LegalForm};
use sim_engine::registries::enums::Sector;
use sim_engine::state::banking::{calculate_credit_score, issue_loan};
use sim_engine::state::banking::{InterestType, LoanType};
use sim_engine::state::{
    BankBalanceSheet, BankType, CentralBank, InterbankMarket, Loan, LoanStatus,
};

#[test]
fn test_full_loan_issuance_workflow() {
    // Setup: Create a bank with sufficient reserves
    let mut bank_balance_sheet = BankBalanceSheet::default();
    bank_balance_sheet.reserves_at_central_bank = 500_000.0;
    bank_balance_sheet.deposits = 2_000_000.0;
    bank_balance_sheet.tier_1_capital = 200_000.0;

    // Create a borrower company
    let borrower = Company::new(
        "BORROWER-1".to_string(),
        "Test Borrower".to_string(),
        Sector::LightIndustry,
        LegalForm::JointStockCompany(sim_engine::entities::JointStockData::default()),
        300_000.0, // fixed_capital
        200_000.0, // liquid_capital
        100,
    );

    // Setup Central Bank with dynamic parameters
    let mut central_bank = CentralBank::default();
    central_bank.reserve_requirement_ratio = 0.10;
    central_bank.interest_rates.reference_rate = 0.03;
    central_bank.interest_rates.deposit_rate = 0.02;
    central_bank.interest_rates.lombard_rate = 0.05;

    // Step 1: Credit scoring
    let credit_score = calculate_credit_score(
        &borrower,
        LoanType::WorkingCapital,
        150_000.0,
        &central_bank,
        "BANK-1",
        &bank_balance_sheet.loans_issued,
    );

    assert!(
        credit_score.approved,
        "Credit score should approve healthy borrower"
    );
    assert!(credit_score.score > 0.5, "Credit score should be above 0.5");

    // Step 2: Issue loan
    let loan_result = issue_loan(
        &mut bank_balance_sheet,
        "BANK-1",
        0.015, // bank margin
        &borrower,
        "BORROWER-1",
        150_000.0,
        LoanType::WorkingCapital,
        12, // 12 turns
        &central_bank,
        0.03, // XIBOR
    );

    assert!(loan_result.is_ok(), "Loan issuance should succeed");
    let result = loan_result.unwrap();

    // Step 3: Verify balance sheet changes (double-entry bookkeeping)
    assert_eq!(
        bank_balance_sheet.loans_issued.len(),
        1,
        "Should have one loan"
    );
    assert!(
        (bank_balance_sheet.deposits - 2_150_000.0).abs() < 1e-9,
        "Deposits should increase by loan amount"
    );
    assert!(
        (bank_balance_sheet.reserves_at_central_bank - 500_000.0).abs() < 1e-9,
        "Reserves unchanged during loan creation"
    );

    // Step 4: Verify loan record
    assert_eq!(result.loan.principal, 150_000.0);
    assert_eq!(result.loan.outstanding_balance, 150_000.0);
    assert_eq!(result.loan.borrower_id, "BORROWER-1");
    assert_eq!(result.loan.loan_type, LoanType::WorkingCapital);
    assert_eq!(result.loan.status, LoanStatus::Current);
    assert_eq!(result.principal_amount, 150_000.0);
}

#[test]
fn test_interbank_market_clearing_with_multiple_banks() {
    // Setup: Create three banks with different reserve positions
    let mut bank1 = Company::new(
        "BANK-1".to_string(),
        "Bank 1".to_string(),
        Sector::Banking,
        LegalForm::JointStockCompany(sim_engine::entities::JointStockData::default()),
        0.0,
        0.0,
        0,
    );
    bank1.bank_type = Some(BankType::Commercial);
    bank1.balance_sheet = Some(BankBalanceSheet {
        reserves_at_central_bank: 300_000.0,
        deposits: 1_000_000.0,
        tier_1_capital: 100_000.0,
        ..Default::default()
    });

    let mut bank2 = Company::new(
        "BANK-2".to_string(),
        "Bank 2".to_string(),
        Sector::Banking,
        LegalForm::JointStockCompany(sim_engine::entities::JointStockData::default()),
        0.0,
        0.0,
        0,
    );
    bank2.bank_type = Some(BankType::Commercial);
    bank2.balance_sheet = Some(BankBalanceSheet {
        reserves_at_central_bank: 100_000.0,
        deposits: 1_500_000.0,
        tier_1_capital: 100_000.0,
        ..Default::default()
    });

    let mut bank3 = Company::new(
        "BANK-3".to_string(),
        "Bank 3".to_string(),
        Sector::Banking,
        LegalForm::JointStockCompany(sim_engine::entities::JointStockData::default()),
        0.0,
        0.0,
        0,
    );
    bank3.bank_type = Some(BankType::Universal);
    bank3.balance_sheet = Some(BankBalanceSheet {
        reserves_at_central_bank: 200_000.0,
        deposits: 1_200_000.0,
        tier_1_capital: 150_000.0,
        ..Default::default()
    });

    // Setup Central Bank
    let mut central_bank = CentralBank::default();
    central_bank.reserve_requirement_ratio = 0.10;
    central_bank.interest_rates.deposit_rate = 0.02;
    central_bank.interest_rates.lombard_rate = 0.05;

    // Setup Interbank Market
    let mut market = InterbankMarket::default();

    // Clear the market
    let mut banks = vec![&mut bank1, &mut bank2, &mut bank3];
    market.clear_market(&mut banks, &central_bank, 1);

    // Verify market state
    // Bank 1: 300k reserves, needs 100k (surplus: 200k)
    // Bank 2: 100k reserves, needs 150k (deficit: 50k)
    // Bank 3: 200k reserves, needs 120k (surplus: 80k)
    // Total surplus: 280k, Total deficit: 50k
    // Transfer: 50k from surplus banks to deficit bank

    assert!(
        (market.available_liquidity - 280_000.0).abs() < 1e-9,
        "Available liquidity should be 280k"
    );
    assert!(
        (market.demanded_liquidity - 50_000.0).abs() < 1e-9,
        "Demanded liquidity should be 50k"
    );

    // Verify XIBOR calculation
    // Supply/demand ratio: 280k / 50k = 5.6 (surplus > deficit)
    // XIBOR should be near deposit rate (0.02)
    assert!(
        market.xibor >= 0.02,
        "XIBOR should be at least deposit rate"
    );
    assert!(market.xibor <= 0.05, "XIBOR should not exceed lombard rate");

    // Verify bank balance sheet updates
    // Bank 1 should have lent proportionally: (200k/280k) * 50k = 35.7k
    let bank1_bs = bank1.balance_sheet.as_ref().unwrap();
    let bank1_lent: f64 = bank1_bs.interbank_loans_given.values().sum();
    assert!(
        (bank1_lent - 35_714.0).abs() < 100.0,
        "Bank 1 should have lent ~35.7k"
    );

    // Bank 2 should have borrowed 50k
    let bank2_bs = bank2.balance_sheet.as_ref().unwrap();
    let bank2_borrowed: f64 = bank2_bs.interbank_loans_taken.values().sum();
    assert!(
        (bank2_borrowed - 50_000.0).abs() < 1e-9,
        "Bank 2 should have borrowed 50k"
    );
    assert!(
        (bank2_bs.reserves_at_central_bank - 150_000.0).abs() < 1e-9,
        "Bank 2 reserves should increase to 150k"
    );
}

#[test]
fn test_central_bank_parameter_integration() {
    // Test that Central Bank parameters dynamically affect loan pricing
    let mut balance_sheet = BankBalanceSheet::default();
    balance_sheet.reserves_at_central_bank = 500_000.0;
    balance_sheet.deposits = 2_000_000.0;
    balance_sheet.tier_1_capital = 200_000.0;

    let borrower = Company::new(
        "BORROWER-1".to_string(),
        "Test Borrower".to_string(),
        Sector::LightIndustry,
        LegalForm::JointStockCompany(sim_engine::entities::JointStockData::default()),
        300_000.0,
        200_000.0,
        100,
    );

    // Scenario 1: Low interest rate environment
    let mut cb_low = CentralBank::default();
    cb_low.reserve_requirement_ratio = 0.08; // Lower reserve requirement
    cb_low.interest_rates.reference_rate = 0.02; // Low reference rate
    cb_low.interest_rates.deposit_rate = 0.01;
    cb_low.interest_rates.lombard_rate = 0.03;

    let credit_low = calculate_credit_score(
        &borrower,
        LoanType::Investment,
        150_000.0,
        &cb_low,
        "BANK-1",
        &balance_sheet.loans_issued,
    );

    // Scenario 2: High interest rate environment
    let mut cb_high = CentralBank::default();
    cb_high.reserve_requirement_ratio = 0.12; // Higher reserve requirement
    cb_high.interest_rates.reference_rate = 0.05; // High reference rate
    cb_high.interest_rates.deposit_rate = 0.04;
    cb_high.interest_rates.lombard_rate = 0.07;

    let credit_high = calculate_credit_score(
        &borrower,
        LoanType::Investment,
        150_000.0,
        &cb_high,
        "BANK-1",
        &balance_sheet.loans_issued,
    );

    // Higher interest rates should increase risk premium
    assert!(
        credit_high.risk_premium_bps >= credit_low.risk_premium_bps,
        "Higher CB rates should increase risk premium"
    );

    // Both should approve the same borrower
    assert!(credit_low.approved, "Low rate environment should approve");
    assert!(credit_high.approved, "High rate environment should approve");
}

#[test]
fn test_consolidation_loan_with_equity_swap() {
    // Test the full consolidation loan workflow with debt-to-equity swap
    let mut balance_sheet = BankBalanceSheet::default();
    balance_sheet.reserves_at_central_bank = 500_000.0;
    balance_sheet.deposits = 2_000_000.0;
    balance_sheet.tier_1_capital = 200_000.0;

    // Create an existing debtor
    let mut borrower = Company::new(
        "BORROWER-1".to_string(),
        "Existing Debtor".to_string(),
        Sector::LightIndustry,
        LegalForm::JointStockCompany(sim_engine::entities::JointStockData::default()),
        200_000.0,
        300_000.0, // High liquidity for consolidation viability
        100,
    );
    borrower.liabilities = 100_000.0;

    // Add existing loan
    let existing_loan = Loan {
        id: "LOAN-OLD".to_string(),
        borrower_id: "BORROWER-1".to_string(),
        principal: 50_000.0,
        outstanding_balance: 50_000.0,
        interest_rate: 0.05,
        term_turns: 12,
        turns_remaining: 6,
        collateral_value: None,
        loan_type: LoanType::WorkingCapital,
        last_payment_turn: 0,
        status: LoanStatus::Current,
        interest_type: InterestType::Variable,
        duration_risk_premium: 0.0,
        base_xibor: 0.03,
        bank_margin: 0.02,
        securitized: false,
        pledged_to_covered_bond: None,
        extra: serde_json::Map::new(),
    };
    balance_sheet.loans_issued.push(existing_loan.clone());

    let central_bank = CentralBank::default();

    // Credit scoring for consolidation
    let credit_score = calculate_credit_score(
        &borrower,
        LoanType::Consolidation,
        100_000.0,
        &central_bank,
        "BANK-1",
        &balance_sheet.loans_issued,
    );

    assert!(
        credit_score.approved,
        "Consolidation should be approved for existing debtor"
    );
    assert!(
        credit_score.required_equity_swap.is_some(),
        "Should require equity swap"
    );
    assert!(
        (credit_score.required_equity_swap.unwrap() - 0.15).abs() < 1e-9,
        "Should require 15% equity swap"
    );

    // Issue consolidation loan
    let loan_result = issue_loan(
        &mut balance_sheet,
        "BANK-1",
        0.015,
        &borrower,
        "BORROWER-1",
        100_000.0,
        LoanType::Consolidation,
        12,
        &central_bank,
        0.03,
    );

    assert!(loan_result.is_ok(), "Consolidation loan should be issued");
    let result = loan_result.unwrap();

    // Verify consolidation loan properties
    assert_eq!(result.loan.loan_type, LoanType::Consolidation);
    assert_eq!(result.loan.principal, 100_000.0);

    // Verify that the bank now has two loans (old + new)
    assert_eq!(
        balance_sheet.loans_issued.len(),
        2,
        "Should have two loans after consolidation"
    );
}

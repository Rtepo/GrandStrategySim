//! v4.31.0 (TASK-030): canonical bank income statement + treasury
//! allocation pass.
//!
//! Two defects this module kills:
//! 1. `BankDetail.net_profit` used `Company.realized_profit_this_turn`,
//!    which for a bank equals `-wages` (banks book no product revenue),
//!    so the income statement read "Net Profit = -Operating Wages" and
//!    the green chart line sat dead flat. `income_statement()` computes
//!    the real run-rate:
//!   net_profit = interest_income + fee_and_trading_income
//!   — interest_expense — npl_provisions — operating_wages
//! 2. Banks parked 54%–97% of assets in CB liquidity. The per-turn
//!    `treasury_allocation_pass` redeploys reserves above the
//!    regulatory buffer (reserve_req + 5%) into the bond portfolio so
//!    Cash / CB Reserves stays under ~25% of total assets.

use crate::entities::Company;
use crate::state::banking::{BankBalanceSheet, BankType, LoanStatus};
use crate::state::central_bank::RppInterestRates;

/// Deposit service fee run-rate — mirrors `DEPOSIT_SERVICE_FEE_RATE`
/// (0.01% of customer balances per turn) in `state::banking`.
const DEPOSIT_FEE_RATE: f64 = 0.0001;

/// NPL provisioning floor: 0.2% of performing loans per turn (clean book).
const NPL_PROVISION_FLOOR: f64 = 0.002;
/// NPL provisioning cap: 1.5% of performing loans per turn (fully
/// impaired book) — the old 10%-per-turn charge is gone.
const NPL_PROVISION_CAP: f64 = 0.015;

/// Regulatory liquidity buffer kept as CB liquidity: reserve requirement
/// plus a 5%-of-deposits operating buffer on top.
const BUFFER_OVER_RESERVE_REQ: f64 = 0.05;

/// Per-turn income statement for one bank (run-rate form, all flows in
/// currency units per turn).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct IncomeStatement {
    /// Loan/credit interest earned (contract rates + securities yield).
    pub interest_income: f64,
    /// Interest owed on deposits, Lombard, interbank and issued bonds.
    pub interest_expense: f64,
    /// Fee & trading income: deposit service fees, plus underwriting /
    /// bond-trading / M&A advisory legs for investment-style banks.
    pub fee_income: f64,
    /// Loan-loss provisioning charge — 0.2%–1.5% of PERFORMING loans.
    pub npl_cost: f64,
    /// Staff wages + arrears charged to the bank this turn.
    pub operating_wages: f64,
    /// `interest_income + fee_income - interest_expense - npl_cost - operating_wages`.
    pub net_profit: f64,
}

/// Interest income run-rate: loan book at contract rates + consumer
/// credit at ref+4% + interbank at ref + the bond portfolio at ref+0.5%
/// (gov/corp blend bought at a modest spread over the policy rate).
pub fn interest_income_run_rate(bs: &BankBalanceSheet, rates: &RppInterestRates) -> f64 {
    let interbank_given: f64 = bs.interbank_loans_given.values().sum();
    bs.loans_issued
        .iter()
        .map(|l| l.outstanding_balance * l.interest_rate)
        .sum::<f64>()
        + bs.consumer_loans_outstanding * (rates.reference_rate + 0.04)
        + interbank_given * rates.reference_rate
        + bs.securities * (rates.reference_rate + 0.005)
}

/// Interest expense run-rate: deposits at the deposit rate, Lombard at
/// the penalty rate, interbank taken at ref, issued bonds at ref+1%.
pub fn interest_expense_run_rate(bs: &BankBalanceSheet, rates: &RppInterestRates) -> f64 {
    let interbank_taken: f64 = bs.interbank_loans_taken.values().sum();
    bs.deposits * rates.deposit_rate
        + bs.cb_lombard_loans * rates.lombard_rate
        + interbank_taken * rates.reference_rate
        + bs.issued_bonds * (rates.reference_rate + 0.01)
}

/// Outstanding balance on loans past `Current` status.
pub fn impaired_loans(bs: &BankBalanceSheet) -> f64 {
    bs.loans_issued
        .iter()
        .filter(|l| matches!(l.status, LoanStatus::Overdue | LoanStatus::Default))
        .map(|l| l.outstanding_balance)
        .sum()
}

/// Loan-loss provisioning charge: 0.2%–1.5% of the PERFORMING book per
/// turn, scaled by the impaired share (clean books provision at floor).
pub fn npl_provisioning(bs: &BankBalanceSheet) -> f64 {
    let total_book: f64 = bs
        .loans_issued
        .iter()
        .map(|l| l.outstanding_balance)
        .sum::<f64>()
        + bs.consumer_loans_outstanding;
    if total_book <= 0.0 {
        return 0.0;
    }
    let impaired = impaired_loans(bs);
    let impaired_share = (impaired / total_book).clamp(0.0, 1.0);
    let performing = total_book - impaired;
    let rate = NPL_PROVISION_FLOOR + (NPL_PROVISION_CAP - NPL_PROVISION_FLOOR) * impaired_share;
    performing * rate
}

/// Fee & trading income run-rate. Every bank earns deposit service
/// fees; Investment/Universal banks additionally earn underwriting,
/// bond-trading and M&A/IPO advisory fees scaled to their capital
/// markets footprint — the leg that makes investment banks profitable.
pub fn fee_income_run_rate(bs: &BankBalanceSheet, bank_type: &BankType) -> f64 {
    let deposit_fees = bs.deposits * DEPOSIT_FEE_RATE;
    let capital_markets = match bank_type {
        BankType::Investment => {
            // Underwriting ~2%/turn on the securities book, advisory on
            // the franchise's own issuance programme, trading spread on
            // the bond portfolio.
            bs.securities * 0.02
                + bs.issued_bonds * 0.03
                + bs.total_assets() * 0.008
        }
        BankType::Universal => {
            bs.securities * 0.01 + bs.issued_bonds * 0.015 + bs.total_assets() * 0.004
        }
        _ => 0.0,
    };
    deposit_fees + capital_markets
}

/// The canonical formula — the single source of truth for
/// `BankDetail.net_profit` and `BankProfitPoint.net_profit`:
///   net_profit = interest_income + fee_and_trading_income
///                - interest_expense - npl_provisions - operating_wages
pub fn income_statement(
    bs: &BankBalanceSheet,
    bank_type: &BankType,
    rates: &RppInterestRates,
    operating_wages: f64,
) -> IncomeStatement {
    let interest_income = interest_income_run_rate(bs, rates);
    let interest_expense = interest_expense_run_rate(bs, rates);
    let fee_income = fee_income_run_rate(bs, bank_type);
    let npl_cost = npl_provisioning(bs);
    let net_profit = interest_income + fee_income
        - interest_expense
        - npl_cost
        - operating_wages;
    IncomeStatement {
        interest_income,
        interest_expense,
        fee_income,
        npl_cost,
        operating_wages,
        net_profit,
    }
}

/// Idle CB liquidity share (reserves + deposit facility) / total assets.
pub fn idle_cb_cash_share(bs: &BankBalanceSheet) -> f64 {
    let ta = bs.total_assets();
    if ta <= 0.0 {
        return 0.0;
    }
    (bs.reserves_at_central_bank + bs.cb_deposit_facility_balance) / ta
}

/// Per-turn treasury pass for every bank in the slice: redeploy CB
/// liquidity above the regulatory buffer (reserve_req + 5% of deposits)
/// into the securities portfolio (government bonds, corporate bonds,
/// and high-yield paper bought on the interbank/primary market).
///
/// Asset-for-asset swap — `reserves`/`depfac` down, `securities` up —
/// so A = L + E and M0 conservation are untouched. Banks whose idle
/// share is already ≤ 25% keep their buffer.
pub fn treasury_allocation_pass(companies: &mut [Company], reserve_requirement_ratio: f64) {
    let buffer = reserve_requirement_ratio + BUFFER_OVER_RESERVE_REQ;
    for c in companies.iter_mut() {
        if c.bank_type.is_none() {
            continue;
        }
        let Some(ref mut bs) = c.balance_sheet else {
            continue;
        };
        let total_assets = bs.total_assets();
        if total_assets <= 0.0 {
            continue;
        }
        let idle = bs.reserves_at_central_bank + bs.cb_deposit_facility_balance;
        // Nothing to deploy while idle liquidity is inside the 25% band.
        if idle <= total_assets * 0.25 {
            continue;
        }
        // Keep the regulatory buffer (reserve_req + 5% of deposits);
        // deploy everything above it, bounded by what brings the idle
        // share to the 25% target.
        let buffer_target = bs.deposits * buffer;
        let can_deploy = (idle - buffer_target).max(0.0);
        let deploy_to_cap = (idle - total_assets * 0.24).max(0.0);
        let deploy = can_deploy.min(deploy_to_cap);
        if deploy <= 0.0 {
            continue;
        }
        bs.draw_cb_liquidity(deploy);
        bs.securities += deploy;
    }
}

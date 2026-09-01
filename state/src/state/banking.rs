//! Commercial banking state for the grand-strategy economy.
//!
//! This module mirrors the per-country bank dictionaries from the Python
//! `data/banks.json` file. Each [`Bank`] captures the balance-sheet state of
//! one financial institution: deposits, loans, reserves, capital, and the
//! interest rates it offers/charges.
//!
//! STAGE D PHASE 2: Enhanced banking structures for fractional reserve banking,
//! interbank markets, and credit scoring.

use crate::securities::mbs::MortgageBackedSecurity;
use crate::state::macro_data::annual_to_per_turn_rate;
use crate::state::CentralBank;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Trait for entities that can borrow from banks
pub trait Borrower {
    /// Unique entity identifier
    fn id(&self) -> &str;

    /// Liquid capital available for working capital loans
    fn liquid_capital(&self) -> f64;

    /// Fixed capital for collateral assessment (investment/consolidation loans)
    fn fixed_capital(&self) -> f64;

    /// Total outstanding liabilities for liquidity ratio calculation
    fn liabilities(&self) -> f64;

    /// Computed liquid capital (may differ from stored liquid_capital)
    fn computed_liquid_capital(&self) -> f64;
}

/// Default reserve requirement ratio (10%) used when a bank is loaded from
/// legacy JSON that does not specify `reserve_requirement_ratio`.
fn default_reserve_requirement_ratio() -> f64 {
    0.10
}

/// Default bank condition string.
fn default_condition() -> String {
    "Stable".to_string()
}

// ============================================================================
// STAGE D PHASE 2: BANKING SECTOR STRUCTURES
// ============================================================================

/// Operational classification of financial institutions.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum BankType {
    /// Commercial (Retail) Bank: Takes deposits from Demographics (B2C),
    /// offers working capital/mortgage loans. Regulated by reserve requirements.
    #[default]
    Commercial,
    /// Investment Bank: No retail deposits. Funded by Aristocracy/Funds.
    /// Handles IPOs, corporate bonds, and CAPEX loans. Higher risk tolerance.
    Investment,
    /// Universal Bank: Combines Retail and Investment operations.
    /// Requires massive Tier 1 Capital (Equity) to legally operate.
    Universal,
    /// Cooperative (SKOK): Owned by its members. Re-invests profits into
    /// lowering loan rates for members. Member-focused lending.
    Cooperative,
}

/// Classification of loan purpose for credit risk assessment.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum LoanType {
    /// Working Capital Loan (Kredyt obrotowy) - Short-term financing for operations.
    /// Lower risk, secured by cashflow.
    #[default]
    WorkingCapital,
    /// Investment Loan (Kredyt inwestycyjny) - Long-term CAPEX financing.
    /// Higher risk, requires strict LTV and prospect analysis.
    Investment,
    /// Consolidation Loan (Kredyt konsolidacyjny) - Restructuring existing debt.
    /// Medium risk, depends on borrower's debt service capacity.
    Consolidation,
}

/// Loan payment status.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum LoanStatus {
    /// Payments are current.
    #[default]
    Current,
    /// Payment overdue but not yet in default.
    Overdue,
    /// Loan in default (collections initiated).
    Default,
    /// Loan fully repaid.
    Repaid,
    /// Loan re-titled to another entity through a merger or acquisition.
    Merged,
}

/// Interest rate type for loans - determines duration risk exposure.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum InterestType {
    /// Fixed rate: Locked for loan duration. Bank bears duration risk.
    /// Higher rate includes duration risk premium.
    Fixed,
    /// Variable rate: Tracks XIBOR + bank margin. Rate resets each turn.
    /// Borrower bears interest rate risk.
    #[default]
    Variable,
}

/// Individual loan record for tracking credit creation.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Loan {
    /// Unique loan identifier.
    #[serde(default)]
    pub id: String,
    /// Borrower entity ID (company_id, demographic_id, or household_id).
    #[serde(default)]
    pub borrower_id: String,
    /// Principal amount (original loan amount).
    #[serde(default)]
    pub principal: f64,
    /// Outstanding balance.
    #[serde(default)]
    pub outstanding_balance: f64,
    /// Interest rate (annualized, e.g., 0.05 for 5%).
    #[serde(default)]
    pub interest_rate: f64,
    /// Loan term in turns.
    #[serde(default)]
    pub term_turns: u32,
    /// Turns remaining until maturity.
    #[serde(default)]
    pub turns_remaining: u32,
    /// Collateral value (if secured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collateral_value: Option<f64>,
    /// Loan type (determines risk assessment logic).
    #[serde(default)]
    pub loan_type: LoanType,
    /// Last payment turn.
    #[serde(default)]
    pub last_payment_turn: u32,
    /// Payment status (current, overdue, default).
    #[serde(default)]
    pub status: LoanStatus,
    /// Interest type (Fixed or Variable).
    #[serde(default)]
    pub interest_type: InterestType,
    /// Duration risk premium (only for Fixed loans).
    #[serde(default)]
    pub duration_risk_premium: f64,
    /// Base XIBOR rate at loan origination (for Variable rate resets).
    #[serde(default)]
    pub base_xibor: f64,
    /// Bank margin (spread over XIBOR for Variable loans).
    #[serde(default)]
    pub bank_margin: f64,
    /// Resurrection Phase 2: Whether this loan has been securitized into an MBS pool.
    #[serde(default)]
    pub securitized: bool,
    /// Resurrection Phase 2: ID of covered bond this loan is pledged as backing for (None = unpledged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pledged_to_covered_bond: Option<String>,
    /// Any additional loan fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Double-entry balance sheet for banking companies.
/// Assets = Liabilities + Equity must always hold true.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct BankBalanceSheet {
    // ========================================================================
    // ASSETS (What the bank owns)
    // ========================================================================
    /// Physical cash held at Central Bank to meet reserve requirements.
    /// This is actual reserves, not a percentage.
    #[serde(default)]
    pub reserves_at_central_bank: f64,

    /// Credit created and lent to public/companies.
    /// Each loan entry represents money created by the bank.
    #[serde(default)]
    pub loans_issued: Vec<Loan>,

    /// Liquidity lent to other banks in the interbank market.
    /// Maps borrower_bank_id -> amount lent.
    #[serde(default)]
    pub interbank_loans_given: HashMap<String, f64>,

    /// Government bonds and other liquid securities held.
    #[serde(default)]
    pub securities: f64,

    /// Phase D.5: Senior MBS holdings (for QE purchases).
    #[serde(default)]
    pub mbs_holdings: Vec<MortgageBackedSecurity>,

    /// Physical buildings and infrastructure owned by the bank.
    #[serde(default)]
    pub real_estate: f64,

    // ========================================================================
    // LIABILITIES (What the bank owes)
    // ========================================================================
    /// Money deposited by citizens/companies (demand + time deposits).
    /// This is money the bank owes to depositors.
    #[serde(default)]
    pub deposits: f64,

    /// Emergency liquidity borrowed from Central Bank via Lombard facility.
    /// Expensive rate, used as last resort.
    #[serde(default)]
    pub cb_lombard_loans: f64,

    /// Reserves physically parked at CB deposit facility earning deposit_rate interest.
    /// These are separated from operational reserves_at_central_bank.
    #[serde(default)]
    pub cb_deposit_facility_balance: f64,

    /// Liquidity borrowed from other banks in the interbank market.
    /// Maps lender_bank_id -> amount borrowed.
    #[serde(default)]
    pub interbank_loans_taken: HashMap<String, f64>,

    /// Bonds and other debt instruments issued by the bank.
    #[serde(default)]
    pub issued_bonds: f64,

    // ========================================================================
    // EQUITY (The bank's own capital)
    // ========================================================================
    /// Tier 1 Capital (Common Equity + Retained Earnings).
    /// This is the bank's own money / shareholder capital.
    /// Regulatory requirement: Tier 1 Capital >= 6% of Risk-Weighted Assets.
    #[serde(default)]
    pub tier_1_capital: f64,

    /// Any additional balance sheet fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl BankBalanceSheet {
    /// Calculates total assets.
    ///
    /// # Returns
    /// Assets = reserves + loans + interbank_given + securities + real_estate
    pub fn total_assets(&self) -> f64 {
        self.reserves_at_central_bank
            + self.cb_deposit_facility_balance
            + self
                .loans_issued
                .iter()
                .map(|l| l.outstanding_balance)
                .sum::<f64>()
            + self.interbank_loans_given.values().sum::<f64>()
            + self.securities
            + self.real_estate
    }

    /// Calculates total liabilities.
    ///
    /// # Returns
    /// Liabilities = deposits + cb_lombard + interbank_taken + issued_bonds
    pub fn total_liabilities(&self) -> f64 {
        self.deposits
            + self.cb_lombard_loans
            + self.interbank_loans_taken.values().sum::<f64>()
            + self.issued_bonds
    }

    /// Calculates total equity.
    ///
    /// # Returns
    /// Equity = tier_1_capital
    pub fn total_equity(&self) -> f64 {
        self.tier_1_capital
    }

    /// Validates double-entry accounting: Assets = Liabilities + Equity.
    ///
    /// # Returns
    /// true if balance sheet balances (within floating-point tolerance)
    pub fn is_balanced(&self) -> bool {
        let assets = self.total_assets();
        let liabilities_plus_equity = self.total_liabilities() + self.total_equity();
        (assets - liabilities_plus_equity).abs() < 1e-6
    }

    /// Calculates reserve ratio against Central Bank requirement.
    ///
    /// # Arguments
    /// * `cb_reserve_ratio` - Central Bank's reserve requirement ratio (e.g., 0.10 for 10%)
    ///
    /// # Returns
    /// Current reserve ratio (reserves / deposits)
    pub fn current_reserve_ratio(&self) -> f64 {
        if self.deposits > 0.0 {
            self.reserves_at_central_bank / self.deposits
        } else {
            1.0 // No deposits = 100% reserve ratio (trivially compliant)
        }
    }

    /// Checks if bank meets reserve requirements.
    ///
    /// # Arguments
    /// * `cb_reserve_ratio` - Central Bank's reserve requirement ratio
    ///
    /// # Returns
    /// true if reserves >= required reserves
    pub fn meets_reserve_requirement(&self, cb_reserve_ratio: f64) -> bool {
        let required_reserves = self.deposits * cb_reserve_ratio;
        self.reserves_at_central_bank >= required_reserves
    }

    /// Calculates reserve deficit (shortfall) or surplus.
    ///
    /// # Arguments
    /// * `cb_reserve_ratio` - Central Bank's reserve requirement ratio
    ///
    /// # Returns
    /// Positive = surplus, Negative = deficit
    pub fn reserve_position(&self, cb_reserve_ratio: f64) -> f64 {
        let required_reserves = self.deposits * cb_reserve_ratio;
        self.reserves_at_central_bank - required_reserves
    }
}

/// Interbank market for daily liquidity exchange between banks.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct InterbankMarket {
    /// Current XIBOR (Interbank Offered Rate) - clearing price of liquidity.
    /// All commercial loan rates peg to XIBOR + Bank Margin.
    #[serde(default)]
    pub xibor: f64,

    /// Total liquidity available in the market (sum of all bank surplus reserves).
    #[serde(default)]
    pub available_liquidity: f64,

    /// Total liquidity demanded (sum of all bank reserve deficits).
    #[serde(default)]
    pub demanded_liquidity: f64,

    /// Last clearing turn.
    #[serde(default)]
    pub last_clearing_turn: u32,

    /// Market stress indicator (0.0 = normal, 1.0 = crisis).
    /// High stress = wider spreads, higher XIBOR.
    #[serde(default)]
    pub stress_indicator: f64,

    /// Maximum stress premium in basis points (e.g., 0.02 for +200 bps).
    /// Applied to XIBOR during market stress.
    #[serde(default = "default_stress_premium")]
    pub max_stress_premium: f64,

    /// Any additional interbank market fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_stress_premium() -> f64 {
    0.02 // +200 bps max stress premium
}

impl InterbankMarket {
    /// Clears the interbank market and calculates XIBOR.
    /// Banks with surplus reserves lend to banks with deficits.
    ///
    /// # Arguments
    /// * `banks` - Mutable reference to all banks in the economy
    /// * `central_bank` - Reference to Central Bank for reserve ratio and rates
    /// * `current_turn` - Current turn number
    ///
    /// # Rules
    /// * XIBOR is bounded by CB Deposit Rate (floor) and CB Lombard Rate (ceiling)
    /// * Banks prefer interbank market over expensive CB Lombard facility
    /// * Market stress widens spreads and increases XIBOR
    pub fn clear_market(
        &mut self,
        banks: &mut Vec<&mut crate::entities::Company>,
        central_bank: &CentralBank,
        current_turn: u32,
    ) {
        let cb_reserve_ratio = central_bank.reserve_requirement_ratio;
        let cb_deposit_rate = central_bank.interest_rates.deposit_rate;
        let cb_lombard_rate = central_bank.interest_rates.lombard_rate;

        // Separate banks into surplus and deficit
        let mut surplus_banks: Vec<(String, f64)> = Vec::new();
        let mut deficit_banks: Vec<(String, f64)> = Vec::new();

        for bank in banks.iter() {
            if let (
                Some(BankType::Commercial | BankType::Universal | BankType::Cooperative),
                Some(bs),
            ) = (&bank.bank_type, &bank.balance_sheet)
            {
                let position = bs.reserve_position(cb_reserve_ratio);

                if position > 0.0 {
                    surplus_banks.push((bank.id.clone(), position));
                } else if position < 0.0 {
                    deficit_banks.push((bank.id.clone(), -position));
                }
            }
        }

        // Calculate totals
        let total_surplus: f64 = surplus_banks.iter().map(|(_, amount)| *amount).sum();
        let total_deficit: f64 = deficit_banks.iter().map(|(_, amount)| *amount).sum();

        self.available_liquidity = total_surplus;
        self.demanded_liquidity = total_deficit;

        // Calculate XIBOR based on supply/demand balance
        // If surplus >= deficit: XIBOR near CB deposit rate
        // If deficit > surplus: XIBOR rises toward CB lombard rate
        let supply_demand_ratio = if total_deficit > 0.0 {
            total_surplus / total_deficit
        } else {
            1.0
        };

        // Base XIBOR calculation
        let base_xibor =
            cb_deposit_rate + (cb_lombard_rate - cb_deposit_rate) * (1.0 - supply_demand_ratio);

        // Apply stress indicator (widens spreads during crisis)
        let stressed_xibor = base_xibor + (self.stress_indicator * self.max_stress_premium);

        // Bound XIBOR between CB rates
        self.xibor = stressed_xibor.max(cb_deposit_rate).min(cb_lombard_rate);
        self.last_clearing_turn = current_turn;

        // Execute transfers using proportional distribution
        let transfer_amount = total_surplus.min(total_deficit);

        // Update bank balance sheets with proportional allocation
        for bank in banks.iter_mut() {
            if let (Some(_), Some(ref mut bs)) = (&bank.bank_type, &mut bank.balance_sheet) {
                let position = bs.reserve_position(cb_reserve_ratio);

                if position > 0.0 {
                    // This bank lends liquidity proportionally to its surplus
                    let lend_amount = (position * (transfer_amount / total_surplus)).min(position);
                    bs.reserves_at_central_bank -= lend_amount;
                    // In full implementation: Track specific borrower-bank relationships
                    // For now, simplified proportional distribution
                    let per_borrower_amount = if deficit_banks.is_empty() {
                        0.0
                    } else {
                        lend_amount / deficit_banks.len() as f64
                    };
                    for (borrower_id, _) in &deficit_banks {
                        *bs.interbank_loans_given
                            .entry(borrower_id.clone())
                            .or_insert(0.0) += per_borrower_amount;
                    }
                } else if position < 0.0 {
                    // This bank borrows liquidity proportionally to its deficit
                    let borrow_amount =
                        (-position * (transfer_amount / total_deficit)).min(-position);
                    bs.reserves_at_central_bank += borrow_amount;
                    // In full implementation: Track specific lender-bank relationships
                    // For now, simplified proportional distribution
                    let per_lender_amount = if surplus_banks.is_empty() {
                        0.0
                    } else {
                        borrow_amount / surplus_banks.len() as f64
                    };
                    for (lender_id, _) in &surplus_banks {
                        *bs.interbank_loans_taken
                            .entry(lender_id.clone())
                            .or_insert(0.0) += per_lender_amount;
                    }
                }
            }
        }
    }

    /// Updates market stress indicator based on systemic conditions.
    ///
    /// # Arguments
    /// * `bank_failures_this_turn` - Number of bank failures this turn
    /// * `total_banks` - Total number of banks
    /// * `xibor_volatility` - Recent XIBOR volatility
    ///
    /// # Rules
    /// * Bank failures increase stress
    /// * High XIBOR volatility increases stress
    /// * Stress decays slowly over time if no new shocks
    pub fn update_stress_indicator(
        &mut self,
        bank_failures_this_turn: u32,
        total_banks: usize,
        xibor_volatility: f64,
    ) {
        let failure_rate = if total_banks > 0 {
            bank_failures_this_turn as f64 / total_banks as f64
        } else {
            0.0
        };

        // Stress increases with failures and volatility
        let stress_increase = (failure_rate * 5.0) + (xibor_volatility * 10.0);

        // Stress decays by 10% per turn naturally
        self.stress_indicator = (self.stress_indicator * 0.9 + stress_increase)
            .max(0.0)
            .min(1.0);
    }
}

// ============================================================================
// STAGE D PHASE 2: CREDIT SCORING AND LOAN ISSUANCE
// ============================================================================

/// Credit scoring result for loan approval decision.
#[derive(Debug, Clone, PartialEq)]
pub struct CreditScore {
    /// Overall score (0.0 - 1.0, higher = better credit).
    pub score: f64,
    /// Maximum recommended loan amount based on collateral.
    pub max_loan_amount: f64,
    /// Recommended interest rate premium over XIBOR (basis points).
    pub risk_premium_bps: f64,
    /// Whether loan should be approved.
    pub approved: bool,
    /// Rejection reason if not approved.
    pub rejection_reason: Option<String>,
    /// Required debt-to-equity swap percentage (0.0 - 1.0) for consolidation loans.
    /// None for WorkingCapital and Investment loans.
    pub required_equity_swap: Option<f64>,
}

/// Calculates credit score for a potential borrower.
///
/// # Arguments
/// * `borrower` - Reference to the borrower (implements Borrower trait)
/// * `loan_type` - Type of loan being requested
/// * `requested_principal` - Amount being requested
/// * `central_bank` - Reference to Central Bank for economic context
/// * `bank_id` - ID of the bank evaluating the loan (for consolidation debt checking)
/// * `existing_loans` - Reference to bank's existing loans (to check for existing debtor status)
///
/// # Returns
/// CreditScore with approval decision, risk assessment, and required equity swap
///
/// # Rules
/// * LTV (Loan-to-Value): principal <= collateral_value × max_ltv_ratio
/// * Cashflow History: Borrower must be profitable in 2 of last 3 turns
/// * Investment Prospect (Investment loans only): Projected ROI must exceed hurdle rate
/// * Consolidation Loans: Existing debtor check + Debt-to-Equity Swap requirements
pub fn calculate_credit_score(
    borrower: &impl Borrower,
    loan_type: LoanType,
    requested_principal: f64,
    central_bank: &CentralBank,
    _bank_id: &str,
    existing_loans: &[Loan],
) -> CreditScore {
    let mut score = 0.5; // Base score
    let mut risk_premium_bps: f64 = 0.0;
    let mut approved = true;
    let mut rejection_reason = None;
    let mut required_equity_swap = None;

    // Phase 25: Creditworthiness gate — do not issue loans to economically
    // inactive borrowers. If a company has no liquid capital at all (neither
    // stored nor computed), it has no cash flow to service debt. This stops
    // the M3 ventilator from injecting credit into a dead economy.
    let total_liquid = borrower
        .liquid_capital()
        .max(borrower.computed_liquid_capital());
    if total_liquid <= 0.0 && borrower.liabilities() <= 0.0 {
        // No cash and no existing liabilities = never traded, never hired.
        // Reject unless there's fixed capital to collateralize (startup case).
        if borrower.fixed_capital() <= 0.0 {
            return CreditScore {
                score: 0.0,
                max_loan_amount: 0.0,
                risk_premium_bps: 500.0,
                approved: false,
                rejection_reason: Some(
                    "Economically inactive borrower: no liquid capital, no fixed capital"
                        .to_string(),
                ),
                required_equity_swap: None,
            };
        }
        // Has fixed capital but no cash — penalize heavily but allow
        // (this is a startup/seed case where the company has assets but
        // hasn't generated revenue yet).
        score -= 0.2;
        risk_premium_bps += 150.0;
    }

    // LTV Assessment
    let ltv_ratio = match loan_type {
        LoanType::WorkingCapital => 0.8, // 80% LTV for working capital
        LoanType::Investment => 0.6,     // 60% LTV for investment (stricter)
        LoanType::Consolidation => 0.7,  // 70% LTV for consolidation (may be overridden)
    };

    // Collateral base depends on loan type
    // WorkingCapital: Use liquid capital (service sector friendly)
    // Investment/Consolidation: Use fixed capital (requires hard assets)
    let collateral_value = match loan_type {
        LoanType::WorkingCapital => borrower.liquid_capital().max(borrower.fixed_capital()),
        LoanType::Investment => borrower.fixed_capital(),
        LoanType::Consolidation => borrower.fixed_capital(),
    };

    let max_loan_amount = collateral_value * ltv_ratio;

    if requested_principal > max_loan_amount {
        approved = false;
        rejection_reason = Some(format!(
            "LTV violation: requested {} exceeds maximum {} (collateral: {}, LTV: {})",
            requested_principal, max_loan_amount, collateral_value, ltv_ratio
        ));
        return CreditScore {
            score: 0.0,
            max_loan_amount,
            risk_premium_bps: 500.0,
            approved,
            rejection_reason,
            required_equity_swap,
        };
    }

    // LTV Score (closer to max = lower score)
    let ltv_utilization = requested_principal / max_loan_amount;
    score += (1.0 - ltv_utilization) * 0.2;

    // Cashflow History Assessment
    // For consolidation loans, project bilateral balance sheet (assets and liabilities)
    let liquidity_ratio = if loan_type == LoanType::Consolidation {
        // Sum outstanding balances of existing loans for this borrower
        let existing_loan_outstanding: f64 = existing_loans
            .iter()
            .filter(|loan| loan.borrower_id == borrower.id())
            .map(|loan| loan.outstanding_balance)
            .sum();

        // Bilateral projection: both assets and liabilities change
        let projected_liquid_assets =
            borrower.computed_liquid_capital() + requested_principal - existing_loan_outstanding;
        let projected_liabilities =
            borrower.liabilities() + requested_principal - existing_loan_outstanding;

        if projected_liabilities > 0.0 {
            projected_liquid_assets / projected_liabilities
        } else {
            2.0
        }
    } else if borrower.liabilities() > 0.0 {
        borrower.computed_liquid_capital() / borrower.liabilities()
    } else {
        2.0
    };

    if liquidity_ratio < 1.0 {
        score -= 0.3;
        risk_premium_bps += 100.0;
    } else if liquidity_ratio > 2.0 {
        score += 0.1;
        risk_premium_bps -= 25.0;
    }

    // Investment Prospect (Investment loans only)
    if loan_type == LoanType::Investment {
        let _hurdle_rate = central_bank.interest_rates.reference_rate + 0.05;
        // Relative capital strength: borrower must have 1.5x the requested principal in fixed capital
        let capital_strength = if requested_principal > 0.0 {
            borrower.fixed_capital() / requested_principal
        } else {
            0.0
        };

        if capital_strength >= 1.5 {
            score += 0.1;
            risk_premium_bps -= 50.0;
        } else {
            score -= 0.1;
            risk_premium_bps += 50.0;
        }
    }

    // Consolidation Loan: Debt-to-Equity Swap Logic
    if loan_type == LoanType::Consolidation {
        // Check if borrower is already an existing debtor of this bank
        let is_existing_debtor = existing_loans
            .iter()
            .any(|loan| loan.borrower_id == borrower.id());

        if is_existing_debtor {
            // Existing debtor: Bank tries to save them
            // Requires positive operating cash flow before debt service
            if liquidity_ratio >= 1.2 {
                // Viable restructuring plan
                score += 0.1;
                required_equity_swap = Some(0.15); // 15% equity swap for bailout
                risk_premium_bps += 75.0;
            } else {
                // Not viable - reject
                approved = false;
                rejection_reason = Some(
                    "Existing debtor with insufficient cash flow for restructuring".to_string(),
                );
            }
        } else {
            // New debtor: Bank demands massive equity swap for consolidation
            // Only approved if LTV is exceptionally low (< 50%)
            if ltv_utilization < 0.5 {
                required_equity_swap = Some(0.51); // 51% equity swap (majority control)
                risk_premium_bps += 200.0;
                score -= 0.2; // Penalty for risky new consolidation
            } else {
                approved = false;
                rejection_reason = Some(
                    "New consolidation borrower requires LTV < 50% for majority equity swap"
                        .to_string(),
                );
            }
        }
    }

    // Clamp score and premium
    score = score.max(0.0).min(1.0);
    risk_premium_bps = risk_premium_bps.max(0.0_f64).min(500.0_f64);

    // Final approval threshold
    if score < 0.3 {
        approved = false;
        rejection_reason = Some(format!("Credit score too low: {:.2}", score));
    }

    CreditScore {
        score,
        max_loan_amount,
        risk_premium_bps,
        approved,
        rejection_reason,
        required_equity_swap,
    }
}

/// Result of a loan issuance operation.
#[derive(Debug, Clone, PartialEq)]
pub struct LoanResult {
    /// The created loan record.
    pub loan: Loan,
    /// Principal amount to be credited to borrower (for external mutation).
    pub principal_amount: f64,
    /// Required debt-to-equity swap percentage (if applicable).
    pub required_equity_swap: Option<f64>,
}

/// Borrower-side reference to a loan on a bank's balance sheet.
/// Source of truth remains the bank's `loans_issued`; this is an auditable index.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct LoanRef {
    /// Unique loan identifier.
    pub loan_id: String,
    /// ID of the lending bank.
    pub bank_id: String,
    /// Original principal amount.
    pub principal: f64,
    /// Current outstanding balance.
    pub outstanding_balance: f64,
    /// Current interest rate.
    pub interest_rate: f64,
    /// Remaining / original term in turns.
    pub term_turns: u32,
    /// Current payment status.
    pub status: LoanStatus,
}

/// Issues a new loan with fractional reserve credit creation.
/// This function operates on BankBalanceSheet to avoid Rust borrow checker violations.
/// The caller must handle borrower.liquid_capital mutation externally.
///
/// # Arguments
/// * `balance_sheet` - Mutable reference to the bank's balance sheet
/// * `bank_id` - ID of the bank issuing the loan
/// * `bank_margin` - Bank's margin over XIBOR (e.g., 0.015 for +150 bps)
/// * `borrower` - Reference to the borrower (implements Borrower trait, immutable for scoring)
/// * `borrower_id` - ID of the borrower
/// * `principal` - Loan amount
/// * `loan_type` - Type of loan (WorkingCapital, Investment, Consolidation)
/// * `term_turns` - Loan term in turns
/// * `central_bank` - Reference to Central Bank for reserve ratio and rates
/// * `xibor` - Current interbank offered rate
///
/// # Returns
/// * Ok(LoanResult) if loan issued successfully
/// * Err if credit check fails or reserve requirement cannot be met
///
/// # Rules
/// * Credit scoring must approve the loan
/// * After loan creation, bank must meet reserve requirement
/// * If reserves insufficient, bank must borrow from interbank/CB Lombard first
/// * Double-entry: Bank's loans_issued (asset) and deposits (liability) both increase
/// * Caller must add principal_amount to borrower.liquid_capital externally
pub fn issue_loan(
    balance_sheet: &mut BankBalanceSheet,
    bank_id: &str,
    bank_margin: f64,
    borrower: &impl Borrower,
    borrower_id: &str,
    principal: f64,
    loan_type: LoanType,
    term_turns: u32,
    central_bank: &CentralBank,
    xibor: f64,
) -> Result<LoanResult, String> {
    // Step 1: Credit Scoring
    let credit_score = calculate_credit_score(
        borrower,
        loan_type.clone(),
        principal,
        central_bank,
        bank_id,
        &balance_sheet.loans_issued,
    );

    if !credit_score.approved {
        return Err(format!(
            "Credit check failed: {}",
            credit_score
                .rejection_reason
                .unwrap_or("Unknown reason".to_string())
        ));
    }

    // Step 2: Calculate loan interest rate (XIBOR + Bank Margin + Risk Premium)
    let risk_premium = credit_score.risk_premium_bps / 10000.0; // Convert bps to decimal
    let interest_rate = xibor + bank_margin + risk_premium;

    // Step 3: Simulate balance sheet expansion to check reserve requirement.
    // Phase 77: Subtract Lombard loans from effective reserves — borrowed
    // reserves from the CB Lombard facility cannot support further credit
    // creation. Only the bank's OWN reserves count toward lending capacity.
    let new_deposits = balance_sheet.deposits + principal;
    let required_reserves = new_deposits * central_bank.reserve_requirement_ratio;
    let effective_reserves =
        balance_sheet.reserves_at_central_bank - balance_sheet.cb_lombard_loans;

    if effective_reserves < required_reserves {
        return Err(format!(
            "Reserve requirement violation: need {} reserves, have {} effective ({} raw - {} lombard)",
            required_reserves, effective_reserves,
            balance_sheet.reserves_at_central_bank, balance_sheet.cb_lombard_loans
        ));
    }

    // Step 4: Create loan record
    // Use timestamp-based ID instead of uuid to avoid dependency issues
    let loan_id = format!(
        "LOAN-{}-{}",
        bank_id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let loan = Loan {
        id: loan_id.clone(),
        borrower_id: borrower_id.to_string(),
        principal,
        outstanding_balance: principal,
        interest_rate,
        term_turns,
        turns_remaining: term_turns,
        collateral_value: Some(borrower.fixed_capital()),
        loan_type,
        last_payment_turn: 0,
        status: LoanStatus::Current,
        interest_type: InterestType::default(),
        duration_risk_premium: 0.0,
        base_xibor: xibor,
        bank_margin,
        securitized: false,
        pledged_to_covered_bond: None,
        extra: Map::new(),
    };

    // Step 5: FRACTIONAL RESERVE CREDIT CREATION (Double-Entry)
    // Asset side: New loan created
    balance_sheet.loans_issued.push(loan.clone());

    // Liability side: New deposit created (this is the money creation)
    balance_sheet.deposits += principal;

    // IMPORTANT: reserves_at_central_bank DOES NOT change during loan creation
    // Reserves only change during clearing when borrower wires money to another bank

    // Step 6: Return result for external borrower mutation
    Ok(LoanResult {
        loan,
        principal_amount: principal,
        required_equity_swap: credit_score.required_equity_swap,
    })
}

/// Phase 35: A consumer loan (B2C) issued by a bank to a class demographic.
///
/// Tracks the principal, interest rate, and repayment state so that every
/// turn the class can pay down debt plus interest, with interest flowing back
/// to the issuing bank as B2C revenue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConsumerLoan {
    /// Region ID where the borrowing class resides.
    #[serde(default)]
    pub region_id: String,
    /// Class key (e.g., "Aristocracy", "Peasants") identifying the demographic.
    #[serde(default)]
    pub class_key: String,
    /// Whether the class is rural (true) or urban (false).
    #[serde(default)]
    pub is_rural: bool,
    /// Outstanding principal balance.
    #[serde(default)]
    pub outstanding_principal: f64,
    /// Annualized interest rate (e.g., 0.08 = 8%).
    #[serde(default)]
    pub interest_rate: f64,
    /// Turn when the loan was issued.
    #[serde(default)]
    pub issued_turn: u32,
    /// Original principal amount.
    #[serde(default)]
    pub original_principal: f64,
}

/// A single commercial bank.
///
/// # Rules
/// * Field names mirror the English keys in `data/banks.json`.
/// * `reserve_requirement_ratio` and `liquid_reserves` are Rust-only runtime
///   fields (not present in the Python JSON) and default to sensible values.
/// * The `extra` catch-all preserves any runtime-added fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bank {
    /// Unique bank identifier.
    #[serde(default)]
    pub id: String,

    /// Display name, e.g. "Main State Bank Illyria".
    #[serde(default)]
    pub name: String,

    /// Bank type, e.g. "Commercial", "Cooperative",
    /// "Investment", "State".
    #[serde(default)]
    pub bank_type: String,

    /// Optional subtype, e.g. "Uniwersalny".
    #[serde(default)]
    pub subtype: Option<String>,

    /// Own capital / equity.
    #[serde(default)]
    pub own_capital: f64,

    /// Total customer deposits.
    #[serde(default)]
    pub total_deposits: f64,

    /// Total loans currently issued.
    #[serde(default)]
    pub issued_loans: f64,

    /// Mandatory reserves as declared in the Python save.
    #[serde(default)]
    pub mandatory_reserves: f64,

    /// Liquid reserves available to the bank. This is a Rust-only field; it is
    /// initialised from `mandatory_reserves` if `liquid_reserves` is missing.
    #[serde(default)]
    pub liquid_reserves: f64,

    /// Liquidity ratio, typically `liquid_reserves / required_reserves`.
    #[serde(default)]
    pub liquidity: f64,

    /// Interest rate paid on deposits.
    #[serde(default)]
    pub deposit_interest_rate: f64,

    /// Interest rate charged on loans.
    #[serde(default)]
    pub interest_rate: f64,

    /// Current condition / rating, e.g. "Excellent" or "Endangered".
    #[serde(default = "default_condition")]
    pub condition: String,

    /// Last turn's new credit issuance.
    #[serde(default)]
    pub last_new_credit: f64,

    /// Fractional reserve requirement ratio, e.g. `0.10` for 10%.
    ///
    /// This is a Rust-only runtime field; it is copied from the currency
    /// settings when a full game state is loaded.
    #[serde(default = "default_reserve_requirement_ratio")]
    pub reserve_requirement_ratio: f64,

    /// Phase 35: DSPW (Domestic Fulfilling Leading Entity) Primary Dealer status.
    /// When true, this bank is authorized to participate directly in primary
    /// sovereign bond auctions. Non-DSPW banks can only buy sovereign bonds
    /// on the secondary market.
    #[serde(default)]
    pub is_dspw: bool,

    /// Phase 35: Consumer loan portfolio (B2C loans issued to households).
    /// Each entry tracks an outstanding consumer loan with the class demographic
    /// key, principal, interest rate, and issuing bank.
    #[serde(default)]
    pub consumer_loans: Vec<ConsumerLoan>,

    /// Any additional bank fields not explicitly modeled.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Bank {
    /// Computes the required reserves for this bank.
    ///
    /// # Returns
    /// `total_deposits * reserve_requirement_ratio`.
    ///
    /// # Rules
    /// * Direct port of the fractional-reserve reserve floor used in the Python
    ///   banking turn.
    pub fn required_reserves(&self) -> f64 {
        self.total_deposits * self.reserve_requirement_ratio
    }

    /// Computes the maximum additional credit this bank can create given its
    /// deposits, current loans, and reserve requirement.
    ///
    /// # Returns
    /// The maximum new credit (turned into `last_new_credit` and added to
    /// `issued_loans` by the turn processor). If liquid reserves are below the
    /// required floor, the capacity is `0.0`.
    ///
    /// # Rules
    /// * `max_new_credit = max(0, total_deposits - required_reserves - issued_loans)`
    ///   when the bank is sufficiently reserved.
    /// * This is the deterministic reserve-limit formula.
    pub fn max_new_credit(&self) -> f64 {
        let required = self.required_reserves();
        if self.liquid_reserves < required {
            return 0.0;
        }
        let capacity = self.total_deposits - required - self.issued_loans;
        capacity.max(0.0)
    }
}

// ============================================================================
// PHASE 77: BANK OPERATIONAL CAPACITY — LABOR & SERVICE CONSTRAINTS
// ============================================================================

/// Phase 77: Operational capacity of a bank, derived from its fulfilled labor.
///
/// A bank's ability to manage assets, originate loans, and handle deposits is
/// directly proportional to its workforce. A bank with 30 employees cannot
/// manage billions in assets — it needs thousands of clerks, tellers, and
/// administrative staff.
///
/// The capacity scales with `average_wage` (not a magic nominal constant)
/// because a clerk earning more in a high-wage economy processes proportionally
/// larger transaction values.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BankCapacity {
    /// Maximum total assets (loans + securities) the bank can manage.
    pub max_asset_under_management: f64,
    /// Maximum new loan principal that can be originated in a single turn.
    pub max_new_loans_per_turn: f64,
    /// Maximum deposit volume the bank can service per turn.
    pub max_deposit_handling: f64,
}

/// Phase 77: Compute a bank's operational capacity from its fulfilled FTE.
///
/// # Arguments
/// * `fulfilled_fte` - Number of workers currently employed by the bank.
/// * `average_wage` - The national average wage (scales capacity dynamically).
///
/// # Rules
/// * Each clerk can manage ~200× their wage in total assets (ongoing portfolio).
/// * Each clerk can originate ~50× their wage in new loans per turn (origination workload).
/// * Each clerk can service ~500× their wage in deposits (transaction processing).
/// * These are structural economic ratios, not magic nominal constants.
/// * A bank with 0 FTE has zero capacity — it cannot operate.
pub fn bank_operational_capacity(fulfilled_fte: f64, average_wage: f64) -> BankCapacity {
    if fulfilled_fte <= 0.0 || average_wage <= 0.0 {
        return BankCapacity::default();
    }
    let fte = fulfilled_fte;
    let wage = average_wage.max(1.0);
    BankCapacity {
        max_asset_under_management: fte * wage * 200.0,
        max_new_loans_per_turn: fte * wage * 50.0,
        max_deposit_handling: fte * wage * 500.0,
    }
}

// ============================================================================
// STAGE D PHASE 3: BANKING SAFETY NETS AND RESOLUTION
// ============================================================================

/// Mandatory deposit insurance fund for protecting depositors.
/// Every Commercial and Universal bank pays premiums into this pool.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct BfgFund {
    /// Total reserves in the BFG pool (funded by bank premiums).
    #[serde(default)]
    pub reserves: f64,

    /// Premium rate (percentage of total deposits charged each turn).
    /// Typical range: 0.05% to 0.20% (5-20 bps).
    #[serde(default = "default_bfg_premium")]
    pub premium_rate: f64,

    /// Insurance limit multiplier (multiple of average national wage).
    /// Calculated dynamically: max_insured = average_wage * this_multiplier.
    #[serde(default = "default_insurance_multiplier")]
    pub insurance_limit_multiplier: f64,

    /// Total payouts made (historical record).
    #[serde(default)]
    pub total_payouts: f64,

    /// Number of bank failures covered.
    #[serde(default)]
    pub failures_covered: u32,

    /// Last premium collection turn.
    #[serde(default)]
    pub last_premium_turn: u32,

    /// Emergency liquidity loan from Central Bank (when reserves depleted).
    #[serde(default)]
    pub cb_emergency_loan: f64,

    /// State subsidy received (non-refundable from Treasury).
    #[serde(default)]
    pub state_subsidy: f64,

    /// Any additional BFG fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_bfg_premium() -> f64 {
    0.001 // 0.1% of deposits (10 bps)
}

fn default_insurance_multiplier() -> f64 {
    100.0 // 100x average national wage (dynamic, not hardcoded)
}

impl BfgFund {
    /// Calculates the maximum insured amount based on current economic conditions.
    ///
    /// # Arguments
    /// * `average_wage` - Current average national wage from macro_indicators
    ///
    /// # Returns
    /// Maximum insured amount per depositor (dynamic, not hardcoded)
    pub fn calculate_max_insured_amount(&self, average_wage: f64) -> f64 {
        average_wage * self.insurance_limit_multiplier
    }

    /// Collects mandatory premiums from all Commercial and Universal banks.
    ///
    /// # Arguments
    /// * `banks` - Mutable reference to all banks in the economy
    /// * `current_turn` - Current turn number
    ///
    /// # Double-Entry Flow
    /// * Bank: reserves_at_central_bank decreases (Asset debit)
    /// * BFG: reserves increases (Asset credit)
    /// * Money mass preserved: Reserves move from bank to BFG ledger (both at CB)
    /// * tier_1_capital is NOT debited — the premium is a transfer of reserves,
    ///   not a capital destruction. The bank's equity position is unaffected.
    pub fn collect_premiums(
        &mut self,
        banks: &mut Vec<&mut crate::entities::Company>,
        current_turn: u32,
    ) {
        for bank in banks.iter_mut() {
            if let Some(bs) = &mut bank.balance_sheet {
                // Only Commercial and Universal banks pay premiums
                if let Some(ref bt) = bank.bank_type {
                    if bt == &BankType::Commercial || bt == &BankType::Universal {
                        let premium = bs.deposits * self.premium_rate;

                        // Double-entry: Bank pays premium from reserves only (asset transfer)
                        // tier_1_capital is NOT touched — premium is a reserve transfer,
                        // not a capital reduction. Previous code debited both asset and
                        // equity, destroying money mass by `premium` each turn (Black Hole 1.9).
                        bs.reserves_at_central_bank -= premium; // Asset decreases
                        if bs.reserves_at_central_bank < 0.0 {
                            bs.reserves_at_central_bank = 0.0;
                        } // Phase 43: clamp
                        self.reserves += premium; // BFG receives
                    }
                }
            }
        }
        self.last_premium_turn = current_turn;
    }

    /// Receives emergency liquidity from Central Bank (short-term loan).
    ///
    /// # Arguments
    /// * `central_bank` - Reference to Central Bank for emergency lending
    /// * `amount` - Requested loan amount
    ///
    /// # Rules
    /// * CB expands M0 to bail out the safety net (lender of last resort)
    /// * Loan is short-term, low-interest (below market rate)
    /// * Must be repaid from future premium collections
    pub fn receive_cb_liquidity_line(&mut self, central_bank: &mut CentralBank, amount: f64) {
        if amount <= 0.0 {
            return;
        }
        // CB grants emergency loan (M0 expansion)
        self.cb_emergency_loan += amount;
        self.reserves += amount;
        central_bank.liquidity_injected += amount;
    }

    /// Receives state subsidy from Treasury (non-refundable).
    ///
    /// # Arguments
    /// * `treasury` - Reference to Country treasury
    /// * `amount` - Subsidy amount
    ///
    /// # Rules
    /// * Direct cash injection from government budget
    /// * Non-refundable (not a loan)
    /// * Used to replenish BFG reserves during systemic crises
    pub fn receive_state_subsidy(&mut self, treasury: &mut crate::state::Treasury, amount: f64) {
        // Treasury transfers liquid reserves to BFG
        treasury.liquid_reserves -= amount;
        self.state_subsidy += amount;
        self.reserves += amount;
    }

    /// Repays Central Bank emergency loan from premium collections.
    ///
    /// # Arguments
    /// * `central_bank` - Reference to Central Bank
    /// * `amount` - Repayment amount
    pub fn repay_cb_liquidity_line(&mut self, central_bank: &mut CentralBank, amount: f64) {
        let repayment = amount.min(self.cb_emergency_loan);
        if repayment <= 0.0 {
            return;
        }
        self.cb_emergency_loan -= repayment;
        self.reserves -= repayment;
        central_bank.liquidity_injected = (central_bank.liquidity_injected - repayment).max(0.0);
    }
}

/// Voluntary Institutional Protection Scheme for member banks.
/// Provides emergency liquidity at preferential rates before CB Lombard.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SobkScheme {
    /// Total liquidity pool (funded by voluntary member contributions).
    #[serde(default)]
    pub pool: f64,

    /// Preferential rate spread over XIBOR (below CB Lombard).
    /// Typical: 50 bps (vs Lombard: 150-200 bps).
    #[serde(default = "default_sobk_spread")]
    pub preferential_spread: f64,

    /// Maximum loan percentage of pool per member per turn.
    /// Prevents single member from draining the entire pool.
    #[serde(default = "default_max_loan_percent")]
    pub max_loan_percent_of_pool: f64,

    /// Member bank IDs and their contribution history.
    #[serde(default)]
    pub members: Vec<String>,

    /// Outstanding SOBK loans (member_id -> amount).
    #[serde(default)]
    pub outstanding_loans: HashMap<String, f64>,

    /// Last turn when pool was rebalanced.
    #[serde(default)]
    pub last_rebalance_turn: u32,

    /// Emergency liquidity loan from Central Bank (when pool depleted).
    #[serde(default)]
    pub cb_emergency_loan: f64,

    /// State subsidy received (non-refundable from Treasury).
    #[serde(default)]
    pub state_subsidy: f64,

    /// Any additional SOBK fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_sobk_spread() -> f64 {
    0.005 // 50 bps preferential spread over XIBOR
}

fn default_max_loan_percent() -> f64 {
    0.20 // 20% of pool max per member per turn (dynamic, not hardcoded)
}

impl SobkScheme {
    /// Accepts voluntary liquidity contribution from a member bank.
    ///
    /// # Arguments
    /// * `bank` - The contributing bank
    ///
    /// # Double-Entry Flow
    /// * Bank: reserves_at_central_bank decreases (contribution)
    /// * SOBK: pool increases (contribution received)
    /// * Money mass preserved: Reserves move from bank to SOBK ledger
    pub fn accept_contribution(&mut self, bank: &mut crate::entities::Company) {
        if let Some(bs) = &mut bank.balance_sheet {
            // Banks contribute excess reserves (above requirement)
            let excess = bs.reserve_position(0.10); // Using 10% reserve ratio
            if excess > 0.0 {
                let contribution = excess * 0.5; // Contribute 50% of excess

                bs.reserves_at_central_bank -= contribution;
                self.pool += contribution;

                if !self.members.contains(&bank.id) {
                    self.members.push(bank.id.clone());
                }
            }
        }
    }

    /// Provides emergency loan to member bank (called from InterbankMarket).
    ///
    /// # Arguments
    /// * `bank_id` - The member bank requesting emergency liquidity
    /// * `amount` - Requested loan amount
    /// * `current_xibor` - Current XIBOR rate for pricing
    ///
    /// # Returns
    /// Actual loan amount (may be less if pool insufficient)
    ///
    /// # Integration Point
    /// Called from `InterbankMarket::clear_market()` when:
    /// 1. Bank cannot find liquidity on standard interbank market
    /// 2. XIBOR is too high or supply exhausted
    /// 3. Before bank is forced to use expensive CB Lombard rate
    pub fn provide_emergency_loan(
        &mut self,
        bank_id: &str,
        amount: f64,
        _current_xibor: f64,
    ) -> f64 {
        // Check if bank is a member
        if !self.members.contains(&bank_id.to_string()) {
            return 0.0;
        }

        // Dynamic max loan based on pool percentage (not hardcoded)
        let max_allowed = self.pool * self.max_loan_percent_of_pool;

        // Check if bank has outstanding loans
        let current_outstanding = self.outstanding_loans.get(bank_id).copied().unwrap_or(0.0);
        if current_outstanding >= max_allowed {
            return 0.0;
        }

        // Cap loan amount
        let available = self.pool;
        let remaining_allowance = max_allowed - current_outstanding;
        let loan_amount = amount.min(available).min(remaining_allowance);

        if loan_amount > 0.0 {
            self.pool -= loan_amount;
            *self
                .outstanding_loans
                .entry(bank_id.to_string())
                .or_insert(0.0) += loan_amount;
        }

        loan_amount
    }

    /// Receives emergency liquidity from Central Bank (short-term loan).
    ///
    /// # Arguments
    /// * `central_bank` - Reference to Central Bank for emergency lending
    /// * `amount` - Requested loan amount
    ///
    /// # Rules
    /// * CB expands M0 to bail out the safety net (lender of last resort)
    /// * Loan is short-term, low-interest (below market rate)
    /// * Must be repaid from future member contributions
    pub fn receive_cb_liquidity_line(&mut self, central_bank: &mut CentralBank, amount: f64) {
        if amount <= 0.0 {
            return;
        }
        // CB grants emergency loan (M0 expansion)
        self.cb_emergency_loan += amount;
        self.pool += amount;
        central_bank.liquidity_injected += amount;
    }

    /// Receives state subsidy from Treasury (non-refundable).
    ///
    /// # Arguments
    /// * `treasury` - Reference to Country treasury
    /// * `amount` - Subsidy amount
    ///
    /// # Rules
    /// * Direct cash injection from government budget
    /// * Non-refundable (not a loan)
    /// * Used to replenish SOBK pool during systemic crises
    pub fn receive_state_subsidy(&mut self, treasury: &mut crate::state::Treasury, amount: f64) {
        // Treasury transfers liquid reserves to SOBK
        treasury.liquid_reserves -= amount;
        self.state_subsidy += amount;
        self.pool += amount;
    }

    /// Repays SOBK emergency loan.
    ///
    /// # Arguments
    /// * `bank_id` - The member bank repaying
    /// * `amount` - Repayment amount
    pub fn repay_loan(&mut self, bank_id: &str, amount: f64) {
        if let Some(outstanding) = self.outstanding_loans.get_mut(bank_id) {
            let repayment = amount.min(*outstanding);
            *outstanding -= repayment;
            self.pool += repayment;

            if *outstanding < 1e-9 {
                self.outstanding_loans.remove(bank_id);
            }
        }
    }
}

/// Phase 86.5A: A distressed asset seized from a failed bank.
///
/// Represents a loan or bond transferred from a failed bank's balance sheet
/// to the State's distressed assets ledger at RECOVERY value (not face value).
///
/// **Critical invariant**: Distressed assets are NOT spendable Treasury funds.
/// They must not be counted in `liquid_reserves`, budget projections, or
/// State AI spending capacity. Cash enters liquid reserves only when
/// actually collected from loan recovery or bond maturity.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct DistressedAsset {
    /// Original face value of the asset (for accounting reference only).
    #[serde(default)]
    pub face_value: f64,

    /// Estimated recovery value (what can realistically be collected).
    /// This is the value used for all accounting purposes.
    #[serde(default)]
    pub recovery_value: f64,

    /// Asset type: "loan" or "bond".
    #[serde(default)]
    pub asset_type: String,

    /// Original borrower/issuer ID (for loan recovery routing).
    #[serde(default)]
    pub counterparty_id: String,

    /// Source bank ID (the failed bank this was seized from).
    #[serde(default)]
    pub source_bank_id: String,

    /// Turn when the asset was seized.
    #[serde(default)]
    pub seized_turn: u32,

    /// Amount recovered so far (updated as collections come in).
    #[serde(default)]
    pub recovered_amount: f64,

    /// Whether this asset has been fully resolved (recovered or written off).
    #[serde(default)]
    pub is_resolved: bool,
}

impl DistressedAsset {
    /// Phase 86.5A: Record a cash recovery from this distressed asset.
    /// Returns the amount recovered (clamped to remaining recovery value).
    pub fn record_recovery(&mut self, amount: f64) -> f64 {
        let remaining = self.recovery_value - self.recovered_amount;
        let actual = amount.min(remaining).max(0.0);
        self.recovered_amount += actual;
        if self.recovered_amount >= self.recovery_value {
            self.is_resolved = true;
        }
        actual
    }

    /// Phase 86.5A: Remaining unrecovered value.
    pub fn remaining_value(&self) -> f64 {
        (self.recovery_value - self.recovered_amount).max(0.0)
    }
}

/// Bank resolution authority for handling failed banks through bridge institutions.
/// Implements Good Bank/Bad Bank split: Bridge Bank gets assets + insured liabilities,
/// BFG absorbs toxic liabilities.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct BankResolution {
    /// Banks currently under bridge bank administration (bank_id -> takeover_turn).
    #[serde(default)]
    pub bridge_banks: HashMap<String, u32>,

    /// Maximum duration a bridge bank can operate before resolution.
    /// After this, the bank must be reprivatized or liquidated.
    #[serde(default = "default_bridge_duration")]
    pub max_bridge_duration_turns: u32,

    /// Total number of banks resolved (historical).
    #[serde(default)]
    pub banks_resolved: u32,

    /// Total equity wiped out from shareholders (historical).
    #[serde(default)]
    pub equity_wiped_out: f64,

    /// Total toxic liabilities absorbed by BFG (historical).
    #[serde(default)]
    pub toxic_liabilities_absorbed: f64,

    /// Revenue generated from bridge bank privatizations (historical).
    #[serde(default)]
    pub privatization_revenue: f64,

    /// Phase 86.5A: Distressed assets ledger — seized loans and bonds from
    /// failed banks at RECOVERY value (not face value).
    ///
    /// These assets are PHYSICALLY ISOLATED from `liquid_reserves` and must
    /// NOT be treated as spendable Treasury funds. Cash received later from
    /// loan recovery or bond maturity may enter liquid reserves only when
    /// actually collected.
    ///
    /// Key: asset_id (e.g., "failed_bank_id:loan:borrower_id")
    /// Value: DistressedAsset entry with recovery value and metadata.
    #[serde(default)]
    pub distressed_assets: HashMap<String, DistressedAsset>,

    /// Any additional bank resolution fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_bridge_duration() -> u32 {
    24 // 24 turns (e.g., 2 years) max bridge bank operation
}

impl BankResolution {
    /// Executes bank resolution through Good Bank/Bad Bank split.
    ///
    /// # Arguments
    /// * `failed_bank_id` - The ID of the bank that failed
    /// * `current_turn` - Current turn number
    /// * `bfg_fund` - Reference to BFG fund for toxic liability absorption
    /// * `average_wage` - Current average national wage for insurance limit calculation
    /// * `all_banks` - Reference to all banks (for creditor routing)
    /// * `central_bank` - Reference to Central Bank (for Lombard repayment)
    ///
    /// # Rules - Good Bank / Bad Bank Split
    /// * **Bridge Bank (Good Bank)**: Receives clients, all assets (loans, real_estate),
    ///   and insured portion of liabilities (retail deposits up to insurance limit)
    /// * **BFG Fund (Bad Bank)**: Absorbs toxic/excess liabilities (uninsured deposits,
    ///   interbank debt, CB Lombard debt) to clean the Bridge Bank's balance sheet
    /// * **Creditor Routing**: BFG must actually pay the creditors:
    ///   - Interbank loans: Route to specific lending banks' reserves_at_central_bank
    ///   - CB Lombard loans: Return to Central Bank
    ///   - Uninsured deposits: Payout to depositors (or written off)
    /// * **Equity Wipeout**: Previous owners are completely wiped out (owners.clear())
    /// * **BFG Ownership**: BFG becomes 100% owner of the Bridge Bank
    /// * **Privatization Path**: Bridge Bank flagged for privatization once Tier 1 stabilizes
    ///
    /// # Borrow Checker Compliance
    /// * Takes `failed_bank_id` instead of mutable reference to avoid double borrow
    /// * Extracts bank from vector internally using index manipulation
    pub fn execute_bank_resolution(
        &mut self,
        failed_bank_id: &str,
        current_turn: u32,
        bfg_fund: &mut BfgFund,
        average_wage: f64,
        all_banks: &mut Vec<&mut crate::entities::Company>,
        central_bank: &mut CentralBank,
    ) {
        // Find and extract the failed bank from the vector
        let failed_bank_index = all_banks
            .iter()
            .position(|b| b.id == failed_bank_id)
            .expect("Failed bank must exist");

        let failed_bank = all_banks.swap_remove(failed_bank_index);

        let bs = failed_bank
            .balance_sheet
            .as_mut()
            .expect("Bank must have balance sheet");

        // Step 1: Calculate insured vs uninsured deposits
        let _max_insured = bfg_fund.calculate_max_insured_amount(average_wage);
        let total_deposits = bs.deposits;

        // Assume average depositor has 50% of deposits uninsured (simplified)
        let insured_deposits = total_deposits * 0.5;
        let uninsured_deposits = total_deposits - insured_deposits;

        // Step 2: Wipe out existing shareholders (Equity → 0)
        let equity_wiped = failed_bank.company_capital;
        failed_bank.owners.clear();
        failed_bank.state_share = 0.0;
        failed_bank.company_capital = 0.0;
        bs.tier_1_capital = 0.0; // Equity wiped

        // Step 3: Good Bank / Bad Bank Split
        // Bridge Bank (Good Bank) keeps assets and insured liabilities
        let _bridge_bank_assets = bs.loans_issued.clone();
        let _bridge_bank_real_estate = bs.real_estate;
        let bridge_bank_insured_deposits = insured_deposits;

        // BFG (Bad Bank) absorbs toxic liabilities
        // Use HashMap for specific creditor tracking
        let mut toxic_interbank_total = 0.0;
        let failed_bank_key = failed_bank_id.to_string();
        for (lender_id, amount) in bs.interbank_loans_taken.iter() {
            toxic_interbank_total += amount;

            // Repay specific lending bank
            for bank in all_banks.iter_mut() {
                if bank.id == *lender_id {
                    if let Some(other_bs) = &mut bank.balance_sheet {
                        other_bs.reserves_at_central_bank += amount;
                        // Decrement the interbank loan amount
                        if let Some(loan_amount) =
                            other_bs.interbank_loans_given.get_mut(&failed_bank_key)
                        {
                            *loan_amount -= amount;
                        }
                    }
                    break;
                }
            }
        }

        // Clean up zero or negative interbank loans after all repayments
        for bank in all_banks.iter_mut() {
            if let Some(other_bs) = &mut bank.balance_sheet {
                other_bs.interbank_loans_given.retain(|_, v| *v > 0.0);
            }
        }

        // Clean up the specific failed bank entry from all lenders
        for bank in all_banks.iter_mut() {
            if let Some(other_bs) = &mut bank.balance_sheet {
                other_bs.interbank_loans_given.remove(&failed_bank_key);
            }
        }

        let toxic_lombard = bs.cb_lombard_loans;
        let toxic_uninsured = uninsured_deposits;
        let total_toxic = toxic_interbank_total + toxic_lombard + toxic_uninsured;

        // Step 4: Clean Bridge Bank balance sheet
        bs.deposits = bridge_bank_insured_deposits;
        bs.interbank_loans_taken.clear(); // Toxic debt removed (HashMap)
        bs.cb_lombard_loans = 0.0; // Toxic debt removed

        // Step 5: Route creditor payments (Money Mass Preservation)
        // 5a: Interbank loans already repaid to specific lenders above

        // 5b: Repay CB Lombard loans to Central Bank
        // CB liquidity_injected decreases — M0 contracts as Lombard loan is extinguished
        central_bank.liquidity_injected =
            (central_bank.liquidity_injected - toxic_lombard).max(0.0);

        // 5c: Uninsured deposits are written off (depositors take haircut)
        // The deposits were already extinguished at line 1460 (bs.deposits = insured_deposits).
        // BFG does NOT pay for uninsured deposits — no one receives the money,
        // so debiting BFG reserves would destroy money (Black Hole 1.11).

        // Step 6: BFG absorbs interbank and Lombard costs (NOT uninsured)
        let total_bfg_payout = toxic_interbank_total + toxic_lombard;

        // If BFG reserves are insufficient, get CB emergency liquidity line
        if total_bfg_payout > bfg_fund.reserves {
            let shortfall = total_bfg_payout - bfg_fund.reserves;
            bfg_fund.receive_cb_liquidity_line(central_bank, shortfall);
        }

        // Floor at 0.0 to prevent BFG from going negative (Black Hole 1.11)
        bfg_fund.reserves = (bfg_fund.reserves - total_bfg_payout).max(0.0);
        bfg_fund.total_payouts += total_bfg_payout;

        // Step 7: BFG becomes 100% owner of Bridge Bank
        failed_bank.owners.insert("BFG".to_string(), 1.0);
        failed_bank.state_share = 1.0;

        // Step 8: Mark as bridge bank with takeover timestamp
        self.bridge_banks
            .insert(failed_bank.id.clone(), current_turn);

        // Step 9: Update statistics
        self.banks_resolved += 1;
        self.equity_wiped_out += equity_wiped;
        self.toxic_liabilities_absorbed += total_toxic;

        // Step 10: Return the bridge bank to the vector
        all_banks.push(failed_bank);
    }

    /// Checks if a bridge bank is ready for privatization.
    ///
    /// # Arguments
    /// * `bridge_bank` - The bridge bank to check
    /// * `current_turn` - Current turn number
    ///
    /// # Returns
    /// true if bridge bank should be privatized (Tier 1 stabilized)
    pub fn ready_for_privatization(
        &self,
        bridge_bank: &crate::entities::Company,
        current_turn: u32,
    ) -> bool {
        // Check if bridge bank has operated for minimum duration
        if let Some(takeover_turn) = self.bridge_banks.get(&bridge_bank.id) {
            let duration = current_turn - takeover_turn;
            if duration < 12 {
                return false; // Minimum 12 turns before privatization
            }
        }

        // Check if Tier 1 Capital has stabilized (positive and growing)
        if let Some(bs) = &bridge_bank.balance_sheet {
            return bs.tier_1_capital > 0.0 && bs.is_balanced();
        }

        false
    }

    /// Privatizes a bridge bank by auctioning shares to private investors.
    ///
    /// # Arguments
    /// * `bridge_bank` - The bridge bank to privatize
    /// * `auction_price` - Total auction price paid by investors
    /// * `new_owners` - Map of new owner IDs to their share percentages
    /// * `bfg_fund` - Reference to BFG fund to receive privatization revenue
    ///
    /// # Rules
    /// * BFG transfers ownership to new private investors
    /// * Bank exits bridge bank status
    /// * Auction revenue goes to BFG to replenish reserves
    /// * New equity capital injected by investors
    pub fn privatize_bridge_bank(
        &mut self,
        bridge_bank: &mut crate::entities::Company,
        auction_price: f64,
        new_owners: HashMap<String, f64>,
        bfg_fund: &mut BfgFund,
    ) {
        // Remove BFG ownership
        bridge_bank.owners.remove("BFG");
        bridge_bank.state_share = 0.0;

        // Transfer to new owners
        for (owner_id, share) in new_owners {
            bridge_bank.owners.insert(owner_id, share);
        }

        // Inject new equity capital
        bridge_bank.company_capital = auction_price;
        if let Some(bs) = &mut bridge_bank.balance_sheet {
            bs.tier_1_capital = auction_price;
        }

        // Transfer auction revenue to BFG
        bfg_fund.reserves += auction_price;
        self.privatization_revenue += auction_price;

        // Remove from bridge bank registry
        self.bridge_banks.remove(&bridge_bank.id);
    }

    /// Liquidates a bridge bank that cannot be privatized.
    ///
    /// # Arguments
    /// * `bridge_bank` - The bridge bank to liquidate
    /// * `asset_buyers` - Map of buyer IDs to asset purchases
    /// * `bfg_fund` - Reference to BFG fund
    ///
    /// # Rules
    /// * Assets sold to highest bidders
    /// * Remaining liabilities paid from proceeds
    /// * Bank entity deleted from simulation
    /// * BFG absorbs any remaining shortfall
    pub fn liquidate_bridge_bank(
        &mut self,
        bridge_bank: &mut crate::entities::Company,
        _asset_buyers: HashMap<String, f64>,
        bfg_fund: &mut BfgFund,
    ) {
        let bs = bridge_bank
            .balance_sheet
            .as_ref()
            .expect("Bank must have balance sheet");

        // Calculate total asset value (simplified)
        let total_assets = bs.total_assets();

        // Calculate remaining liabilities
        let total_liabilities = bs.total_liabilities();

        // If assets > liabilities, surplus goes to BFG
        // If assets < liabilities, BFG absorbs shortfall
        let shortfall = total_liabilities - total_assets;
        if shortfall > 0.0 {
            bfg_fund.reserves -= shortfall;
            bfg_fund.total_payouts += shortfall;
        }

        // Remove from bridge bank registry
        self.bridge_banks.remove(&bridge_bank.id);

        // In full implementation: Delete bank entity from simulation
    }

    /// Phase 86.5A: Transfer failed bank loans to distressed assets ledger.
    ///
    /// Loans are transferred at RECOVERY value, not face value. Recovery value
    /// is estimated as a fraction of face value based on collateral and borrower
    /// creditworthiness. These assets must NOT enter spendable liquid reserves.
    ///
    /// # Arguments
    /// * `bank_id` - The failed bank's ID
    /// * `loans` - Map of borrower_id -> outstanding loan amount (face value)
    /// * `recovery_rate` - Estimated recovery fraction (0.0-1.0)
    /// * `seized_turn` - Current turn number
    pub fn transfer_loans_to_distressed(
        &mut self,
        bank_id: &str,
        loans: &HashMap<String, f64>,
        recovery_rate: f64,
        seized_turn: u32,
    ) {
        for (borrower_id, face_value) in loans {
            let recovery_value = face_value * recovery_rate.clamp(0.0, 1.0);
            let asset_id = format!("{}:loan:{}", bank_id, borrower_id);
            self.distressed_assets.insert(
                asset_id,
                DistressedAsset {
                    face_value: *face_value,
                    recovery_value,
                    asset_type: "loan".to_string(),
                    counterparty_id: borrower_id.clone(),
                    source_bank_id: bank_id.to_string(),
                    seized_turn,
                    recovered_amount: 0.0,
                    is_resolved: recovery_value <= 0.0,
                },
            );
        }
    }

    /// Phase 86.5A: Transfer failed bank bonds to distressed assets ledger.
    ///
    /// Bonds are transferred at current market value (not face value).
    pub fn transfer_bonds_to_distressed(
        &mut self,
        bank_id: &str,
        bonds: &HashMap<String, f64>,
        market_value_rate: f64,
        seized_turn: u32,
    ) {
        for (issuer_id, face_value) in bonds {
            let recovery_value = face_value * market_value_rate.clamp(0.0, 1.0);
            let asset_id = format!("{}:bond:{}", bank_id, issuer_id);
            self.distressed_assets.insert(
                asset_id,
                DistressedAsset {
                    face_value: *face_value,
                    recovery_value,
                    asset_type: "bond".to_string(),
                    counterparty_id: issuer_id.clone(),
                    source_bank_id: bank_id.to_string(),
                    seized_turn,
                    recovered_amount: 0.0,
                    is_resolved: recovery_value <= 0.0,
                },
            );
        }
    }

    /// Phase 86.5A: Record a cash recovery from a distressed asset.
    ///
    /// Returns the amount recovered. This amount may be added to liquid
    /// reserves (it is now actual cash, not a contingent asset).
    pub fn record_distressed_recovery(&mut self, asset_id: &str, amount: f64) -> f64 {
        if let Some(asset) = self.distressed_assets.get_mut(asset_id) {
            asset.record_recovery(amount)
        } else {
            0.0
        }
    }

    /// Phase 86.5A: Total recovery value of all distressed assets.
    /// This is NOT spendable Treasury funds.
    pub fn total_distressed_recovery_value(&self) -> f64 {
        self.distressed_assets
            .values()
            .filter(|a| !a.is_resolved)
            .map(|a| a.remaining_value())
            .sum()
    }

    /// Phase 86.5A: Total unrecovered distressed assets.
    /// This is NOT spendable Treasury funds.
    pub fn total_unrecovered_distressed(&self) -> f64 {
        self.distressed_assets
            .values()
            .filter(|a| !a.is_resolved)
            .map(|a| a.remaining_value())
            .sum()
    }

    /// Phase 86.5A: Check if a bridge bank's balance sheet sums to zero.
    ///
    /// A bank can only be removed from the simulation after its balance sheet
    /// is fully settled (assets = liabilities = 0).
    pub fn verify_zero_balance(&self, bridge_bank: &crate::entities::Company) -> bool {
        if let Some(bs) = &bridge_bank.balance_sheet {
            let total_assets = bs.total_assets();
            let total_liabilities = bs.total_liabilities();
            // Both must be effectively zero (within floating point tolerance).
            total_assets.abs() < 0.01 && total_liabilities.abs() < 0.01
        } else {
            false
        }
    }

    /// Phase 86.5A: Check if a bridge bank has exceeded its sunset timer.
    ///
    /// After `max_bridge_duration_turns`, the bridge bank must be liquidated
    /// or reprivatized — it cannot operate indefinitely.
    pub fn check_bridge_sunset(&self, bank_id: &str, current_turn: u32) -> bool {
        if let Some(&takeover_turn) = self.bridge_banks.get(bank_id) {
            current_turn - takeover_turn >= self.max_bridge_duration_turns
        } else {
            false
        }
    }
}

/// Temporary macro-fiscal tool triggered by Government when banking sector is highly profitable.
/// Tax revenue is split routed: 50% to Treasury, 25% to BFG, 25% to SOBK.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct BankTax {
    /// Number of turns remaining for the tax to be active.
    /// When 0, the tax is inactive.
    #[serde(default)]
    pub active_turns_remaining: u32,

    /// Tax rate applied to total bank assets (e.g., 0.01 for 1%).
    #[serde(default)]
    pub tax_rate: f64,

    /// Revenue split percentage to Treasury (e.g., 0.50 for 50%).
    #[serde(default = "default_treasury_split")]
    pub treasury_split_percent: f64,

    /// Revenue split percentage to BFG (e.g., 0.25 for 25%).
    #[serde(default = "default_bfg_split")]
    pub bfg_split_percent: f64,

    /// Revenue split percentage to SOBK (e.g., 0.25 for 25%).
    #[serde(default = "default_sobk_split")]
    pub sobk_split_percent: f64,

    /// Total tax collected (historical).
    #[serde(default)]
    pub total_collected: f64,

    /// Treasury revenue received (historical).
    #[serde(default)]
    pub treasury_revenue: f64,

    /// BFG revenue received (historical).
    #[serde(default)]
    pub bfg_revenue: f64,

    /// SOBK revenue received (historical).
    #[serde(default)]
    pub sobk_revenue: f64,

    /// Any additional bank tax fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_treasury_split() -> f64 {
    0.50 // 50% to Treasury
}

fn default_bfg_split() -> f64 {
    0.25 // 25% to BFG
}

fn default_sobk_split() -> f64 {
    0.25 // 25% to SOBK
}

impl BankTax {
    /// Activates the bank tax for a specified duration.
    ///
    /// # Arguments
    /// * `duration_turns` - Number of turns the tax will be active
    /// * `tax_rate` - Tax rate applied to total bank assets
    pub fn activate(&mut self, duration_turns: u32, tax_rate: f64) {
        self.active_turns_remaining = duration_turns;
        self.tax_rate = tax_rate;
    }

    /// Collects bank tax from all Commercial and Universal banks.
    ///
    /// # Arguments
    /// * `banks` - Mutable reference to all banks in the economy
    /// * `treasury` - Reference to Country treasury (receives 50%)
    /// * `bfg_fund` - Reference to BFG fund (receives 25%)
    /// * `sobk_scheme` - Reference to SOBK scheme (receives 25%)
    /// * `central_bank` - Reference to Central Bank for emergency liquidity
    /// * `bank_resolution` - Reference to BankResolution for default handling
    /// * `current_turn` - Current turn number
    ///
    /// # Double-Entry Flow
    /// * Bank: reserves_at_central_bank decreases (tax paid)
    /// * Bank: tier_1_capital decreases (equity reduced by tax)
    /// * Treasury: liquid_reserves increases (50% of tax)
    /// * BFG: reserves increases (25% of tax)
    /// * SOBK: pool increases (25% of tax)
    /// * Money mass preserved: Tax is redistribution, not destruction
    ///
    /// # Illiquidity Handling
    /// * If tax_amount > reserves_at_central_bank, bank attempts emergency liquidity
    /// * If cannot cover tax, bank defaults and triggers execute_bank_resolution
    ///
    /// # Borrow Checker Compliance
    /// * Collects bank IDs for resolution instead of passing mutable references
    /// * Calls resolution after the main loop to avoid double mutable borrow
    pub fn collect_bank_tax(
        &mut self,
        banks: &mut Vec<&mut crate::entities::Company>,
        treasury: &mut crate::state::Treasury,
        bfg_fund: &mut BfgFund,
        sobk_scheme: &mut SobkScheme,
        central_bank: &mut CentralBank,
        bank_resolution: &mut BankResolution,
        current_turn: u32,
    ) {
        if self.active_turns_remaining == 0 {
            return; // Tax not active
        }

        let mut total_tax_collected = 0.0;
        let mut banks_to_resolve: Vec<String> = Vec::new();

        for bank in banks.iter_mut() {
            if let Some(bs) = &mut bank.balance_sheet {
                // Only Commercial and Universal banks pay bank tax
                if let Some(ref bt) = bank.bank_type {
                    if bt == &BankType::Commercial || bt == &BankType::Universal {
                        let total_assets = bs.total_assets();
                        let tax_amount = total_assets * self.tax_rate;

                        // Check illiquidity before paying tax
                        let available_reserves = bs.reserves_at_central_bank;

                        if tax_amount > available_reserves {
                            // Bank cannot cover tax - attempt emergency liquidity
                            let shortfall = tax_amount - available_reserves;

                            // Try CB Lombard facility (last resort)
                            let lombard_available = central_bank.interest_rates.lombard_rate > 0.0;

                            if lombard_available {
                                // Take Lombard loan to cover tax
                                bs.cb_lombard_loans += shortfall;
                                bs.reserves_at_central_bank += shortfall;
                                central_bank.liquidity_injected += shortfall;

                                // Now pay tax
                                bs.reserves_at_central_bank -= tax_amount;
                                if bs.reserves_at_central_bank < 0.0 {
                                    bs.reserves_at_central_bank = 0.0;
                                } // Phase 43: clamp
                                bs.tier_1_capital -= tax_amount;
                                total_tax_collected += tax_amount;
                            } else {
                                // Cannot source liquidity - bank defaults
                                banks_to_resolve.push(bank.id.clone());
                                continue; // Skip tax collection for this bank
                            }
                        } else {
                            // Bank has sufficient reserves - pay tax normally
                            bs.reserves_at_central_bank -= tax_amount; // Asset decreases
                            if bs.reserves_at_central_bank < 0.0 {
                                bs.reserves_at_central_bank = 0.0;
                            } // Phase 43: clamp
                            bs.tier_1_capital -= tax_amount; // Equity decreases
                            total_tax_collected += tax_amount;
                        }
                    }
                }
            }
        }

        // Handle banks that defaulted due to tax illiquidity
        // This happens AFTER the main loop to avoid borrow checker violation
        for bank_id in banks_to_resolve {
            let average_wage = 1000.0; // In full implementation: get from macro_indicators
            bank_resolution.execute_bank_resolution(
                bank_id.as_str(),
                current_turn,
                bfg_fund,
                average_wage,
                banks,
                central_bank,
            );
        }

        // Revenue split routing
        let treasury_share = total_tax_collected * self.treasury_split_percent;
        let bfg_share = total_tax_collected * self.bfg_split_percent;
        let sobk_share = total_tax_collected * self.sobk_split_percent;

        // Distribute revenue
        treasury.liquid_reserves += treasury_share;
        bfg_fund.reserves += bfg_share;
        sobk_scheme.pool += sobk_share;

        // Update statistics
        self.total_collected += total_tax_collected;
        self.treasury_revenue += treasury_share;
        self.bfg_revenue += bfg_share;
        self.sobk_revenue += sobk_share;

        // Decrement active turns
        self.active_turns_remaining = self.active_turns_remaining.saturating_sub(1);
    }
}

// ============================================================================
// PHASE 5: BANKING TURN ORCHESTRATOR
// ============================================================================

/// Result of a single banking turn, for diagnostics and logging.
#[derive(Debug, Clone, Default)]
pub struct BankingTurnResult {
    /// Total new credit issued this turn.
    pub total_new_credit: f64,
    /// Total loan principal repaid this turn.
    pub total_loan_repayments: f64,
    /// Number of bank failures (resolution triggered).
    pub bank_failures: u32,
    /// XIBOR rate after interbank clearing.
    pub xibor: f64,
    /// Total deposits in the banking system after the turn.
    pub total_deposits: f64,
    /// Total reserves at central bank after the turn.
    pub total_reserves: f64,
    /// Total reserves parked at CB deposit facility by banks this turn.
    pub total_deposit_facility_balance: f64,
    /// Interest paid by CB to banks on deposit facility balances this turn.
    pub deposit_facility_interest_paid: f64,
    /// Total reserves borrowed by banks from CB Lombard facility this turn.
    pub total_lombard_loans: f64,
    /// Interest received by CB from banks on Lombard facility loans this turn.
    pub lombard_facility_interest_received: f64,
    /// Net OMO operation amount (positive = CB bought bonds/injected reserves, negative = sold bonds/absorbed).
    pub omo_net_amount: f64,
    /// CB's target rate for XIBOR after OMO.
    pub omo_target_rate: f64,
}

/// Processes one banking turn for a country.
///
/// This is the Phase 2 orchestrator that replaces the legacy
/// `economy::process_banking_system`. It operates exclusively on `Company`
/// entities with `bank_type` and `balance_sheet` fields, using strict
/// double-entry accounting.
///
/// # Arguments
/// * `country` - Mutable reference to the country (for CB, interbank, BFG, SOBK, bank tax, bank resolution).
/// * `companies` - Mutable slice of all companies (banks and non-banks).
/// * `current_turn` - Current turn number.
///
/// # Returns
/// `BankingTurnResult` with diagnostics.
///
/// # Rules
/// * **Step 1 — CB Rate Update:** `central_bank.update_reference_rate()` based on inflation and GDP growth.
/// * **Step 2 — Pre-clearing OMO:** CB buys/sells government bonds from/to banks to adjust aggregate reserves, steering XIBOR toward target rate.
/// * **Step 3 — Interbank Clearing:** `interbank_market.clear_market()` sets XIBOR and settles surplus/deficit positions.
/// * **Step 4 — Deposit Facility:** Banks with surplus reserves park them at CB, earning deposit rate interest. Creates physical floor for interbank rate.
/// * **Step 5 — Lombard Facility:** Banks still in deficit after interbank borrow from CB at penalty rate. Creates physical ceiling for interbank rate.
/// * **Step 6 — Loan Repayment:** Existing loans accrue interest; borrowers repay from `available_cash`. Double-entry: bank `loans_issued` decreases, `reserves_at_central_bank` increases; borrower `available_cash` decreases.
/// * **Step 7 — New Loan Issuance:** Non-bank companies seek credit. `issue_loan()` creates deposits (money creation). Rate = XIBOR + bank_margin + risk_premium. Reserve requirement constrains issuance.
/// * **Step 8 — Deposit Insurance:** `bfg_fund.collect_premiums()` moves reserves from banks to BFG.
/// * **Step 9 — Bank Tax:** If active, `bank_tax.collect_bank_tax()` levies tax on bank assets.
/// * **Step 10 — Bank Resolution:** Banks failing reserve requirements after interbank + CB Lombard are resolved via `bank_resolution.execute_bank_resolution()`.
/// * **Step 11 — SOBK Contributions:** Voluntary scheme members contribute via `sobk_scheme.accept_contribution()`.
pub fn process_banking_turn(
    country: &mut crate::state::Country,
    companies: &mut [crate::entities::Company],
    current_turn: u32,
) -> BankingTurnResult {
    let mut result = BankingTurnResult::default();

    // Step 1: CB Rate Update (Phase 36: Taylor Rule with real GDP growth)
    let inflation = country.macro_indicators.inflation / 100.0; // Convert percent to decimal
                                                                // Phase 36: Compute real GDP growth from telemetry history instead of
                                                                // hardcoding 0.02. This was the root cause of the frozen 0% CB rate.
    let gdp_growth = {
        let hist = &country.macro_indicators.telemetry_history;
        if hist.samples.len() >= 2 {
            let prev_gdp = hist.samples[hist.samples.len() - 2].official_gdp;
            let cur_gdp = country.budget.gdp;
            if prev_gdp > 0.0 {
                (cur_gdp - prev_gdp) / prev_gdp
            } else {
                country.central_bank.potential_growth
            }
        } else {
            country.central_bank.potential_growth
        }
    };
    // Phase 36: target_inflation is now read from self.target_inflation inside
    // update_reference_rate. The signature no longer takes target_inflation.
    country
        .central_bank
        .update_reference_rate(inflation, gdp_growth, current_turn);

    // Step 2: Pre-clearing OMO — CB adjusts aggregate reserves to steer XIBOR toward target.
    // Calculate total bank reserves and total securities (government bonds) held by banks.
    let (total_bank_reserves, total_bank_securities) = companies
        .iter()
        .filter(|c| c.bank_type.is_some() && c.balance_sheet.is_some())
        .fold((0.0_f64, 0.0_f64), |(res, sec), c| {
            if let Some(ref bs) = c.balance_sheet {
                (res + bs.reserves_at_central_bank, sec + bs.securities)
            } else {
                (res, sec)
            }
        });
    // Use previous XIBOR as starting point for OMO decision
    let pre_omo_xibor = country.interbank_market.xibor;
    let omo_net = country.central_bank.execute_omo(
        pre_omo_xibor,
        total_bank_reserves,
        total_bank_securities,
        current_turn,
    );
    result.omo_net_amount = omo_net;
    result.omo_target_rate = country.central_bank.omo_target_rate;

    // Physically execute OMO: adjust each bank's reserves and securities proportionally.
    if omo_net.abs() > 0.0 {
        let proportion = if total_bank_securities > 0.0 {
            omo_net / total_bank_securities
        } else {
            0.0
        };
        for bank in companies.iter_mut() {
            if let (Some(_), Some(ref mut bs)) = (&bank.bank_type, &mut bank.balance_sheet) {
                let bank_share = bs.securities * proportion;
                if omo_net > 0.0 {
                    // CB buys bonds from banks: bank gives up securities, receives reserves
                    let amount = bank_share.min(bs.securities);
                    bs.securities -= amount;
                    bs.reserves_at_central_bank += amount;
                } else {
                    // CB sells bonds to banks: bank receives securities, gives up reserves
                    let amount = (-bank_share).min(bs.reserves_at_central_bank.max(0.0));
                    bs.securities += amount;
                    bs.reserves_at_central_bank -= amount;
                }
            }
        }
    }

    // Step 3: Interbank Clearing
    // Collect mutable references to bank companies
    let cb_clone = country.central_bank.clone();
    let mut bank_refs: Vec<&mut crate::entities::Company> = companies
        .iter_mut()
        .filter(|c| c.bank_type.is_some() && c.balance_sheet.is_some())
        .collect();
    country
        .interbank_market
        .clear_market(&mut bank_refs, &cb_clone, current_turn);
    result.xibor = country.interbank_market.xibor;

    // Step 4: Deposit Facility — banks with surplus reserves park them at CB and earn deposit rate.
    // This creates the physical floor for interbank rates.
    let cb_reserve_ratio = country.central_bank.reserve_requirement_ratio;
    for bank in companies.iter_mut() {
        if let (Some(_), Some(ref mut bs)) = (&bank.bank_type, &mut bank.balance_sheet) {
            let position = bs.reserve_position(cb_reserve_ratio);
            if position > 0.0 {
                // Move surplus to deposit facility (earns interest)
                bs.cb_deposit_facility_balance += position;
                bs.reserves_at_central_bank -= position;
            }
            // Accrue interest on existing deposit facility balance
            let interest = country
                .central_bank
                .accrue_deposit_facility_interest(bs.cb_deposit_facility_balance);
            bs.reserves_at_central_bank += interest;
            result.deposit_facility_interest_paid += interest;
            result.total_deposit_facility_balance += bs.cb_deposit_facility_balance;
        }
    }

    // Step 5: Lombard Facility — banks still in deficit after interbank borrow from CB at penalty rate.
    // This creates the physical ceiling for interbank rates.
    for bank in companies.iter_mut() {
        if let (Some(_), Some(ref mut bs)) = (&bank.bank_type, &mut bank.balance_sheet) {
            let position = bs.reserve_position(cb_reserve_ratio);
            if position < 0.0 {
                // Bank is still in deficit — borrow from Lombard facility
                let needed = -position;
                bs.cb_lombard_loans += needed;
                bs.reserves_at_central_bank += needed;
            }
            // Accrue interest on existing Lombard loans (paid by bank to CB)
            let interest = country
                .central_bank
                .accrue_lombard_facility_interest(bs.cb_lombard_loans);
            bs.reserves_at_central_bank -= interest;
            if bs.reserves_at_central_bank < 0.0 {
                bs.reserves_at_central_bank = 0.0;
            } // Phase 43: clamp
            result.lombard_facility_interest_received += interest;
            result.total_lombard_loans += bs.cb_lombard_loans;
        }
    }

    // Step 6: Loan Repayment
    // Process interest accrual and repayments for existing loans
    // Phase 24A.2: Fix Black Hole #2 — borrowers must be debited when loans
    // are repaid. Previously, the bank's reserves increased but the borrower's
    // cash was never debited, creating money from nothing.
    // Phase 39: Interest income is credited to brokerage_account.cash so banks
    // can pay teller payroll. Previously, all repayments went to reserves only,
    // leaving banks with no operating cash for wages.
    let xibor = country.interbank_market.xibor;
    // Collect pending debits to avoid borrow checker conflicts (can't mutate
    // companies for borrower debit while iterating banks).
    // Tuple: (borrower_id, loan_id, lending_bank_idx, total_payment, principal_portion)
    let mut pending_loan_debits: Vec<(String, String, usize, f64, f64)> = Vec::new();
    for (bi, bank) in companies.iter_mut().enumerate() {
        if let (Some(_), Some(ref mut bs)) = (&bank.bank_type, &mut bank.balance_sheet) {
            let mut repaid_total = 0.0;
            let mut interest_income_total = 0.0;
            let mut principal_repaid_total = 0.0; // Phase 40: track principal for operating liquidity
            for loan in &mut bs.loans_issued {
                if loan.status == LoanStatus::Default {
                    continue;
                }
                // Accrue interest (Phase 74: compound per-turn rate)
                let per_turn_rate = annual_to_per_turn_rate(loan.interest_rate);
                let interest = loan.outstanding_balance * per_turn_rate;
                loan.outstanding_balance += interest;

                // Update variable rate loans
                if loan.interest_type == InterestType::Variable {
                    loan.interest_rate = xibor + loan.bank_margin;
                }

                // Attempt repayment (simplified: 1/term_turns per turn)
                if loan.term_turns > 0 {
                    let principal_portion = loan.principal / loan.term_turns as f64;
                    let payment = principal_portion + interest;
                    let actual_payment = payment.min(loan.outstanding_balance);
                    let actual_interest = interest.min(actual_payment);
                    let actual_principal = actual_payment - actual_interest;
                    loan.outstanding_balance -= actual_payment;
                    repaid_total += actual_payment;
                    interest_income_total += actual_interest;
                    principal_repaid_total += actual_principal;
                    loan.turns_remaining = loan.turns_remaining.saturating_sub(1);
                    loan.last_payment_turn = current_turn;
                    if loan.outstanding_balance <= 0.01 {
                        loan.outstanding_balance = 0.0;
                        loan.status = LoanStatus::Repaid;
                    }
                    // Phase 24A.2: Queue borrower debit for later execution
                    pending_loan_debits.push((
                        loan.borrower_id.clone(),
                        loan.id.clone(),
                        bi,
                        actual_payment,
                        principal_portion.min(actual_payment),
                    ));
                }
            }
            // Repaid amounts return to bank reserves
            bs.reserves_at_central_bank += repaid_total;
            // Phase 39: Credit interest income to brokerage_account.cash so
            // the bank can pay teller payroll. This is double-entry: the
            // interest was already added to reserves (asset side), now we
            // make it available as operating cash. The principal stays in
            // reserves; only the interest portion flows to brokerage cash.
            // Phase 40: Also credit 10% of principal repayment to brokerage
            // cash for operating liquidity. This ensures banks have organic
            // cash flow to fund teller payroll and repay wage arrears.
            let operating_cash_credit = interest_income_total + principal_repaid_total * 0.10;
            if operating_cash_credit > 0.0 {
                if let Some(ref mut ba) = bank.brokerage_account {
                    ba.cash += operating_cash_credit;
                } else {
                    bank.available_cash += operating_cash_credit;
                }
            }
            result.total_loan_repayments += repaid_total;
        }
    }

    // Phase 24A.2: Execute borrower debits (deferred to avoid double-borrow).
    // Three cases: (A) company borrower, (B) state/treasury borrower, (C) vanished borrower.
    const STATE_BORROWER_ID: &str = "STATE";
    for (borrower_id, loan_id, bank_idx, amount, _principal_portion) in pending_loan_debits {
        if borrower_id == STATE_BORROWER_ID {
            // CASE B: State/Treasury borrower — debit liquid_reserves, never Default.
            // The lending bank's reserves were already credited in the loan loop.
            country.budget.liquid_reserves = (country.budget.liquid_reserves - amount).max(0.0);
        } else if let Some(borrower_idx) = companies.iter().position(|c| c.id == borrower_id) {
            // CASE A: Company borrower — debit cash and sync borrower's bank.
            let payer_bank_id = companies[borrower_idx].primary_bank_id.clone();
            let lending_bank_id = companies[bank_idx].id.clone();

            // Debit borrower's cash (brokerage first, then available_cash)
            if let Some(ref mut ba) = companies[borrower_idx].brokerage_account {
                let debit = amount.min(ba.cash);
                ba.cash -= debit;
                if debit < amount {
                    companies[borrower_idx].available_cash =
                        (companies[borrower_idx].available_cash - (amount - debit)).max(0.0);
                }
            } else {
                companies[borrower_idx].available_cash =
                    (companies[borrower_idx].available_cash - amount).max(0.0);
            }

            // Sync borrower's bank balance sheet (double-entry)
            if let Some(ref p_bank_id) = payer_bank_id {
                if p_bank_id == &lending_bank_id {
                    // Intra-bank: deposit is destroyed, reserves already adjusted
                    // in the loan loop (reserves += repaid_total). Adjust deposits only.
                    if let Some(ref mut bs) = companies[bank_idx].balance_sheet {
                        bs.deposits = (bs.deposits - amount).max(0.0);
                    }
                } else {
                    // Inter-bank: borrower's bank loses deposits + reserves,
                    // lending bank's reserves already increased in the loan loop.
                    if let Some(bank) = companies.iter_mut().find(|c| c.id == p_bank_id.as_str()) {
                        if let Some(ref mut bs) = bank.balance_sheet {
                            bs.deposits = (bs.deposits - amount).max(0.0);
                            bs.reserves_at_central_bank =
                                (bs.reserves_at_central_bank - amount).max(0.0);
                        }
                    }
                }
            }

            // Reduce the matching LoanRef and recompute total liabilities.
            if let Some(borrower) = companies.get_mut(borrower_idx) {
                if let Some(loan_ref) = borrower
                    .outstanding_loans
                    .iter_mut()
                    .find(|l| l.loan_id == loan_id)
                {
                    loan_ref.outstanding_balance = (loan_ref.outstanding_balance - amount).max(0.0);
                    if loan_ref.outstanding_balance <= 0.01 {
                        loan_ref.status = LoanStatus::Repaid;
                    }
                }
                borrower.liabilities = borrower
                    .outstanding_loans
                    .iter()
                    .map(|l| l.outstanding_balance)
                    .sum();
            }
        } else {
            // CASE C: Borrower vanished — mark loan as Default (cleaned in bankruptcy)
            if let Some(bank) = companies.get_mut(bank_idx) {
                if let Some(ref mut bs) = bank.balance_sheet {
                    for loan in &mut bs.loans_issued {
                        if loan.borrower_id == borrower_id && loan.status != LoanStatus::Repaid {
                            loan.status = LoanStatus::Default;
                        }
                    }
                }
            }
        }
    }

    // Step 7: New Loan Issuance
    // Non-bank companies seek working capital loans.
    // Phase 77: Competitive allocation — banks with the most excess reserves
    // get priority. Also enforce operational capacity (labor-based) caps.
    let cb_for_loans = country.central_bank.clone();
    let avg_wage = country.macro_indicators.average_wage.max(1.0);

    // Collect bank info: (bank_idx, margin, excess_reserves, new_loans_this_turn)
    // Sort by excess reserves descending so the most-capable bank gets first pick.
    let mut bank_info: Vec<(usize, f64, f64, f64)> = Vec::new();
    for (bi, c) in companies.iter().enumerate() {
        if c.bank_type.is_none() || c.balance_sheet.is_none() {
            continue;
        }
        let bs = c.balance_sheet.as_ref().unwrap();
        let required = bs.deposits * cb_for_loans.reserve_requirement_ratio;
        let effective_reserves = bs.reserves_at_central_bank - bs.cb_lombard_loans;
        let excess = (effective_reserves - required).max(0.0);
        let margin = c.loan_margin.unwrap_or(0.02);
        bank_info.push((bi, margin, excess, 0.0_f64));
    }
    // Sort by excess reserves descending (most-capable bank first)
    bank_info.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    for borrower_idx in 0..companies.len() {
        let is_bank = companies[borrower_idx].bank_type.is_some();
        if is_bank {
            continue;
        }

        let needed = (companies[borrower_idx].worker_capacity as f64 * 1000.0)
            - companies[borrower_idx].available_cash;
        if needed <= 0.0 {
            continue;
        }

        let principal = needed.min(500_000.0); // Cap per-loan
        let borrower_clone = companies[borrower_idx].clone();

        // Try each bank in order of excess reserves until one can issue the loan
        for entry in &mut bank_info {
            let (bi, margin, excess, new_loans_turn) = (entry.0, entry.1, entry.2, entry.3);
            // Check excess reserves
            if excess < principal {
                continue;
            }
            // Phase 77: Check operational capacity (labor-based)
            let bank_fte = companies[bi].fulfilled_fte as f64;
            let capacity = bank_operational_capacity(bank_fte, avg_wage);
            if capacity.max_new_loans_per_turn <= 0.0 {
                continue;
            }
            if new_loans_turn + principal > capacity.max_new_loans_per_turn {
                continue;
            }
            // Check total asset under management cap
            let current_assets = companies[bi]
                .balance_sheet
                .as_ref()
                .map(|bs| {
                    bs.loans_issued
                        .iter()
                        .map(|l| l.outstanding_balance)
                        .sum::<f64>()
                        + bs.securities
                })
                .unwrap_or(0.0);
            if current_assets + principal > capacity.max_asset_under_management {
                continue;
            }

            let loan_result = issue_loan(
                companies[bi].balance_sheet.as_mut().unwrap(),
                &companies[bi].id,
                margin,
                &borrower_clone,
                &borrower_clone.id,
                principal,
                LoanType::WorkingCapital,
                12,
                &cb_for_loans,
                xibor,
            );

            if let Ok(lr) = loan_result {
                // Double-entry: borrower receives principal
                companies[borrower_idx].outstanding_loans.push(LoanRef {
                    loan_id: lr.loan.id.clone(),
                    bank_id: companies[bi].id.clone(),
                    principal: lr.loan.principal,
                    outstanding_balance: lr.loan.outstanding_balance,
                    interest_rate: lr.loan.interest_rate,
                    term_turns: lr.loan.term_turns,
                    status: lr.loan.status.clone(),
                });
                companies[borrower_idx].liabilities = companies[borrower_idx]
                    .outstanding_loans
                    .iter()
                    .map(|l| l.outstanding_balance)
                    .sum();
                companies[borrower_idx].available_cash += lr.principal_amount;
                if let Some(ref mut ba) = companies[borrower_idx].brokerage_account {
                    ba.cash += lr.principal_amount;
                }
                result.total_new_credit += lr.principal_amount;
                // Update this bank's tracking: reduce excess, increase new_loans_turn
                entry.2 -= lr.principal_amount;
                entry.3 += lr.principal_amount;
                break; // Loan issued, move to next borrower
            }
        }
    }

    // Step 8: Deposit Insurance Premiums
    let mut bank_refs_2: Vec<&mut crate::entities::Company> = companies
        .iter_mut()
        .filter(|c| c.bank_type.is_some() && c.balance_sheet.is_some())
        .collect();
    country
        .bfg_fund
        .collect_premiums(&mut bank_refs_2, current_turn);

    // BFG repays CB emergency loan from premium collections (Black Hole 1.10 fix)
    // Each turn, BFG uses available reserves to repay outstanding CB emergency debt.
    if country.bfg_fund.cb_emergency_loan > 0.0 && country.bfg_fund.reserves > 0.0 {
        let repayment = country
            .bfg_fund
            .reserves
            .min(country.bfg_fund.cb_emergency_loan);
        country
            .bfg_fund
            .repay_cb_liquidity_line(&mut country.central_bank, repayment);
    }

    // Step 9: Bank Tax (if active)
    if country.bank_tax.active_turns_remaining > 0 {
        let mut bank_refs_3: Vec<&mut crate::entities::Company> = companies
            .iter_mut()
            .filter(|c| c.bank_type.is_some() && c.balance_sheet.is_some())
            .collect();
        country.bank_tax.collect_bank_tax(
            &mut bank_refs_3,
            &mut country.budget,
            &mut country.bfg_fund,
            &mut country.sobk_scheme,
            &mut country.central_bank,
            &mut country.bank_resolution,
            current_turn,
        );
    }

    // Step 10: Bank Resolution
    // Check for banks that still fail reserve requirements after interbank + Lombard
    let mut failed_banks: Vec<String> = Vec::new();
    for bank in companies.iter() {
        if let (Some(_), Some(ref bs)) = (&bank.bank_type, &bank.balance_sheet) {
            if !bs.meets_reserve_requirement(cb_reserve_ratio)
                && bs.cb_lombard_loans > 0.0
                && bs.reserves_at_central_bank < 0.0
            {
                failed_banks.push(bank.id.clone());
            }
        }
    }
    let average_wage = country.macro_indicators.average_wage;
    for failed_id in &failed_banks {
        let mut failed_bank_refs: Vec<&mut crate::entities::Company> = companies
            .iter_mut()
            .filter(|c| c.bank_type.is_some() && c.balance_sheet.is_some())
            .collect();
        country.bank_resolution.execute_bank_resolution(
            failed_id,
            current_turn,
            &mut country.bfg_fund,
            average_wage,
            &mut failed_bank_refs,
            &mut country.central_bank,
        );
        result.bank_failures += 1;
    }

    // Update stress indicator
    let total_banks = companies.iter().filter(|c| c.bank_type.is_some()).count();
    country.interbank_market.update_stress_indicator(
        result.bank_failures,
        total_banks,
        0.0, // XIBOR volatility placeholder
    );

    // Step 11: SOBK Contributions (voluntary)
    for bank in companies.iter_mut() {
        if bank.bank_type.is_some() && bank.balance_sheet.is_some() {
            country.sobk_scheme.accept_contribution(bank);
        }
    }

    // Step 12 (Phase 35 / Phase 77): B2B Micro-Loans — banks issue small
    // working-capital loans to non-bank companies that have insufficient
    // brokerage cash for operations. This creates actual banking activity and
    // loan interest revenue.
    // Phase 40: Reserve payroll cash BEFORE lending so banks can pay tellers.
    // Phase 77: Route through issue_loan() to enforce fractional reserve
    // requirements. Previously this pushed loans directly to bs.loans_issued
    // WITHOUT checking reserves — a rogue money-creation path.
    let cb_ref_rate = country.central_bank.interest_rates.reference_rate;
    let avg_wage_for_reserve = country.macro_indicators.average_wage.max(1.0);
    let n = companies.len();
    for bank_idx in 0..n {
        if companies[bank_idx].bank_type.is_none() || companies[bank_idx].balance_sheet.is_none() {
            continue;
        }
        // Phase 40: Compute payroll reserve before lending.
        // The bank must keep enough cash to pay its current tellers for one turn.
        let bank_wage = (avg_wage_for_reserve * 1.2).max(1.0);
        let current_fte = (companies[bank_idx].prev_fulfilled_fte as f64).max(2.0);
        let payroll_reserve = current_fte * bank_wage;
        let bank_cash = companies[bank_idx]
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash)
            .unwrap_or(companies[bank_idx].available_cash);
        // Available for lending = equity-based credit, minus payroll reserve.
        let equity_credit = {
            let bs = companies[bank_idx].balance_sheet.as_ref().unwrap();
            (bs.total_assets() - bs.total_liabilities()).max(0.0) * 0.3
        };
        let max_credit = (equity_credit - payroll_reserve).max(0.0);
        if max_credit < 1000.0 {
            continue;
        }
        // Also cap by available cash minus payroll reserve
        let cash_available_for_lending = (bank_cash - payroll_reserve).max(0.0);
        let lending_cap = max_credit.min(cash_available_for_lending);
        if lending_cap < 100.0 {
            continue;
        }
        // Phase 77: Also cap by operational capacity (labor-based)
        let bank_fte = companies[bank_idx].fulfilled_fte as f64;
        let op_capacity = bank_operational_capacity(bank_fte, avg_wage_for_reserve);
        if op_capacity.max_new_loans_per_turn <= 0.0 {
            continue;
        }
        let lending_cap = lending_cap.min(op_capacity.max_new_loans_per_turn);

        let bank_region = companies[bank_idx].region_id.clone();
        let bank_id = companies[bank_idx].id.clone();
        let bank_margin = companies[bank_idx].loan_margin.unwrap_or(0.02);
        let mut lent_total = 0.0;
        for borrower_idx in 0..n {
            if borrower_idx == bank_idx {
                continue;
            }
            if companies[borrower_idx].bank_type.is_some() {
                continue;
            }
            if !bank_region.is_empty() && companies[borrower_idx].region_id != bank_region {
                continue;
            }
            let borrower_cash = companies[borrower_idx]
                .brokerage_account
                .as_ref()
                .map(|ba| ba.cash)
                .unwrap_or(0.0);
            if borrower_cash > 50000.0 {
                continue;
            }
            let loan_amount = lending_cap.min(50000.0 - borrower_cash).max(0.0);
            if loan_amount < 100.0 {
                continue;
            }
            // Phase 77: Route through issue_loan() for reserve check
            let borrower_clone = companies[borrower_idx].clone();
            let loan_result = issue_loan(
                companies[bank_idx].balance_sheet.as_mut().unwrap(),
                &bank_id,
                bank_margin,
                &borrower_clone,
                &borrower_clone.id,
                loan_amount,
                LoanType::WorkingCapital,
                24,
                &country.central_bank,
                xibor,
            );
            if let Ok(lr) = loan_result {
                // Credit the borrower
                if let Some(ba) = &mut companies[borrower_idx].brokerage_account {
                    ba.cash += lr.principal_amount;
                } else {
                    companies[borrower_idx].available_cash += lr.principal_amount;
                }
                lent_total += lr.principal_amount;
                if lent_total >= lending_cap {
                    break;
                }
            }
        }
        let _ = cb_ref_rate;
        let _ = bank_id;
        result.total_new_credit += lent_total;
    }

    // Step 13 (Phase 35): B2C Consumer Loans — banks issue small consumer
    // loans to class demographics. On issuance: savings += principal,
    // debt += principal. Every turn, repayment deducts from savings, reduces
    // debt, and credits interest to the bank.
    for bank in companies.iter_mut() {
        if bank.bank_type.is_none() {
            continue;
        }
        // Process existing consumer loan repayments first
        let mut repayment_interest = 0.0;
        let mut repayment_principal = 0.0;
        let bank_region = bank.region_id.clone();
        let mut loans_to_process: Vec<(usize, f64, f64)> = Vec::new();
        for (i, loan) in bank.consumer_loans.iter().enumerate() {
            // Per-turn repayment: 1/24 of principal + interest (Phase 74: compound rate)
            let principal_payment = loan.outstanding_principal / 24.0;
            let per_turn_rate = annual_to_per_turn_rate(loan.interest_rate);
            let interest_payment = loan.outstanding_principal * per_turn_rate;
            loans_to_process.push((i, principal_payment, interest_payment));
        }
        // Find the region and class for each loan and process repayment
        for (i, principal_pay, interest_pay) in loans_to_process {
            let loan = &mut bank.consumer_loans[i];
            let region = country.regions.iter_mut().find(|r| r.id == loan.region_id);
            if let Some(region) = region {
                let class = if loan.is_rural {
                    region
                        .class_demographics
                        .rural_classes
                        .get_mut(&loan.class_key)
                } else {
                    region
                        .class_demographics
                        .urban_classes
                        .get_mut(&loan.class_key)
                };
                if let Some(class) = class {
                    let total_payment = principal_pay + interest_pay;
                    if class.savings >= total_payment {
                        class.savings -= total_payment;
                        class.debt -= principal_pay;
                        loan.outstanding_principal -= principal_pay;
                        repayment_principal += principal_pay;
                        repayment_interest += interest_pay;
                    } else if class.savings > 0.0 {
                        // Partial payment
                        let partial = class.savings;
                        let principal_portion = partial * (principal_pay / total_payment);
                        let interest_portion = partial - principal_portion;
                        class.savings = 0.0;
                        class.debt -= principal_portion;
                        loan.outstanding_principal -= principal_portion;
                        repayment_principal += principal_portion;
                        repayment_interest += interest_portion;
                    }
                }
            }
        }
        // Credit interest to bank's brokerage cash (B2C revenue)
        if repayment_interest > 0.0 {
            if let Some(ba) = &mut bank.brokerage_account {
                ba.cash += repayment_interest;
            }
            if let Some(bs) = &mut bank.balance_sheet {
                bs.reserves_at_central_bank += repayment_interest;
            }
        }
        result.total_loan_repayments += repayment_principal + repayment_interest;

        // Remove fully repaid loans
        bank.consumer_loans
            .retain(|l| l.outstanding_principal > 1.0);

        // Issue new consumer loans to classes with low savings
        // Phase 40: Reserve payroll cash before consumer lending.
        let bs_cap = bank
            .balance_sheet
            .as_ref()
            .map(|bs| bs.total_assets() - bs.total_liabilities())
            .unwrap_or(0.0)
            .max(0.0);
        let bank_wage_cons = (avg_wage_for_reserve * 1.2).max(1.0);
        let current_fte_cons = (bank.prev_fulfilled_fte as f64).max(2.0);
        let payroll_reserve_cons = current_fte_cons * bank_wage_cons;
        let bank_cash_cons = bank
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash)
            .unwrap_or(bank.available_cash);
        let cash_avail_for_cons = (bank_cash_cons - payroll_reserve_cons).max(0.0);
        let max_consumer_credit = (bs_cap * 0.2).min(cash_avail_for_cons);
        if max_consumer_credit < 500.0 {
            continue;
        }
        let mut issued_total = 0.0;
        for region in &mut country.regions {
            if !bank_region.is_empty() && region.id != bank_region {
                continue;
            }
            // Issue to rural classes
            let rural_keys: Vec<String> = region
                .class_demographics
                .rural_classes
                .keys()
                .cloned()
                .collect();
            for key in &rural_keys {
                if issued_total >= max_consumer_credit {
                    break;
                }
                let class = region
                    .class_demographics
                    .rural_classes
                    .get_mut(key)
                    .unwrap();
                if class.population <= 0 || class.debt > 0.0 {
                    continue; // Already has debt
                }
                let per_capita_savings = if class.population > 0 {
                    class.savings / class.population as f64
                } else {
                    0.0
                };
                let avg_wage = country.macro_indicators.average_wage;
                if per_capita_savings > avg_wage * 0.5 {
                    continue; // Not poor enough to need a loan
                }
                let loan_amount = (avg_wage * class.population as f64 * 0.1)
                    .min(max_consumer_credit - issued_total)
                    .min(100000.0);
                if loan_amount < 100.0 {
                    continue;
                }
                let rate = cb_ref_rate + 0.05; // 5% margin for consumer loans
                class.savings += loan_amount;
                class.debt += loan_amount;
                bank.consumer_loans.push(ConsumerLoan {
                    region_id: region.id.clone(),
                    class_key: key.clone(),
                    is_rural: true,
                    outstanding_principal: loan_amount,
                    interest_rate: rate,
                    issued_turn: current_turn,
                    original_principal: loan_amount,
                });
                issued_total += loan_amount;
            }
            if issued_total >= max_consumer_credit {
                break;
            }
            // Issue to urban classes
            let urban_keys: Vec<String> = region
                .class_demographics
                .urban_classes
                .keys()
                .cloned()
                .collect();
            for key in &urban_keys {
                if issued_total >= max_consumer_credit {
                    break;
                }
                let class = region
                    .class_demographics
                    .urban_classes
                    .get_mut(key)
                    .unwrap();
                if class.population <= 0 || class.debt > 0.0 {
                    continue;
                }
                let per_capita_savings = if class.population > 0 {
                    class.savings / class.population as f64
                } else {
                    0.0
                };
                let avg_wage = country.macro_indicators.average_wage;
                if per_capita_savings > avg_wage * 0.5 {
                    continue;
                }
                let loan_amount = (avg_wage * class.population as f64 * 0.1)
                    .min(max_consumer_credit - issued_total)
                    .min(100000.0);
                if loan_amount < 100.0 {
                    continue;
                }
                let rate = cb_ref_rate + 0.05;
                class.savings += loan_amount;
                class.debt += loan_amount;
                bank.consumer_loans.push(ConsumerLoan {
                    region_id: region.id.clone(),
                    class_key: key.clone(),
                    is_rural: false,
                    outstanding_principal: loan_amount,
                    interest_rate: rate,
                    issued_turn: current_turn,
                    original_principal: loan_amount,
                });
                issued_total += loan_amount;
            }
        }
        result.total_new_credit += issued_total;
    }

    // Step 14 (Phase 35): QE for Deflation — if CPI inflation < 0%, the
    // Central Bank purchases sovereign bonds from DSPW banks on the secondary
    // market, creating fresh M0/reserves. Capped at 5% of GDP per turn.
    let cpi_inflation = country.macro_indicators.inflation; // Already in percent
    if cpi_inflation < 0.0 {
        let gdp = country.budget.gdp.max(1.0);
        let qe_cap = gdp * 0.05; // 5% of GDP per turn
                                 // Find DSPW banks with securities to sell
        let mut total_qe = 0.0;
        for bank in companies.iter_mut() {
            if bank.bank_type.is_none() || !bank.is_dspw {
                continue;
            }
            if let Some(bs) = &mut bank.balance_sheet {
                let available_securities = bs.securities.min(qe_cap - total_qe);
                if available_securities <= 0.0 {
                    continue;
                }
                // CB buys bonds: bank gives up securities, receives fresh reserves
                bs.securities -= available_securities;
                bs.reserves_at_central_bank += available_securities;
                total_qe += available_securities;
                if total_qe >= qe_cap {
                    break;
                }
            }
        }
        // Record QE on central bank's balance sheet
        if total_qe > 0.0 {
            country.central_bank.omo_bond_holdings += total_qe;
            country.central_bank.liquidity_injected += total_qe;
            country.central_bank.omo_last_operation_turn = current_turn;
            country.central_bank.omo_last_operation_amount = total_qe;
            result.omo_net_amount += total_qe;
        }
    }

    // Step 15 (Phase 35/36/38): Bank Labor Demand — banks set FTE demand AND wages
    // based on their loan portfolio and activity scale, rather than staying at zero.
    // Phase 36: Also set offered_wage_per_fte so the labor market can actually
    // hire bank employees. Previously, only target_fte_demand was set, but
    // without a wage offer, the labor market computed max_affordable_fte = 0.
    // Phase 38: Cap FTE growth at 10% per turn (conservative for banks),
    // reduce payroll budget fraction from 30% to 15%, and smooth the portfolio
    // using a moving average to prevent boom/bust hiring cycles.
    let avg_wage = country.macro_indicators.average_wage.max(1.0);
    const BANK_FTE_GROWTH_CAP: f64 = 0.10; // 10% max growth per turn
    const BANK_PAYROLL_FRACTION: f64 = 0.15; // 15% of cash for payroll
    const BANK_PORTFOLIO_SMOOTHING: f64 = 0.5; // 50% weight on new portfolio
    for bank in companies.iter_mut() {
        if bank.bank_type.is_none() {
            continue;
        }
        let bs = bank.balance_sheet.as_ref();
        let total_loans: f64 = bs
            .map(|b| {
                b.loans_issued
                    .iter()
                    .map(|l| l.outstanding_balance)
                    .sum::<f64>()
            })
            .unwrap_or(0.0);
        let consumer_loans_total: f64 = bank
            .consumer_loans
            .iter()
            .map(|l| l.outstanding_principal)
            .sum::<f64>();
        let current_portfolio = total_loans + consumer_loans_total;
        // Phase 38: Smooth portfolio using a moving average stored in extra.
        // This prevents FTE demand from spiking when a batch of loans is issued
        // and crashing when they're repaid.
        let prev_portfolio = bank.temporary_disruption_modifier; // reuse as scratch
        let smoothed_portfolio = if prev_portfolio > 0.0 {
            prev_portfolio * (1.0 - BANK_PORTFOLIO_SMOOTHING)
                + current_portfolio * BANK_PORTFOLIO_SMOOTHING
        } else {
            current_portfolio
        };
        bank.temporary_disruption_modifier = smoothed_portfolio; // store for next turn
                                                                 // 1 FTE per 100,000 currency units of loans (tellers, loan officers, etc.)
        let fte_demand = (smoothed_portfolio / 100_000.0).ceil();
        // Phase 38: Cap FTE growth at 10% per turn relative to prev_fulfilled_fte.
        // Banks start small and scale conservatively. Min 2 FTE for basic operations.
        let prev_fte = (bank.prev_fulfilled_fte as f64).max(2.0);
        let max_growth_fte = prev_fte * (1.0 + BANK_FTE_GROWTH_CAP);
        let growth_capped_demand = fte_demand.min(max_growth_fte);
        // Phase 41: Use target_wage for banks, same mechanism as other companies.
        // Initialize target_wage on first turn with max(50.0) fallback.
        if bank.target_wage == 0.0 {
            bank.target_wage = (avg_wage * 1.2).max(50.0);
        } else {
            // Slowly adjust toward 120% of market average, max 2% per turn.
            let bank_target = (avg_wage * 1.2).max(50.0);
            let adjustment = (bank_target - bank.target_wage)
                .clamp(-bank.target_wage * 0.02, bank.target_wage * 0.02);
            bank.target_wage = (bank.target_wage + adjustment).max(50.0);
        }
        let bank_wage = bank.target_wage;
        bank.offered_wage_per_fte = bank_wage;
        // Phase 38: Compute max affordable FTE from available cash (15% for payroll)
        let bank_cash = bank
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash)
            .unwrap_or(bank.available_cash);
        let payroll_budget = bank_cash * BANK_PAYROLL_FRACTION;
        let max_affordable = if bank_wage > 0.0 {
            payroll_budget / bank_wage
        } else {
            0.0
        };
        bank.target_fte_demand = growth_capped_demand.min(max_affordable).max(2.0).round() as u32; // Min 2 FTE
        bank.physical_fte_demand = bank.target_fte_demand;
    }

    // Aggregate diagnostics
    for bank in companies.iter() {
        if let Some(ref bs) = bank.balance_sheet {
            result.total_deposits += bs.deposits;
            result.total_reserves += bs.reserves_at_central_bank;
        }
    }

    result
}

/// Phase 38: DSPW Auction Settlement — primary dealer banks pull-purchase
/// unpurchased securities from the debt market's auction inventory.
///
/// This function runs AFTER `issue_treasury_securities` in the turn loop.
/// It has access to both `&mut [Company]` (banks) and `&mut Country`
/// (debt market + treasury), enabling strict double-entry without passing
/// companies into `debt_market.rs`.
///
/// # Flow
/// 1. Find all securities with `is_auction_inventory == true`.
/// 2. For each, find a DSPW primary dealer bank that has reserves.
/// 3. The bank purchases up to 5% of its reserves worth of the security.
/// 4. Double-entry: debit `bs.reserves_at_central_bank`, credit `bs.securities`,
///    add bank as `SecurityHolder`, credit `country.budget.liquid_reserves`.
/// 5. Clear the `is_auction_inventory` flag.
///
/// If no DSPW bank can purchase (all broke), the security remains as
/// auction inventory and the treasury doesn't get cash for it.
pub fn dspw_auction_settlement(
    country: &mut crate::state::Country,
    companies: &mut [crate::entities::Company],
    _current_turn: u32,
) {
    use crate::economy::finance::debt_market::{SecurityHolder, SecurityHolderType};

    // Collect indices of auction-inventory securities.
    let auction_indices: Vec<usize> = country
        .debt_market
        .outstanding_securities
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_auction_inventory)
        .map(|(i, _)| i)
        .collect();

    if auction_indices.is_empty() {
        return;
    }

    let primary_dealer_ids: Vec<String> = country.debt_market.primary_dealers.clone();

    for sec_idx in auction_indices {
        // Get the security face value and issue price.
        let (face_value, issue_price) = {
            let sec = &country.debt_market.outstanding_securities[sec_idx];
            (sec.face_value, sec.issue_price)
        };

        // Try to find a DSPW bank with sufficient reserves.
        let purchase_price = face_value * issue_price;
        let max_bank_reserve_fraction = 0.05; // Banks allocate up to 5% of reserves

        // Find a primary dealer bank with enough reserves.
        let buyer_idx = companies.iter().position(|c| {
            c.is_dspw
                && primary_dealer_ids.contains(&c.id)
                && c.balance_sheet
                    .as_ref()
                    .map(|bs| {
                        bs.reserves_at_central_bank * max_bank_reserve_fraction >= purchase_price
                    })
                    .unwrap_or(false)
        });

        if let Some(bank_idx) = buyer_idx {
            // Perform strict double-entry purchase.
            let bank_id = companies[bank_idx].id.clone();
            if let Some(ref mut bs) = companies[bank_idx].balance_sheet {
                // Debit bank reserves.
                bs.reserves_at_central_bank -= purchase_price;
                if bs.reserves_at_central_bank < 0.0 {
                    bs.reserves_at_central_bank = 0.0;
                } // Phase 43: clamp
                  // Credit bank securities holdings.
                bs.securities += purchase_price;
            }

            // Credit treasury with the purchase price.
            country.budget.liquid_reserves += purchase_price;

            // Add bank as security holder and clear auction flag.
            let sec = &mut country.debt_market.outstanding_securities[sec_idx];
            sec.holders.push(SecurityHolder {
                entity_id: bank_id,
                holder_type: SecurityHolderType::PrimaryDealer,
                quantity: face_value,
                purchase_price,
            });
            sec.is_auction_inventory = false;
        }
        // If no buyer found, the security remains as auction inventory.
        // The treasury doesn't get cash — the deficit isn't funded this turn.
    }

    country.debt_market.recalculate();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Company;
    use crate::registries::enums::Sector;

    #[test]
    fn fully_loaned_bank_has_zero_capacity() {
        let bank = Bank {
            total_deposits: 1_000_000.0,
            issued_loans: 900_000.0,
            liquid_reserves: 100_000.0,
            reserve_requirement_ratio: 0.10,
            ..Bank::test_default()
        };
        assert!((bank.max_new_credit() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn fractional_reserve_credit_capacity() {
        let bank = Bank {
            total_deposits: 1_000_000.0,
            issued_loans: 0.0,
            liquid_reserves: 100_000.0,
            reserve_requirement_ratio: 0.10,
            ..Bank::test_default()
        };
        assert!((bank.max_new_credit() - 900_000.0).abs() < 1e-9);
    }

    // ========================================================================
    // STAGE D PHASE 2: BANK BALANCE SHEET TESTS
    // ========================================================================

    #[test]
    fn balance_sheet_calculates_total_assets() {
        let mut bs = BankBalanceSheet::default();
        bs.reserves_at_central_bank = 100_000.0;
        bs.loans_issued.push(Loan {
            id: "LOAN-1".to_string(),
            borrower_id: "COMP-1".to_string(),
            principal: 50_000.0,
            outstanding_balance: 50_000.0,
            interest_rate: 0.05,
            term_turns: 12,
            turns_remaining: 12,
            collateral_value: None,
            loan_type: LoanType::WorkingCapital,
            last_payment_turn: 0,
            status: LoanStatus::Current,
            interest_type: InterestType::default(),
            duration_risk_premium: 0.0,
            base_xibor: 0.0,
            bank_margin: 0.0,
            securitized: false,
            pledged_to_covered_bond: None,
            extra: Map::new(),
        });
        bs.interbank_loans_given
            .insert("BANK-2".to_string(), 20_000.0);
        bs.securities = 30_000.0;
        bs.real_estate = 200_000.0;

        let assets = bs.total_assets();
        assert!((assets - 400_000.0).abs() < 1e-9);
    }

    #[test]
    fn balance_sheet_calculates_total_liabilities() {
        let mut bs = BankBalanceSheet::default();
        bs.deposits = 500_000.0;
        bs.cb_lombard_loans = 10_000.0;
        bs.interbank_loans_taken
            .insert("BANK-2".to_string(), 20_000.0);
        bs.issued_bonds = 100_000.0;

        let liabilities = bs.total_liabilities();
        assert!((liabilities - 630_000.0).abs() < 1e-9);
    }

    #[test]
    fn balance_sheet_is_balanced() {
        let mut bs = BankBalanceSheet::default();
        bs.reserves_at_central_bank = 100_000.0;
        bs.deposits = 80_000.0;
        bs.tier_1_capital = 20_000.0;

        assert!(bs.is_balanced());
    }

    #[test]
    fn balance_sheet_reserve_position() {
        let mut bs = BankBalanceSheet::default();
        bs.reserves_at_central_bank = 100_000.0;
        bs.deposits = 1_000_000.0;

        let position = bs.reserve_position(0.10);
        assert!((position - 0.0).abs() < 1e-9); // Exactly at requirement

        bs.reserves_at_central_bank = 150_000.0;
        let position = bs.reserve_position(0.10);
        assert!((position - 50_000.0).abs() < 1e-9); // Surplus

        bs.reserves_at_central_bank = 50_000.0;
        let position = bs.reserve_position(0.10);
        assert!((position - (-50_000.0)).abs() < 1e-9); // Deficit
    }

    #[test]
    fn balance_sheet_meets_reserve_requirement() {
        let mut bs = BankBalanceSheet::default();
        bs.reserves_at_central_bank = 100_000.0;
        bs.deposits = 1_000_000.0;

        assert!(bs.meets_reserve_requirement(0.10));
        assert!(!bs.meets_reserve_requirement(0.15));
    }

    // ========================================================================
    // STAGE D PHASE 2: CREDIT SCORING TESTS
    // ========================================================================

    #[test]
    fn credit_score_rejects_ltv_violation() {
        let borrower = Company::new(
            "COMP-1".to_string(),
            "Test Company".to_string(),
            Sector::LightIndustry,
            crate::entities::LegalForm::JointStockCompany(
                crate::entities::JointStockData::default(),
            ),
            100_000.0, // fixed_capital
            50_000.0,  // liquid_capital
            100,
        );

        let central_bank = CentralBank::default();
        let existing_loans = vec![];

        let score = calculate_credit_score(
            &borrower,
            LoanType::Investment,
            100_000.0, // Requesting 100k with only 100k collateral (60% LTV = 60k max)
            &central_bank,
            "BANK-1",
            &existing_loans,
        );

        assert!(!score.approved);
        assert!(score.rejection_reason.is_some());
        assert!(score.rejection_reason.unwrap().contains("LTV violation"));
    }

    #[test]
    fn credit_score_approves_healthy_borrower() {
        let borrower = Company::new(
            "COMP-1".to_string(),
            "Test Company".to_string(),
            Sector::LightIndustry,
            crate::entities::LegalForm::JointStockCompany(
                crate::entities::JointStockData::default(),
            ),
            200_000.0, // fixed_capital
            150_000.0, // liquid_capital
            100,
        );

        let central_bank = CentralBank::default();
        let existing_loans = vec![];

        let score = calculate_credit_score(
            &borrower,
            LoanType::WorkingCapital,
            100_000.0, // Requesting 100k with 150k liquid collateral (80% LTV = 120k max)
            &central_bank,
            "BANK-1",
            &existing_loans,
        );

        assert!(score.approved);
        assert!(score.score > 0.5);
    }

    #[test]
    fn credit_score_consolidation_existing_debtor_viable() {
        let mut borrower = Company::new(
            "COMP-1".to_string(),
            "Test Company".to_string(),
            Sector::LightIndustry,
            crate::entities::LegalForm::JointStockCompany(
                crate::entities::JointStockData::default(),
            ),
            200_000.0, // fixed_capital
            300_000.0, // liquid_capital (high liquidity ratio)
            100,
        );
        borrower.liabilities = 100_000.0; // Set liabilities for liquidity ratio calculation

        let central_bank = CentralBank::default();
        let existing_loans = vec![Loan {
            id: "LOAN-OLD".to_string(),
            borrower_id: "COMP-1".to_string(),
            principal: 50_000.0,
            outstanding_balance: 50_000.0,
            interest_rate: 0.05,
            term_turns: 12,
            turns_remaining: 6,
            collateral_value: None,
            loan_type: LoanType::WorkingCapital,
            last_payment_turn: 0,
            status: LoanStatus::Current,
            interest_type: InterestType::default(),
            duration_risk_premium: 0.0,
            base_xibor: 0.0,
            bank_margin: 0.0,
            securitized: false,
            pledged_to_covered_bond: None,
            extra: Map::new(),
        }];

        let score = calculate_credit_score(
            &borrower,
            LoanType::Consolidation,
            100_000.0,
            &central_bank,
            "BANK-1",
            &existing_loans,
        );

        assert!(score.approved);
        assert!(score.required_equity_swap.is_some());
        assert!((score.required_equity_swap.unwrap() - 0.15).abs() < 1e-9); // 15% swap
    }

    #[test]
    fn credit_score_consolidation_new_debtor_rejects_high_ltv() {
        let borrower = Company::new(
            "COMP-1".to_string(),
            "Test Company".to_string(),
            Sector::LightIndustry,
            crate::entities::LegalForm::JointStockCompany(
                crate::entities::JointStockData::default(),
            ),
            200_000.0, // fixed_capital
            150_000.0, // liquid_capital
            100,
        );

        let central_bank = CentralBank::default();
        let existing_loans = vec![];

        let score = calculate_credit_score(
            &borrower,
            LoanType::Consolidation,
            140_000.0, // 70% LTV (needs < 50% for new debtor)
            &central_bank,
            "BANK-1",
            &existing_loans,
        );

        assert!(!score.approved);
        assert!(score.rejection_reason.is_some());
        assert!(score.rejection_reason.unwrap().contains("LTV < 50%"));
    }

    // ========================================================================
    // STAGE D PHASE 2: LOAN ISSUANCE TESTS
    // ========================================================================

    #[test]
    fn issue_loan_creates_money_via_double_entry() {
        let mut balance_sheet = BankBalanceSheet::default();
        balance_sheet.reserves_at_central_bank = 200_000.0;
        balance_sheet.deposits = 1_000_000.0;
        balance_sheet.tier_1_capital = 100_000.0;

        let borrower = Company::new(
            "COMP-1".to_string(),
            "Test Company".to_string(),
            Sector::LightIndustry,
            crate::entities::LegalForm::JointStockCompany(
                crate::entities::JointStockData::default(),
            ),
            200_000.0,
            150_000.0,
            100,
        );

        let mut central_bank = CentralBank::default();
        central_bank.reserve_requirement_ratio = 0.10;

        let result = issue_loan(
            &mut balance_sheet,
            "BANK-1",
            0.015,
            &borrower,
            "COMP-1",
            100_000.0,
            LoanType::WorkingCapital,
            12,
            &central_bank,
            0.03,
        );

        assert!(result.is_ok());
        let loan_result = result.unwrap();

        // Check double-entry: loans_issued increased, deposits increased
        assert_eq!(balance_sheet.loans_issued.len(), 1);
        assert!((balance_sheet.deposits - 1_100_000.0).abs() < 1e-9); // +100k deposits

        // Reserves unchanged during loan creation
        assert!((balance_sheet.reserves_at_central_bank - 200_000.0).abs() < 1e-9);

        // Loan record created correctly
        assert_eq!(loan_result.loan.principal, 100_000.0);
        assert_eq!(loan_result.loan.outstanding_balance, 100_000.0);
        assert_eq!(loan_result.principal_amount, 100_000.0);
    }

    #[test]
    fn issue_loan_rejects_insufficient_reserves() {
        let mut balance_sheet = BankBalanceSheet::default();
        balance_sheet.reserves_at_central_bank = 50_000.0;
        balance_sheet.deposits = 1_000_000.0;
        balance_sheet.tier_1_capital = 100_000.0;

        let borrower = Company::new(
            "COMP-1".to_string(),
            "Test Company".to_string(),
            Sector::LightIndustry,
            crate::entities::LegalForm::JointStockCompany(
                crate::entities::JointStockData::default(),
            ),
            200_000.0,
            150_000.0,
            100,
        );

        let mut central_bank = CentralBank::default();
        central_bank.reserve_requirement_ratio = 0.10;

        // Requesting 100k loan would require 110k reserves (1.1M * 0.10)
        // Only have 50k reserves
        let result = issue_loan(
            &mut balance_sheet,
            "BANK-1",
            0.015,
            &borrower,
            "COMP-1",
            100_000.0,
            LoanType::WorkingCapital,
            12,
            &central_bank,
            0.03,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Reserve requirement violation"));
    }

    // ========================================================================
    // STAGE D PHASE 2: INTERBANK MARKET TESTS
    // ========================================================================

    #[test]
    fn interbank_market_clears_proportionally() {
        let mut bank1 = Company::new(
            "BANK-1".to_string(),
            "Bank 1".to_string(),
            Sector::Banking,
            crate::entities::LegalForm::JointStockCompany(
                crate::entities::JointStockData::default(),
            ),
            0.0,
            0.0,
            0,
        );
        bank1.bank_type = Some(BankType::Commercial);
        bank1.balance_sheet = Some(BankBalanceSheet {
            reserves_at_central_bank: 200_000.0,
            deposits: 1_000_000.0,
            tier_1_capital: 100_000.0,
            ..Default::default()
        });

        let mut bank2 = Company::new(
            "BANK-2".to_string(),
            "Bank 2".to_string(),
            Sector::Banking,
            crate::entities::LegalForm::JointStockCompany(
                crate::entities::JointStockData::default(),
            ),
            0.0,
            0.0,
            0,
        );
        bank2.bank_type = Some(BankType::Commercial);
        bank2.balance_sheet = Some(BankBalanceSheet {
            reserves_at_central_bank: 150_000.0,
            deposits: 1_500_000.0,
            tier_1_capital: 100_000.0,
            ..Default::default()
        });

        let mut market = InterbankMarket::default();
        let mut central_bank = CentralBank::default();
        central_bank.reserve_requirement_ratio = 0.10;
        central_bank.interest_rates.deposit_rate = 0.02;
        central_bank.interest_rates.lombard_rate = 0.05;

        let mut banks = vec![&mut bank1, &mut bank2];
        market.clear_market(&mut banks, &central_bank, 1);

        // Bank 1 has 200k reserves, needs 100k (surplus: 100k)
        // Bank 2 has 150k reserves, needs 150k (deficit: 0k)
        // No clearing needed
        assert!((market.available_liquidity - 100_000.0).abs() < 1e-9);
        assert!((market.demanded_liquidity - 0.0).abs() < 1e-9);
    }

    /// Test-only default for `Bank` so the unit tests do not need to name every
    /// field.
    impl Bank {
        fn test_default() -> Self {
            Self {
                id: String::new(),
                name: String::new(),
                bank_type: String::new(),
                subtype: None,
                own_capital: 0.0,
                total_deposits: 0.0,
                issued_loans: 0.0,
                mandatory_reserves: 0.0,
                liquid_reserves: 0.0,
                liquidity: 0.0,
                deposit_interest_rate: 0.0,
                interest_rate: 0.0,
                condition: default_condition(),
                last_new_credit: 0.0,
                reserve_requirement_ratio: default_reserve_requirement_ratio(),
                is_dspw: false,
                consumer_loans: Vec::new(),
                extra: Map::new(),
            }
        }
    }
}

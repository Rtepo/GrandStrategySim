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
use crate::society::geography::{RuralClass, UrbanClass};
use crate::state::macro_data::annual_to_per_turn_rate;
use crate::state::CentralBank;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

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

/// v4.20 WS3: Hard floor on corporate loan contract rates (50 bps annual).
///
/// Nominal loan rates must never be negative — a negative rate pays the
/// borrower interest out of bank equity and produces negative interest
/// income (the observed −280K/turn anomaly under deflationary NIRP).
/// With the zero-lower-bound central bank this floor is rarely reached,
/// but it is the honest invariant on every repricing/issuance path.
pub const MIN_LOAN_RATE: f64 = 0.005;

/// v4.20 WS3: Hard floor on household consumer-credit rates (100 bps
/// annual). Consumer credit carries a wider margin than corporate
/// facilities; the floor keeps B2C interest income strictly positive.
pub const MIN_CONSUMER_RATE: f64 = 0.01;

/// Default bank condition string.
fn default_condition() -> String {
    "Stable".to_string()
}

/// v4.20 WS3: Household credit-demand calibration.
///
/// Baseline leverage households seek against their permanent income for
/// housing + durable-goods financing (mortgage-style): 1.0× ANNUAL income
/// when the durable stock is at target and real rates are neutral.
const CONSUMER_DEBT_INCOME_BASE: f64 = 1.0;
/// Each unmet durable unit per capita adds this much leverage demand —
/// a household short of its target durable stock finances the gap with
/// credit (cars, furniture, appliances — the goods the B2C market keeps
/// unmet when savings are thin). Cap at +200% extra leverage.
const CONSUMER_GAP_CREDIT_K: f64 = 0.5;
const CONSUMER_GAP_CAP: f64 = 3.0;
/// Neutral real rate (annual, 3%) for the interest-rate channel and the
/// per-point sensitivity of desired leverage. Real rates BELOW neutral
/// stimulate borrowing (cheap credit), above neutral depress it —
/// demand responds to real economic conditions, not deposit levels.
const CONSUMER_TARGET_REAL_RATE: f64 = 0.03;
const CONSUMER_RATE_SENSITIVITY: f64 = 6.0;
/// Per-class per-turn origination cap — converges to the desired debt
/// stock over a few turns instead of a single-turn lump.
const CONSUMER_MAX_LOAN_PER_CLASS_TURN: f64 = 250_000.0;

/// v4.20 WS3: Adaptive expected CPI inflation (percent, per turn) — an
/// exponential-weighted average of realized `cpi_inflation` samples in
/// `TelemetryHistory`. Same λ=0.20 convention as the wage Phillips curve
/// (`economy::labor::labor::expected_inflation_ewma`); duplicated here to
/// keep `state/` modules free of economy-layer dependencies.
fn expected_cpi_inflation_pct(
    history: &crate::state::macro_data::TelemetryHistory,
    current: f64,
) -> f64 {
    const LAMBDA: f64 = 0.20;
    let mut acc = current * LAMBDA;
    let mut weight_sum = LAMBDA;
    let mut decay = 1.0 - LAMBDA;
    for sample in history.samples.iter().rev() {
        let w = LAMBDA * decay;
        acc += sample.cpi_inflation * w;
        weight_sum += w;
        decay *= 1.0 - LAMBDA;
    }
    if weight_sum > 0.0 {
        acc / weight_sum
    } else {
        current
    }
}

/// v4.20 WS3: Unmet durable-consumption gap for a class, in units per
/// capita — the real stock deficit between the consumption-registry
/// target (`base_per_capita × durability`, the same anchor the retail
/// demand builder uses) and the class's serviceable durable stock
/// (`household_durables` cohorts still above condition 0.2).
fn unmet_durable_gap_pc(
    class: &crate::society::geography::ClassDemographics,
    class_name: &str,
) -> f64 {
    let Some(basket) = crate::data::consumption_registry::consumption_registry().get(class_name)
    else {
        return 0.0;
    };
    let mut gap = 0.0;
    for tier_commodities in basket.tiers.values() {
        for (commodity, base_pc) in tier_commodities {
            if !commodity.is_household_durable() {
                continue;
            }
            let durability = commodity.household_durable_turns();
            let target_pc = if durability.is_finite() && durability > 0.0 {
                base_pc * durability
            } else {
                *base_pc
            };
            let serviceable_pc: f64 = class
                .household_durables
                .iter()
                .filter(|c| c.commodity == *commodity && c.condition >= 0.2)
                .map(|c| c.count)
                .sum();
            gap += (target_pc - serviceable_pc).max(0.0);
        }
    }
    gap
}

/// v4.20 WS3: Desired NEW household credit this turn — demand driven by
/// real economic conditions, not by deposit levels:
///
///   desired_debt_stock = annual_income_pc × DEBT_INCOME_BASE
///                        × (1 + K·unmet_durable_gap_pc)   [housing/durable channel]
///                        × rate_factor(real_rate)         [low real rates stimulate]
///   new_credit = max(0, desired_debt_stock − current_debt)
///
/// * Income: `smoothed_income` (permanent-income EWMA already maintained
///   by the B2C layer) — real wage/income channel. Falls back to the
///   national average wage when a class has no recorded income stream.
/// * `consumer_rate`/`expected_inflation` are ANNUAL decimals; the gap is
///   per-capita units; `population` scales the stock.
fn consumer_credit_demand(
    class: &crate::society::geography::ClassDemographics,
    class_name: &str,
    avg_wage: f64,
    consumer_rate: f64,
    expected_inflation_annual: f64,
) -> f64 {
    let population = class.population.max(0) as f64;
    if population <= 0.0 {
        return 0.0;
    }
    // Permanent income per capita (turn units → annual).
    let income_pc = if class.smoothed_income > 0.0 {
        class.smoothed_income / population
    } else {
        avg_wage
    };
    if income_pc <= 0.0 {
        return 0.0;
    }
    let annual_income_pc = income_pc * crate::state::macro_data::TURNS_PER_YEAR as f64;

    let gap_pc = unmet_durable_gap_pc(class, class_name);
    let gap_boost = (1.0 + CONSUMER_GAP_CREDIT_K * gap_pc).min(CONSUMER_GAP_CAP);
    let real_rate = consumer_rate - expected_inflation_annual;
    let rate_factor =
        (1.0 + CONSUMER_RATE_SENSITIVITY * (CONSUMER_TARGET_REAL_RATE - real_rate))
            .clamp(0.25, 2.0);
    let desired_debt = annual_income_pc * CONSUMER_DEBT_INCOME_BASE
        * gap_boost
        * rate_factor
        * population;
    (desired_debt - class.debt.max(0.0)).max(0.0)
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
    pub interbank_loans_given: BTreeMap<String, f64>,

    /// Government bonds and other liquid securities held.
    #[serde(default)]
    pub securities: f64,

    /// Phase D.5: Senior MBS holdings (for QE purchases).
    #[serde(default)]
    pub mbs_holdings: Vec<MortgageBackedSecurity>,

    /// Physical buildings and infrastructure owned by the bank.
    #[serde(default)]
    pub real_estate: f64,

    /// Phase 94: Outstanding consumer loans (B2C credit) issued by the bank.
    /// This is the total outstanding principal of consumer loans held as
    /// a bank asset. Tracked separately from `loans_issued` (which are
    /// corporate loans). Consumer loans are issued to class demographics
    /// and credit citizen savings (M0). Bank reserves are debited on
    /// issuance to keep M0 conserved (Directive 1: closed-loop).
    #[serde(default)]
    pub consumer_loans_outstanding: f64,

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
    pub interbank_loans_taken: BTreeMap<String, f64>,

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
            + self.consumer_loans_outstanding
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
        let diff = (assets - liabilities_plus_equity).abs();
        // Phase 94: Use relative tolerance for large balance sheets.
        // Absolute 1e-6 is too strict when assets are ~10B (f64 rounding
        // error at that scale is ~1e-6). Use 1e-9 relative tolerance, with
        // a 1e-6 absolute floor for small/zero balance sheets.
        let scale = assets.abs().max(liabilities_plus_equity.abs());
        if scale < 1e6 {
            diff < 1e-6
        } else {
            diff < scale * 1e-9
        }
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

    /// v4.20 WS3b: Pay `amount` of central-bank money, draining the deposit
    /// facility before the settlement account.
    ///
    /// The facility is a same-day callable asset (the Step-4 pass already
    /// withdraws it at par to cure reserve deficits before resorting to
    /// Lombard), so a solvent bank always spends the parked buffer before
    /// letting `reserves_at_central_bank` go negative. Net effect: total
    /// CB liquidity (`reserves + depfac`) decreases by exactly `amount`;
    /// A = L + E and M0 accounting are unchanged (both fields are already
    /// counted as bank reserves).
    pub fn draw_cb_liquidity(&mut self, amount: f64) {
        let from_depfac = amount.min(self.cb_deposit_facility_balance.max(0.0));
        self.cb_deposit_facility_balance -= from_depfac;
        self.reserves_at_central_bank -= amount - from_depfac;
    }

    /// W3/R1: Total outstanding credit extended by this bank.
    ///
    /// Sums every loan asset on the book: corporate `loans_issued` plus
    /// `consumer_loans_outstanding` (B2C credit is a real loan asset and
    /// counts toward the regulatory Loan-to-Deposit cap). Interbank loans
    /// are excluded — they are reserve re-lending between banks, not
    /// deposit-funded credit creation.
    pub fn total_loans_outstanding(&self) -> f64 {
        self.loans_issued
            .iter()
            .map(|l| l.outstanding_balance)
            .sum::<f64>()
            + self.consumer_loans_outstanding
    }

    /// W3/R1: Regulatory Loan-to-Deposit cap — maximum total credit this
    /// bank may carry: `deposits × (1 − reserve_requirement_ratio)`.
    ///
    /// The cap is derived from the reserve requirement (the classic
    /// fractional-reserve bound: banks lend deposits net of required
    /// reserves), not a magic number. `deposits` is the post-issuance
    /// deposit base — callers pass `bs.deposits + principal` because loan
    /// issuance creates a deposit liability 1:1.
    pub fn ldr_cap(deposits: f64, reserve_requirement_ratio: f64) -> f64 {
        deposits * (1.0 - reserve_requirement_ratio)
    }

    /// W3/R1: Loan-to-Deposit headroom — how much additional loan asset this
    /// bank can hold without the deposit leg counting toward the cap.
    ///
    /// Returns `deposits × (1 − rr) − total_loans` clamped at zero. For
    /// issuance paths that do NOT create a deposit (unbanked cash loans,
    /// consumer loans), the headroom must cover the full new principal.
    /// For `issue_loan` the deposit created by the loan itself is added to
    /// the deposit base, so only `principal × rr` of headroom is required —
    /// that check lives inside `issue_loan`.
    pub fn ldr_headroom(&self, reserve_requirement_ratio: f64) -> f64 {
        (Self::ldr_cap(self.deposits, reserve_requirement_ratio)
            - self.total_loans_outstanding())
        .max(0.0)
    }

    /// v4.18 WS3: Realized Loan-to-Deposit ratio `total_loans / deposits`.
    ///
    /// This is the production/UI metric that must stay `<= 1.0` in
    /// equilibrium (regulatory cap is actually `1 − reserve_requirement`,
    /// strictly below 1). `0.0` when the bank carries no deposits — a
    /// bank with no deposits may not lend at all under the gates below.
    pub fn ldr(&self) -> f64 {
        if self.deposits > 0.0 {
            self.total_loans_outstanding() / self.deposits
        } else {
            0.0
        }
    }
}

/// v4.18 WS3: Uniform prudential gate for ALL credit creation.
///
/// Every lending path — fractional-reserve `issue_loan` AND the
/// reserve-funded overdrafts settled in the trade/logistics passes —
/// must clear the same reserve-requirement, hard LDR cap, and leverage
/// gates. Previously the overdraft paths pushed `loans_issued` directly
/// with no gate at all, which let a state/universal bank run to ~381%
/// LDR (loan book 3.8× its deposit base, reserves fully drained) while
/// commercial banks were bound by the real checks.
///
/// * `creates_deposit = true` — classic credit creation: the loan mints
///   its own deposit (`deposits += P`, reserves untouched at issuance).
///   The new deposit needs `rr × P` of reserve cover.
/// * `creates_deposit = false` — reserve-funded credit (settlement
///   overdrafts whose proceeds are spent in the same instant, so no
///   deposit liability is minted): the disbursement leaves the bank's
///   reserve account, so the full `P` must be covered by excess reserves
///   and the LDR cap binds against the *existing* deposit base.
///
/// # Gates (identical meaning on both forms)
/// 1. Reserve requirement: post-state effective reserves (own reserves,
///   net of Lombard credit — borrowed reserves cannot support credit
///   creation) must cover `deposits_after × rr`.
/// 2. LDR cap: `loans_after ≤ deposits_after × (1 − rr)` — the classic
///    fractional-reserve bound.
/// 3. Leverage: `liabilities_after ≤ 9.5 × tier_1_capital` — a bank past
///    the prudential leverage cap takes no new credit exposure.
///
/// A rejection leaves ZERO ledger trace (the caller must not book the
/// loan).
pub fn check_credit_prudential_gates(
    balance_sheet: &BankBalanceSheet,
    principal: f64,
    reserve_requirement_ratio: f64,
    creates_deposit: bool,
) -> Result<(), String> {
    // Gate 1 — reserve requirement.
    let deposits_after = if creates_deposit {
        balance_sheet.deposits + principal
    } else {
        balance_sheet.deposits
    };
    // v4.20 WS3b: the CB deposit facility is a same-day callable reserve
    // asset — Step 4 already withdraws it at par to cure reserve deficits
    // before resorting to Lombard. Measuring lendable liquidity on raw
    // `reserves_at_central_bank` alone was the LDR killer: Step 4 sweeps
    // every surplus unit into the facility, leaving `res ≈ deposits × rr`
    // so this gate rejected essentially ALL new credit (avg LDR 0.11 while
    // banks parked 10-100× their reserves at the CB). A solvent bank can
    // always fund a loan by drawing the facility down.
    let cb_liquidity_after = if creates_deposit {
        balance_sheet.reserves_at_central_bank + balance_sheet.cb_deposit_facility_balance
    } else {
        balance_sheet.reserves_at_central_bank
            + balance_sheet.cb_deposit_facility_balance
            - principal
    };
    let required_reserves = deposits_after * reserve_requirement_ratio;
    let effective_reserves = cb_liquidity_after - balance_sheet.cb_lombard_loans;
    if effective_reserves < required_reserves {
        return Err(format!(
            "Reserve requirement violation: need {} reserves, have {} effective ({} raw + {} depfac - {} lombard)",
            required_reserves, effective_reserves,
            balance_sheet.reserves_at_central_bank,
            balance_sheet.cb_deposit_facility_balance,
            balance_sheet.cb_lombard_loans
        ));
    }

    // Gate 2 — LDR cap: total outstanding credit may not exceed
    // `deposits_after × (1 − reserve_requirement_ratio)` after issuance.
    // v4.20 WS3b: the prudential bound is further tightened by an
    // OPERATIONAL ceiling of 0.85 — the top of the healthy leverage band.
    // Without it the loan book saturates the deposit base system-wide at
    // ~1−rr, leaving zero secondary-market headroom: a bank whose realized
    // LDR drifts past 1.0 through deposit flight then has no buyer for its
    // excess paper (every other bank sits at the same cap). Holding
    // issuance to 0.85 reserves the 11-point gap between operational and
    // prudential caps as the reservoir `enforce_ldr_cap` sells into.
    let loans_after = balance_sheet.total_loans_outstanding() + principal;
    let ldr_cap = BankBalanceSheet::ldr_cap(deposits_after, reserve_requirement_ratio)
        .min(deposits_after * 0.85);
    if loans_after > ldr_cap + 1e-9 * ldr_cap.abs().max(1.0) {
        return Err(format!(
            "LDR cap violation: loans_after={:.2} exceeds cap={:.2} (deposits_after={:.2} × min(1 − rr={:.4}, 0.85))",
            loans_after, ldr_cap, deposits_after, reserve_requirement_ratio
        ));
    }

    // Gate 3 — prudential leverage: post-state liabilities may not exceed
    // 9.5× tier_1_capital. Deposit-creating issuance grows liabilities by
    // `principal`; reserve-funded credit leaves them unchanged but a bank
    // already over the cap takes no new credit exposure.
    let liabilities_after = balance_sheet.total_liabilities()
        + if creates_deposit { principal } else { 0.0 };
    if balance_sheet.tier_1_capital > 0.0
        && liabilities_after > balance_sheet.tier_1_capital * 9.5
    {
        return Err(format!(
            "Leverage cap violation: liabilities_after={:.2} exceeds {:.2}× tier_1 ({:.2})",
            liabilities_after, 9.5, balance_sheet.tier_1_capital
        ));
    }

    Ok(())
}

/// v4.18 WS3: Integrator/crucible helper — the worst Loan-to-Deposit ratio
/// across all bank balance sheets.
///
/// With `check_credit_prudential_gates` applied to EVERY credit-creation
/// path (fractional-reserve issuance and reserve-funded settlement
/// overdrafts alike), a live bank can never issue past
/// `LDR = 1 − reserve_requirement_ratio`, so the observed maximum must
/// stay `<= 1.0` (the pre-fix state bank ran ~3.81 because the overdraft
/// paths pushed `loans_issued` with no gate at all). A value above 1.0 can
/// only come from a legacy/genesis state or a new ungated loan push — both
/// are bugs the crucible should catch.
pub fn max_bank_ldr(companies: &[crate::entities::Company]) -> f64 {
    companies
        .iter()
        .filter(|c| c.bank_type.is_some())
        .filter_map(|c| c.balance_sheet.as_ref())
        .map(|bs| bs.ldr())
        .fold(0.0, f64::max)
}

/// Phase 95: Emergency Liquidity Assistance — draw CB Lombard credit so an
/// upcoming reserve debit cannot push `reserves_at_central_bank` below zero.
///
/// The draw covers BOTH the new debit and any pre-existing negative balance:
/// `draw = max(0, upcoming_debit - reserves)`. Booked as `cb_lombard_loans`
/// (liability) against new reserves (asset), so A = L + E is preserved and
/// `central_bank.liquidity_injected` tracks the M0 creation. The Central Bank
/// is the lender of last resort — ELA is always available (the Lombard rate
/// prices the loan; it does not gate availability).
///
/// Call this BEFORE applying the debit. Returns the amount drawn.
pub fn ela_cover_debit(
    bs: &mut BankBalanceSheet,
    central_bank: &mut CentralBank,
    upcoming_debit: f64,
) -> f64 {
    let shortfall = upcoming_debit - bs.reserves_at_central_bank;
    if shortfall <= 0.0 {
        return 0.0;
    }
    bs.cb_lombard_loans += shortfall;
    bs.reserves_at_central_bank += shortfall;
    central_bank.liquidity_injected += shortfall;
    shortfall
}

/// Debit a bank's customer-deposit liability for an outflow already applied
/// to a customer's cash leg. `deposits` is floored at 0 — a debit exceeding
/// the recorded liability is an unbooked impairment (the bank paid out more
/// than customers held), charged to `tier_1_capital` so A = L + E stays
/// exact. Returns the equity-charged residual.
pub fn settle_deposit_debit(bs: &mut BankBalanceSheet, amount: f64) -> f64 {
    if amount <= 0.0 {
        return 0.0;
    }
    let debit = amount.min(bs.deposits.max(0.0));
    bs.deposits -= debit;
    let residual = amount - debit;
    bs.tier_1_capital -= residual;
    residual
}

/// End-of-phase deposit-floor reconciliation. `deposits` is a liability owed
/// to customers and can never be negative; a debit that bypasses the guarded
/// settlement paths (unbacked outflow) is an impairment — the bank owes money
/// it no longer holds. Repairing it lifts `deposits` to 0 and books the
/// shortfall against `tier_1_capital` so L↓ and E↓ keep A = L + E balanced.
/// Banks driven to/below zero equity are left to the resolution regime.
/// Returns total equity written off.
pub fn enforce_deposit_floor(companies: &mut [crate::entities::Company]) -> f64 {
    let mut written_off = 0.0;
    for company in companies.iter_mut() {
        if company.bank_type.is_none() {
            continue;
        }
        let Some(ref mut bs) = company.balance_sheet else {
            continue;
        };
        if bs.deposits >= 0.0 {
            continue;
        }
        let shortfall = -bs.deposits;
        bs.deposits = 0.0;
        bs.tier_1_capital -= shortfall;
        written_off += shortfall;
    }
    written_off
}

/// Phase 95: End-of-phase reserve-floor reconciliation. Lifts every bank's
/// `reserves_at_central_bank` to >= 0 via ELA, repairing debits that bypassed
/// the guarded settlement paths (batch wage/tax syncs, `debit_company_by_id`,
/// Lombard interest accrual). Returns total liquidity injected.
pub fn enforce_reserve_floor(
    companies: &mut [crate::entities::Company],
    central_bank: &mut CentralBank,
) -> f64 {
    let mut injected = 0.0;
    for company in companies.iter_mut() {
        if let Some(ref mut bs) = company.balance_sheet {
            injected += ela_cover_debit(bs, central_bank, 0.0);
        }
    }
    injected
}

/// Prudential leverage supervision: a bank whose liabilities exceed the
/// regulatory 10x-equity limit must be recapitalized before the horizon
/// audit — deposits cannot be refused, so liability growth alone can
/// strand a small bank past the limit even with no new lending. The
/// treasury purchases equity (liquid_reserves -> bank reserves at the
/// CB): M0-neutral since both legs sit inside the monetary walk, and
/// the sheet stays balanced (reserves and equity both rise; liabilities
/// untouched). When the treasury cannot cover the gap, the residual is
/// financed by the central bank under the same liquidity_injected
/// identity the ELA reserve cover uses — a lender-of-last-resort recap
/// rather than an unbooked equity patch. Banks at/below zero equity are
/// left to the resolution regime instead.
///
/// Called inside `process_banking_turn` AND again at the end-of-turn
/// reserve-floor pass: settlement phases grow deposit liabilities all
/// turn long, so an early-turn recap alone cannot bound the snapshot
/// the audit reads.
pub fn enforce_leverage_cap(
    country: &mut crate::state::Country,
    companies: &mut [crate::entities::Company],
) {
    for bank in companies.iter_mut() {
        if bank.bank_type.is_none() {
            continue;
        }
        let Some(ref mut bs) = bank.balance_sheet else {
            continue;
        };
        if bs.tier_1_capital <= 0.0 {
            continue;
        }
        let liabilities = bs.total_liabilities();
        // Recap to 9x — a margin below the 10x hard limit so routine
        // liability growth does not immediately re-breach.
        let needed = (liabilities / 9.0 - bs.tier_1_capital).max(0.0);
        if needed <= 0.0 {
            continue;
        }
        let from_treasury = needed.min(country.budget.liquid_reserves.max(0.0));
        if from_treasury > 0.0 {
            country.budget.liquid_reserves -= from_treasury;
            bs.reserves_at_central_bank += from_treasury;
            bs.tier_1_capital += from_treasury;
        }
        let residual = needed - from_treasury;
        if residual > 0.0 {
            country.central_bank.liquidity_injected += residual;
            bs.reserves_at_central_bank += residual;
            bs.tier_1_capital += residual;
        }
    }
}

/// v4.20 WS3b: Realized-LDR supervision — secondary-market whole-loan
/// sales between banks.
///
/// `check_credit_prudential_gates` caps credit at ISSUANCE
/// (`loans_after <= deposits_after × (1 − rr)`), but `deposits` are
/// demand liabilities that migrate between banks every turn while the
/// loan book is sticky stock. A lending bank's REALIZED LDR can
/// therefore drift past 1.0 through deposit flight alone — its book
/// ends up funded by interbank/Lombard credit rather than by its own
/// deposit base. No issuance-time control can bound that ex-post
/// drift; the remedy real banks use is the secondary loan market: a
/// deposit-poor lender sells whole loans at par to a deposit-rich bank
/// holding regulatory headroom, moving the asset toward the funding.
/// Both legs stay inside the banking system — M0-neutral, and
/// A = L + E is exact on both sheets (assets swap loans↔reserves,
/// liabilities untouched).
///
/// Mechanics — every bank with `ldr() > 0.97` sells down to 0.90:
///   * Sellers move whole `Loan` records, skipping defaulted,
///     securitized, and covered-bond-pledged paper — impaired or
///     encumbered assets do not trade at par. The scalar
///     `consumer_loans_outstanding` book is NOT transferable: a
///     participation slice would desynchronize the repayment loop,
///     which debits the originating bank's scalar against borrower
///     cash regardless of who holds a notional share.
///   * Buyers may hold up to their own issuance cap
///     `deposits × (1 − rr)` post-purchase, and must keep
///     `reserves + depfac − price` covering the reserve requirement on
///     their unchanged deposit base. The CB deposit facility is drawn
///     into reserves to settle — the same liquidity ladder the lending
///     gates already treat as lendable.
///   * Settlement is in reserves: buyer `reserves −= price`, seller
///     `reserves += price`. No money is created or destroyed;
///     aggregate loans and deposits are untouched.
///
/// Called after the leverage-cap pass (in-turn AND end-of-turn):
/// deposit churn during the turn's settlement phases is what pushes
/// realized LDR out of band, so supervision must read the final
/// balance-sheet snapshot the audit sees. Borrower-side `LoanRef`s key
/// on `loan_id`, so the next turn's accrual loop services the paper
/// under its new owner with no borrower-visible change. Returns total
/// outstanding principal transferred.
pub fn enforce_ldr_cap(
    companies: &mut [crate::entities::Company],
    reserve_requirement_ratio: f64,
) -> f64 {
    const LDR_SALE_TRIGGER: f64 = 0.97;
    const LDR_TARGET: f64 = 0.90;
    let n = companies.len();
    // Planned transfers: (seller_idx, buyer_idx, loan_vec_idx, price).
    // Two-phase: the plan is computed on immutable reads with
    // `pending_*` counters so concurrent seller demand is allocated to
    // buyers deterministically in index order.
    let mut plan: Vec<(usize, usize, usize, f64)> = Vec::new();
    let mut pending_loans = vec![0.0f64; n];
    let mut pending_spend = vec![0.0f64; n];

    for s in 0..n {
        if companies[s].bank_type.is_none() {
            continue;
        }
        let Some(sbs) = companies[s].balance_sheet.as_ref() else {
            continue;
        };
        if sbs.ldr() <= LDR_SALE_TRIGGER || sbs.deposits <= 0.0 {
            continue;
        }
        let needed = sbs.total_loans_outstanding() - sbs.deposits * LDR_TARGET;
        // Sellable paper, smallest first. Loans are atomic (borrower
        // LoanRefs key on the record id, so a tranche sale would orphan
        // the buyer's repayment stream), which means the marginal sale
        // may overshoot the target — landing below LDR_TARGET is legal.
        // Ordering ascending by (price, index) keeps the marginal piece
        // as small as the book allows and the plan deterministic.
        let mut candidates: Vec<(usize, f64)> = sbs
            .loans_issued
            .iter()
            .enumerate()
            .filter(|(_, loan)| {
                loan.status != LoanStatus::Default
                    && !loan.securitized
                    && loan.pledged_to_covered_bond.is_none()
                    && loan.outstanding_balance > 0.0
            })
            .map(|(li, loan)| (li, loan.outstanding_balance))
            .collect();
        candidates
            .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.0.cmp(&b.0)));
        let mut sold = 0.0;
        for (li, price) in candidates {
            if sold >= needed {
                break;
            }
            // First buyer (index order) that can hold this loan inside
            // its own (1 − rr) cap and settle it out of free reserves
            // + deposit-facility liquidity.
            for b in 0..n {
                if b == s || companies[b].bank_type.is_none() {
                    continue;
                }
                let Some(bbs) = companies[b].balance_sheet.as_ref() else {
                    continue;
                };
                if bbs.deposits <= 0.0 {
                    continue;
                }
                let cap = BankBalanceSheet::ldr_cap(bbs.deposits, reserve_requirement_ratio);
                let headroom = cap - (bbs.total_loans_outstanding() + pending_loans[b]);
                if headroom < price {
                    continue;
                }
                let free = bbs.reserves_at_central_bank + bbs.cb_deposit_facility_balance
                    - pending_spend[b]
                    - bbs.deposits * reserve_requirement_ratio;
                if free < price {
                    continue;
                }
                plan.push((s, b, li, price));
                pending_loans[b] += price;
                pending_spend[b] += price;
                sold += price;
                break;
            }
        }
        #[cfg(feature = "diagnostic")]
        if sold < needed {
            let mut sellable = 0.0;
            let mut pledged = 0.0;
            let mut defaulted = 0.0;
            let mut securitized = 0.0;
            for l in &sbs.loans_issued {
                if l.status == LoanStatus::Default {
                    defaulted += l.outstanding_balance;
                } else if l.securitized {
                    securitized += l.outstanding_balance;
                } else if l.pledged_to_covered_bond.is_some() {
                    pledged += l.outstanding_balance;
                } else {
                    sellable += l.outstanding_balance;
                }
            }
            eprintln!(
                "LDRSALE_SHORT: bank={} ldr={:.4} needed={:.0} sold={:.0} sellable={:.0} pledged={:.0} defaulted={:.0} sec={:.0} book={:.0}",
                companies[s].id,
                sbs.ldr(),
                needed,
                sold,
                sellable,
                pledged,
                defaulted,
                securitized,
                sbs.total_loans_outstanding()
            );
        }
    }

    // Phase 2 — apply. Group removals per seller and strip loan indices
    // in descending order so planned positions stay valid.
    let mut moved_total = 0.0;
    let mut by_seller: BTreeMap<usize, Vec<(usize, usize, f64)>> = BTreeMap::new();
    for (s, b, li, price) in plan {
        by_seller.entry(s).or_default().push((b, li, price));
    }
    for (s, mut moves) in by_seller {
        moves.sort_by_key(|m| m.1);
        let mut taken: Vec<(usize, Loan, f64)> = Vec::new();
        if let Some(ref mut bs) = companies[s].balance_sheet {
            for &(b, li, price) in moves.iter().rev() {
                let loan = bs.loans_issued.remove(li);
                bs.reserves_at_central_bank += price;
                taken.push((b, loan, price));
            }
        }
        for (b, loan, price) in taken {
            if let Some(ref mut bbs) = companies[b].balance_sheet {
                bbs.reserves_at_central_bank -= price;
                // Draw the deposit facility into reserves to restore the
                // reserve requirement after settlement.
                let required = bbs.deposits * reserve_requirement_ratio;
                let topup = (required - bbs.reserves_at_central_bank)
                    .min(bbs.cb_deposit_facility_balance)
                    .max(0.0);
                bbs.cb_deposit_facility_balance -= topup;
                bbs.reserves_at_central_bank += topup;
                bbs.loans_issued.push(loan);
                moved_total += price;
            }
        }
    }
    moved_total
}

/// v4.20 WS3c: Realized-LDR supervision — deposit migration.
///
/// The secondary loan market (`enforce_ldr_cap`) cannot clear a breach
/// when the seller's book is fully encumbered — covered-bond-pledged or
/// securitized paper legally does not trade, and the scalar consumer
/// book is not portable. The residual fix real systems exhibit is
/// funding-side: a deposit-poor lender bids balances away from
/// deposit-rich banks (rate competition / wholesale placement), moving
/// the DENOMINATOR instead of the asset.
///
/// Mechanics — every bank still above `LDR_SALE_TRIGGER` after the sale
/// pass attracts deposits down to `LDR_TARGET`. Each migrated unit is
/// settled through the payment system: donor `deposits −= x,
/// reserves −= x`, receiver `deposits += x, reserves += x` — the
/// reserves behind the balance follow it. A = L + E holds on both
/// sheets and aggregate deposits/reserves are conserved (M0 untouched).
/// Donors are chosen lowest-LDR first and are bounded on BOTH sides:
/// their post-gift LDR stays under the same target and the reserves
/// they pay out keep their own requirement covered
/// (`free/(1 − rr)` capacity). Runs after the sale pass at both call
/// sites; returns total deposits migrated.
pub fn enforce_ldr_deposit_migration(
    companies: &mut [crate::entities::Company],
    reserve_requirement_ratio: f64,
) -> f64 {
    const LDR_SALE_TRIGGER: f64 = 0.97;
    const LDR_TARGET: f64 = 0.90;
    let n = companies.len();
    // Donor order is deterministic: lowest realized LDR first, index as
    // tiebreak — the deepest funding pools absorb the migration.
    let mut donor_order: Vec<usize> = (0..n)
        .filter(|&b| {
            companies[b].bank_type.is_some()
                && companies[b]
                    .balance_sheet
                    .as_ref()
                    .is_some_and(|bs| bs.deposits > 0.0)
        })
        .collect();
    donor_order.sort_by(|&a, &b| {
        let la = companies[a]
            .balance_sheet
            .as_ref()
            .map(|bs| bs.ldr())
            .unwrap_or(0.0);
        let lb = companies[b]
            .balance_sheet
            .as_ref()
            .map(|bs| bs.ldr())
            .unwrap_or(0.0);
        la.partial_cmp(&lb).unwrap().then(a.cmp(&b))
    });
    // Planned transfers: (donor_idx, receiver_idx, amount).
    let mut plan: Vec<(usize, usize, f64)> = Vec::new();
    let mut pending_give = vec![0.0f64; n];
    let mut pending_take = vec![0.0f64; n];
    for s in 0..n {
        if companies[s].bank_type.is_none() {
            continue;
        }
        let Some(sbs) = companies[s].balance_sheet.as_ref() else {
            continue;
        };
        if sbs.ldr() <= LDR_SALE_TRIGGER || sbs.deposits <= 0.0 {
            continue;
        }
        // Deposits needed so loans / (deposits + x) <= LDR_TARGET.
        let mut needed =
            sbs.total_loans_outstanding() / LDR_TARGET - sbs.deposits - pending_take[s];
        if needed <= 0.0 {
            continue;
        }
        for &d in &donor_order {
            if d == s {
                continue;
            }
            let Some(dbs) = companies[d].balance_sheet.as_ref() else {
                continue;
            };
            let dep_after = dbs.deposits - pending_give[d];
            if dep_after <= 0.0 {
                continue;
            }
            // Donor LDR bound: loans / (dep − give) <= LDR_TARGET.
            let ldr_room = dep_after - dbs.total_loans_outstanding() / LDR_TARGET;
            // Donor reserve bound: paying x reserves while deposits fall
            // x leaves required = (dep − x)·rr, so free reserves shrink
            // by x·(1 − rr).
            let free = dbs.reserves_at_central_bank + dbs.cb_deposit_facility_balance
                - pending_give[d]
                - dep_after * reserve_requirement_ratio;
            let res_room = free / (1.0 - reserve_requirement_ratio).max(1e-9);
            let give = needed.min(ldr_room).min(res_room);
            if give <= 0.0 {
                continue;
            }
            plan.push((d, s, give));
            pending_give[d] += give;
            pending_take[s] += give;
            needed -= give;
            if needed <= 0.0 {
                break;
            }
        }
        #[cfg(feature = "diagnostic")]
        if needed > 0.0 {
            eprintln!(
                "LDRMIG_SHORT: bank={} ldr={:.4} needed={:.0} short={:.0}",
                companies[s].id,
                sbs.ldr(),
                sbs.total_loans_outstanding() / LDR_TARGET - sbs.deposits,
                needed
            );
        }
    }
    // Phase 2 — apply the migration.
    let mut moved = 0.0;
    for (d, s, x) in plan {
        if let Some(ref mut dbs) = companies[d].balance_sheet {
            dbs.deposits -= x;
            dbs.reserves_at_central_bank -= x;
        }
        if let Some(ref mut sbs) = companies[s].balance_sheet {
            sbs.deposits += x;
            sbs.reserves_at_central_bank += x;
            moved += x;
        }
    }
    moved
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
    /// * `sobk` - Mutable reference to the SOBK voluntary liquidity scheme.
    ///   Member banks still in deficit after the standard interbank clearing
    ///   draw preferential emergency loans here, before resorting to expensive
    ///   CB Lombard; member banks with surplus repay their outstanding SOBK
    ///   loans.
    /// * `current_turn` - Current turn number
    ///
    /// # Rules
    /// * XIBOR is bounded by CB Deposit Rate (floor) and CB Lombard Rate (ceiling)
    /// * Banks prefer interbank market over expensive CB Lombard facility
    /// * Market stress widens spreads and increases XIBOR
    /// * SOBK emergency lending is double-entry: the SOBK pool decreases and
    ///   the borrowing bank's `reserves_at_central_bank` increases by the exact
    ///   loan amount (Directive 1). The SOBK claim (`outstanding_loans`) is not
    ///   fiat â€” the fiat now sits in the borrowing bank's reserves, which are
    ///   counted in the M0 walk.
    /// * SOBK repayment is double-entry: the repaying bank's
    ///   `reserves_at_central_bank` decreases and the SOBK pool increases by
    ///   the exact repayment amount (Directive 1). Without the bank-side debit,
    ///   M0 would be created (pool up, reserves unchanged).
    pub fn clear_market(
        &mut self,
        banks: &mut Vec<&mut crate::entities::Company>,
        central_bank: &CentralBank,
        sobk: &mut SobkScheme,
        current_turn: u32,
    ) {
        let cb_reserve_ratio = central_bank.reserve_requirement_ratio;
        let cb_deposit_rate = central_bank.interest_rates.deposit_rate;
        let cb_lombard_rate = central_bank.interest_rates.lombard_rate;

        // Separate banks into surplus and deficit
        let mut surplus_banks: Vec<(String, f64)> = Vec::new();
        let mut deficit_banks: Vec<(String, f64)> = Vec::new();

        for bank in banks.iter() {
            // Phase 94: Include ALL bank types (Commercial, Universal, Cooperative,
            // AND Investment) in the surplus/deficit calculation. The second loop
            // (transfer execution) processes all bank types, so the first loop
            // must also include all types. Previously, Investment banks were
            // excluded from total_surplus/total_deficit but still lent/borrowed
            // in the second loop, causing M0 destruction (the ratio
            // transfer_amount/total_surplus was wrong for Investment banks).
            if let (Some(_), Some(bs)) = (&bank.bank_type, &bank.balance_sheet) {
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

        // â”€â”€ SOBK Emergency Liquidity Pass (M0 Step 8 / F1-F4 fix) â”€â”€
        // After standard interbank clearing, member banks still in deficit
        // draw preferential emergency loans from the SOBK pool before
        // resorting to expensive CB Lombard. Member banks with surplus and
        // outstanding SOBK loans repay them.
        //
        // Double-Entry (Directive 1):
        // * Emergency loan (F3 fix): SOBK pool â†“, borrowing bank
        //   `reserves_at_central_bank` â†‘ by the exact loan amount. The
        //   `outstanding_loans` tracker is a claim, not fiat â€” the fiat now
        //   sits in the borrowing bank's reserves (counted in the M0 walk).
        // * Repayment (F4 fix): repaying bank `reserves_at_central_bank` â†“,
        //   SOBK pool â†‘ by the exact repayment amount. Without the bank-side
        //   debit, M0 would be created (pool up, reserves unchanged).
        let current_xibor = self.xibor;
        for bank in banks.iter_mut() {
            if let (Some(_), Some(ref mut bs)) = (&bank.bank_type, &mut bank.balance_sheet) {
                let position = bs.reserve_position(cb_reserve_ratio);
                let is_member = sobk.members.contains(&bank.id);
                if !is_member {
                    continue;
                }
                if position < 0.0 {
                    // Member bank still in deficit â€” draw SOBK emergency loan.
                    let needed = -position;
                    let loan = sobk.provide_emergency_loan(&bank.id, needed, current_xibor);
                    if loan > 0.0 {
                        bs.reserves_at_central_bank += loan;
                        // Record the SOBK loan as a bank liability so the
                        // balance-sheet identity holds (Directive 1: assets
                        // increase via reserves, liabilities must increase
                        // by the same amount).
                        *bs
                            .interbank_loans_taken
                            .entry("SOBK".to_string())
                            .or_insert(0.0) += loan;
                    }
                } else if position > 0.0 {
                    // Member bank with surplus â€” repay outstanding SOBK loans.
                    let outstanding = sobk.outstanding_loans.get(&bank.id).copied().unwrap_or(0.0);
                    if outstanding > 0.0 {
                        let repay = position.min(outstanding);
                        sobk.repay_loan(&bank.id, repay);
                        bs.reserves_at_central_bank -= repay;
                        // Reduce the SOBK liability on the bank's balance
                        // sheet so the identity holds (Directive 1: assets
                        // decrease via reserves, liabilities must decrease
                        // by the same amount).
                        if let Some(sobk_loan) = bs.interbank_loans_taken.get_mut("SOBK") {
                            *sobk_loan = (*sobk_loan - repay).max(0.0);
                        }
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
/// * LTV (Loan-to-Value): principal <= collateral_value Ă— max_ltv_ratio
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

    // Phase 25: Creditworthiness gate â€” do not issue loans to economically
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
        // Has fixed capital but no cash â€” penalize heavily but allow
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
/// * `borrower_bank_id` - The borrower's `primary_bank_id`. The minted
///   deposit only counts toward the LDR/reserve caps when it lands on THIS
///   bank's book (`borrower_bank_id == Some(bank_id)`); cross-bank and
///   unbanked issuance is gated as reserve-funded credit.
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
    borrower_bank_id: Option<&str>,
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
    // v4.20 WS3: floor the contract rate — a negative retail loan rate pays
    // the borrower interest and produces negative bank interest income,
    // which is incoherent (with the ZLB this can only be reached via a
    // stale/deserialized XIBOR, but the floor is the honest invariant).
    let risk_premium = credit_score.risk_premium_bps / 10000.0; // Convert bps to decimal
    let interest_rate = (xibor + bank_margin + risk_premium).max(MIN_LOAN_RATE);

    // Step 3: Simulate balance sheet expansion to check reserve requirement.
    // Phase 77: Subtract Lombard loans from effective reserves â€” borrowed
    // reserves from the CB Lombard facility cannot support further credit
    // creation. Only the bank's OWN reserves count toward lending capacity.
    // Step 3a–3c: Prudential gates — v4.18 WS3 routes all three checks
    // through the shared `check_credit_prudential_gates` helper so the
    // reserve-requirement, LDR-cap, and leverage gates are identical on
    // every credit-creation path (fractional-reserve issuance here AND
    // reserve-funded overdrafts in the trade/logistics passes).
    //
    // `creates_deposit = true` is only honest when the minted deposit
    // actually LANDS on this bank's book — i.e. the borrower banks here.
    // For a borrower banked elsewhere the caller's reconciliation ships
    // `deposits −= P` and `reserves −= P` to the borrower's bank in the
    // same turn (and unbanked borrowers reverse the deposit entirely),
    // so the true post-state is loans+P / deposits+0 / reserves−P —
    // reserve-funded credit. Gating that flow as deposit-creating let
    // repeated cross-bank issuance stack a loan book above the lender's
    // real deposit base (observed end-state LDR 1.0047 at BANK-ILL-001:
    // loans > deposits while every path passed its gates).
    let creates_deposit = borrower_bank_id == Some(bank_id);
    check_credit_prudential_gates(
        balance_sheet,
        principal,
        central_bank.reserve_requirement_ratio,
        creates_deposit,
    )?;

    // Step 4: Create loan record
    // Use deterministic ID from seeded RNG to avoid non-deterministic timestamps.
    let loan_id = format!(
        "LOAN-{}-{}",
        bank_id,
        crate::engine::seeded_rng::thread_rng().next_u64()
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
// PHASE 77: BANK OPERATIONAL CAPACITY â€” LABOR & SERVICE CONSTRAINTS
// ============================================================================

/// Phase 77: Operational capacity of a bank, derived from its fulfilled labor.
///
/// A bank's ability to manage assets, originate loans, and handle deposits is
/// directly proportional to its workforce. A bank with 30 employees cannot
/// manage billions in assets â€” it needs thousands of clerks, tellers, and
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
/// * Each clerk can manage ~20,000Ă— their wage in total assets (~$9M book
///   per relationship banker — a commercial banker runs a multi-million
///   portfolio, not a ledger of pocket change).
/// * Each clerk can originate ~2,000Ă— their wage in new loans per turn
///   (~$900K per banker-turn; a loan officer closes million-dollar credit,
///   not $22K microloans — the old 50Ă— multiplier capped sector origination
///   near $27M/turn and extinguished the working-capital channel).
/// * Each clerk can service ~20,000Ă— their wage in deposits.
/// * These are structural economic ratios, not magic nominal constants.
/// * A bank with 0 FTE has zero capacity â€” it cannot operate.
/// * Prudential bounds still apply downstream: reserve requirement, the
///   hard LDR cap, and the 9.5Ă— leverage gate in `issue_loan` bound total
///   credit — this capacity is a staffing throttle, not the solvency check.
pub fn bank_operational_capacity(fulfilled_fte: f64, average_wage: f64) -> BankCapacity {
    if fulfilled_fte <= 0.0 || average_wage <= 0.0 {
        return BankCapacity::default();
    }
    let fte = fulfilled_fte;
    let wage = average_wage.max(1.0);
    BankCapacity {
        max_asset_under_management: fte * wage * 20_000.0,
        max_new_loans_per_turn: fte * wage * 2_000.0,
        max_deposit_handling: fte * wage * 20_000.0,
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
    /// * tier_1_capital is NOT debited â€” the premium is a transfer of reserves,
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

                        // Phase 94: Double-entry â€” BFG premium is an expense.
                        // Asset decreases (reserves out), Equity decreases (expense recognized).
                        // M0 is preserved because BFG reserves are counted in the M0 walk.
                        // The old comment about "Black Hole 1.9" was about the M0 walk not
                        // counting BFG reserves â€” that is now fixed.
                        bs.reserves_at_central_bank -= premium; // Asset decreases
                        // Phase 94: No reserve clamping â€” negative reserves
                        // represent CB Lombard borrowing. Clamping breaks
                        // A=L+E and causes M0 FiatCreation.
                        bs.tier_1_capital -= premium; // Equity decreases (expense)
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
    pub outstanding_loans: BTreeMap<String, f64>,

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
                let mut contribution = excess * 0.5; // Contribute 50% of excess

                // F6 fix: Cap contribution so tier_1 capital never breaches the
                // regulatory minimum (Basel III: 6% of risk-weighted assets).
                // Without this cap, a large contribution can push a bank below
                // the KNF floor and trigger unnecessary resolution.
                let rwa: f64 = bs.loans_issued.iter().map(|l| l.outstanding_balance).sum();
                let regulatory_minimum = rwa * 0.06;
                let max_safe = (bs.tier_1_capital - regulatory_minimum).max(0.0);
                contribution = contribution.min(max_safe);
                if contribution <= 0.0 {
                    return;
                }

                // Phase 94: Double-entry â€” SOBK contribution is an expense.
                // Asset decreases (reserves out), Equity decreases (expense recognized).
                // M0 is preserved because SOBK pool is counted in the M0 walk.
                bs.reserves_at_central_bank -= contribution;
                bs.tier_1_capital -= contribution; // Equity decreases (expense)
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

    /// Repays Central Bank emergency loan from pool surplus (F7 fix â€” complete
    /// lifecycle for `cb_emergency_loan`). Without this method the CB liquidity
    /// line is an immortal liability, violating Directive 4.
    ///
    /// # Arguments
    /// * `central_bank` - Reference to Central Bank
    /// * `amount` - Repayment amount
    ///
    /// # Double-Entry Flow
    /// * SOBK: pool decreases (asset debit), `cb_emergency_loan` decreases
    ///   (liability debit).
    /// * Central Bank: `liquidity_injected` decreases (M0 contraction â€” the
    ///   CB loan is extinguished, fiat returns to the CB).
    /// * Money mass contracts by the exact repayment amount.
    pub fn repay_cb_liquidity_line(&mut self, central_bank: &mut CentralBank, amount: f64) {
        let repayment = amount.min(self.cb_emergency_loan);
        if repayment <= 0.0 {
            return;
        }
        self.cb_emergency_loan -= repayment;
        self.pool -= repayment;
        central_bank.liquidity_injected = (central_bank.liquidity_injected - repayment).max(0.0);
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

    /// Phase 86.5A: Distressed assets ledger â€” seized loans and bonds from
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

        // Step 2: Wipe out existing shareholders (Equity â†’ 0)
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

        // Step 4b: Restore balance-sheet identity (Directive 1).
        // After cleaning liabilities (uninsured deposits, interbank, Lombard
        // removed), the bridge bank's assets exceed its liabilities. Set
        // tier_1_capital to the residual so BFG's 100% ownership has real
        // equity value and assets == liabilities + equity holds.
        let post_resolution_equity = (bs.total_assets() - bs.total_liabilities()).max(0.0);
        bs.tier_1_capital = post_resolution_equity;

        // Step 5: Route creditor payments (Money Mass Preservation)
        // 5a: Interbank loans already repaid to specific lenders above

        // 5b: Repay CB Lombard loans to Central Bank
        // CB liquidity_injected decreases â€” M0 contracts as Lombard loan is extinguished
        central_bank.liquidity_injected =
            (central_bank.liquidity_injected - toxic_lombard).max(0.0);

        // 5c: Uninsured deposits are written off (depositors take haircut)
        // The deposits were already extinguished at line 1460 (bs.deposits = insured_deposits).
        // BFG does NOT pay for uninsured deposits â€” no one receives the money,
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
        // Sync company_capital with the post-resolution equity (Directive 1:
        // company_capital must reflect the real balance-sheet equity).
        failed_bank.company_capital = post_resolution_equity;

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
    /// or reprivatized â€” it cannot operate indefinitely.
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
                                // Phase 94: No reserve clamping â€” negative
                                // reserves represent CB Lombard borrowing.
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
                            // Phase 94: No reserve clamping â€” negative
                            // reserves represent CB Lombard borrowing.
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
/// * **Step 1 â€” CB Rate Update:** `central_bank.update_reference_rate()` based on inflation and GDP growth.
/// * **Step 2 â€” Pre-clearing OMO:** CB buys/sells government bonds from/to banks to adjust aggregate reserves, steering XIBOR toward target rate.
/// * **Step 3 â€” Interbank Clearing:** `interbank_market.clear_market()` sets XIBOR and settles surplus/deficit positions.
/// * **Step 4 â€” Deposit Facility:** Banks with surplus reserves park them at CB, earning deposit rate interest. Creates physical floor for interbank rate.
/// * **Step 5 â€” Lombard Facility:** Banks still in deficit after interbank borrow from CB at penalty rate. Creates physical ceiling for interbank rate.
/// * **Step 6 â€” Loan Repayment:** Existing loans accrue interest; borrowers repay from `available_cash`. Double-entry: bank `loans_issued` decreases, `reserves_at_central_bank` increases; borrower `available_cash` decreases.
/// * **Step 7 â€” New Loan Issuance:** Non-bank companies seek credit. `issue_loan()` creates deposits (money creation). Rate = XIBOR + bank_margin + risk_premium. Reserve requirement constrains issuance.
/// * **Step 8 â€” Deposit Insurance:** `bfg_fund.collect_premiums()` moves reserves from banks to BFG.
/// * **Step 9 â€” Bank Tax:** If active, `bank_tax.collect_bank_tax()` levies tax on bank assets.
/// * **Step 10 â€” Bank Resolution:** Banks failing reserve requirements after interbank + CB Lombard are resolved via `bank_resolution.execute_bank_resolution()`.
/// * **Step 11 â€” SOBK Contributions:** Voluntary scheme members contribute via `sobk_scheme.accept_contribution()`.
pub fn process_banking_turn(
    country: &mut crate::state::Country,
    companies: &mut [crate::entities::Company],
    current_turn: u32,
) -> BankingTurnResult {
    let mut result = BankingTurnResult::default();
    #[cfg(feature = "diagnostic")]
    let _diag_turn = current_turn;
    #[cfg(feature = "diagnostic")]
    let _diag_cb_inj_start = country.central_bank.liquidity_injected;
    // Phase 94: Snapshot M0 at start of banking turn for per-country leak detection
    #[cfg(feature = "diagnostic")]
    let _diag_m0_start: f64 = {
        let br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let cit: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + cit + corp
    };
    // Phase 94: Per-step M0 leak tracker. Computes (m0 - cb) delta from start.
    #[cfg(feature = "diagnostic")]
    let _diag_m0cb_start = _diag_m0_start - country.central_bank.liquidity_injected;
    #[cfg(feature = "diagnostic")]
    macro_rules! _diag_m0_step {
        ($step:expr) => {{
            let br: f64 = companies.iter()
                .filter(|c| c.bank_type.is_some())
                .filter_map(|c| c.balance_sheet.as_ref())
                .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
                .sum();
            let cit: f64 = country.regions.iter()
                .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
                .map(|d| d.savings)
                .sum::<f64>()
                + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
            let corp: f64 = companies.iter()
                .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
                .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
                .sum::<f64>();
            let m0 = br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + cit + corp;
            let m0cb = m0 - country.central_bank.liquidity_injected;
            let leak = m0cb - _diag_m0cb_start;
            if leak.abs() > 1.0 {
                eprintln!("M0LEAK[{}]: turn={} country={} leak={:.2} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0} cb={:.0}",
                    $step, current_turn, &country.name, leak, br, cit, corp,
                    country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves,
                    country.central_bank.liquidity_injected);
            }
        }};
    }
    // Phase 94: Per-step trace of the targeted bank (BANK-ANA-003) to
    // isolate which banking-turn step creates the M0 leak.
    #[cfg(feature = "diagnostic")]
    let _diag_target_bank = "BANK-ANA-003";
    #[cfg(feature = "diagnostic")]
    let mut _diag_prev_res: f64 = companies.iter()
        .find(|c| c.id == _diag_target_bank)
        .and_then(|c| c.balance_sheet.as_ref())
        .map(|bs| bs.reserves_at_central_bank)
        .unwrap_or(0.0);
    #[cfg(feature = "diagnostic")]
    let mut _diag_prev_dep: f64 = companies.iter()
        .find(|c| c.id == _diag_target_bank)
        .and_then(|c| c.balance_sheet.as_ref())
        .map(|bs| bs.deposits)
        .unwrap_or(0.0);
    #[cfg(feature = "diagnostic")]
    macro_rules! _diag_step_trace {
        ($step:expr) => {{
            let (res, dep, loans, t1, lom, ibl, ibg): (f64, f64, f64, f64, f64, f64, f64) = companies.iter()
                .find(|c| c.id == _diag_target_bank)
                .and_then(|c| c.balance_sheet.as_ref())
                .map(|bs| (
                    bs.reserves_at_central_bank,
                    bs.deposits,
                    bs.loans_issued.iter().map(|l| l.outstanding_balance).sum::<f64>(),
                    bs.tier_1_capital,
                    bs.cb_lombard_loans,
                    bs.interbank_loans_taken.values().sum::<f64>(),
                    bs.interbank_loans_given.values().sum::<f64>(),
                ))
                .unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
            eprintln!("BANKSTEP_DELTA[{}]: turn={} bank={} d_res={:.0} d_dep={:.0} res={:.0} dep={:.0} loans={:.0} t1={:.0} lom={:.0} ibl={:.0} ibg={:.0}",
                $step, current_turn, _diag_target_bank,
                res - _diag_prev_res, dep - _diag_prev_dep,
                res, dep, loans, t1, lom, ibl, ibg);
            _diag_prev_res = res;
            _diag_prev_dep = dep;
            // v4.18 determinism probe: hash cash/disr + bank sheet fields over
            // the whole companies slice — first divergent step names the writer.
            {
                use std::hash::{Hash, Hasher};
                let mut fh = std::collections::hash_map::DefaultHasher::new();
                for c in companies.iter() {
                    c.id.hash(&mut fh);
                    c.available_cash.to_bits().hash(&mut fh);
                    c.brokerage_account
                        .as_ref()
                        .map(|b| b.cash)
                        .unwrap_or(0.0)
                        .to_bits()
                        .hash(&mut fh);
                    c.temporary_disruption_modifier.to_bits().hash(&mut fh);
                    if let Some(ref bs) = c.balance_sheet {
                        bs.reserves_at_central_bank.to_bits().hash(&mut fh);
                        bs.deposits.to_bits().hash(&mut fh);
                        bs.tier_1_capital.to_bits().hash(&mut fh);
                    }
                }
                eprintln!("BKFPC[{}]: turn={} country={} fh={:016x}", $step, current_turn, country.name, fh.finish());
            }
        }};
    }
    #[cfg(feature = "diagnostic")]
    {
        // Phase 94: Trace companies with mismatched bank_type/sector
        for c in companies.iter() {
            let has_bank_type = c.bank_type.is_some();
            let is_banking_sector = c.sector == crate::registries::enums::Sector::Banking;
            if has_bank_type != is_banking_sector {
                let cash = c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash;
                eprintln!("MISMATCH: turn={} country={} id={} bank_type={:?} sector={:?} cash={:.0} primary_bank={:?}", current_turn, country.name, c.id, c.bank_type, c.sector, cash, c.primary_bank_id);
            }
        }
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        // Per-bank balance sheet trace for ALL banks with imbalances
        for bank in companies.iter() {
            if bank.bank_type.is_none() { continue; }
            if let Some(ref bs) = bank.balance_sheet {
                let a = bs.total_assets();
                let l = bs.total_liabilities();
                let e = bs.total_equity();
                let ale = a - l - e;
                if ale.abs() > 100.0 {
                    eprintln!("BANKBS[0_start]: turn={} bank={} A={:.4} L={:.4} E={:.4} A-L-E={:.4} res={:.4} dep={:.4} loans={:.4} ibg={:.4} ibl={:.4} cl={:.4} t1={:.4}",
                        current_turn, bank.id, a, l, e, ale,
                        bs.reserves_at_central_bank, bs.deposits,
                        bs.loans_issued.iter().map(|l| l.outstanding_balance).sum::<f64>(),
                        bs.interbank_loans_given.values().sum::<f64>(),
                        bs.interbank_loans_taken.values().sum::<f64>(),
                        bs.consumer_loans_outstanding, bs.tier_1_capital);
                }
            }
        }
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[0_start]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        // Phase 94: Full fiat trace including ALL walk_global_fiat reservoirs
        {
            let mut ministry_cash = 0.0_f64;
            if let Some(ref config) = country.politics.ministry_config {
                for m in &config.ministries { ministry_cash += m.ministry_cash; }
            }
            ministry_cash += country.ministry_public_service_pool;
            let arb = country.arbitration_court.unclaimed_arbitration_funds;
            let blackops = country.budget.black_ops_budget;
            let intel = country.intelligence_budget.current_budget;
            let mut frozen = 0.0_f64;
            if let Some(ref js) = country.politics.justice_state {
                for &v in js.frozen_company_cash.values() { frozen += v.max(0.0); }
            }
            let full_m0 = total_m0 + ministry_cash + arb + blackops + intel + frozen;
            eprintln!("FULLFIAT_PRE: turn={} country={} full_m0={:.0} bankstep_m0={:.0} diff={:.0} min={:.0} arb={:.0} blackops={:.0} intel={:.0} frozen={:.0}", current_turn, country.name, full_m0, total_m0, full_m0 - total_m0, ministry_cash, arb, blackops, intel, frozen);
        }
        _diag_step_trace!("0_start");
        _diag_m0_step!("0_start");
    }

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

    
// Step 2: Pre-clearing OMO â€” CB adjusts aggregate reserves to steer XIBOR toward target.
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
        let mut total_omo_executed: f64 = 0.0;
        for bank in companies.iter_mut() {
            if let (Some(_), Some(ref mut bs)) = (&bank.bank_type, &mut bank.balance_sheet) {
                let bank_share = bs.securities * proportion;
                if omo_net > 0.0 {
                    // CB buys bonds from banks: bank gives up securities, receives reserves
                    let amount = bank_share.min(bs.securities);
                    bs.securities -= amount;
                    bs.reserves_at_central_bank += amount;
                    total_omo_executed += amount;
                } else {
                    // CB sells bonds to banks: bank receives securities, gives up reserves
                    let amount = (-bank_share).min(bs.reserves_at_central_bank.max(0.0));
                    bs.securities += amount;
                    bs.reserves_at_central_bank -= amount;
                    total_omo_executed -= amount;
                }
            }
        }
        // Phase 94: Track M0 creation/destruction via OMO.
        country.central_bank.liquidity_injected += total_omo_executed;
    }
    
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[2_omo]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("2_omo");
        _diag_m0_step!("2_omo");
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
        .clear_market(&mut bank_refs, &cb_clone, &mut country.sobk_scheme, current_turn);
    result.xibor = country.interbank_market.xibor;
    
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[3_interbank]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("3_interbank");
        _diag_m0_step!("3_interbank");
    }
// Step 4: Deposit Facility â€” banks with surplus reserves park them at CB and earn deposit rate.
    // This creates the physical floor for interbank rates.
    let cb_reserve_ratio = country.central_bank.reserve_requirement_ratio;
    for bank in companies.iter_mut() {
        if let (Some(_), Some(ref mut bs)) = (&bank.bank_type, &mut bank.balance_sheet) {
            let mut position = bs.reserve_position(cb_reserve_ratio);
            if position < 0.0 {
                // Withdraw deposit-facility balances to cover a deficit
                // before resorting to Lombard credit in Step 5 — a bank's
                // own CB deposit is always cheaper than penalty-rate
                // borrowing.
                let withdraw =
                    (-position).min(bs.cb_deposit_facility_balance.max(0.0));
                if withdraw > 0.0 {
                    bs.cb_deposit_facility_balance -= withdraw;
                    bs.reserves_at_central_bank += withdraw;
                    position += withdraw;
                }
            }
            if position > 0.0 {
                // Repay outstanding CB Lombard credit before parking the
                // surplus — ELA draws are intraday liquidity that must
                // extinguish once offsetting credits arrive. Repayment
                // destroys the base money the draw created, keeping the
                // money stock anchored to net structural deficits rather
                // than accumulating gross settlement timing gaps.
                let repay = position.min(bs.cb_lombard_loans.max(0.0));
                if repay > 0.0 {
                    bs.cb_lombard_loans -= repay;
                    bs.reserves_at_central_bank -= repay;
                    country.central_bank.liquidity_injected =
                        (country.central_bank.liquidity_injected - repay).max(0.0);
                    position -= repay;
                }
                // The Lombard penalty rate exceeds the deposit-facility
                // rate, so residual Lombard debt is also repaid from the
                // depfac balance — carrying both sides simultaneously is a
                // pure spread loss no solvent bank would hold.
                let depfac_repay = bs
                    .cb_lombard_loans
                    .max(0.0)
                    .min(bs.cb_deposit_facility_balance.max(0.0));
                if depfac_repay > 0.0 {
                    bs.cb_lombard_loans -= depfac_repay;
                    bs.cb_deposit_facility_balance -= depfac_repay;
                    country.central_bank.liquidity_injected =
                        (country.central_bank.liquidity_injected - depfac_repay).max(0.0);
                }
                if position > 0.0 {
                    // Move remaining surplus to deposit facility (earns interest)
                    bs.cb_deposit_facility_balance += position;
                    bs.reserves_at_central_bank -= position;
                }
            }
            // Accrue interest on existing deposit facility balance
            let interest = country
                .central_bank
                .accrue_deposit_facility_interest(bs.cb_deposit_facility_balance);
            bs.reserves_at_central_bank += interest;
            // Phase 94: Credit interest income to equity (double-entry:
            // asset increases, equity must increase to maintain A = L + E).
            bs.tier_1_capital += interest;
            result.deposit_facility_interest_paid += interest;
            // Phase 94: Track M0 creation â€” CB pays interest from void, creating base money.
            country.central_bank.liquidity_injected += interest;
            result.total_deposit_facility_balance += bs.cb_deposit_facility_balance;
        }
    }

    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[4_depfac]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("4_depfac");
        _diag_m0_step!("4_depfac");
    }
    
// Step 5: Lombard Facility â€” banks still in deficit after interbank borrow from CB at penalty rate.
    // This creates the physical ceiling for interbank rates.
    for bank in companies.iter_mut() {
        if let (Some(_), Some(ref mut bs)) = (&bank.bank_type, &mut bank.balance_sheet) {
            let position = bs.reserve_position(cb_reserve_ratio);
            if position < 0.0 {
                // Bank is still in deficit â€” borrow from Lombard facility
                let needed = -position;
                bs.cb_lombard_loans += needed;
                bs.reserves_at_central_bank += needed;
                // Phase 94: Track M0 creation â€” CB lends new base money to banks.
                country.central_bank.liquidity_injected += needed;
            }
            // Accrue interest on existing Lombard loans (paid by bank to CB)
            let interest = country
                .central_bank
                .accrue_lombard_facility_interest(bs.cb_lombard_loans);
            // Phase 95: Hard reserve floor — ELA covers the interest debit so
            // reserves never settle below zero (draw booked as cb_lombard_loans).
            ela_cover_debit(bs, &mut country.central_bank, interest);
            bs.reserves_at_central_bank -= interest;
            // Phase 94: No reserve clamping â€” negative reserves represent
            // CB Lombard borrowing. Clamping breaks A=L+E and causes M0
            // FiatCreation.
            // Phase 94: Debit interest expense from equity (double-entry:
            // asset decreases, equity must decrease to maintain A = L + E).
            bs.tier_1_capital -= interest;
            result.lombard_facility_interest_received += interest;
            // Phase 94: Interest paid to CB destroys base money â€” track it.
            country.central_bank.liquidity_injected -= interest;
            result.total_lombard_loans += bs.cb_lombard_loans;
        }
    }
    
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[5_lombard]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("5_lombard");
        _diag_m0_step!("5_lombard");
    }
// Step 6: Loan Repayment
    // Process interest accrual and repayments for existing loans
    // Phase 24A.2: Fix Black Hole #2 â€” borrowers must be debited when loans
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
                // v4.20 WS3: clamp the ACCRUAL itself at ≥ 0 — new
                // origination and variable repricing are floored, but a
                // fixed-rate loan booked in an earlier turn at a negative
                // rate would otherwise keep accruing negative interest
                // income forever. Nominal interest income can never be
                // negative.
                let per_turn_rate = annual_to_per_turn_rate(loan.interest_rate);
                let interest = (loan.outstanding_balance * per_turn_rate).max(0.0);
                loan.outstanding_balance += interest;

                // Update variable rate loans
                if loan.interest_type == InterestType::Variable {
                    // v4.20 WS3: floor at MIN_LOAN_RATE — variable-rate
                    // repricing through a negative XIBOR was the direct
                    // source of NEGATIVE interest income (outstanding
                    // balance × negative rate) shredding bank equity.
                    loan.interest_rate = (xibor + loan.bank_margin).max(MIN_LOAN_RATE);
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
            // v4.17: drop fully-repaid loans from the book — they carry
            // outstanding_balance=0 (no asset contribution), can never emit
            // another lifecycle event, and accumulate ~one OD line per
            // shortfall-store per turn forever (unbounded Vec growth).
            // Borrower-side LoanRefs keep their own Repaid marker.
            bs.loans_issued
                .retain(|l| l.status != LoanStatus::Repaid);
            // Phase 94: Interest income accounting.
            // The interest accrual (line 2438) increased the loan asset by
            // `interest`. The repayment reduces the loan by `actual_payment`
            // (which includes `interest`). Net loan change from accrual + repay
            // = interest - actual_payment = -(actual_payment - interest) =
            // -actual_principal. The borrower's deposit is debited by the full
            // actual_payment in the borrower debit loop. So:
            //   Assets: -actual_principal (net loan change)
            //   Liabilities: -actual_payment (deposits)
            //   Equity: 0
            //   Check: -actual_principal = -actual_payment + 0
            //   => -actual_principal = -(actual_principal + actual_interest)  -- WRONG!
            //
            // The fix: credit equity for interest_income to balance:
            //   Assets: -actual_principal
            //   Liabilities: -actual_payment
            //   Equity: +actual_interest
            //   Check: -actual_principal = -actual_payment + actual_interest
            //   => -actual_principal = -actual_principal - actual_interest + actual_interest  âś“
            bs.tier_1_capital += interest_income_total;
            // Phase 39: Credit operating cash for teller payroll.
            // Phase 94: Operating cash is off-balance-sheet but M0 base money.
            // The credit is backed by interest income (already in tier_1_capital)
            // and principal repayment (already in reserves from borrower debit).
            // Debiting reserves here would break A=L+E since operating cash is
            // off-balance-sheet. The M0 walk includes operating cash separately.
            // Phase 40: 10% of principal repayment also goes to operating cash.
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
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[6_loanrepay]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("6_loanrepay");
        _diag_m0_step!("6_loanrepay");
    }
    // Phase 24A.2: Execute borrower debits
    // Three cases: (A) company borrower, (B) state/treasury borrower, (C) vanished borrower.
    // Phase 94: Fixed double-entry â€” reserves are now adjusted per-loan here,
    // NOT in the loan loop. For intra-bank, deposits are destroyed (no reserve
    // movement). For inter-bank, the lending bank receives reserves from the
    // borrower's bank.
    const STATE_BORROWER_ID: &str = "STATE";
    for (borrower_id, loan_id, bank_idx, amount, _principal_portion) in pending_loan_debits {
        if borrower_id == STATE_BORROWER_ID {
            // CASE B: State/Treasury borrower â€” debit liquid_reserves, never Default.
            // The lending bank receives reserves from the treasury.
            // Phase 94: No .max(0.0) clamping â€” if treasury can't pay the full
            // amount, the lending bank still gets credited, creating an
            // overdraft that will be reconciled later. Clamping breaks A=L+E.
            country.budget.liquid_reserves -= amount;
            if let Some(ref mut bs) = companies[bank_idx].balance_sheet {
                bs.reserves_at_central_bank += amount;
            }
        } else if let Some(borrower_idx) = companies.iter().position(|c| c.id == borrower_id) {
            // CASE A: Company borrower â€” debit cash and sync borrower's bank.
            let payer_bank_id = companies[borrower_idx].primary_bank_id.clone();
            let lending_bank_id = companies[bank_idx].id.clone();

            // Debit borrower's cash (brokerage first, then available_cash)
            // Phase 94: No .max(0.0) clamping â€” negative available_cash
            // represents an overdraft that must be resolved via lending or
            // bankruptcy. Clamping breaks double-entry because the bank's
            // deposit is debited by the full amount regardless.
            // 24-turn fix: bounded collection — scheduled amortization used
            // to debit the FULL amount unconditionally, overdrafting solvent
            // borrowers to negative cash before payroll ran (manufacturing
            // wage arrears). Collect only cash held net of the payroll
            // obligation; the uncollected portion stays on the loan and the
            // bank is credited only what it received — double-entry exact.
            let spendable = companies[borrower_idx]
                .brokerage_account
                .as_ref()
                .map(|ba| ba.cash.max(0.0))
                .unwrap_or(0.0)
                + companies[borrower_idx].available_cash.max(0.0);
            let payroll_reserve = companies[borrower_idx].wages_paid_this_turn
                + companies[borrower_idx].wage_arrears
                + companies[borrower_idx].severance_arrears;
            let collected = amount.min((spendable - payroll_reserve).max(0.0));

            let mut remaining = collected;
            if let Some(ref mut ba) = companies[borrower_idx].brokerage_account {
                let debit = remaining.min(ba.cash.max(0.0));
                ba.cash -= debit;
                remaining -= debit;
            }
            if remaining > 0.0 {
                companies[borrower_idx].available_cash -= remaining;
            }

            // Restore the uncollected portion onto the loan (the accrual
            // loop already reduced outstanding_balance and ticked the term),
            // and reverse the uncollected interest income on the bank's equity.
            let uncollected = amount - collected;
            if uncollected > 0.0 {
                if let Some(ref mut bs) = companies[bank_idx].balance_sheet {
                    if let Some(loan) =
                        bs.loans_issued.iter_mut().find(|l| l.id == loan_id)
                    {
                        // Only the unpaid PRINCIPAL share returns to the
                        // balance — the accrual loop reduced outstanding by
                        // `_principal_portion`, not by `amount`.
                        loan.outstanding_balance +=
                            _principal_portion * (uncollected / amount);
                        loan.turns_remaining = loan.turns_remaining.saturating_add(1);
                    }
                    let interest_portion = (amount - _principal_portion).max(0.0);
                    if amount > 0.0 && interest_portion > 0.0 {
                        bs.tier_1_capital -= interest_portion * (uncollected / amount);
                    }
                }
            }
            let amount = collected;
            if amount <= 0.0 {
                continue;
            }

            // Sync borrower's bank balance sheet (double-entry)
            if let Some(ref p_bank_id) = payer_bank_id {
                if p_bank_id == &lending_bank_id {
                    // Intra-bank: deposit is destroyed, loan asset was already
                    // reduced in the loan loop. No reserve movement â€” the
                    // deposit and loan cancel out within the same bank.
                    if let Some(ref mut bs) = companies[bank_idx].balance_sheet {
                        settle_deposit_debit(bs, amount);
                    }
                } else {
                    // Inter-bank: borrower's bank loses deposits + reserves,
                    // lending bank receives reserves.
                    // Phase 94: No .max(0.0) clamping â€” negative reserves
                    // represent CB Lombard borrowing. Clamping breaks A=L+E.
                    if let Some(bank) = companies.iter_mut().find(|c| c.id == p_bank_id.as_str()) {
                        if let Some(ref mut bs) = bank.balance_sheet {
                            settle_deposit_debit(bs, amount);
                            bs.reserves_at_central_bank -= amount;
                        }
                    }
                    if let Some(ref mut bs) = companies[bank_idx].balance_sheet {
                        bs.reserves_at_central_bank += amount;
                    }
                }
            } else {
                // Phase 94: Unbanked borrower (no primary_bank_id). The
                // borrower's cash (brokerage_account.cash and/or
                // available_cash) is M0 base money. The lending bank's
                // reserves must be credited by the full repayment amount to
                // keep M0 neutral (Directive 1). Without this, M0 is
                // destroyed because borrower cash decreases but bank
                // reserves don't increase.
                if let Some(ref mut bs) = companies[bank_idx].balance_sheet {
                    bs.reserves_at_central_bank += amount;
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
            // CASE C: Borrower vanished â€” mark loan as Default (cleaned in bankruptcy)
            if let Some(bank) = companies.get_mut(bank_idx) {
                if let Some(ref mut bs) = bank.balance_sheet {
                    for loan in &mut bs.loans_issued {
                        // The queued repayment is fictitious for a vanished
                        // borrower: the loan loop already reduced
                        // `outstanding_balance` by `amount` and credited
                        // `tier_1_capital` by the interest component, but no
                        // deposit/cash debit can run. Restore the deducted
                        // balance so the accrued-interest asset remains,
                        // matching the equity credit (keeps A = L + E),
                        // then mark Default for the bankruptcy waterfall.
                        if loan.id == loan_id {
                            loan.outstanding_balance += amount;
                            loan.status = LoanStatus::Default;
                        } else if loan.borrower_id == borrower_id
                            && loan.status != LoanStatus::Repaid
                        {
                            loan.status = LoanStatus::Default;
                        }
                    }
                }
            }
        }
    }
    // v4.17: drop repaid borrower-side LoanRefs — every OD draw leaves a
    // permanent entry otherwise (~6k/turn runaway state growth). Repaid refs
    // carry outstanding_balance=0 so removal doesn't change `liabilities`
    // (recomputed above from remaining entries) or any payable.
    for c in companies.iter_mut() {
        if !c.outstanding_loans.is_empty() {
            c.outstanding_loans
                .retain(|l| l.status != LoanStatus::Repaid);
        }
    }
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[6b_borrower_debits]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("6b_borrower_debits");
        _diag_m0_step!("6b_borrower_debits");
    }
    
// Step 7: New Loan Issuance
    // Non-bank companies seek working capital loans.
    // Phase 77: Competitive allocation â€” banks with the most excess reserves
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
        // v4.20 WS3b: depfac counts as lendable liquidity (same-day
        // callable, Step 4 already converts it at par to cure deficits) —
        // without it every bank's "excess" collapses to ~0 after the
        // facility sweep and the whole book sits idle at the CB.
        let effective_reserves = bs.reserves_at_central_bank
            + bs.cb_deposit_facility_balance
            - bs.cb_lombard_loans;
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

        // Working-capital need ≈ payroll + input restock for ~3 turns. At
        // ~$450+/FTE wages plus materials the seat×$1000 rule covered less
        // than one payroll — borrowed liquidity evaporated before it could
        // finance a production cycle. ×$2500 is still conservative.
        let needed = (companies[borrower_idx].worker_capacity as f64 * 2500.0)
            - companies[borrower_idx].available_cash;
        if needed <= 0.0 {
            continue;
        }

        let principal = needed.min(5_000_000.0); // Cap per-loan
        let borrower_clone = companies[borrower_idx].clone();
        let borrower_bank_id = borrower_clone.primary_bank_id.clone();
        let reserve_ratio = cb_for_loans.reserve_requirement_ratio;

        // v4.20 WS3: lend through the borrower's OWN bank first. A loan to
        // an in-house customer mints its deposit on THIS bank's book (the
        // `creates_deposit` gate inside `issue_loan`), so it only consumes
        // `rr × P` of reserve cover. Cross-bank and unbanked lending ships
        // the reserves out with the proceeds and legitimately needs the
        // full principal. The old blanket `excess < principal` pre-filter
        // required 100% reserve cover for EVERY loan — that is the classic
        // full-reserve model, not fractional-reserve credit creation, and
        // it is what collapsed operational LDR to the low single digits:
        // banks could only lend up to their excess RESERVES (~rr% of
        // deposits) rather than converting deposits into performing loans
        // up to the regulatory `1 − rr` cap.
        let mut bank_order: Vec<usize> = (0..bank_info.len()).collect();
        if let Some(ref pb) = borrower_bank_id {
            if let Some(pos) = bank_info
                .iter()
                .position(|(bi, ..)| companies[*bi].id == *pb)
            {
                bank_order.remove(pos);
                bank_order.insert(0, pos);
            }
        }
        for &ei in &bank_order {
            let entry = &mut bank_info[ei];
            let (bi, margin, excess, new_loans_turn) = (entry.0, entry.1, entry.2, entry.3);
            // Check the reserve cover THIS issuance path actually consumes:
            // deposit-creating (in-house) loans need `rr × P`; reserve-
            // funded (cross-bank/unbanked) loans need the full principal.
            let creates_deposit_here =
                borrower_bank_id.as_deref() == Some(companies[bi].id.as_str());
            let required_cover = if creates_deposit_here {
                principal * reserve_ratio
            } else {
                principal
            };
            if excess < required_cover {
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

            // W3/R1: For unbanked borrowers the created deposit is reversed
            // after issue_loan returns (reserve-funded cash loan), so the
            // effective post-state is loans+P ≤ deposits×(1−rr) — stricter
            // than the in-function check which credits the new deposit.
            // Enforce it here so issuance never breaches the cap.
            if borrower_clone.primary_bank_id.is_none() {
                let headroom = companies[bi]
                    .balance_sheet
                    .as_ref()
                    .map(|bs| bs.ldr_headroom(cb_for_loans.reserve_requirement_ratio))
                    .unwrap_or(0.0);
                if principal > headroom {
                    continue;
                }
            }

            let loan_result = issue_loan(
                companies[bi].balance_sheet.as_mut().unwrap(),
                &companies[bi].id,
                margin,
                &borrower_clone,
                &borrower_clone.id,
                borrower_bank_id.as_deref(),
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
                // Phase 94: Credit the borrower's bank deposit (brokerage_account.cash)
                // OR available_cash as fallback â€” NOT both. The loan creates a deposit
                // at the bank, which is the borrower's asset. Double-counting both
                // fields creates money from nothing.
                let _had_ba = companies[borrower_idx].brokerage_account.is_some();
                let _was_unbanked = companies[borrower_idx].primary_bank_id.is_none();
                if let Some(ref mut ba) = companies[borrower_idx].brokerage_account {
                    ba.cash += lr.principal_amount;
                } else {
                    companies[borrower_idx].available_cash += lr.principal_amount;
                }
                #[cfg(feature = "diagnostic")]
                eprintln!("LOANISSUE: turn={} borrower={} bank={} principal={:.2} had_ba={} unbanked={}", current_turn, companies[borrower_idx].id, companies[bi].id, lr.principal_amount, _had_ba, _was_unbanked);
                // Phase 94: For unbanked borrowers (no primary_bank_id), the
                // credited cash (brokerage_account.cash OR available_cash) is
                // M0 base money. The deposit created by issue_loan is NOT in
                // M0. Reverse the deposit creation and debit reserves to keep
                // M0 neutral (Directive 1). This must apply regardless of
                // whether the borrower has a brokerage_account â€” both
                // brokerage_account.cash and available_cash are M0 for
                // unbanked companies (see walk_global_fiat).
                // 24-turn: Reconcile where the created deposit lives.
                // issue_loan books `deposits += P` at the LENDER, but the
                // borrower's cash spends through `primary_bank_id`. For a
                // borrower banked elsewhere the deposit must move to the
                // borrower's bank (funded by a reserve transfer) — otherwise
                // every payment the borrower makes debits deposits at a bank
                // that never received the credit, driving them negative.
                let lender_id = companies[bi].id.clone();
                match companies[borrower_idx].primary_bank_id.clone() {
                    None => {
                        if let Some(ref mut bs) = companies[bi].balance_sheet {
                            settle_deposit_debit(bs, lr.principal_amount);
                            bs.reserves_at_central_bank -= lr.principal_amount;
                        }
                    }
                    Some(pb) if pb == lender_id => {}
                    Some(pb) => {
                        // Inter-bank: mirror the repayment path — the lending
                        // bank ships reserves to the borrower's bank, which
                        // books the new deposit liability instead.
                        if let Some(ref mut bs) = companies[bi].balance_sheet {
                            settle_deposit_debit(bs, lr.principal_amount);
                            bs.reserves_at_central_bank -= lr.principal_amount;
                        }
                        if let Some(bank) = companies.iter_mut().find(|c| c.id == pb) {
                            if let Some(ref mut bs) = bank.balance_sheet {
                                bs.deposits += lr.principal_amount;
                                bs.reserves_at_central_bank += lr.principal_amount;
                            }
                        }
                    }
                }
                result.total_new_credit += lr.principal_amount;
                // Update this bank's tracking: excess reserves fall by the
                // cover this path actually consumed — `rr × P` of new
                // required reserves for deposit-creating (in-house) loans,
                // the full reserve outflow for cross-bank/unbanked ones.
                entry.2 -= if creates_deposit_here {
                    lr.principal_amount * reserve_ratio
                } else {
                    lr.principal_amount
                };
                entry.3 += lr.principal_amount;
                break; // Loan issued, move to next borrower
            }
        }
    }

    
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[7_newloans]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("7_newloans");
        _diag_m0_step!("7_newloans");
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
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[8_bfg]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("8_bfg");
        _diag_m0_step!("8_bfg");
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
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[9_tax]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("9_tax");
        _diag_m0_step!("9_tax");
    }

    
// Step 10: Bank Resolution
    // Check for banks that still fail reserve requirements after interbank + Lombard
    let mut failed_banks: Vec<String> = Vec::new();
    for bank in companies.iter() {
        if let (Some(_), Some(ref bs)) = (&bank.bank_type, &bank.balance_sheet) {
            // 24-turn fix: `reserves_at_central_bank < 0.0` was unreachable —
            // Phase-95 ELA keeps reserves ≥ 0 every turn, so insolvent banks
            // (negative equity, unbounded leverage) could never be resolved
            // and accrued Sev-10 findings forever. Trigger resolution on
            // insolvency (equity wiped) or chronic Lombard dependence.
            let insolvent = bs.tier_1_capital <= 0.0;
            let liquidity_failed = !bs.meets_reserve_requirement(cb_reserve_ratio)
                && bs.cb_lombard_loans > 0.0
                && bs.reserves_at_central_bank < 0.0;
            if insolvent || liquidity_failed {
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

    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[10_resolution]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("10_resolution");
        _diag_m0_step!("10_resolution");
    }
    
// Step 11: SOBK Contributions (voluntary)
    for bank in companies.iter_mut() {
        if bank.bank_type.is_some() && bank.balance_sheet.is_some() {
            country.sobk_scheme.accept_contribution(bank);
        }
    }
    // SOBK repays CB emergency loan from pool surplus (F7 fix â€” complete
    // lifecycle for cb_emergency_loan). The pool must retain enough to honor
    // outstanding loan recalls, so only surplus above that buffer is used.
    // This mirrors the BFG repayment pattern (Step 8 above) and contracts M0
    // by extinguishing the CB loan.
    let sobk_outstanding: f64 = country.sobk_scheme.outstanding_loans.values().sum();
    let sobk_pool = country.sobk_scheme.pool;
    let sobk_cb_debt = country.sobk_scheme.cb_emergency_loan;
    if sobk_cb_debt > 0.0 && sobk_pool > sobk_outstanding {
        let repayment = (sobk_pool - sobk_outstanding).min(sobk_cb_debt);
        country
            .sobk_scheme
            .repay_cb_liquidity_line(&mut country.central_bank, repayment);
    }
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[11_sobk]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("11_sobk");
        _diag_m0_step!("11_sobk");
    }
    
// Step 12 (Phase 35 / Phase 77): B2B Micro-Loans â€” banks issue small
    // working-capital loans to non-bank companies that have insufficient
    // brokerage cash for operations. This creates actual banking activity and
    // loan interest revenue.
    // Phase 40: Reserve payroll cash BEFORE lending so banks can pay tellers.
    // Phase 77: Route through issue_loan() to enforce fractional reserve
    // requirements. Previously this pushed loans directly to bs.loans_issued
    // WITHOUT checking reserves â€” a rogue money-creation path.
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
        // R3.2: Cooperative banks offer member-preference lending.
        // Members are companies with LegalForm::Cooperative in the same region.
        // Members get the lower cooperative margin (already set in loan_margin),
        // non-members get the commercial margin (0.02).
        let is_cooperative_bank = companies[bank_idx].bank_type == Some(BankType::Cooperative);
        let commercial_margin = 0.02_f64;
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
            // R3.2: Determine if borrower is a cooperative member.
            let is_cooperative_member = matches!(
                companies[borrower_idx].legal_form,
                crate::entities::LegalForm::Cooperative(_)
            );
            // R3.2: Cooperative banks apply member margin to members,
            // commercial margin to non-members. Non-cooperative banks
            // always use their own margin.
            let effective_margin = if is_cooperative_bank {
                if is_cooperative_member {
                    bank_margin // 0.01 for cooperative banks
                } else {
                    commercial_margin // 0.02 for non-members
                }
            } else {
                bank_margin
            };
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
            // W3/R1: For unbanked borrowers the created deposit is reversed
            // after issue_loan returns (reserve-funded cash loan), so the
            // effective post-state is loans+P ≤ deposits×(1−rr) — stricter
            // than the in-function check which credits the new deposit.
            if companies[borrower_idx].primary_bank_id.is_none() {
                let headroom = companies[bank_idx]
                    .balance_sheet
                    .as_ref()
                    .map(|bs| bs.ldr_headroom(
                        country.central_bank.reserve_requirement_ratio,
                    ))
                    .unwrap_or(0.0);
                if loan_amount > headroom {
                    continue;
                }
            }
            // Phase 77: Route through issue_loan() for reserve check
            let borrower_clone = companies[borrower_idx].clone();
            let borrower_bank_id = borrower_clone.primary_bank_id.clone();
            let loan_result = issue_loan(
                companies[bank_idx].balance_sheet.as_mut().unwrap(),
                &bank_id,
                effective_margin,
                &borrower_clone,
                &borrower_clone.id,
                borrower_bank_id.as_deref(),
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
                // Phase 94: For unbanked borrowers (no primary_bank_id), the
                // credited cash (brokerage_account.cash OR available_cash) is
                // M0 base money. The deposit created by issue_loan is NOT in
                // M0. Reverse the deposit creation and debit reserves to keep
                // M0 neutral (Directive 1). This must apply regardless of
                // whether the borrower has a brokerage_account.
                match companies[borrower_idx].primary_bank_id.clone() {
                    None => {
                        if let Some(ref mut bs) = companies[bank_idx].balance_sheet {
                            settle_deposit_debit(bs, lr.principal_amount);
                            bs.draw_cb_liquidity(lr.principal_amount);
                        }
                    }
                    Some(pb) if pb == bank_id => {}
                    Some(pb) => {
                        // Inter-bank: move the created deposit to the
                        // borrower's bank, funded by a reserve transfer —
                        // drawn from the deposit facility first so the
                        // settlement account isn't drained spuriously.
                        if let Some(ref mut bs) = companies[bank_idx].balance_sheet {
                            settle_deposit_debit(bs, lr.principal_amount);
                            bs.draw_cb_liquidity(lr.principal_amount);
                        }
                        if let Some(bank) = companies.iter_mut().find(|c| c.id == pb) {
                            if let Some(ref mut bs) = bank.balance_sheet {
                                bs.deposits += lr.principal_amount;
                                bs.reserves_at_central_bank += lr.principal_amount;
                            }
                        }
                    }
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
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[12_microloans]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("12_microloans");
        _diag_m0_step!("12_microloans");
    }

    
// Step 13 (Phase 35): B2C Consumer Loans â€” banks issue small consumer
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
            // v4.20 WS3: floor the interest leg at ≥ 0 — a consumer loan
            // booked while administered rates were negative would
            // otherwise pay the borrower to borrow (negative
            // `interest_payment` → `repayment_interest` < 0 →
            // `tier_1_capital` shredded, and a total_payment BELOW the
            // principal portion — a permanent interest subsidy).
            let principal_payment = loan.outstanding_principal / 24.0;
            let per_turn_rate = annual_to_per_turn_rate(loan.interest_rate);
            let interest_payment = (loan.outstanding_principal * per_turn_rate).max(0.0);
            loans_to_process.push((i, principal_payment, interest_payment));
        }
        // Find the region and class for each loan and process repayment
        for (i, principal_pay, interest_pay) in loans_to_process {
            let loan = &mut bank.consumer_loans[i];
            let region = country.regions.iter_mut().find(|r| r.id == loan.region_id);
            if let Some(region) = region {
                let class = if loan.is_rural {
                    RuralClass::from_str(&loan.class_key).and_then(|k| {
                        region
                            .class_demographics
                            .rural_classes
                            .get_mut(&k)
                    })
                } else {
                    UrbanClass::from_str(&loan.class_key).and_then(|k| {
                        region
                            .class_demographics
                            .urban_classes
                            .get_mut(&k)
                    })
                };
                if let Some(class) = class {
                    let total_payment = principal_pay + interest_pay;
                    if class.savings >= total_payment {
                        class.savings -= total_payment;
                        class.debt = (class.debt - principal_pay).max(0.0);
                        loan.outstanding_principal =
                            (loan.outstanding_principal - principal_pay).max(0.0);
                        repayment_principal += principal_pay;
                        repayment_interest += interest_pay;
                    } else if class.savings > 0.0 {
                        // Partial payment
                        let partial = class.savings;
                        let principal_portion = partial * (principal_pay / total_payment);
                        let interest_portion = partial - principal_portion;
                        class.savings = 0.0;
                        class.debt = (class.debt - principal_portion).max(0.0);
                        loan.outstanding_principal =
                            (loan.outstanding_principal - principal_portion).max(0.0);
                        repayment_principal += principal_portion;
                        repayment_interest += interest_portion;
                    }
                }
            }
        }
        // Credit repayment to bank reserves (double-entry).
        // Phase 94: Citizen savings (M0) decreased by (principal + interest).
        // Bank reserves (M0) must increase by the same amount to keep M0
        // conserved (Directive 1). Previously only interest was credited
        // to reserves (and double-counted in brokerage cash), destroying
        // the principal portion from M0 and breaking A=L+E. Now credit
        // reserves for the full payment and equity for interest income.
        if repayment_principal + repayment_interest > 0.0 {
            if let Some(bs) = &mut bank.balance_sheet {
                bs.reserves_at_central_bank += repayment_principal + repayment_interest;
                bs.tier_1_capital += repayment_interest;
                bs.consumer_loans_outstanding -= repayment_principal;
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
        // W3/R1: Consumer loans create a credit asset with NO deposit leg
        // (citizen savings are M0 cash, reserves leave the bank). The full
        // new principal therefore consumes LDR headroom — cap consumer
        // issuance by `deposits×(1−rr) − total_loans`. Banks above the cap
        // (headroom = 0) stop issuing and let the book amortize down.
        let ldr_headroom = bank
            .balance_sheet
            .as_ref()
            .map(|bs| bs.ldr_headroom(country.central_bank.reserve_requirement_ratio))
            .unwrap_or(0.0);
        let max_consumer_credit = max_consumer_credit.min(ldr_headroom);
        // v4.20 WS3: Consumer disbursements are cash leaving the reserve
        // account — the origination budget must also fit inside excess
        // reserves (own reserves net of Lombard, minus required cover).
        // v4.20 WS3b: the deposit-facility balance is same-day callable
        // central-bank money, so it counts toward the disbursement budget —
        // without it Step 4's sweep leaves every bank "reserve-poor" and
        // household credit stops dead (avg LDR pinned near zero).
        let excess_reserves = bank
            .balance_sheet
            .as_ref()
            .map(|bs| {
                let required = bs.deposits * country.central_bank.reserve_requirement_ratio;
                (bs.reserves_at_central_bank + bs.cb_deposit_facility_balance
                    - bs.cb_lombard_loans - required).max(0.0)
            })
            .unwrap_or(0.0);
        let max_consumer_credit = max_consumer_credit.min(excess_reserves);
        if max_consumer_credit < 500.0 {
            continue;
        }
        // v4.20 WS3: household credit demand is priced ONCE per bank-turn.
        // `consumer_rate` = CB reference + a 5pp retail margin, floored at
        // MIN_CONSUMER_RATE so B2C interest income can never go negative.
        // `expected_inflation_annual` feeds the real-rate channel in
        // `consumer_credit_demand` — cheap real credit stimulates housing/
        // durable borrowing; expensive real credit depresses it.
        let consumer_rate = (cb_ref_rate + 0.05).max(MIN_CONSUMER_RATE);
        let expected_inflation_annual = expected_cpi_inflation_pct(
            &country.macro_indicators.telemetry_history,
            country.macro_indicators.inflation,
        ) / 100.0
            * crate::state::macro_data::TURNS_PER_YEAR as f64;
        let avg_wage = country.macro_indicators.average_wage;
        let mut issued_total = 0.0;
        for region in &mut country.regions {
            if !bank_region.is_empty() && region.id != bank_region {
                continue;
            }
            // Issue to rural classes
            let rural_keys: Vec<RuralClass> = region
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
                // v4.20 WS3: demand = f(income, real rate, unmet durable
                // stock) — replaces the subsistence-only gates that killed
                // household credit (`debt > 0` blacklist + "too rich to
                // borrow" savings test). A class keeps borrowing toward its
                // desired leverage as long as income supports it.
                let demand = consumer_credit_demand(
                    class,
                    key.as_str(),
                    avg_wage,
                    consumer_rate,
                    expected_inflation_annual,
                );
                let loan_amount = demand
                    .min(max_consumer_credit - issued_total)
                    .min(CONSUMER_MAX_LOAN_PER_CLASS_TURN);
                if loan_amount < 100.0 {
                    continue;
                }
                let rate = consumer_rate;
                class.savings += loan_amount;
                class.debt += loan_amount;
                bank.consumer_loans.push(ConsumerLoan {
                    region_id: region.id.clone(),
                    class_key: key.to_string(),
                    is_rural: true,
                    outstanding_principal: loan_amount,
                    interest_rate: rate,
                    issued_turn: current_turn,
                    original_principal: loan_amount,
                });
                // Phase 94: Debit bank reserves and track consumer loan asset
                // (double-entry). Consumer loans credit citizen savings (M0
                // physical cash). Bank reserves must decrease by the same
                // amount (cash withdrawal) to keep M0 conserved (Directive 1).
                // The consumer loan is a bank asset tracked via
                // consumer_loans_outstanding to maintain A=L+E.
                if let Some(ref mut bs) = bank.balance_sheet {
                    bs.draw_cb_liquidity(loan_amount);
                    bs.consumer_loans_outstanding += loan_amount;
                }
                issued_total += loan_amount;
            }
            if issued_total >= max_consumer_credit {
                break;
            }
            // Issue to urban classes
            let urban_keys: Vec<UrbanClass> = region
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
                let demand = consumer_credit_demand(
                    class,
                    key.as_str(),
                    avg_wage,
                    consumer_rate,
                    expected_inflation_annual,
                );
                let loan_amount = demand
                    .min(max_consumer_credit - issued_total)
                    .min(CONSUMER_MAX_LOAN_PER_CLASS_TURN);
                if loan_amount < 100.0 {
                    continue;
                }
                let rate = consumer_rate;
                class.savings += loan_amount;
                class.debt += loan_amount;
                bank.consumer_loans.push(ConsumerLoan {
                    region_id: region.id.clone(),
                    class_key: key.to_string(),
                    is_rural: false,
                    outstanding_principal: loan_amount,
                    interest_rate: rate,
                    issued_turn: current_turn,
                    original_principal: loan_amount,
                });
                // Phase 94: Debit bank reserves and track consumer loan asset
                // (double-entry, see rural above).
                if let Some(ref mut bs) = bank.balance_sheet {
                    bs.draw_cb_liquidity(loan_amount);
                    bs.consumer_loans_outstanding += loan_amount;
                }
                issued_total += loan_amount;
            }
        }
        result.total_new_credit += issued_total;
    }
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[13_consumerloans]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("13_consumerloans");
        _diag_m0_step!("13_consumerloans");
    }

    
// Step 14 (Phase 35): QE for Deflation â€” if CPI inflation < 0%, the
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
    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        eprintln!("BANKSTEP3[14_qe]: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves);
        _diag_step_trace!("14_qe");
        _diag_m0_step!("14_qe");
    }

    
// Step 14b (v4.18 WS3): Deposit service fees — the real revenue leg for a
    // deposit-holding bank whose loan book is still thin. Payment/account
    // services are how real banks monetize the deposit book while the asset
    // side is building; without ANY deposit-linked income a bank's payroll
    // pocket stays empty (the `operating_cash_credit` above only feeds off
    // loan interest), so thin-book banks can never staff up — and staff is
    // exactly what `bank_operational_capacity` multiplies into lending
    // capacity. That circularity is the observed ~1% commercial LDR stall.
    //
    // Double-entry (closed loop, same convention as the interest-income
    // path at Step 6):
    //   customer: spendable cash −f      (their deposit balance is debited)
    //   bank BS : deposits −f            (liability extinguished)
    //             tier_1_capital +f      (fee income booked to equity → A=L+E)
    //   bank    : operating pocket +f    (off-BS M0 pocket — the wage fund;
    //             M0 walk conserves because the customer's pocket loses f)
    // The fee is bounded by both the customer's positive spendable balance
    // and the bank's remaining deposit liability, so deposits can never be
    // driven negative.
    {
        const DEPOSIT_SERVICE_FEE_RATE: f64 = 0.0001; // 0.01% of balance per turn
        // Remaining debitable deposit liability per bank (fees can only
        // extinguish what the balance sheet says the bank owes customers).
        let mut remaining: HashMap<String, f64> = companies
            .iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| {
                c.balance_sheet
                    .as_ref()
                    .map(|bs| (c.id.clone(), bs.deposits.max(0.0)))
            })
            .collect();
        let mut collected: HashMap<String, f64> = HashMap::new();
        for customer in companies.iter_mut() {
            if customer.bank_type.is_some() {
                continue;
            }
            let Some(ref bank_id) = customer.primary_bank_id else {
                continue; // unbanked cash holders pay no deposit fees
            };
            let Some(rem) = remaining.get_mut(bank_id) else {
                continue;
            };
            let balance = customer
                .brokerage_account
                .as_ref()
                .map(|ba| ba.cash)
                .unwrap_or(0.0)
                .max(0.0)
                + customer.available_cash.max(0.0);
            let fee = (balance * DEPOSIT_SERVICE_FEE_RATE).min(*rem);
            if fee <= 0.0 {
                continue;
            }
            // Debit the customer's spendable pockets (brokerage first, the
            // primary spendable account, then available_cash).
            let mut rem_fee = fee;
            if let Some(ref mut ba) = customer.brokerage_account {
                let take = ba.cash.max(0.0).min(rem_fee);
                ba.cash -= take;
                rem_fee -= take;
            }
            if rem_fee > 0.0 {
                customer.available_cash -= rem_fee;
            }
            *rem -= fee;
            *collected.entry(bank_id.clone()).or_default() += fee;
        }
        for bank in companies.iter_mut() {
            if bank.bank_type.is_none() {
                continue;
            }
            let Some(income) = collected.get(&bank.id).copied() else {
                continue;
            };
            if income <= 0.0 {
                continue;
            }
            if let Some(ref mut bs) = bank.balance_sheet {
                bs.deposits -= income; // extinguished customer liability
                bs.tier_1_capital += income; // fee income → equity (A=L+E)
            }
            // Operating pocket (off-BS, M0-counted) — same convention as
            // `operating_cash_credit` in Step 6.
            if let Some(ref mut ba) = bank.brokerage_account {
                ba.cash += income;
            } else {
                bank.available_cash += income;
            }
        }
    }
    #[cfg(feature = "diagnostic")]
    _diag_step_trace!("14b_fees");

    // Step 15 (Phase 35/36/38): Bank Labor Demand â€” banks set FTE demand AND wages
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
        let deposits_book: f64 = bs.map(|b| b.deposits.max(0.0)).unwrap_or(0.0);
        // v4.18 WS3: labor demand has two legs with different intensities,
        // each piecewise for scale economies. Credit administration
        // (underwriting, servicing, collections) is the dense leg at the
        // capacity-model fixed point: 1 FTE per `20_000 × avg_wage` of book
        // (~$10M/FTE at wage ~500) — the staffing inverse of
        // `bank_operational_capacity`, so a fully staffed bank's
        // `max_asset_under_management` covers its book. Deposit servicing
        // (payments, account ops, teller lines) is real labor a
        // deposit-rich bank must staff even while its loan book is thin —
        // keying demand off loans alone deadlocked commercial banks at ~1%
        // LDR: no loans → no staff → no capacity → no loans. But deposit
        // ops are lighter-touch per dollar than credit administration,
        // modeled at 5× the dollars per FTE.
        //
        // Below `STAFF_SCALE_KNEE` ($10B of book) the flat densities hold —
        // a small bank really does staff proportionally. Beyond the knee,
        // centralization and automation make marginal book administration
        // ~10× cheaper per dollar (`STAFF_TAIL_DENSITY_MULT`): a $300B
        // universal bank does NOT employ 30× the staff of a $10B bank.
        // Without the knee, a monopoly-scaled bank ($340B deposits)
        // demanded ~277k FTE — 18% of the national workforce (FS-8 bloat).
        const STAFF_SCALE_KNEE: f64 = 10.0e9;
        const STAFF_TAIL_DENSITY_MULT: f64 = 10.0;
        let piecewise_fte = |book: f64, per_fte: f64| -> f64 {
            if per_fte <= 0.0 {
                return 0.0;
            }
            let knee = book.min(STAFF_SCALE_KNEE);
            knee / per_fte
                + (book - knee).max(0.0) / (per_fte * STAFF_TAIL_DENSITY_MULT)
        };
        let credit_book = total_loans + consumer_loans_total;
        let credit_book_per_fte = 20_000.0 * avg_wage;
        let deposit_book_per_fte = credit_book_per_fte * 5.0;
        let current_fte_need = piecewise_fte(credit_book, credit_book_per_fte)
            + piecewise_fte(deposits_book, deposit_book_per_fte);
        // Phase 38: Smooth the FTE requirement using a moving average stored
        // in extra. This prevents FTE demand from spiking when a batch of
        // loans is issued and crashing when they're repaid.
        let prev_need = bank.temporary_disruption_modifier; // reuse as scratch
        let smoothed_fte_need = if prev_need > 0.0 {
            prev_need * (1.0 - BANK_PORTFOLIO_SMOOTHING)
                + current_fte_need * BANK_PORTFOLIO_SMOOTHING
        } else {
            current_fte_need
        };
        bank.temporary_disruption_modifier = smoothed_fte_need; // store for next turn
        let fte_demand = smoothed_fte_need.ceil();
        // Phase 38: Cap FTE growth at 10% per turn relative to prev_fulfilled_fte.
        // Banks start small and scale conservatively. Min 2 FTE for basic operations.
        // The cap is upward-only: when `fte_demand` falls below the cap the
        // target follows the portfolio down immediately — no downward ratchet.
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
        #[cfg(feature = "diagnostic")]
        eprintln!(
            "BANKFTE_DBG: turn={} bank={} credit_book={:.0} deposits={:.0} fte_raw={:.0} growth_cap={:.0} affordable={:.0} target={} fulfilled={}",
            current_turn, bank.id, credit_book, deposits_book, fte_demand, growth_capped_demand,
            max_affordable, bank.target_fte_demand, bank.fulfilled_fte
        );
    }
    #[cfg(feature = "diagnostic")]
    _diag_step_trace!("15_banklabor");

    // Prudential leverage supervision — see `enforce_leverage_cap`.
    enforce_leverage_cap(country, companies);
    #[cfg(feature = "diagnostic")]
    _diag_step_trace!("16_leverage_cap");

    // Realized-LDR supervision — see `enforce_ldr_cap`. Runs in-turn so
    // the secondary market can rebalance before end-of-day settlement.
    enforce_ldr_cap(
        companies,
        country.central_bank.reserve_requirement_ratio,
    );
    // Funding-side fallback — deposit migration for breaches the asset
    // market could not clear (encumbered books). See
    // `enforce_ldr_deposit_migration`.
    enforce_ldr_deposit_migration(
        companies,
        country.central_bank.reserve_requirement_ratio,
    );

    // Aggregate diagnostics
    for bank in companies.iter() {
        if let Some(ref bs) = bank.balance_sheet {
            result.total_deposits += bs.deposits;
            result.total_reserves += bs.reserves_at_central_bank;
        }
    }

    #[cfg(feature = "diagnostic")]
    {
        let total_br: f64 = companies.iter()
            .filter(|c| c.bank_type.is_some())
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank + bs.cb_deposit_facility_balance)
            .sum();
        let total_citizen: f64 = country.regions.iter()
            .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
            .map(|d| d.savings)
            .sum::<f64>()
            + country.cultural_institutions.iter().map(|b| b.available_cash).sum::<f64>();
        let total_corp: f64 = companies.iter()
            .filter(|c| c.bank_type.is_none() && c.primary_bank_id.is_none())
            .map(|c| c.available_cash + c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0) + c.rd_budget + c.debit_cash)
            .sum::<f64>();
        let total_m0 = total_br + country.bfg_fund.reserves + country.sobk_scheme.pool + country.budget.liquid_reserves + total_citizen + total_corp;
        let delta_m0 = total_m0 - _diag_m0_start;
        let delta_cb = country.central_bank.liquidity_injected - _diag_cb_inj_start;
        let leak = delta_m0 - delta_cb;
        eprintln!("BANKSUMMARY: turn={} country={} m0={:.0} cb={:.0} br={:.0} cit={:.0} corp={:.0} bfg={:.0} sobk={:.0} tr={:.0} dm0={:.0} dcb={:.0} leak={:.0}", current_turn, country.name, total_m0, country.central_bank.liquidity_injected, total_br, total_citizen, total_corp, country.bfg_fund.reserves, country.sobk_scheme.pool, country.budget.liquid_reserves, delta_m0, delta_cb, leak);
    }

    result
}

/// Phase 38: DSPW Auction Settlement â€” primary dealer banks pull-purchase
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
                // Phase 94: No reserve clamping â€” negative reserves
                // represent CB Lombard borrowing. Clamping breaks A=L+E
                // (securities credited by full amount but reserves debited
                // by less) and causes M0 FiatCreation.
                // Credit bank securities holdings.
                bs.securities += purchase_price;
            }

            // Credit treasury with the purchase price.
            country.budget.liquid_reserves += purchase_price;
            // Phase M0-Audit: This is M0-neutral (bank reserves decreased,
            // treasury increased â€” both in walk_global_fiat). No
            // external_financing_injected increment needed.
            // FUTURE: When foreign bond buyers are implemented, their
            // purchases would bring money from outside the tracked M0
            // system. In that case, increment
            // country.budget.external_financing_injected by the foreign
            // purchase amount to prevent false M0 conservation violations.

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
        // The treasury doesn't get cash â€” the deficit isn't funded this turn.
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
        // 10x leverage (1M/100K) would trip the 9.5x prudential gate on any
        // deposit-minting loan — the fixture uses compliant equity so the
        // double-entry mechanics under test can execute.
        balance_sheet.tier_1_capital = 500_000.0;

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
            Some("BANK-1"), // borrower banks at the lender — deposit persists
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
            Some("BANK-1"),
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

        let mut sobk = SobkScheme::default();
        let mut banks = vec![&mut bank1, &mut bank2];
        market.clear_market(&mut banks, &central_bank, &mut sobk, 1);

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

    fn loan(id: &str, balance: f64) -> Loan {
        Loan {
            id: id.to_string(),
            borrower_id: "COMP-X".to_string(),
            principal: balance,
            outstanding_balance: balance,
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
        }
    }

    fn bank_company(id: &str, bs: BankBalanceSheet) -> Company {
        Company {
            id: id.to_string(),
            bank_type: Some(BankType::Commercial),
            balance_sheet: Some(bs),
            ..Company::default()
        }
    }

    #[test]
    fn ldr_cap_sales_move_loans_to_deposit_rich_banks() {
        // rr = 10%. Seller: loans 980 vs deposits 1000 -> LDR 0.98 > 0.97
        // trigger. Excess over the 0.90 target = 980 - 900 = 80, so the
        // 80-unit loan sells and the oversized paper stays.
        let mut seller_bs = BankBalanceSheet::default();
        seller_bs.deposits = 1_000.0;
        seller_bs.reserves_at_central_bank = 100.0;
        for (id, bal) in [("L1", 500.0), ("L2", 300.0), ("L3", 80.0), ("L4", 70.0), ("L5", 30.0)] {
            seller_bs.loans_issued.push(loan(id, bal));
        }
        // Buyer: loans 200 vs deposits 2000 -> LDR 0.10, headroom to the
        // 1800 cap = 1600. Free liquidity = 400 res + 100 depfac - 200
        // required = 300, enough to settle the 100-unit sale.
        let mut buyer_bs = BankBalanceSheet::default();
        buyer_bs.deposits = 2_000.0;
        buyer_bs.reserves_at_central_bank = 400.0;
        buyer_bs.cb_deposit_facility_balance = 100.0;
        buyer_bs.loans_issued.push(loan("B1", 200.0));
        let mut companies = vec![
            bank_company("BANK-SELLER", seller_bs),
            bank_company("BANK-BUYER", buyer_bs),
            Company::default(),
        ];
        // Seller needs to shed 80 (book 980 vs target 900). Loans are
        // atomic, so the pass sells smallest-first until covered: L5(30)
        // then L4(70) = 100 moved, landing the seller at LDR 0.88 —
        // slightly under target because no 50-unit piece exists.
        let moved = enforce_ldr_cap(&mut companies, 0.10);
        assert!((moved - 100.0).abs() < 1e-9);
        let seller = companies[0].balance_sheet.as_ref().unwrap();
        let buyer = companies[1].balance_sheet.as_ref().unwrap();
        // Seller shed the two smallest records; oversized paper stayed.
        assert!((seller.total_loans_outstanding() - 880.0).abs() < 1e-9);
        assert!((seller.ldr() - 0.88).abs() < 1e-9);
        assert_eq!(seller.loans_issued.len(), 3);
        // Buyer absorbed both at par: loans 300, reserves 300 (>= required 200).
        assert!((buyer.total_loans_outstanding() - 300.0).abs() < 1e-9);
        assert!((buyer.reserves_at_central_bank - 300.0).abs() < 1e-9);
        assert!((buyer.cb_deposit_facility_balance - 100.0).abs() < 1e-9);
        assert!(buyer.loans_issued.iter().any(|l| l.id == "L5"));
        assert!(buyer.loans_issued.iter().any(|l| l.id == "L4"));
        // Conservation: aggregate loans, deposits, reserves untouched.
        assert!((seller.reserves_at_central_bank - 200.0).abs() < 1e-9);
    }

    #[test]
    fn ldr_cap_buyer_draws_facility_to_settle() {
        // Same seller; buyer holds only 250 reserves — the 80-unit sale
        // leaves reserves at 170 < required 200, so the facility tops up
        // 30 and falls to 70.
        let mut seller_bs = BankBalanceSheet::default();
        seller_bs.deposits = 1_000.0;
        seller_bs.loans_issued.push(loan("L1", 980.0));
        seller_bs.loans_issued.push(loan("L2", 80.0));
        // loans total 1060 -> ldr 1.06; excess = 1060 - 900 = 160; only
        // the 80-unit loan fits (980 > 160).
        let mut buyer_bs = BankBalanceSheet::default();
        buyer_bs.deposits = 2_000.0;
        buyer_bs.reserves_at_central_bank = 250.0;
        buyer_bs.cb_deposit_facility_balance = 100.0;
        buyer_bs.loans_issued.push(loan("B1", 200.0));
        let mut companies = vec![
            bank_company("BANK-SELLER", seller_bs),
            bank_company("BANK-BUYER", buyer_bs),
        ];
        let moved = enforce_ldr_cap(&mut companies, 0.10);
        assert!((moved - 80.0).abs() < 1e-9);
        let buyer = companies[1].balance_sheet.as_ref().unwrap();
        assert!((buyer.reserves_at_central_bank - 200.0).abs() < 1e-9);
        assert!((buyer.cb_deposit_facility_balance - 70.0).abs() < 1e-9);
        // Seller still carries the oversized L1 — residual LDR 0.98.
        let seller = companies[0].balance_sheet.as_ref().unwrap();
        assert!((seller.ldr() - 0.98).abs() < 1e-9);
    }

    #[test]
    fn ldr_cap_skips_untradable_and_untriggered_banks() {
        let mut seller_bs = BankBalanceSheet::default();
        seller_bs.deposits = 1_000.0;
        let mut bad = loan("BAD", 50.0);
        bad.status = LoanStatus::Default;
        let mut sec = loan("SEC", 50.0);
        sec.securitized = true;
        seller_bs.loans_issued.push(bad);
        seller_bs.loans_issued.push(sec);
        seller_bs.consumer_loans_outstanding = 900.0; // LDR 1.0 but no
        // tradable paper and no transferable consumer records.
        let mut buyer_bs = BankBalanceSheet::default();
        buyer_bs.deposits = 2_000.0;
        buyer_bs.reserves_at_central_bank = 500.0;
        let mut healthy_bs = BankBalanceSheet::default();
        healthy_bs.deposits = 1_000.0;
        healthy_bs.loans_issued.push(loan("OK", 500.0)); // LDR 0.50
        let mut companies = vec![
            bank_company("BANK-UNTRADABLE", seller_bs),
            bank_company("BANK-BUYER", buyer_bs),
            bank_company("BANK-HEALTHY", healthy_bs),
        ];
        let moved = enforce_ldr_cap(&mut companies, 0.10);
        assert_eq!(moved, 0.0);
        assert_eq!(companies[0].balance_sheet.as_ref().unwrap().loans_issued.len(), 2);
        assert_eq!(companies[1].balance_sheet.as_ref().unwrap().loans_issued.len(), 0);
        assert_eq!(companies[2].balance_sheet.as_ref().unwrap().loans_issued.len(), 1);
    }

    #[test]
    fn ldr_migration_fixes_untradable_breach() {
        // The case the sale pass cannot clear: bank's book is fully
        // encumbered (consumer scalar + securitized paper), so no Loan
        // record can move. Deposits migrate in instead: receiver lands
        // at the 0.90 target, donor stays inside its own target.
        let mut recv_bs = BankBalanceSheet::default();
        recv_bs.deposits = 1_000.0;
        recv_bs.reserves_at_central_bank = 200.0;
        let mut sec = loan("SEC", 100.0);
        sec.securitized = true;
        recv_bs.loans_issued.push(sec);
        recv_bs.consumer_loans_outstanding = 900.0; // LDR 1.00, nothing sellable
        let mut donor_bs = BankBalanceSheet::default();
        donor_bs.deposits = 2_000.0;
        donor_bs.reserves_at_central_bank = 500.0;
        donor_bs.loans_issued.push(loan("D1", 200.0)); // LDR 0.10
        let mut companies = vec![
            bank_company("BANK-RECV", recv_bs),
            bank_company("BANK-DONOR", donor_bs),
            Company::default(),
        ];
        let dep0 = 3_000.0;
        let res0 = 700.0;
        let moved = enforce_ldr_deposit_migration(&mut companies, 0.10);
        // Receiver needs dep' = loans/0.90 = 1111.11 → migrate 111.11.
        let recv = companies[0].balance_sheet.as_ref().unwrap();
        let donor = companies[1].balance_sheet.as_ref().unwrap();
        let need = 1_000.0 / 0.90 - 1_000.0;
        assert!((moved - need).abs() < 1e-6);
        assert!((recv.deposits - (1_000.0 + need)).abs() < 1e-6);
        assert!((recv.reserves_at_central_bank - (200.0 + need)).abs() < 1e-6);
        assert!((recv.ldr() - 0.90).abs() < 1e-6);
        // Donor gave deposits + reserves, stays inside its own cap.
        assert!((donor.deposits - (2_000.0 - need)).abs() < 1e-6);
        assert!((donor.reserves_at_central_bank - (500.0 - need)).abs() < 1e-6);
        assert!(donor.ldr() <= 0.90 + 1e-9);
        assert!(donor.reserves_at_central_bank >= donor.deposits * 0.10 - 1e-6);
        // Conservation: total deposits and reserves unchanged.
        let total_dep: f64 = companies
            .iter()
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.deposits)
            .sum();
        let total_res: f64 = companies
            .iter()
            .filter_map(|c| c.balance_sheet.as_ref())
            .map(|bs| bs.reserves_at_central_bank)
            .sum();
        assert!((total_dep - dep0).abs() < 1e-6);
        assert!((total_res - res0).abs() < 1e-6);
    }

    #[test]
    fn ldr_migration_bounds_donor_liquidity() {
        // Donor with thin free reserves can only give free/(1−rr):
        // receiver keeps a residual rather than draining the donor.
        let mut recv_bs = BankBalanceSheet::default();
        recv_bs.deposits = 1_000.0;
        recv_bs.reserves_at_central_bank = 100.0;
        recv_bs.loans_issued.push(loan("L1", 1_200.0)); // LDR 1.20, needs 333.3
        let mut donor_bs = BankBalanceSheet::default();
        donor_bs.deposits = 1_000.0;
        donor_bs.reserves_at_central_bank = 150.0;
        // donor free = 150 − 100 = 50 → res_room = 50/0.9 ≈ 55.56;
        // ldr_room = 1000 − 0/0.9 = 1000 → binding bound is reserves.
        let mut companies = vec![
            bank_company("BANK-RECV", recv_bs),
            bank_company("BANK-DONOR", donor_bs),
        ];
        let moved = enforce_ldr_deposit_migration(&mut companies, 0.10);
        let donor = companies[1].balance_sheet.as_ref().unwrap();
        let res_room = 50.0 / 0.9;
        assert!((moved - res_room).abs() < 1e-6);
        // Donor keeps required reserves on its post-gift deposit base.
        assert!(
            donor.reserves_at_central_bank >= donor.deposits * 0.10 - 1e-6,
            "donor reserves {} below requirement on deposits {}",
            donor.reserves_at_central_bank,
            donor.deposits
        );
        assert!(donor.ldr() <= 0.90 + 1e-9);
    }

    #[test]
    fn ldr_migration_skips_untriggered_and_absent_donors() {
        // A bank under the trigger is untouched; a breached bank with no
        // viable donor keeps its state unchanged.
        let mut ok_bs = BankBalanceSheet::default();
        ok_bs.deposits = 1_000.0;
        ok_bs.reserves_at_central_bank = 100.0;
        ok_bs.loans_issued.push(loan("OK", 800.0)); // LDR 0.80
        let mut lone_bs = BankBalanceSheet::default();
        lone_bs.deposits = 0.0; // no deposit franchise — not a donor
        lone_bs.loans_issued.push(loan("X", 10.0));
        let mut companies = vec![
            bank_company("BANK-OK", ok_bs),
            bank_company("BANK-LONE", lone_bs),
        ];
        let moved = enforce_ldr_deposit_migration(&mut companies, 0.10);
        assert_eq!(moved, 0.0);
        assert!((companies[0].balance_sheet.as_ref().unwrap().deposits - 1_000.0).abs() < 1e-9);
        assert!((companies[1].balance_sheet.as_ref().unwrap().deposits - 0.0).abs() < 1e-9);
    }
}


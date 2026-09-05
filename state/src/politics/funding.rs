//! Funding models for public services
//!
//! This module defines the funding models for healthcare, education, and care facilities,
//! including public, private, mixed, and insurance-based funding.

use serde::{Deserialize, Serialize};

/// Funding model for public services
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FundingModel {
    /// Citizens pay out of pocket
    /// If unable to afford, no service access
    Private,

    /// Fully funded by Central/Regional budgets
    /// Free at point of use (e.g., NFZ / Kasy Chorych)
    Public,

    /// State pays base subsidy, citizens pay remainder
    /// e.g., co-payment (co-payment)
    Mixed {
        /// State subsidy percentage (0.0-1.0)
        state_subsidy_rate: f64,

        /// Citizen co-payment percentage (0.0-1.0)
        citizen_co_payment_rate: f64,
    },

    /// Insurance-based (private or public insurance)
    Insurance {
        /// Mandatory insurance contribution rate
        insurance_premium_rate: f64,

        /// Coverage percentage
        coverage_rate: f64,
    },
}

/// Budget source for funding
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BudgetSource {
    #[default]
    Central,
    Regional,
    Mixed {
        central_share: f64,

        regional_share: f64,
    },
}

/// Loan provider for student loans
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoanProvider {
    /// Commercial bank loans (default, market interest rates)
    CommercialBank,

    /// State-subsidized loans (lower interest, requires political pressure)
    StateSubsidized,
}

/// Student loan configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StudentLoanConfig {
    /// Loan provider (commercial or state-subsidized)
    pub loan_provider: LoanProvider,

    /// Maximum loan amount per student
    pub max_loan_amount: f64,

    /// Interest rate (market rate for CommercialBank, subsidized for StateSubsidized)
    pub interest_rate: f64,

    /// Repayment period in years
    pub repayment_period_years: u32,

    /// Income threshold for repayment start
    pub income_threshold: f64,

    /// Political pressure threshold for state to offer subsidized loans
    /// If public support for education exceeds this, state offers cheaper loans
    #[serde(default)]
    pub political_pressure_threshold: f64,
}

/// Funding configuration for a service category
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceFundingConfig {
    /// Primary funding model
    pub funding_model: FundingModel,

    /// Budget source (Central, Regional, or Mixed)
    pub budget_source: BudgetSource,

    /// Per-capita funding amount (for Public/Mixed models)
    pub per_capita_funding: f64,

    /// Student loan configuration (for Higher Education)
    #[serde(default)]
    pub student_loans: Option<StudentLoanConfig>,
}

// ============================================================================
// PHASE 18E: PARK FUNDING — C2G ADMINISTRATIVE FEE ROUTING
// ============================================================================

/// Phase 18E: Funding source for parks and protected areas.
///
/// Park funding follows strict double-entry bookkeeping:
/// - Entry fees: C2G (Citizen-to-Government) administrative fee collection.
///   Debited from citizen Labor accounts (class.savings), credited to
///   the park's funding_balance sub-account.
/// - Ecological taxes: Debited from industrial company liquid_capital,
///   credited to the park's funding_balance sub-account.
/// - Government subsidy: Debited from country.budget.liquid_reserves,
///   credited to the park's funding_balance sub-account.
/// - If funding_balance < 0 (revenue < costs), ecological_health degrades
///   proportionally to the deficit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParkFundingSource {
    /// Fully government-funded (no entry fees).
    /// Management cost debited from country.budget.liquid_reserves.
    #[default]
    GovernmentFunded,

    /// Entry fees cover management costs (C2G administrative fee).
    /// Citizens debited via Labor account savings, credited to park sub-account.
    EntryFeeFunded,

    /// Mixed: entry fees + government subsidy.
    /// Entry fees cover a fraction; remainder from budget.
    MixedFunding {
        /// Fraction covered by entry fees (0.0-1.0)
        entry_fee_fraction: f64,
        /// Fraction covered by government budget (0.0-1.0)
        government_fraction: f64,
    },

    /// Ecological tax funded: industrial firms in buffer zone pay ecological tax.
    /// Tax debited from company.liquid_capital, credited to park sub-account.
    EcologicalTaxFunded,
}

/// Phase 18E: Park funding configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParkFundingConfig {
    /// Primary funding source
    pub funding_source: ParkFundingSource,
    /// Entry fee as fraction of average_wage (0.0 = free, 0.01 = 1% of avg wage)
    #[serde(default)]
    pub entry_fee_wage_fraction: f64,
    /// Ecological tax per hectare of industrial land in buffer zone
    #[serde(default)]
    pub ecological_tax_per_hectare: f64,
    /// CAPEX amortization period in turns (for infrastructure cost-plus pricing)
    #[serde(default = "default_park_amortization_turns")]
    pub capex_amortization_turns: u32,
    /// Ecological health degradation rate when funding_balance < 0
    #[serde(default = "default_park_health_degradation_rate")]
    pub health_degradation_rate: f64,
}

fn default_park_amortization_turns() -> u32 { 60 }
fn default_park_health_degradation_rate() -> f64 { 0.02 }

impl Default for ParkFundingConfig {
    fn default() -> Self {
        Self {
            funding_source: ParkFundingSource::default(),
            entry_fee_wage_fraction: 0.001,
            ecological_tax_per_hectare: 0.0,
            capex_amortization_turns: default_park_amortization_turns(),
            health_degradation_rate: default_park_health_degradation_rate(),
        }
    }
}
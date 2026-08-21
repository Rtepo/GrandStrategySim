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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetSource {
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

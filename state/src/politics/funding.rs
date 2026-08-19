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
    /// e.g., współpłacenie (co-payment)
    Mixed {
        /// State subsidy percentage (0.0-1.0)
        #[serde(rename = "subwencja_państwowa")]
        state_subsidy_rate: f64,

        /// Citizen co-payment percentage (0.0-1.0)
        #[serde(rename = "współpłacenie")]
        citizen_co_payment_rate: f64,
    },

    /// Insurance-based (private or public insurance)
    Insurance {
        /// Mandatory insurance contribution rate
        #[serde(rename = "składka_ubezpieczeniowa")]
        insurance_premium_rate: f64,

        /// Coverage percentage
        #[serde(rename = "pokrycie")]
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
        #[serde(rename = "udział_centralny")]
        central_share: f64,
        #[serde(rename = "udział_regionalny")]
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
    #[serde(rename = "dostawca_pożyczki")]
    pub loan_provider: LoanProvider,

    /// Maximum loan amount per student
    #[serde(rename = "maksymalna_kwota")]
    pub max_loan_amount: f64,

    /// Interest rate (market rate for CommercialBank, subsidized for StateSubsidized)
    #[serde(rename = "oprocentowanie")]
    pub interest_rate: f64,

    /// Repayment period in years
    #[serde(rename = "okres_spłaty")]
    pub repayment_period_years: u32,

    /// Income threshold for repayment start
    #[serde(rename = "próg_wysokości Dochodu")]
    pub income_threshold: f64,

    /// Political pressure threshold for state to offer subsidized loans
    /// If public support for education exceeds this, state offers cheaper loans
    #[serde(rename = "próg_presji_politycznej", default)]
    pub political_pressure_threshold: f64,
}

/// Funding configuration for a service category
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceFundingConfig {
    /// Primary funding model
    #[serde(rename = "model_finansowania")]
    pub funding_model: FundingModel,

    /// Budget source (Central, Regional, or Mixed)
    #[serde(rename = "źródło_budżetu")]
    pub budget_source: BudgetSource,

    /// Per-capita funding amount (for Public/Mixed models)
    #[serde(rename = "finansowanie_na_osobę")]
    pub per_capita_funding: f64,

    /// Student loan configuration (for Higher Education)
    #[serde(rename = "pożyczki_studenckie", default)]
    pub student_loans: Option<StudentLoanConfig>,
}

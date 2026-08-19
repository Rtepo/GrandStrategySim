//! Configuration for securities market operations (no magic numbers).
//!
//! All rates, thresholds, and ratios governing the securities market
//! are centralised in [`SecuritiesMarketConfig`] to eliminate hardcoded
//! constants.

use serde::{Deserialize, Serialize};

/// Configuration for securities market operations (no magic numbers).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SecuritiesMarketConfig {
    // ── Exchange ──
    /// Exchange transaction fee percentage (e.g., 0.002 for 0.2%).
    #[serde(default)]
    pub transaction_fee_rate: f64,
    /// AMM slippage factor for market orders (higher = more slippage).
    #[serde(default)]
    pub amm_slippage_factor: f64,
    /// Circuit breaker threshold (e.g., 0.10 for 10% index move).
    #[serde(default)]
    pub circuit_breaker_threshold: f64,
    /// Circuit breaker halt duration in turns.
    #[serde(default)]
    pub circuit_breaker_duration: u32,

    // ── KNF ──
    /// KNF penalty multiplier for Tier 1 shortfall (fine = severity * assets * this).
    #[serde(default)]
    pub knf_penalty_multiplier: f64,
    /// KNF minimum Tier 1 capital ratio (e.g., 0.08 for 8%).
    #[serde(default)]
    pub knf_min_tier1_ratio: f64,
    /// OTC derivative fine rate (percentage of notional).
    #[serde(default)]
    pub otc_fine_rate: f64,

    // ── CCP ──
    /// CCP initial margin ratio (e.g., 0.10 for 10%).
    #[serde(default)]
    pub ccp_initial_margin_ratio: f64,
    /// CCP maintenance margin ratio (e.g., 0.05 for 5%).
    #[serde(default)]
    pub ccp_maintenance_margin_ratio: f64,
    /// CCP default fund contribution ratio (e.g., 0.01 of member notional).
    #[serde(default)]
    pub ccp_default_fund_ratio: f64,

    // ── Funds ──
    /// Fraction of citizen savings that flows into investment funds each turn (e.g., 0.05 for 5%).
    /// This is the SUBSCRIPTION rate — how much people invest, NOT the management fee.
    #[serde(default)]
    pub fund_subscription_rate: f64,
    /// Fund management fee rate (e.g., 0.02 for 2% of AUM). Deducted from fund assets annually.
    #[serde(default)]
    pub fund_management_fee_rate: f64,
    /// Fund performance fee rate (e.g., 0.20 for 20% of excess returns above benchmark).
    #[serde(default)]
    pub fund_performance_fee_rate: f64,
    /// Minimum P/E ratio threshold below which funds consider a stock undervalued (e.g., 5.0).
    #[serde(default)]
    pub fund_min_pe_threshold: f64,
    /// Maximum P/E ratio threshold above which funds consider a stock overvalued (e.g., 25.0).
    #[serde(default)]
    pub fund_max_pe_threshold: f64,
    /// Minimum dividend yield to attract income funds (e.g., 0.03 for 3%).
    #[serde(default)]
    pub fund_min_dividend_yield: f64,
    /// Minimum yield on fixed-income securities (MBS/Bonds) to attract fund bids (e.g., 0.04 for 4%).
    #[serde(default)]
    pub fund_min_bond_yield: f64,
    /// Risk-free benchmark rate for performance fee calculation (e.g., 0.04 for 4%).
    #[serde(default)]
    pub fund_benchmark_rate: f64,

    // ── Securitization ──
    /// MBS servicing spread default (e.g., 0.005 for 0.5%).
    #[serde(default)]
    pub mbs_servicing_spread: f64,
    /// Senior tranche fraction of total notional (e.g., 0.70).
    #[serde(default)]
    pub mbs_senior_fraction: f64,
    /// Mezzanine tranche fraction of total notional (e.g., 0.20).
    #[serde(default)]
    pub mbs_mezzanine_fraction: f64,
    /// Junior tranche fraction of total notional (e.g., 0.10).
    #[serde(default)]
    pub mbs_junior_fraction: f64,
    /// Covered bond minimum coverage ratio (e.g., 1.0 for 100%).
    #[serde(default)]
    pub covered_bond_min_coverage: f64,

    // ── Trade Finance ──
    /// Standard LTV ratio for Bills of Lading collateral (e.g., 0.80).
    #[serde(default)]
    pub trade_finance_ltv: f64,
}

//! Configuration for securities market operations (no magic numbers).
//!
//! All rates, thresholds, and ratios governing the securities market
//! are centralised in [`SecuritiesMarketConfig`] to eliminate hardcoded
//! constants.

use serde::{Deserialize, Serialize};

/// Configuration for securities market operations (no magic numbers).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

    // ── Phase 56: Price Discovery ──
    /// Mean-reversion drift rate for share prices with no trades (e.g., 0.05 for 5% per turn).
    /// When no trades occur, share price drifts toward book value by this fraction.
    #[serde(default = "default_mean_reversion_rate")]
    pub mean_reversion_rate: f64,
    /// Weight of book value vs current price in mean-reversion target (e.g., 0.5 for 50/50 blend).
    /// Target = current_price * (1 - weight) + book_value_per_share * weight.
    #[serde(default = "default_mean_reversion_target_weight")]
    pub mean_reversion_target_weight: f64,

    // ── Phase 56: Commodity Spot Market ──
    /// Retail premium applied to commodity spot prices above B2B clearing VWAP (e.g., 0.05 for 5%).
    /// Spot price = B2B VWAP * (1 + premium).
    #[serde(default = "default_commodity_spot_retail_premium")]
    pub commodity_spot_retail_premium: f64,

    // ── KNF ──
    /// Defaults to 1.0 (neutral enforcement) to prevent a missing-data hazard
    /// where securities regulation becomes toothless with 0.0 penalty multiplier.
    #[serde(default = "default_knf_penalty_multiplier")]
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

// ── Phase 56: Default value functions for config fields ──

fn default_mean_reversion_rate() -> f64 {
    0.05 // 5% per turn — configurable, not hardcoded in logic
}

fn default_mean_reversion_target_weight() -> f64 {
    0.5 // 50/50 blend of current price and book value
}

fn default_commodity_spot_retail_premium() -> f64 {
    0.05 // 5% above B2B VWAP — configurable, not hardcoded in logic
}

/// Default KNF penalty multiplier.
/// Defaults to 1.0 (neutral enforcement) to prevent a missing-data hazard
/// where securities regulation becomes toothless with 0.0 penalty multiplier.
fn default_knf_penalty_multiplier() -> f64 {
    1.0
}

/// Manual Default implementation to ensure Phase 56 config fields use
/// their serde default functions rather than 0.0.
impl Default for SecuritiesMarketConfig {
    fn default() -> Self {
        SecuritiesMarketConfig {
            // ── Exchange ──
            transaction_fee_rate: 0.0,
            amm_slippage_factor: 0.0,
            circuit_breaker_threshold: 0.0,
            circuit_breaker_duration: 0,
            // ── Phase 56: Price Discovery ──
            mean_reversion_rate: default_mean_reversion_rate(),
            mean_reversion_target_weight: default_mean_reversion_target_weight(),
            // ── Phase 56: Commodity Spot ──
            commodity_spot_retail_premium: default_commodity_spot_retail_premium(),
            // ── KNF ──
            knf_penalty_multiplier: default_knf_penalty_multiplier(),
            knf_min_tier1_ratio: 0.0,
            otc_fine_rate: 0.0,
            // ── CCP ──
            ccp_initial_margin_ratio: 0.0,
            ccp_maintenance_margin_ratio: 0.0,
            ccp_default_fund_ratio: 0.0,
            // ── Funds ──
            // R6.1: Non-zero defaults to activate fund lifecycle.
            fund_subscription_rate: 0.02,
            fund_management_fee_rate: 0.015,
            fund_performance_fee_rate: 0.20,
            fund_min_pe_threshold: 5.0,
            fund_max_pe_threshold: 25.0,
            fund_min_dividend_yield: 0.02,
            fund_min_bond_yield: 0.0,
            fund_benchmark_rate: 0.04,
            // ── Securitization ──
            mbs_servicing_spread: 0.0,
            mbs_senior_fraction: 0.0,
            mbs_mezzanine_fraction: 0.0,
            mbs_junior_fraction: 0.0,
            covered_bond_min_coverage: 0.0,
            // ── Trade Finance ──
            trade_finance_ltv: 0.0,
        }
    }
}

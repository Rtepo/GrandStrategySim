//! Bounded rationality system for corporate decision-making.
//!
//! This module implements information access tiers and market research mechanics
//! that simulate realistic decision-making limitations for companies of different sizes.

use crate::registries::enums::{Commodity, Sector};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Information quality tier for corporate decision-making.
///
/// Companies operate in a "fog of war" - they lack perfect market information
/// and must use trial and error unless they purchase market research.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InformationQuality {
    /// No market data - trial and error only
    Blind,
    /// Regional prices only
    Local,
    /// National market prices
    National,
    /// International prices and trends
    Global,
    /// AI-driven demand forecasting
    Predictive,
}

/// Determines information quality based on company capital and macro indicators.
///
/// # Arguments
/// * `company_capital` - The company's total capital
/// * `average_wage` - The country's average wage (inflation index)
///
/// # Returns
/// The appropriate `InformationQuality` tier for the company
///
/// # Rules
/// * Uses dynamic thresholds based on average_wage, not hardcoded floats
/// * This ensures information quality tiers scale with inflation and economic development
pub fn determine_information_quality(company_capital: f64, average_wage: f64) -> InformationQuality {
    if company_capital < average_wage * 10.0 {
        InformationQuality::Blind
    } else if company_capital < average_wage * 100.0 {
        InformationQuality::Local
    } else if company_capital < average_wage * 1_000.0 {
        InformationQuality::National
    } else if company_capital < average_wage * 10_000.0 {
        InformationQuality::Global
    } else {
        InformationQuality::Predictive
    }
}

/// Attempts to upgrade a company to Predictive information quality tier.
///
/// # Arguments
/// * `company_capital` - The company's total capital
/// * `market_research_units` - Amount of market research commodities available
/// * `average_wage` - The country's average wage (inflation index)
///
/// # Returns
/// `true` if the upgrade succeeds, `false` otherwise
///
/// # Rules
/// * Uses dynamic threshold based on average_wage, not hardcoded float
/// * Normalizes required research units against inflation using wage index
/// * Scales by real company size (inflation-adjusted), not nominal capital
pub fn try_upgrade_to_predictive(
    company_capital: f64,
    market_research_units: f64,
    average_wage: f64,
) -> bool {
    let predictive_threshold = average_wage * 10_000.0;
    
    // Normalize required research units against inflation using wage index
    // Real size = company_capital / (average_wage * 1000.0) represents employee-equivalent scale
    let real_company_size = company_capital / (average_wage * 1000.0);
    let required_research_units = 1.0 + real_company_size;
    
    market_research_units >= required_research_units && company_capital >= predictive_threshold
}

/// Phase 24C.7: Apply information quality estimation error to a cost estimate.
///
/// Companies with lower information quality tiers estimate costs less accurately.
/// The error is applied as a symmetric percentage deviation from the true cost.
///
/// # Arguments
/// * `true_cost` - The actual cost of the project/bid
/// * `quality` - The company's information quality tier
///
/// # Returns
/// * `f64` - The company's (mis)estimated cost
///
/// # Error rates by tier:
/// * `Blind` — ±30% error (trial and error, no market data)
/// * `Local` — ±20% error (regional prices only)
/// * `National` — ±10% error (national market prices)
/// * `Global` — ±5% error (international prices and trends)
/// * `Predictive` — 0% error (AI-driven demand forecasting)
pub fn apply_estimation_error(true_cost: f64, quality: InformationQuality) -> f64 {
    let error_rate = match quality {
        InformationQuality::Blind => 0.30,
        InformationQuality::Local => 0.20,
        InformationQuality::National => 0.10,
        InformationQuality::Global => 0.05,
        InformationQuality::Predictive => 0.0,
    };
    // Deterministic midpoint estimate — companies with poor information
    // systematically overestimate by half the error rate (conservative bias)
    // plus a deterministic deviation based on the cost magnitude.
    if error_rate == 0.0 {
        return true_cost;
    }
    // Use a deterministic hash of the cost to produce a stable pseudo-random
    // deviation in [-error_rate, +error_rate] without requiring an RNG.
    let hash = (true_cost.to_bits().wrapping_add(0x9E3779B97F4A7C15) as u64) as f64;
    let frac = (hash / u64::MAX as f64) * 2.0 - 1.0; // [-1, 1]
    true_cost * (1.0 + frac * error_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_information_quality_blind() {
        let quality = determine_information_quality(50.0, 10.0);
        assert_eq!(quality, InformationQuality::Blind);
    }

    #[test]
    fn test_determine_information_quality_local() {
        let quality = determine_information_quality(500.0, 10.0);
        assert_eq!(quality, InformationQuality::Local);
    }

    #[test]
    fn test_determine_information_quality_national() {
        let quality = determine_information_quality(5_000.0, 10.0);
        assert_eq!(quality, InformationQuality::National);
    }

    #[test]
    fn test_determine_information_quality_global() {
        let quality = determine_information_quality(50_000.0, 10.0);
        assert_eq!(quality, InformationQuality::Global);
    }

    #[test]
    fn test_determine_information_quality_predictive() {
        let quality = determine_information_quality(500_000.0, 10.0);
        assert_eq!(quality, InformationQuality::Predictive);
    }

    #[test]
    fn test_try_upgrade_to_predictive_success() {
        let result = try_upgrade_to_predictive(200_000.0, 200.0, 10.0);
        assert!(result);
    }

    #[test]
    fn test_try_upgrade_to_predictive_insufficient_capital() {
        let result = try_upgrade_to_predictive(50_000.0, 10.0, 10.0);
        assert!(!result);
    }

    #[test]
    fn test_try_upgrade_to_predictive_insufficient_research() {
        let result = try_upgrade_to_predictive(200_000.0, 10.0, 1.0);
        assert!(!result);
    }
}

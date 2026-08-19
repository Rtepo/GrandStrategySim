//! Corporate technology and R&D configuration.
//!
//! This module defines the configuration parameters for corporate R&D allocation
//! and licensing decisions in Phase 7.

use serde::{Deserialize, Serialize};

/// Configuration parameters for corporate technology research and licensing.
///
/// These parameters control how companies allocate resources to R&D and make
/// licensing decisions for patented production methods.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorporateTechConfig {
    /// Ratio of operating expenses that must be available in cash before R&D allocation.
    /// Default 2.0 (200% of operating expenses).
    #[serde(rename = "próg_alokacji_rd", default = "default_rd_threshold")]
    pub rd_allocation_threshold_ratio: f64,

    /// Percentage of excess cash allocated to R&D budget.
    /// Default 0.10 (10% of excess cash).
    #[serde(rename = "procent_alokacji_rd", default = "default_rd_percentage")]
    pub rd_allocation_percentage: f64,

    /// Minimum net benefit threshold for licensing a patented method.
    /// Companies only license if (current_cost - new_cost - royalty) > this threshold.
    #[serde(rename = "próg_korzyści_licencji", default = "default_licensing_threshold")]
    pub licensing_benefit_threshold: f64,

    /// State patent royalty rate charged to ALL companies (state-owned + private).
    /// Default 0.03 (3% of output commodity VWAP).
    #[serde(rename = "stawka_royalty_państwa", default = "default_state_patent_royalty")]
    pub state_patent_royalty_ratio: f64,

    /// Maximum R&D budget as fraction of company_capital.
    /// Default 0.2 (20% of company capital).
    #[serde(rename = "maks_budżet_rd", default = "default_max_rd_budget_ratio")]
    pub max_rd_budget_ratio: f64,
}

fn default_rd_threshold() -> f64 {
    2.0
}

fn default_rd_percentage() -> f64 {
    0.10
}

fn default_licensing_threshold() -> f64 {
    0.0
}

fn default_state_patent_royalty() -> f64 {
    0.03
}

fn default_max_rd_budget_ratio() -> f64 {
    0.2
}

impl Default for CorporateTechConfig {
    fn default() -> Self {
        Self {
            rd_allocation_threshold_ratio: default_rd_threshold(),
            rd_allocation_percentage: default_rd_percentage(),
            licensing_benefit_threshold: default_licensing_threshold(),
            state_patent_royalty_ratio: default_state_patent_royalty(),
            max_rd_budget_ratio: default_max_rd_budget_ratio(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = CorporateTechConfig::default();
        assert_eq!(config.rd_allocation_threshold_ratio, 2.0);
        assert_eq!(config.rd_allocation_percentage, 0.10);
        assert_eq!(config.licensing_benefit_threshold, 0.0);
        assert_eq!(config.state_patent_royalty_ratio, 0.03);
        assert_eq!(config.max_rd_budget_ratio, 0.2);
    }

    #[test]
    fn custom_config() {
        let config = CorporateTechConfig {
            rd_allocation_threshold_ratio: 3.0,
            rd_allocation_percentage: 0.15,
            licensing_benefit_threshold: 100.0,
            state_patent_royalty_ratio: 0.05,
            max_rd_budget_ratio: 0.3,
        };
        assert_eq!(config.rd_allocation_threshold_ratio, 3.0);
        assert_eq!(config.rd_allocation_percentage, 0.15);
        assert_eq!(config.licensing_benefit_threshold, 100.0);
        assert_eq!(config.state_patent_royalty_ratio, 0.05);
        assert_eq!(config.max_rd_budget_ratio, 0.3);
    }
}

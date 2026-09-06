//! B2C service pricing configuration.
//!
//! Phase C.2: Full Cost-Plus pricing with smoothed volumes (Rule 21).
//!
//! State and municipal utilities (Water, Heat, Waste, Education, Healthcare)
//! must NOT operate on raw OPEX or un-smoothed spot pricing, which leads to
//! volatile death spirals. This config implements a **Cost-Plus** model that:
//!
//! 1. Includes the **amortization of CAPEX** (Rule 21).
//! 2. Uses **smoothed historical volumes** (24-turn rolling averages) to
//!    guarantee long-term solvency despite seasonal demand fluctuations.
//! 3. Scales with `average_wage` (Rule 2: no magic nominal constants).
//!
//! The price per unit is computed as:
//! ```text
//!   price = (opex_per_unit + capex_amortization_per_unit) * (1 + margin)
//! ```
//!
//! where:
//! * `opex_per_unit = average_wage * opex_wage_multiplier / smoothed_volume`
//! * `capex_amortization_per_unit = capex_per_unit / amortization_turns`
//! * `smoothed_volume` = 24-turn rolling average of actual consumption

use serde::{Deserialize, Serialize};

/// Configuration for B2C service pricing.
///
/// Phase C.2: Cost-Plus model with CAPEX amortization and smoothed volumes.
/// All monetary values are `average_wage` multipliers (Rule 2: inflation-proof).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServicePricingConfig {
    /// OPEX wage multiplier for education (how many average_wages per slot).
    /// Default 0.05 (5% of average wage per education slot).
    #[serde(default = "default_education_opex")]
    pub education_opex_wage_multiplier: f64,

    /// CAPEX per education slot, as average_wage multiplier.
    /// Default 0.5 (50% of average wage per slot for building amortization).
    #[serde(default = "default_education_capex")]
    pub education_capex_wage_multiplier: f64,

    /// OPEX wage multiplier for healthcare (how many average_wages per capacity).
    /// Default 0.08 (8% of average wage per health capacity unit).
    #[serde(default = "default_health_opex")]
    pub health_opex_wage_multiplier: f64,

    /// CAPEX per health capacity unit, as average_wage multiplier.
    /// Default 0.8 (80% of average wage per capacity for building amortization).
    #[serde(default = "default_health_capex")]
    pub health_capex_wage_multiplier: f64,

    /// OPEX wage multiplier for default services.
    /// Default 0.04 (4% of average wage per unit).
    #[serde(default = "default_service_opex")]
    pub default_opex_wage_multiplier: f64,

    /// CAPEX per default service unit, as average_wage multiplier.
    /// Default 0.3 (30% of average wage per unit for amortization).
    #[serde(default = "default_service_capex")]
    pub default_capex_wage_multiplier: f64,

    /// OPEX wage multiplier for information/media services.
    /// Default 0.02 (2% of average wage per information unit).
    #[serde(default = "default_information_opex")]
    pub information_opex_wage_multiplier: f64,

    /// CAPEX per information unit, as average_wage multiplier.
    /// Default 0.1 (10% of average wage per unit for amortization).
    #[serde(default = "default_information_capex")]
    pub information_capex_wage_multiplier: f64,

    /// Cost-plus margin (profit/solvency buffer above cost).
    /// Default 0.10 (10% margin above OPEX + CAPEX amortization).
    #[serde(default = "default_margin")]
    pub cost_plus_margin: f64,

    /// Amortization period in turns (CAPEX spread over this many turns).
    /// Default 24 (24-turn rolling average, matching smoothed volumes).
    #[serde(default = "default_amortization_turns")]
    pub amortization_turns: u32,

    /// Smoothing window for historical volumes (rolling average turns).
    /// Default 24 (24-turn rolling average per Rule 21).
    #[serde(default = "default_smoothing_window")]
    pub smoothing_window: u32,

    /// Phase C.2: When true, education is free at point of use (price = 0).
    /// Set by education laws (StateRun model). The cost is borne by the
    /// treasury via subsidies, not by citizens.
    #[serde(default)]
    pub force_free_education: bool,

    /// Phase C.2: When true, healthcare is free at point of use (price = 0).
    /// Set by healthcare laws (Universal universality). The cost is borne by
    /// the treasury via subsidies, not by citizens.
    #[serde(default)]
    pub force_free_healthcare: bool,

    /// Phase 18S: OPEX wage multiplier for sports/recreation.
    /// Default 0.03 (3% of average wage per visitor-slot).
    #[serde(default = "default_sports_opex")]
    pub sports_opex_wage_multiplier: f64,

    /// Phase 18S: CAPEX per sports visitor-slot, as average_wage multiplier.
    /// Default 0.4 (40% of average wage per slot for facility amortization).
    #[serde(default = "default_sports_capex")]
    pub sports_capex_wage_multiplier: f64,
}

fn default_education_opex() -> f64 {
    0.05
}
fn default_education_capex() -> f64 {
    0.5
}
fn default_health_opex() -> f64 {
    0.08
}
fn default_health_capex() -> f64 {
    0.8
}
fn default_service_opex() -> f64 {
    0.04
}
fn default_service_capex() -> f64 {
    0.3
}
fn default_information_opex() -> f64 {
    0.02
}
fn default_information_capex() -> f64 {
    0.1
}
fn default_sports_opex() -> f64 {
    0.03
}
fn default_sports_capex() -> f64 {
    0.4
}
fn default_margin() -> f64 {
    0.10
}
fn default_amortization_turns() -> u32 {
    24
}
fn default_smoothing_window() -> u32 {
    24
}

impl Default for ServicePricingConfig {
    fn default() -> Self {
        Self {
            education_opex_wage_multiplier: default_education_opex(),
            education_capex_wage_multiplier: default_education_capex(),
            health_opex_wage_multiplier: default_health_opex(),
            health_capex_wage_multiplier: default_health_capex(),
            default_opex_wage_multiplier: default_service_opex(),
            default_capex_wage_multiplier: default_service_capex(),
            information_opex_wage_multiplier: default_information_opex(),
            information_capex_wage_multiplier: default_information_capex(),
            cost_plus_margin: default_margin(),
            amortization_turns: default_amortization_turns(),
            smoothing_window: default_smoothing_window(),
            force_free_education: false,
            force_free_healthcare: false,
            sports_opex_wage_multiplier: default_sports_opex(),
            sports_capex_wage_multiplier: default_sports_capex(),
        }
    }
}

impl ServicePricingConfig {
    /// Compute the cost-plus price for an education slot.
    ///
    /// Phase C.2: `price = (opex + capex_amortization) * (1 + margin)`
    /// where all components are scaled by `average_wage` (Rule 2).
    /// Returns 0.0 if `force_free_education` is set (state-run education).
    pub fn education_price_per_slot(&self, average_wage: f64) -> f64 {
        if self.force_free_education {
            return 0.0;
        }
        let wage = average_wage.max(1.0);
        let opex = wage * self.education_opex_wage_multiplier;
        let capex_amort =
            wage * self.education_capex_wage_multiplier / self.amortization_turns.max(1) as f64;
        (opex + capex_amort) * (1.0 + self.cost_plus_margin)
    }

    /// Compute the cost-plus price for a health capacity unit.
    /// Returns 0.0 if `force_free_healthcare` is set (universal healthcare).
    pub fn health_price_per_capacity(&self, average_wage: f64) -> f64 {
        if self.force_free_healthcare {
            return 0.0;
        }
        let wage = average_wage.max(1.0);
        let opex = wage * self.health_opex_wage_multiplier;
        let capex_amort =
            wage * self.health_capex_wage_multiplier / self.amortization_turns.max(1) as f64;
        (opex + capex_amort) * (1.0 + self.cost_plus_margin)
    }

    /// Compute the cost-plus price for a default service unit.
    pub fn default_service_price(&self, average_wage: f64) -> f64 {
        let wage = average_wage.max(1.0);
        let opex = wage * self.default_opex_wage_multiplier;
        let capex_amort =
            wage * self.default_capex_wage_multiplier / self.amortization_turns.max(1) as f64;
        (opex + capex_amort) * (1.0 + self.cost_plus_margin)
    }

    /// Compute the cost-plus price for an information/media unit.
    pub fn information_price_per_unit(&self, average_wage: f64) -> f64 {
        let wage = average_wage.max(1.0);
        let opex = wage * self.information_opex_wage_multiplier;
        let capex_amort =
            wage * self.information_capex_wage_multiplier / self.amortization_turns.max(1) as f64;
        (opex + capex_amort) * (1.0 + self.cost_plus_margin)
    }

    /// Phase 18S: Compute the cost-plus price for a sports/recreation visitor-slot.
    ///
    /// Follows the same Cost-Plus model as education and healthcare (Rule 21):
    /// `price = (opex + capex_amortization) * (1 + margin)`
    /// where all components are scaled by `average_wage` (Rule 2).
    pub fn sports_price_per_capacity(&self, average_wage: f64) -> f64 {
        let wage = average_wage.max(1.0);
        let opex = wage * self.sports_opex_wage_multiplier;
        let capex_amort =
            wage * self.sports_capex_wage_multiplier / self.amortization_turns.max(1) as f64;
        (opex + capex_amort) * (1.0 + self.cost_plus_margin)
    }

    /// Smooth a volume series using a rolling average (Rule 21).
    ///
    /// Returns the smoothed volume (last `smoothing_window` entries averaged).
    /// If fewer entries exist, averages what's available.
    pub fn smooth_volume(&self, history: &[f64]) -> f64 {
        if history.is_empty() {
            return 1.0; // Avoid division by zero
        }
        let window = self.smoothing_window as usize;
        let start = history.len().saturating_sub(window);
        let sum: f64 = history[start..].iter().sum();
        let count = (history.len() - start) as f64;
        sum / count.max(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_multipliers() {
        let config = ServicePricingConfig::default();
        assert_eq!(config.education_opex_wage_multiplier, 0.05);
        assert_eq!(config.health_opex_wage_multiplier, 0.08);
        assert_eq!(config.cost_plus_margin, 0.10);
        assert_eq!(config.amortization_turns, 24);
        assert_eq!(config.smoothing_window, 24);
    }

    #[test]
    fn education_price_scales_with_wage() {
        let config = ServicePricingConfig::default();
        let wage_low = 1000.0;
        let wage_high = 10_000.0;
        let price_low = config.education_price_per_slot(wage_low);
        let price_high = config.education_price_per_slot(wage_high);
        assert!(price_high > price_low, "price must scale with wage");
        assert!(price_low > 0.0);
    }

    #[test]
    fn education_price_includes_capex_amortization() {
        let config = ServicePricingConfig::default();
        let wage = 1000.0;
        // opex = 1000 * 0.05 = 50
        // capex_amort = 1000 * 0.5 / 24 = 20.833...
        // price = (50 + 20.833) * 1.10 = 77.916...
        let price = config.education_price_per_slot(wage);
        let expected = (50.0 + 1000.0 * 0.5 / 24.0) * 1.10;
        assert!((price - expected).abs() < 0.01);
    }

    #[test]
    fn smooth_volume_rolling_average() {
        let config = ServicePricingConfig::default();
        let history = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let smoothed = config.smooth_volume(&history);
        // Window=24 but only 5 entries, so average = 30.0
        assert!((smoothed - 30.0).abs() < 0.01);
    }

    #[test]
    fn smooth_volume_empty_returns_one() {
        let config = ServicePricingConfig::default();
        let smoothed = config.smooth_volume(&[]);
        assert_eq!(smoothed, 1.0);
    }

    #[test]
    fn health_price_includes_margin() {
        let config = ServicePricingConfig::default();
        let wage = 1000.0;
        let price = config.health_price_per_capacity(wage);
        // opex = 80, capex_amort = 800/24 = 33.33
        // price = (80 + 33.33) * 1.10 = 124.66...
        let expected = (80.0 + 800.0 / 24.0) * 1.10;
        assert!((price - expected).abs() < 0.01);
    }
}

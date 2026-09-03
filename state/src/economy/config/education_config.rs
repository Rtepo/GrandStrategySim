//! Education configuration (Phase E.5 — eradicate magic numbers).
//!
//! All education-related magic numbers identified in the forensic audit
//! (D5–D11) are consolidated here as configurable, wage-relative parameters.
//! Every monetary value is an `average_wage` multiplier (Rule 2), and every
//! physical ratio is a dimensionless constant (Rule 3 exempt, Rule 15 compliant).

use serde::{Deserialize, Serialize};

/// Education configuration — replaces all hardcoded education constants.
///
/// # Rules
/// * All monetary thresholds are `average_wage` multipliers (Rule 2).
/// * All physical ratios (seats per worker, transition fractions) are
///   dimensionless constants grounded in physical reality (Rule 3 exempt).
/// * No nominal magic numbers (D5–D11).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EducationConfig {
    // ── D5: Education need base fraction ──
    /// Fraction of population that needs education slots (D5).
    /// Default 0.20 — roughly 20% of population is school-age.
    /// When age structure is available, `age_groups.children` is used instead.
    #[serde(default = "default_education_need_fraction")]
    pub education_need_fraction: f64,

    // ── D6: Poverty threshold ──
    /// Savings-per-capita threshold below which a class is considered in poverty,
    /// as a fraction of `average_wage` (D6). Default 0.05 (5% of avg wage).
    #[serde(default = "default_poverty_threshold_wage_mult")]
    pub poverty_threshold_wage_mult: f64,

    // ── D7: Urban, health, and poverty multipliers ──
    /// Urban education need multiplier (D7). Urban populations have higher
    /// education demand due to urban labor market requirements.
    #[serde(default = "default_urban_education_mult")]
    pub urban_education_mult: f64,

    /// Health-critical/poor education need multiplier (D7).
    /// Classes with poor health need more retraining/education.
    #[serde(default = "default_health_critical_education_mult")]
    pub health_critical_education_mult: f64,

    /// Poverty education need multiplier (D7). Classes in poverty get
    /// bonus education need (retraining/upskilling programs).
    #[serde(default = "default_poverty_education_mult")]
    pub poverty_education_mult: f64,

    // ── D8: Adult reentry share ──
    /// Fraction of working-age adults with basic education who re-enter
    /// education each turn (D8). Scales with coverage.
    #[serde(default = "default_adult_reentry_share")]
    pub adult_reentry_share: f64,

    // ── D9: Rural subsistence income ──
    /// Rural subsistence income as fraction of `average_wage` (D9).
    /// Subsistence farmers don't earn full market wages.
    #[serde(default = "default_rural_subsistence_wage_mult")]
    pub rural_subsistence_wage_mult: f64,

    // ── D10: Emancipated serf seed capital ──
    /// Seed capital grant to emancipated serfs, as fraction of `average_wage`
    /// per person (D10). Funded by Treasury (E.2 fix — no fiat).
    #[serde(default = "default_emancipation_seed_capital_wage_mult")]
    pub emancipation_seed_capital_wage_mult: f64,

    // ── D11: Expert/skilled wage premiums ──
    /// Base expert wage premium multiplier (D11). Expert wage = base * this.
    #[serde(default = "default_expert_premium_base")]
    pub expert_premium_base: f64,

    /// Brain-drain scaling factor for expert premium (D11).
    /// Expert premium = base + brain_drain * this.
    #[serde(default = "default_expert_premium_brain_drain_mult")]
    pub expert_premium_brain_drain_mult: f64,

    /// Expert scarcity bonus: when expert_share < this threshold, premium rises.
    #[serde(default = "default_expert_scarcity_threshold")]
    pub expert_scarcity_threshold: f64,

    /// Base skilled wage premium multiplier (D11).
    #[serde(default = "default_skilled_premium_base")]
    pub skilled_premium_base: f64,

    /// Brain-drain scaling factor for skilled premium (D11).
    #[serde(default = "default_skilled_premium_brain_drain_mult")]
    pub skilled_premium_brain_drain_mult: f64,

    // ── E.9: School system parameters ──
    /// Seats per worker for middle schools (E.9.1).
    #[serde(default = "default_middle_seats_per_worker")]
    pub middle_seats_per_worker: f64,

    /// Capacity boost for primary schools in 2-tier systems that fold
    /// middle school into primary (E.9.1). The 8-grade primary has a
    /// larger student body covering ages 6-14.
    #[serde(default = "default_no_middle_primary_capacity_boost")]
    pub no_middle_primary_capacity_boost: f64,

    // ── E.4: Child labor + education ──
    /// When computing effective child labor fraction, education consumption
    /// reduces child labor by this factor per unit of educated_child_fraction.
    /// 1.0 means fully educated children do not work (E.4).
    #[serde(default = "default_education_child_labor_displacement")]
    pub education_child_labor_displacement: f64,
}

impl Default for EducationConfig {
    fn default() -> Self {
        Self {
            education_need_fraction: default_education_need_fraction(),
            poverty_threshold_wage_mult: default_poverty_threshold_wage_mult(),
            urban_education_mult: default_urban_education_mult(),
            health_critical_education_mult: default_health_critical_education_mult(),
            poverty_education_mult: default_poverty_education_mult(),
            adult_reentry_share: default_adult_reentry_share(),
            rural_subsistence_wage_mult: default_rural_subsistence_wage_mult(),
            emancipation_seed_capital_wage_mult: default_emancipation_seed_capital_wage_mult(),
            expert_premium_base: default_expert_premium_base(),
            expert_premium_brain_drain_mult: default_expert_premium_brain_drain_mult(),
            expert_scarcity_threshold: default_expert_scarcity_threshold(),
            skilled_premium_base: default_skilled_premium_base(),
            skilled_premium_brain_drain_mult: default_skilled_premium_brain_drain_mult(),
            middle_seats_per_worker: default_middle_seats_per_worker(),
            no_middle_primary_capacity_boost: default_no_middle_primary_capacity_boost(),
            education_child_labor_displacement: default_education_child_labor_displacement(),
        }
    }
}

// ── Default value functions ──

fn default_education_need_fraction() -> f64 {
    0.20
}
fn default_poverty_threshold_wage_mult() -> f64 {
    0.05
}
fn default_urban_education_mult() -> f64 {
    1.2
}
fn default_health_critical_education_mult() -> f64 {
    1.3
}
fn default_poverty_education_mult() -> f64 {
    1.5
}
fn default_adult_reentry_share() -> f64 {
    0.05
}
fn default_rural_subsistence_wage_mult() -> f64 {
    0.3
}
fn default_emancipation_seed_capital_wage_mult() -> f64 {
    0.1
}
fn default_expert_premium_base() -> f64 {
    3.0
}
fn default_expert_premium_brain_drain_mult() -> f64 {
    5.0
}
fn default_expert_scarcity_threshold() -> f64 {
    0.2
}
fn default_skilled_premium_base() -> f64 {
    1.5
}
fn default_skilled_premium_brain_drain_mult() -> f64 {
    2.0
}
fn default_middle_seats_per_worker() -> f64 {
    9.0
}
fn default_no_middle_primary_capacity_boost() -> f64 {
    1.25
}
fn default_education_child_labor_displacement() -> f64 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_no_nominal_magic_numbers() {
        let config = EducationConfig::default();
        // All monetary values are wage multipliers (dimensionless ratios).
        assert!(config.poverty_threshold_wage_mult > 0.0);
        assert!(config.rural_subsistence_wage_mult > 0.0);
        assert!(config.emancipation_seed_capital_wage_mult > 0.0);
        // All physical ratios are positive.
        assert!(config.education_need_fraction > 0.0);
        assert!(config.urban_education_mult > 0.0);
        assert!(config.middle_seats_per_worker > 0.0);
    }

    #[test]
    fn expert_premium_components_are_configurable() {
        let config = EducationConfig::default();
        // D11: previously hardcoded as 3.0, 5.0, 0.2, 1.5, 2.0
        assert_eq!(config.expert_premium_base, 3.0);
        assert_eq!(config.expert_premium_brain_drain_mult, 5.0);
        assert_eq!(config.expert_scarcity_threshold, 0.2);
        assert_eq!(config.skilled_premium_base, 1.5);
        assert_eq!(config.skilled_premium_brain_drain_mult, 2.0);
    }
}

//! Labor market configuration (Phase 86.5A).
//!
//! Extracts CRITICAL magic numbers from `economy/labor/labor.rs` into a
//! serializable config struct.

use serde::{Deserialize, Serialize};

/// Configuration for labor market dynamics.
///
/// Replaces hardcoded magic numbers in `labor.rs` with configurable values.
/// All fiat values are multipliers of `effective_wage` (clamped to
/// `minimum_subsistence_wage`) to ensure inflation-proof scaling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaborConfig {
    /// Base life expectancy at birth (years, before healthcare modifiers).
    #[serde(default = "default_base_life_expectancy")]
    pub base_life_expectancy: f64,

    /// Maximum life expectancy (years, with full healthcare).
    #[serde(default = "default_max_life_expectancy")]
    pub max_life_expectancy: f64,

    /// Base healthy life expectancy (years, before healthcare modifiers).
    #[serde(default = "default_base_healthy_life_expectancy")]
    pub base_healthy_life_expectancy: f64,

    /// Maximum healthy life expectancy (years, with full healthcare).
    #[serde(default = "default_max_healthy_life_expectancy")]
    pub max_healthy_life_expectancy: f64,

    /// Healthcare quality contribution to life expectancy (years per 100% quality).
    #[serde(default = "default_healthcare_life_expectancy_bonus")]
    pub healthcare_life_expectancy_bonus: f64,

    /// Healthcare quality contribution to healthy life expectancy (years per 100% quality).
    #[serde(default = "default_healthcare_healthy_life_bonus")]
    pub healthcare_healthy_life_bonus: f64,

    /// Medical infrastructure contribution to life expectancy (years per unit).
    #[serde(default = "default_medical_infra_life_bonus")]
    pub medical_infra_life_bonus: f64,

    /// Medical infrastructure contribution to healthy life expectancy (years per unit).
    #[serde(default = "default_medical_infra_healthy_life_bonus")]
    pub medical_infra_healthy_life_bonus: f64,

    /// Criminal death rate per crime index point (fraction of population).
    #[serde(default = "default_criminal_death_rate")]
    pub criminal_death_rate: f64,

    /// Minimum death rate (prevents zero mortality from over-healthcare).
    #[serde(default = "default_min_death_rate")]
    pub min_death_rate: f64,

    /// Medical infrastructure death rate reduction per unit.
    #[serde(default = "default_medical_infra_death_reduction")]
    pub medical_infra_death_reduction: f64,

    /// Healthcare quality death rate reduction per 100% quality.
    #[serde(default = "default_healthcare_death_reduction")]
    pub healthcare_death_reduction: f64,

    /// Safety index threshold below which fear-driven emigration occurs.
    #[serde(default = "default_fear_emigration_safety_threshold")]
    pub fear_emigration_safety_threshold: f64,

    /// Fear emigration rate per point below safety threshold.
    #[serde(default = "default_fear_emigration_rate")]
    pub fear_emigration_rate: f64,

    /// Frictional unemployment base rate when job agency is active.
    #[serde(default = "default_frictional_unemployment_with_agency")]
    pub frictional_unemployment_with_agency: f64,

    /// Frictional unemployment base rate when job agency is inactive.
    #[serde(default = "default_frictional_unemployment_without_agency")]
    pub frictional_unemployment_without_agency: f64,

    /// Job agency unemployment reduction (percentage points).
    #[serde(default = "default_job_agency_unemployment_reduction")]
    pub job_agency_unemployment_reduction: f64,

    /// Wage pressure coefficient (how much unemployment reduces wages).
    #[serde(default = "default_wage_pressure_coefficient")]
    pub wage_pressure_coefficient: f64,

    /// Expert wage premium base (before brain drain modifier).
    #[serde(default = "default_expert_premium_base")]
    pub expert_premium_base: f64,

    /// Expert wage premium brain drain multiplier.
    #[serde(default = "default_expert_premium_brain_drain_mult")]
    pub expert_premium_brain_drain_mult: f64,

    /// Skilled wage premium base (before brain drain modifier).
    #[serde(default = "default_skilled_premium_base")]
    pub skilled_premium_base: f64,

    /// Skilled wage premium brain drain multiplier.
    #[serde(default = "default_skilled_premium_brain_drain_mult")]
    pub skilled_premium_brain_drain_mult: f64,

    /// Innate active disabled rate (fraction of children entering adulthood).
    #[serde(default = "default_innate_active_disabled_rate")]
    pub innate_active_disabled_rate: f64,

    /// Innate unable to work rate (fraction of children entering adulthood).
    #[serde(default = "default_innate_unable_to_work_rate")]
    pub innate_unable_to_work_rate: f64,

    /// Minimum productive period (years) for aging calculation.
    #[serde(default = "default_min_productive_period")]
    pub min_productive_period: f64,

    /// Age of adulthood (when children become adults).
    #[serde(default = "default_adulthood_age")]
    pub adulthood_age: f64,

    /// Male birth fraction (share of births that are male).
    #[serde(default = "default_male_birth_fraction")]
    pub male_birth_fraction: f64,

    /// Male work death share (fraction of work deaths that are male).
    #[serde(default = "default_male_work_death_share")]
    pub male_work_death_share: f64,

    /// Cyclical unemployment poverty pool weight.
    #[serde(default = "default_cyclical_poverty_weight")]
    pub cyclical_poverty_weight: f64,

    /// Structural unemployment poverty pool weight.
    #[serde(default = "default_structural_poverty_weight")]
    pub structural_poverty_weight: f64,

    /// Unemployment cyclical/structural split (fraction that is cyclical).
    #[serde(default = "default_cyclical_share")]
    pub cyclical_share: f64,
}

fn default_base_life_expectancy() -> f64 {
    60.0
}
fn default_max_life_expectancy() -> f64 {
    95.0
}
fn default_base_healthy_life_expectancy() -> f64 {
    50.0
}
fn default_max_healthy_life_expectancy() -> f64 {
    85.0
}
fn default_healthcare_life_expectancy_bonus() -> f64 {
    15.0
}
fn default_healthcare_healthy_life_bonus() -> f64 {
    10.0
}
fn default_medical_infra_life_bonus() -> f64 {
    0.20
}
fn default_medical_infra_healthy_life_bonus() -> f64 {
    0.15
}
fn default_criminal_death_rate() -> f64 {
    0.002
}
fn default_min_death_rate() -> f64 {
    0.003
}
fn default_medical_infra_death_reduction() -> f64 {
    0.00005
}
fn default_healthcare_death_reduction() -> f64 {
    0.003
}
fn default_fear_emigration_safety_threshold() -> f64 {
    40.0
}
fn default_fear_emigration_rate() -> f64 {
    0.015
}
fn default_frictional_unemployment_with_agency() -> f64 {
    1.5
}
fn default_frictional_unemployment_without_agency() -> f64 {
    3.0
}
fn default_job_agency_unemployment_reduction() -> f64 {
    2.0
}
fn default_wage_pressure_coefficient() -> f64 {
    0.002
}
fn default_expert_premium_base() -> f64 {
    3.0
}
fn default_expert_premium_brain_drain_mult() -> f64 {
    5.0
}
fn default_skilled_premium_base() -> f64 {
    1.5
}
fn default_skilled_premium_brain_drain_mult() -> f64 {
    2.0
}
fn default_innate_active_disabled_rate() -> f64 {
    0.0010
}
fn default_innate_unable_to_work_rate() -> f64 {
    0.0005
}
fn default_min_productive_period() -> f64 {
    20.0
}
fn default_adulthood_age() -> f64 {
    16.0
}
fn default_male_birth_fraction() -> f64 {
    0.505
}
fn default_male_work_death_share() -> f64 {
    0.90
}
fn default_cyclical_poverty_weight() -> f64 {
    0.2
}
fn default_structural_poverty_weight() -> f64 {
    0.3
}
fn default_cyclical_share() -> f64 {
    0.6
}

impl Default for LaborConfig {
    fn default() -> Self {
        LaborConfig {
            base_life_expectancy: default_base_life_expectancy(),
            max_life_expectancy: default_max_life_expectancy(),
            base_healthy_life_expectancy: default_base_healthy_life_expectancy(),
            max_healthy_life_expectancy: default_max_healthy_life_expectancy(),
            healthcare_life_expectancy_bonus: default_healthcare_life_expectancy_bonus(),
            healthcare_healthy_life_bonus: default_healthcare_healthy_life_bonus(),
            medical_infra_life_bonus: default_medical_infra_life_bonus(),
            medical_infra_healthy_life_bonus: default_medical_infra_healthy_life_bonus(),
            criminal_death_rate: default_criminal_death_rate(),
            min_death_rate: default_min_death_rate(),
            medical_infra_death_reduction: default_medical_infra_death_reduction(),
            healthcare_death_reduction: default_healthcare_death_reduction(),
            fear_emigration_safety_threshold: default_fear_emigration_safety_threshold(),
            fear_emigration_rate: default_fear_emigration_rate(),
            frictional_unemployment_with_agency: default_frictional_unemployment_with_agency(),
            frictional_unemployment_without_agency: default_frictional_unemployment_without_agency(
            ),
            job_agency_unemployment_reduction: default_job_agency_unemployment_reduction(),
            wage_pressure_coefficient: default_wage_pressure_coefficient(),
            expert_premium_base: default_expert_premium_base(),
            expert_premium_brain_drain_mult: default_expert_premium_brain_drain_mult(),
            skilled_premium_base: default_skilled_premium_base(),
            skilled_premium_brain_drain_mult: default_skilled_premium_brain_drain_mult(),
            innate_active_disabled_rate: default_innate_active_disabled_rate(),
            innate_unable_to_work_rate: default_innate_unable_to_work_rate(),
            min_productive_period: default_min_productive_period(),
            adulthood_age: default_adulthood_age(),
            male_birth_fraction: default_male_birth_fraction(),
            male_work_death_share: default_male_work_death_share(),
            cyclical_poverty_weight: default_cyclical_poverty_weight(),
            structural_poverty_weight: default_structural_poverty_weight(),
            cyclical_share: default_cyclical_share(),
        }
    }
}

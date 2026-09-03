//! Phase D10: Disability configuration.
//!
//! Extracts all disability-related magic numbers into a serializable config
//! struct, ensuring inflation-proof scaling and configurability (Rule 2).
//!
//! All nominal values are multipliers/fractions, not absolute fiat amounts.

use serde::{Deserialize, Serialize};

/// Phase D10: Centralized disability configuration.
///
/// Replaces hardcoded constants across the disability, rehabilitation,
/// care, and begging systems. All values are fractions or multipliers
/// to ensure inflation-proof scaling (Rule 2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisabilityConfig {
    // ── Accident casualty splits ────────────────────────────────────
    /// Fraction of workplace accident casualties that die (0.0–1.0).
    #[serde(default = "default_ohs_death_rate")]
    pub ohs_death_rate: f64,
    /// Severity of disability from workplace accidents (0.0–1.0).
    #[serde(default = "default_ohs_severity")]
    pub ohs_severity: f64,

    // ── Disaster casualty splits ────────────────────────────────────
    /// Fraction of disaster casualties that die (0.0–1.0).
    #[serde(default = "default_disaster_death_rate")]
    pub disaster_death_rate: f64,
    /// Severity of disability from disasters (0.0–1.0).
    #[serde(default = "default_disaster_severity")]
    pub disaster_severity: f64,

    // ── War wounded ────────────────────────────────────────────────
    /// Fraction of war wounded who recover fully (0.0–1.0).
    #[serde(default = "default_war_recovery_rate")]
    pub war_wounded_recovery_rate: f64,
    /// Severity of disability for war wounded who don't fully recover.
    #[serde(default = "default_war_severity")]
    pub war_wounded_severity: f64,

    // ── Inborn disability ──────────────────────────────────────────
    /// Fraction of children entering adulthood with working disability.
    #[serde(default = "default_inborn_working_disabled_rate")]
    pub inborn_working_disabled_rate: f64,
    /// Fraction of children entering adulthood unable to work.
    #[serde(default = "default_inborn_unable_to_work_rate")]
    pub inborn_unable_to_work_rate: f64,

    // ── Disabled labor capacity ────────────────────────────────────
    /// Coefficient for partially disabled labor capacity:
    /// `disabled_labor_capacity = active_disabled × participation × (1 - severity × this)`.
    #[serde(default = "default_disabled_labor_capacity_coeff")]
    pub disabled_labor_capacity_coeff: f64,

    // ── Caregiver ratios ───────────────────────────────────────────
    /// FTE caregiver requirement per DPS (fully dependent) person.
    #[serde(default = "default_dps_caregiver_ratio")]
    pub dps_caregiver_ratio: f64,
    /// FTE caregiver requirement per DDP (partially dependent) person.
    #[serde(default = "default_ddp_caregiver_ratio")]
    pub ddp_caregiver_ratio: f64,

    // ── Rehabilitation ─────────────────────────────────────────────
    /// Fraction of rehab patients who successfully return to work per turn.
    #[serde(default = "default_rehab_success_rate")]
    pub rehab_success_rate: f64,
    /// Severity reduction per successful rehab turn (0.0–1.0).
    #[serde(default = "default_rehab_severity_reduction")]
    pub rehab_severity_reduction: f64,

    // ── Sheltered workshop ─────────────────────────────────────────
    /// Productivity gap subsidy rate: subsidy = wage × severity × this.
    #[serde(default = "default_sheltered_subsidy_rate")]
    pub sheltered_subsidy_rate: f64,

    // ── Begging ────────────────────────────────────────────────────
    /// Subsistence threshold as fraction of average_wage for begging eligibility.
    #[serde(default = "default_begging_subsistence_fraction")]
    pub begging_subsistence_fraction: f64,
    /// Maximum fraction of donor savings extractable via begging per turn.
    #[serde(default = "default_begging_extraction_rate")]
    pub begging_max_extraction_rate: f64,
    /// Unrest per begging incident per recipient.
    #[serde(default = "default_begging_unrest_per_incident")]
    pub begging_unrest_per_incident: f64,
}

// ── Default value functions ─────────────────────────────────────────

fn default_ohs_death_rate() -> f64 {
    0.30
}
fn default_ohs_severity() -> f64 {
    0.6
}
fn default_disaster_death_rate() -> f64 {
    0.40
}
fn default_disaster_severity() -> f64 {
    0.75
}
fn default_war_recovery_rate() -> f64 {
    0.30
}
fn default_war_severity() -> f64 {
    0.7
}
fn default_inborn_working_disabled_rate() -> f64 {
    0.0010
}
fn default_inborn_unable_to_work_rate() -> f64 {
    0.0005
}
fn default_disabled_labor_capacity_coeff() -> f64 {
    1.0
}
fn default_dps_caregiver_ratio() -> f64 {
    1.0
}
fn default_ddp_caregiver_ratio() -> f64 {
    0.5
}
fn default_rehab_success_rate() -> f64 {
    0.15
}
fn default_rehab_severity_reduction() -> f64 {
    0.2
}
fn default_sheltered_subsidy_rate() -> f64 {
    1.0
}
fn default_begging_subsistence_fraction() -> f64 {
    0.5
}
fn default_begging_extraction_rate() -> f64 {
    0.01
}
fn default_begging_unrest_per_incident() -> f64 {
    0.001
}

impl Default for DisabilityConfig {
    fn default() -> Self {
        DisabilityConfig {
            ohs_death_rate: default_ohs_death_rate(),
            ohs_severity: default_ohs_severity(),
            disaster_death_rate: default_disaster_death_rate(),
            disaster_severity: default_disaster_severity(),
            war_wounded_recovery_rate: default_war_recovery_rate(),
            war_wounded_severity: default_war_severity(),
            inborn_working_disabled_rate: default_inborn_working_disabled_rate(),
            inborn_unable_to_work_rate: default_inborn_unable_to_work_rate(),
            disabled_labor_capacity_coeff: default_disabled_labor_capacity_coeff(),
            dps_caregiver_ratio: default_dps_caregiver_ratio(),
            ddp_caregiver_ratio: default_ddp_caregiver_ratio(),
            rehab_success_rate: default_rehab_success_rate(),
            rehab_severity_reduction: default_rehab_severity_reduction(),
            sheltered_subsidy_rate: default_sheltered_subsidy_rate(),
            begging_subsistence_fraction: default_begging_subsistence_fraction(),
            begging_max_extraction_rate: default_begging_extraction_rate(),
            begging_unrest_per_incident: default_begging_unrest_per_incident(),
        }
    }
}

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Default security power multiplier for state suppression.
/// Defaults to 1.0 (neutral effectiveness) to prevent a missing-data hazard
/// where state suppression becomes impossible with 0.0 security power.
fn default_security_power_multiplier() -> f64 {
    1.0
}

/// Default suppression intensity threshold (Phase 7).
/// Movements with intensity above 0.6 trigger automatic state suppression.
fn default_suppression_intensity_threshold() -> f64 {
    0.6
}

/// Sentiment drivers for political radicalization calculation (Phase 5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SentimentDrivers {
    /// Current real wage growth rate (YoY comparison)
    #[serde(default)]
    pub real_wage_growth: f64,

    /// Current inflation rate (YoY comparison)
    #[serde(default)]
    pub inflation_rate: f64,

    /// Current unemployment rate
    #[serde(default)]
    pub unemployment_rate: f64,

    /// Current savings depletion rate (YoY comparison)
    #[serde(default)]
    pub savings_depletion_rate: f64,

    /// SSE success rate
    #[serde(default)]
    pub sse_success_rate: f64,

    /// Campaign effectiveness
    #[serde(default)]
    pub campaign_effectiveness: f64,

    /// Government approval rating
    #[serde(default)]
    pub government_approval: f64,

    /// Phase 6.2: Exploitation penalty multiplier (applied when overwork + poverty detected)
    #[serde(default)]
    pub exploitation_penalty: f64,
}

/// Configuration for all Chaos Factor mechanics (loaded via JSON)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ChaosConfig {
    // === SENTIMENT THRESHOLDS & WEIGHTS ===
    /// Radical threshold for mass movement spawning (0-1, fraction of regional population)
    #[serde(default)]
    pub radical_threshold: f64,

    /// Sentiment shift magnitude base (0-1, max shift per turn)
    #[serde(default)]
    pub shift_magnitude_base: f64,

    /// Inflation weight in radicalization pressure (0-1)
    #[serde(default)]
    pub inflation_weight: f64,

    /// Real wage weight in radicalization pressure (0-1)
    #[serde(default)]
    pub real_wage_weight: f64,

    /// Unemployment weight in radicalization pressure (0-1)
    #[serde(default)]
    pub unemployment_weight: f64,

    /// Savings depletion weight in radicalization pressure (0-1)
    #[serde(default)]
    pub savings_depletion_weight: f64,

    /// SSE success weight in loyalization pressure (0-1)
    #[serde(default)]
    pub sse_success_weight: f64,

    /// Campaign effectiveness weight in loyalization pressure (0-1)
    #[serde(default)]
    pub campaign_effectiveness_weight: f64,

    /// Government approval weight in loyalization pressure (0-1)
    #[serde(default)]
    pub government_approval_weight: f64,

    // === SUPPRESSION MECHANICS ===
    /// Base cost per participant for state suppression (currency units)
    #[serde(default)]
    pub suppression_cost_per_participant: f64,

    /// Phase 7 (F7): Intensity threshold above which the state automatically
    /// attempts to suppress a mass movement (0.0–1.0). Movements below this
    /// threshold are considered too small to warrant state force.
    #[serde(default = "default_suppression_intensity_threshold")]
    pub suppression_intensity_threshold: f64,

    /// Defaults to 1.0 (neutral effectiveness) to prevent a missing-data hazard
    /// where state suppression becomes impossible with 0.0 security power.
    #[serde(default = "default_security_power_multiplier")]
    pub security_power_multiplier: f64,

    /// Casualty rate during suppression (0-1, fraction of participants killed)
    #[serde(default)]
    pub casualty_rate: f64,

    /// Backlash magnitude (0-1, fraction of undecided that radicalize after suppression)
    #[serde(default)]
    pub backlash_magnitude: f64,

    // === DISRUPTION MULTIPLIERS ===
    /// Industrial strike disruption multiplier (0-1)
    #[serde(default)]
    pub strike_disruption_multiplier: f64,

    /// Riot disruption multiplier (0-1)
    #[serde(default)]
    pub riot_disruption_multiplier: f64,

    /// Occupation disruption multiplier (0-1)
    #[serde(default)]
    pub occupation_disruption_multiplier: f64,

    /// Boycott disruption multiplier (0-1)
    #[serde(default)]
    pub boycott_disruption_multiplier: f64,

    /// Peaceful protest disruption multiplier (0-1)
    #[serde(default)]
    pub protest_disruption_multiplier: f64,

    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

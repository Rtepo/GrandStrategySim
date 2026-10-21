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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

impl Default for ChaosConfig {
    /// v4.19 WS5: Real defaults. The derived all-zero config made mass
    /// movements unreachable (`radical_threshold` semantics aside, zero
    /// sentiment weights meant sentiment could never move) and neutered
    /// suppression/disruption economics entirely.
    fn default() -> Self {
        Self {
            // ~5% of a region radicalized is enough to organize a movement.
            radical_threshold: 0.05,
            // Bounded per-turn drift so a shock takes ~10-30 turns to move
            // a class meaningfully — no instant revolutions.
            shift_magnitude_base: 0.03,
            inflation_weight: 0.30,
            real_wage_weight: 0.40,
            unemployment_weight: 0.50,
            savings_depletion_weight: 0.30,
            sse_success_weight: 0.10,
            campaign_effectiveness_weight: 0.10,
            government_approval_weight: 0.20,
            suppression_cost_per_participant: 50.0,
            suppression_intensity_threshold: default_suppression_intensity_threshold(),
            security_power_multiplier: default_security_power_multiplier(),
            casualty_rate: 0.01,
            backlash_magnitude: 0.10,
            strike_disruption_multiplier: 0.30,
            riot_disruption_multiplier: 0.50,
            occupation_disruption_multiplier: 0.40,
            boycott_disruption_multiplier: 0.15,
            protest_disruption_multiplier: 0.10,
            extra: HashMap::new(),
        }
    }
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use serde_json::Value;

/// Sentiment drivers for political radicalization calculation (Phase 5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SentimentDrivers {
    /// Current real wage growth rate (YoY comparison)
    #[serde(rename = "wzrost_płacy_realnej", default)]
    pub real_wage_growth: f64,
    
    /// Current inflation rate (YoY comparison)
    #[serde(rename = "inflacja", default)]
    pub inflation_rate: f64,
    
    /// Current unemployment rate
    #[serde(rename = "bezrobocie", default)]
    pub unemployment_rate: f64,
    
    /// Current savings depletion rate (YoY comparison)
    #[serde(rename = "ubytek_oszczędności", default)]
    pub savings_depletion_rate: f64,
    
    /// SSE success rate
    #[serde(rename = "sukces_sse", default)]
    pub sse_success_rate: f64,
    
    /// Campaign effectiveness
    #[serde(rename = "skuteczność_kampanii", default)]
    pub campaign_effectiveness: f64,
    
    /// Government approval rating
    #[serde(rename = "aprobacja_rządu", default)]
    pub government_approval: f64,

    /// Phase 6.2: Exploitation penalty multiplier (applied when overwork + poverty detected)
    #[serde(rename = "kara_eksploatacji", default)]
    pub exploitation_penalty: f64,
}

/// Configuration for all Chaos Factor mechanics (loaded via JSON)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ChaosConfig {
    // === SENTIMENT THRESHOLDS & WEIGHTS ===
    
    /// Radical threshold for mass movement spawning (0-1, fraction of regional population)
    #[serde(rename = "próg_radykalizacji", default)]
    pub radical_threshold: f64,
    
    /// Sentiment shift magnitude base (0-1, max shift per turn)
    #[serde(rename = "magnituda_przesunięcia", default)]
    pub shift_magnitude_base: f64,
    
    /// Inflation weight in radicalization pressure (0-1)
    #[serde(rename = "waga_inflacji", default)]
    pub inflation_weight: f64,
    
    /// Real wage weight in radicalization pressure (0-1)
    #[serde(rename = "waga_płacy_realnej", default)]
    pub real_wage_weight: f64,
    
    /// Unemployment weight in radicalization pressure (0-1)
    #[serde(rename = "waga_bezrobocia", default)]
    pub unemployment_weight: f64,
    
    /// Savings depletion weight in radicalization pressure (0-1)
    #[serde(rename = "waga_ubytku_oszczędności", default)]
    pub savings_depletion_weight: f64,
    
    /// SSE success weight in loyalization pressure (0-1)
    #[serde(rename = "waga_sukcesu_sse", default)]
    pub sse_success_weight: f64,
    
    /// Campaign effectiveness weight in loyalization pressure (0-1)
    #[serde(rename = "waga_skuteczności_kampanii", default)]
    pub campaign_effectiveness_weight: f64,
    
    /// Government approval weight in loyalization pressure (0-1)
    #[serde(rename = "waga_aprobacji_rządu", default)]
    pub government_approval_weight: f64,
    
    // === SUPPRESSION MECHANICS ===
    
    /// Base cost per participant for state suppression (currency units)
    #[serde(rename = "koszt_supresji_na_uczestnika", default)]
    pub suppression_cost_per_participant: f64,
    
    /// Security sector power multiplier (0-1, effectiveness of police/military)
    #[serde(rename = "mnożnik_mocy_bezpieczeństwa", default)]
    pub security_power_multiplier: f64,
    
    /// Casualty rate during suppression (0-1, fraction of participants killed)
    #[serde(rename = "wskaźnik_ofiar", default)]
    pub casualty_rate: f64,
    
    /// Backlash magnitude (0-1, fraction of undecided that radicalize after suppression)
    #[serde(rename = "magnituda_odwetu", default)]
    pub backlash_magnitude: f64,
    
    // === DISRUPTION MULTIPLIERS ===
    
    /// Industrial strike disruption multiplier (0-1)
    #[serde(rename = "mnożnik_zakłóceń_strajku", default)]
    pub strike_disruption_multiplier: f64,
    
    /// Riot disruption multiplier (0-1)
    #[serde(rename = "mnożnik_zakłóceń_zamieszek", default)]
    pub riot_disruption_multiplier: f64,
    
    /// Occupation disruption multiplier (0-1)
    #[serde(rename = "mnożnik_zakłóceń_okupacji", default)]
    pub occupation_disruption_multiplier: f64,
    
    /// Boycott disruption multiplier (0-1)
    #[serde(rename = "mnożnik_zakłóceń_bojkotu", default)]
    pub boycott_disruption_multiplier: f64,
    
    /// Peaceful protest disruption multiplier (0-1)
    #[serde(rename = "mnożnik_zakłóceń_protestu", default)]
    pub protest_disruption_multiplier: f64,
    
    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

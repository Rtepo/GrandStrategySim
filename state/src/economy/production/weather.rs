//! Phase 15A: Weather event generation and seasonal modifiers.
//!
//! Each turn, per-region stochastic weather events are rolled based on
//! `ClimateProfile` + `Season` + RNG. Events apply temporary multipliers
//! to agriculture, tourism, energy, and construction. Events last 1–4 turns.

#![allow(missing_docs)]

use crate::society::geography::{ClimateProfile, Region};
use crate::state::{Country, Season};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Type of stochastic weather event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WeatherEventType {
    #[default]
    MildSeason,
    Drought,
    Flood,
    EarlyFrost,
    Heatwave,
    Storm,
}

/// A single active weather event affecting one or more regions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WeatherEvent {
    /// Type of weather event.
    #[serde(default)]
    pub event_type: WeatherEventType,
    /// Severity 0.0–1.0 (higher = more impactful).
    #[serde(default)]
    pub severity: f64,
    /// IDs of affected regions.
    #[serde(default)]
    pub affected_regions: Vec<String>,
    /// Remaining duration in turns.
    #[serde(default)]
    pub remaining_turns: u32,
    /// Turn the event was generated.
    #[serde(default)]
    pub start_turn: u32,
    /// Extra fields for forward compatibility.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Country-level weather state, persisted on `Country`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WeatherState {
    /// Currently active weather events.
    #[serde(default)]
    pub active_events: Vec<WeatherEvent>,
    /// Last turn an event was generated.
    #[serde(default)]
    pub last_event_turn: u32,
    /// RNG seed for deterministic weather generation.
    #[serde(default)]
    pub seed: u64,
    /// Extra fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Temporary modifiers applied to `SeasonalModifiers` by a weather event.
#[derive(Debug, Clone, Default)]
pub struct WeatherModifier {
    /// Multiplier applied to agriculture yield (1.0 = no change).
    pub agriculture_multiplier: f64,
    /// Multiplier applied to tourism (1.0 = no change).
    pub tourism_multiplier: f64,
    /// Multiplier applied to energy consumption (1.0 = no change).
    pub energy_multiplier: f64,
    /// Multiplier applied to construction efficiency (1.0 = no change).
    pub construction_multiplier: f64,
}

impl WeatherModifier {
    /// Returns the default (no-effect) modifier.
    pub fn neutral() -> Self {
        Self {
            agriculture_multiplier: 1.0,
            tourism_multiplier: 1.0,
            energy_multiplier: 1.0,
            construction_multiplier: 1.0,
        }
    }
}

/// Base probability of a weather event per region per turn, by climate profile and season.
fn event_probability(climate: ClimateProfile, season: Season) -> f64 {
    match (climate, season) {
        // Temperate: moderate event chance, higher in summer (drought/heat) and winter (frost/storm)
        (ClimateProfile::Temperate, Season::Summer) => 0.12,
        (ClimateProfile::Temperate, Season::Winter) => 0.10,
        (ClimateProfile::Temperate, _) => 0.06,
        // Continental: extreme swings, high event chance
        (ClimateProfile::Continental, Season::Winter) => 0.15,
        (ClimateProfile::Continental, Season::Summer) => 0.12,
        (ClimateProfile::Continental, _) => 0.08,
        // Coastal: floods and storms more likely
        (ClimateProfile::Coastal, Season::Autumn) => 0.14,
        (ClimateProfile::Coastal, Season::Winter) => 0.12,
        (ClimateProfile::Coastal, _) => 0.07,
        // Mountainous: harsh winters, frost
        (ClimateProfile::Mountainous, Season::Winter) => 0.16,
        (ClimateProfile::Mountainous, Season::Spring) => 0.10,
        (ClimateProfile::Mountainous, _) => 0.06,
        // Tropical: monsoon season (summer/autumn), heatwaves
        (ClimateProfile::Tropical, Season::Summer) => 0.15,
        (ClimateProfile::Tropical, Season::Autumn) => 0.12,
        (ClimateProfile::Tropical, _) => 0.08,
        // Desert: rare but extreme heat events
        (ClimateProfile::Desert, Season::Summer) => 0.14,
        (ClimateProfile::Desert, _) => 0.05,
        // Arctic: blizzards and extreme cold
        (ClimateProfile::Arctic, Season::Winter) => 0.18,
        (ClimateProfile::Arctic, _) => 0.10,
    }
}

/// Pick a weather event type based on climate and season.
fn pick_event_type<R: Rng>(
    rng: &mut R,
    climate: ClimateProfile,
    season: Season,
) -> WeatherEventType {
    let roll: f64 = rng.gen_range(0.0..1.0);
    match (climate, season) {
        (ClimateProfile::Coastal, Season::Autumn) | (ClimateProfile::Coastal, Season::Winter) => {
            if roll < 0.40 {
                WeatherEventType::Storm
            } else if roll < 0.75 {
                WeatherEventType::Flood
            } else {
                WeatherEventType::MildSeason
            }
        }
        (ClimateProfile::Temperate, Season::Summer)
        | (ClimateProfile::Continental, Season::Summer) => {
            if roll < 0.35 {
                WeatherEventType::Drought
            } else if roll < 0.65 {
                WeatherEventType::Heatwave
            } else if roll < 0.80 {
                WeatherEventType::Storm
            } else {
                WeatherEventType::MildSeason
            }
        }
        (ClimateProfile::Temperate, Season::Winter)
        | (ClimateProfile::Continental, Season::Winter)
        | (ClimateProfile::Mountainous, Season::Winter) => {
            if roll < 0.40 {
                WeatherEventType::EarlyFrost
            } else if roll < 0.70 {
                WeatherEventType::Storm
            } else {
                WeatherEventType::MildSeason
            }
        }
        (ClimateProfile::Mountainous, Season::Spring) => {
            if roll < 0.50 {
                WeatherEventType::EarlyFrost
            } else {
                WeatherEventType::MildSeason
            }
        }
        (ClimateProfile::Tropical, Season::Summer) | (ClimateProfile::Tropical, Season::Autumn) => {
            if roll < 0.45 {
                WeatherEventType::Flood
            } else if roll < 0.75 {
                WeatherEventType::Heatwave
            } else {
                WeatherEventType::Storm
            }
        }
        _ => {
            if roll < 0.20 {
                WeatherEventType::Storm
            } else if roll < 0.30 {
                WeatherEventType::Drought
            } else {
                WeatherEventType::MildSeason
            }
        }
    }
}

/// Compute the modifier for a weather event type and severity.
pub fn weather_modifier(event_type: WeatherEventType, severity: f64) -> WeatherModifier {
    let s = severity.clamp(0.0, 1.0);
    match event_type {
        WeatherEventType::MildSeason => WeatherModifier::neutral(),
        WeatherEventType::Drought => WeatherModifier {
            agriculture_multiplier: 1.0 - s * 0.6,
            tourism_multiplier: 1.0 - s * 0.2,
            energy_multiplier: 1.0 + s * 0.15,
            construction_multiplier: 1.0 - s * 0.1,
        },
        WeatherEventType::Flood => WeatherModifier {
            agriculture_multiplier: 1.0 - s * 0.4,
            tourism_multiplier: 1.0 - s * 0.5,
            energy_multiplier: 1.0 - s * 0.1,
            construction_multiplier: 1.0 - s * 0.5,
        },
        WeatherEventType::EarlyFrost => WeatherModifier {
            agriculture_multiplier: 1.0 - s * 0.5,
            tourism_multiplier: 1.0 - s * 0.1,
            energy_multiplier: 1.0 + s * 0.3,
            construction_multiplier: 1.0 - s * 0.3,
        },
        WeatherEventType::Heatwave => WeatherModifier {
            agriculture_multiplier: 1.0 - s * 0.3,
            tourism_multiplier: 1.0 + s * 0.2,
            energy_multiplier: 1.0 + s * 0.25,
            construction_multiplier: 1.0 - s * 0.2,
        },
        WeatherEventType::Storm => WeatherModifier {
            agriculture_multiplier: 1.0 - s * 0.2,
            tourism_multiplier: 1.0 - s * 0.6,
            energy_multiplier: 1.0 + s * 0.05,
            construction_multiplier: 1.0 - s * 0.4,
        },
    }
}

/// Process weather for one turn: expire old events, generate new ones.
///
/// # Arguments
/// * `country` - Mutable country (reads regions for climate, writes weather state).
/// * `season` - Current season.
/// * `turn` - Current global turn.
///
/// # Rules
/// * Each region has an independent roll based on its `ClimateProfile` and `Season`.
/// * Events last 1–4 turns.
/// * Multiple events can be active simultaneously.
/// * `MildSeason` events are not stored (they represent normal weather).
pub fn process_weather_turn(country: &mut Country, season: Season, turn: u32) {
    let seed = country.weather_state.seed;
    let mut rng: rand::rngs::StdRng = if seed > 0 {
        rand::SeedableRng::seed_from_u64(seed.wrapping_add(turn as u64))
    } else {
        rand::SeedableRng::seed_from_u64(turn as u64)
    };

    // Expire events whose remaining_turns has reached 0.
    country.weather_state.active_events.retain(|e| e.remaining_turns > 0);
    for event in &mut country.weather_state.active_events {
        event.remaining_turns = event.remaining_turns.saturating_sub(1);
    }

    // Roll for new events per region.
    let region_data: Vec<(String, ClimateProfile)> = country
        .regions
        .iter()
        .map(|r| (r.id.clone(), r.climate_profile))
        .collect();

    for (region_id, climate) in region_data {
        let prob = event_probability(climate, season);
        let roll: f64 = rng.gen_range(0.0..1.0);
        if roll < prob {
            let event_type = pick_event_type(&mut rng, climate, season);
            if event_type == WeatherEventType::MildSeason {
                continue;
            }
            let severity: f64 = rng.gen_range(0.3..1.0);
            let duration: u32 = rng.gen_range(1..=4);
            country.weather_state.active_events.push(WeatherEvent {
                event_type,
                severity,
                affected_regions: vec![region_id],
                remaining_turns: duration,
                start_turn: turn,
                extra: Map::new(),
            });
        }
    }

    country.weather_state.last_event_turn = turn;
}

/// Get the combined weather modifier for a specific region.
///
/// # Arguments
/// * `weather_state` - Country weather state.
/// * `region_id` - Region to check.
///
/// # Returns
/// Combined `WeatherModifier` from all active events affecting this region.
pub fn get_region_weather_modifier(
    weather_state: &WeatherState,
    region_id: &str,
) -> WeatherModifier {
    let mut combined = WeatherModifier::neutral();
    for event in &weather_state.active_events {
        if event.affected_regions.iter().any(|r| r == region_id) {
            let m = weather_modifier(event.event_type, event.severity);
            combined.agriculture_multiplier *= m.agriculture_multiplier;
            combined.tourism_multiplier *= m.tourism_multiplier;
            combined.energy_multiplier *= m.energy_multiplier;
            combined.construction_multiplier *= m.construction_multiplier;
        }
    }
    combined
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weather_modifier_drought() {
        let m = weather_modifier(WeatherEventType::Drought, 1.0);
        assert!((m.agriculture_multiplier - 0.4).abs() < 0.01);
        assert!(m.energy_multiplier > 1.0);
    }

    #[test]
    fn test_weather_modifier_mild() {
        let m = weather_modifier(WeatherEventType::MildSeason, 0.8);
        assert!((m.agriculture_multiplier - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_event_expiry() {
        let mut state = WeatherState::default();
        state.active_events.push(WeatherEvent {
            event_type: WeatherEventType::Drought,
            severity: 0.5,
            affected_regions: vec!["r1".to_string()],
            remaining_turns: 1,
            start_turn: 1,
            extra: Map::new(),
        });
        // Simulate expiry
        for e in &mut state.active_events {
            e.remaining_turns = e.remaining_turns.saturating_sub(1);
        }
        state.active_events.retain(|e| e.remaining_turns > 0);
        assert!(state.active_events.is_empty());
    }

    #[test]
    fn test_combined_modifier() {
        let state = WeatherState {
            active_events: vec![
                WeatherEvent {
                    event_type: WeatherEventType::Drought,
                    severity: 0.5,
                    affected_regions: vec!["r1".to_string()],
                    remaining_turns: 2,
                    start_turn: 1,
                    extra: Map::new(),
                },
                WeatherEvent {
                    event_type: WeatherEventType::Heatwave,
                    severity: 0.5,
                    affected_regions: vec!["r1".to_string()],
                    remaining_turns: 1,
                    start_turn: 2,
                    extra: Map::new(),
                },
            ],
            last_event_turn: 2,
            seed: 0,
            extra: Map::new(),
        };
        let m = get_region_weather_modifier(&state, "r1");
        // Drought(0.5): ag=0.7, Heatwave(0.5): ag=0.85 → 0.7*0.85=0.595
        assert!((m.agriculture_multiplier - 0.7 * 0.85).abs() < 0.01);
    }
}

use crate::society::geography::ClimateProfile;
use crate::state::Season;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Seasonal modifiers for production and consumption (Phase 6.1)
///
/// World Generation & Climate Audit (v0.5.3): The `Default` implementation
/// returns neutral multipliers (1.0) instead of 0.0. Previously, deriving
/// `Default` produced all-zero fields, which zeroed out agricultural yield
/// whenever the `climate_season_matrix` lacked an entry for a given
/// (climate, season) pair. The manual default ensures that any missing entry
/// is treated as "no modifier" (1.0×) rather than "total suppression" (0.0×).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SeasonalModifiers {
    /// Energy consumption multiplier by season (1.0 = baseline)
    #[serde(default = "one_f64")]
    pub energy_multiplier: f64,

    /// Agricultural yield multiplier by season (1.0 = baseline)
    #[serde(default = "one_f64")]
    pub agriculture_multiplier: f64,

    /// Services/tourism multiplier by season (1.0 = baseline)
    #[serde(default = "one_f64")]
    pub services_multiplier: f64,

    /// Phase 6.3: Tourism-specific multiplier (coastal/mountainous regions)
    #[serde(default = "one_f64")]
    pub tourism_multiplier: f64,

    /// Heating demand multiplier (1.0 = baseline)
    #[serde(default = "one_f64")]
    pub heating_demand_multiplier: f64,

    /// Construction efficiency multiplier (1.0 = baseline)
    #[serde(default = "one_f64")]
    pub construction_multiplier: f64,

    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Serde default helper: returns 1.0 for `f64` fields.
fn one_f64() -> f64 {
    1.0
}

impl Default for SeasonalModifiers {
    /// Returns neutral modifiers (all 1.0×) so that missing matrix entries
    /// do not suppress production. This is the critical fix for the
    /// Phantom Harvest bug.
    fn default() -> Self {
        Self {
            energy_multiplier: 1.0,
            agriculture_multiplier: 1.0,
            services_multiplier: 1.0,
            tourism_multiplier: 1.0,
            heating_demand_multiplier: 1.0,
            construction_multiplier: 1.0,
            extra: Map::new(),
        }
    }
}

/// Climate configuration with climate-season matrix (Phase 6.1)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ClimateConfig {
    /// Mapping: ClimateProfile -> Season -> SeasonalModifiers
    #[serde(default)]
    pub climate_season_matrix: HashMap<(ClimateProfile, Season), SeasonalModifiers>,

    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl ClimateConfig {
    /// Get seasonal modifiers for a given climate and season
    pub fn get_modifiers(&self, climate: ClimateProfile, season: Season) -> SeasonalModifiers {
        self.climate_season_matrix
            .get(&(climate, season))
            .cloned()
            .unwrap_or_default()
    }

    /// World Generation & Climate Audit (v0.5.3): Populate the
    /// `climate_season_matrix` with biologically and physically sensible
    /// multipliers for all 7 climate profiles × 4 seasons = 28 entries.
    ///
    /// This method MUST be called during world generation, before the first
    /// turn runs, to ensure that agricultural yields, energy consumption,
    /// and other seasonal effects are properly modulated.
    ///
    /// # Multiplier Rationale
    ///
    /// **Agriculture**: Tropical climates support year-round growing (1.1-1.3×),
    /// temperate climates peak in summer/autumn (1.0-1.1×), continental
    /// climates have a short productive summer (1.2×) but near-zero winter,
    /// mountainous climates have reduced growing seasons, deserts have very
    /// limited agriculture, and arctic climates are nearly barren.
    ///
    /// **Energy**: Cold climates (Continental, Arctic, Mountainous) have high
    /// winter heating demand (1.3-1.8×). Hot climates (Desert, Tropical) have
    /// high summer cooling demand (1.2-1.4×).
    ///
    /// **Tourism/Services**: Coastal and Temperate climates peak in summer.
    /// Mountainous climates peak in winter (ski tourism) and summer (hiking).
    ///
    /// **Construction**: Reduced in winter for cold climates, near-zero in
    /// arctic winter. Year-round in tropical climates.
    pub fn populate_defaults(&mut self) {
        use ClimateProfile::*;
        use Season::*;

        // Helper to build a modifier entry concisely.
        let m = |agri: f64, energy: f64, services: f64, tourism: f64, heating: f64, constr: f64| {
            SeasonalModifiers {
                energy_multiplier: energy,
                agriculture_multiplier: agri,
                services_multiplier: services,
                tourism_multiplier: tourism,
                heating_demand_multiplier: heating,
                construction_multiplier: constr,
                extra: Map::new(),
            }
        };

        // Temperate: Four distinct seasons, moderate extremes.
        self.climate_season_matrix
            .insert((Temperate, Spring), m(1.0, 0.9, 1.0, 1.0, 0.8, 1.0));
        self.climate_season_matrix
            .insert((Temperate, Summer), m(1.1, 1.0, 1.1, 1.2, 0.5, 1.1));
        self.climate_season_matrix
            .insert((Temperate, Autumn), m(1.0, 1.0, 1.0, 1.0, 0.8, 1.0));
        self.climate_season_matrix
            .insert((Temperate, Winter), m(0.3, 1.3, 0.8, 0.6, 1.5, 0.6));

        // Continental: Extreme temperature swings, harsh winters.
        self.climate_season_matrix
            .insert((Continental, Spring), m(0.9, 1.0, 0.9, 0.8, 1.0, 0.9));
        self.climate_season_matrix
            .insert((Continental, Summer), m(1.2, 1.1, 1.1, 1.0, 0.4, 1.2));
        self.climate_season_matrix
            .insert((Continental, Autumn), m(0.9, 1.2, 0.9, 0.8, 1.2, 0.8));
        self.climate_season_matrix
            .insert((Continental, Winter), m(0.1, 1.6, 0.7, 0.5, 1.8, 0.3));

        // Mountainous: Harsh winters, mild summers, high energy demand.
        self.climate_season_matrix
            .insert((Mountainous, Spring), m(0.7, 1.1, 0.9, 0.8, 1.1, 0.7));
        self.climate_season_matrix
            .insert((Mountainous, Summer), m(1.0, 1.0, 1.1, 1.3, 0.5, 1.0));
        self.climate_season_matrix
            .insert((Mountainous, Autumn), m(0.6, 1.2, 0.9, 0.9, 1.3, 0.6));
        self.climate_season_matrix
            .insert((Mountainous, Winter), m(0.05, 1.5, 0.8, 1.1, 1.8, 0.2));

        // Coastal: Mild winters, tourism boost in summer.
        self.climate_season_matrix
            .insert((Coastal, Spring), m(1.0, 0.9, 1.0, 1.0, 0.7, 1.0));
        self.climate_season_matrix
            .insert((Coastal, Summer), m(1.0, 1.0, 1.1, 1.4, 0.3, 1.1));
        self.climate_season_matrix
            .insert((Coastal, Autumn), m(1.0, 1.0, 1.0, 1.0, 0.6, 1.0));
        self.climate_season_matrix
            .insert((Coastal, Winter), m(0.5, 1.2, 0.9, 0.7, 1.2, 0.8));

        // Tropical: Hot year-round, monsoon season, high agricultural productivity.
        self.climate_season_matrix
            .insert((Tropical, Spring), m(1.2, 1.1, 1.0, 1.0, 0.2, 1.1));
        self.climate_season_matrix
            .insert((Tropical, Summer), m(1.3, 1.3, 0.9, 0.8, 0.1, 1.1));
        self.climate_season_matrix
            .insert((Tropical, Autumn), m(1.2, 1.2, 1.0, 1.0, 0.2, 1.1));
        self.climate_season_matrix
            .insert((Tropical, Winter), m(1.1, 1.1, 1.0, 1.1, 0.3, 1.1));

        // Desert: Extreme heat, cold nights, water scarcity.
        self.climate_season_matrix
            .insert((Desert, Spring), m(0.5, 1.1, 0.9, 0.8, 0.3, 0.9));
        self.climate_season_matrix
            .insert((Desert, Summer), m(0.3, 1.4, 0.7, 0.5, 0.1, 0.7));
        self.climate_season_matrix
            .insert((Desert, Autumn), m(0.6, 1.1, 0.9, 0.9, 0.3, 0.9));
        self.climate_season_matrix
            .insert((Desert, Winter), m(0.4, 1.2, 0.9, 0.8, 0.8, 0.9));

        // Arctic: Permafrost, extreme cold, limited activity.
        self.climate_season_matrix
            .insert((Arctic, Spring), m(0.1, 1.3, 0.7, 0.6, 1.3, 0.3));
        self.climate_season_matrix
            .insert((Arctic, Summer), m(0.5, 1.0, 0.9, 1.0, 0.5, 0.7));
        self.climate_season_matrix
            .insert((Arctic, Autumn), m(0.1, 1.4, 0.7, 0.6, 1.5, 0.3));
        self.climate_season_matrix
            .insert((Arctic, Winter), m(0.0, 1.8, 0.5, 0.4, 2.0, 0.1));
    }
}

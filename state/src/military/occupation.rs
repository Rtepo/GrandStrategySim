//! Phase 71: Occupation mechanics with culture-based garrison requirements.
//!
//! When a region's `RegionControl` changes to `Occupied(attacker)`:
//! - If the region's dominant culture matches the attacker's culture → instant
//!   integration, no garrison penalty.
//! - If foreign culture → `OccupationState` tracks garrison requirements,
//!   unrest level, and integration progress.
//!
//! Garrison requirement scales with population and cultural distance.
//! Insufficient garrison → unrest rises → rebellion risk.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::military::fronts::RegionControl;

// ============================================================================
// OCCUPATION STATE
// ============================================================================

/// State of an occupied region.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OccupationState {
    /// The occupying country.
    pub occupier: String,
    /// The occupied region ID.
    pub region_id: String,
    /// Turn when occupation began.
    pub occupation_start_turn: u32,
    /// Required garrison manpower to maintain order.
    pub garrison_required: i64,
    /// Current garrison manpower stationed in the region.
    pub current_garrison: i64,
    /// Unrest level (0.0 = calm, 1.0 = open rebellion).
    pub unrest_level: f64,
    /// Integration progress (0.0 = just occupied, 1.0 = fully integrated).
    pub integration_progress: f64,
    /// Cultural distance between occupier and occupied (0.0 = same, 1.0 = max).
    pub cultural_distance: f64,
    /// Whether the occupation is considered integrated (same culture).
    pub is_integrated: bool,
}

impl OccupationState {
    /// Creates a new occupation state for a region.
    ///
    /// # Arguments
    /// * `occupier` - The occupying country name.
    /// * `region_id` - The occupied region ID.
    /// * `occupation_start_turn` - Turn when occupation began.
    /// * `region_population` - Population of the occupied region.
    /// * `cultural_distance` - 0.0 = same culture, 1.0 = max distance.
    pub fn new(
        occupier: String,
        region_id: String,
        occupation_start_turn: u32,
        region_population: i64,
        cultural_distance: f64,
    ) -> Self {
        let is_integrated = cultural_distance < 0.1;

        let garrison_required = if is_integrated {
            0
        } else {
            // Garrison scales with population and cultural distance
            // Base: 1 soldier per 100 population, scaled by cultural distance
            ((region_population as f64 / 100.0) * (0.5 + cultural_distance * 0.5)) as i64
        };

        let unrest_level = if is_integrated {
            0.0
        } else {
            // Initial unrest scales with cultural distance
            0.3 + cultural_distance * 0.4
        };

        Self {
            occupier,
            region_id,
            occupation_start_turn,
            garrison_required,
            current_garrison: 0,
            unrest_level,
            integration_progress: if is_integrated { 1.0 } else { 0.0 },
            cultural_distance,
            is_integrated,
        }
    }

    /// Returns true if the garrison is insufficient.
    pub fn is_garrison_insufficient(&self) -> bool {
        !self.is_integrated && self.current_garrison < self.garrison_required
    }

    /// Returns the garrison deficit (required - current), or 0 if sufficient.
    pub fn garrison_deficit(&self) -> i64 {
        if self.is_integrated {
            return 0;
        }
        (self.garrison_required - self.current_garrison).max(0)
    }

    /// Returns the garrison sufficiency ratio (current / required), capped at 1.0.
    pub fn garrison_ratio(&self) -> f64 {
        if self.is_integrated || self.garrison_required == 0 {
            return 1.0;
        }
        (self.current_garrison as f64 / self.garrison_required as f64).min(1.0)
    }
}

// ============================================================================
// OCCUPATION CONFIG
// ============================================================================

/// Configuration for occupation mechanics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OccupationConfig {
    /// Unrest increase per turn when garrison is insufficient.
    pub unrest_increase_per_turn: f64,
    /// Unrest decrease per turn when garrison is sufficient.
    pub unrest_decrease_per_turn: f64,
    /// Integration progress per turn when garrison is sufficient.
    pub integration_rate: f64,
    /// Unrest level at which a rebellion is triggered.
    pub rebellion_threshold: f64,
    /// Maximum unrest level (cap).
    pub max_unrest: f64,
}

impl Default for OccupationConfig {
    fn default() -> Self {
        Self {
            unrest_increase_per_turn: 0.05,
            unrest_decrease_per_turn: 0.02,
            integration_rate: 0.01,
            rebellion_threshold: 0.8,
            max_unrest: 1.0,
        }
    }
}

// ============================================================================
// CULTURAL DISTANCE
// ============================================================================

/// Computes the cultural distance between two countries.
///
/// # Arguments
/// * `occupier_culture` - The cultural group of the occupier (e.g., "slavic").
/// * `occupied_culture` - The cultural group of the occupied region.
///
/// # Returns
/// 0.0 = same culture, 1.0 = maximum distance.
pub fn compute_cultural_distance(occupier_culture: &str, occupied_culture: &str) -> f64 {
    if occupier_culture == occupied_culture {
        return 0.0;
    }

    // Check if they're in the same broad cultural group
    let same_group = is_same_cultural_group(occupier_culture, occupied_culture);

    if same_group {
        // Same broad group, different subculture → moderate distance
        0.3
    } else {
        // Different group → high distance
        0.8
    }
}

/// Checks if two cultures belong to the same broad cultural group.
fn is_same_cultural_group(culture_a: &str, culture_b: &str) -> bool {
    // Known cultural groups (from society/cultures.rs CULTURAL_GROUPS)
    let groups: &[&[&str]] = &[
        &["slavic", "polish", "russian", "ukrainian", "czech", "serbian", "bulgarian"],
        &["germanic", "german", "english", "dutch", "swedish", "danish", "norwegian"],
        &["latin", "italian", "french", "spanish", "portuguese", "romanian"],
        &["nordic", "swedish", "danish", "norwegian", "finnish", "icelandic"],
        &["asian", "chinese", "japanese", "korean", "vietnamese"],
        &["middle_eastern", "arab", "persian", "turkish"],
    ];

    for group in groups {
        let a_in = group.iter().any(|&g| culture_a.eq_ignore_ascii_case(g));
        let b_in = group.iter().any(|&g| culture_b.eq_ignore_ascii_case(g));
        if a_in && b_in {
            return true;
        }
    }
    false
}

// ============================================================================
// OCCUPATION PROCESSING
// ============================================================================

/// Result of processing occupation for a single region.
#[derive(Debug, Clone, PartialEq)]
pub struct OccupationTurnResult {
    /// The region ID.
    pub region_id: String,
    /// Whether a rebellion was triggered.
    pub rebellion_triggered: bool,
    /// Updated unrest level.
    pub unrest_level: f64,
    /// Updated integration progress.
    pub integration_progress: f64,
    /// Whether the region is now fully integrated.
    pub fully_integrated: bool,
    /// Log messages.
    pub messages: Vec<String>,
}

/// Processes occupation state for a single turn.
///
/// This function:
/// 1. Updates unrest based on garrison sufficiency.
/// 2. Updates integration progress.
/// 3. Checks if a rebellion should be triggered.
///
/// # Arguments
/// * `state` - Mutable occupation state.
/// * `config` - Occupation configuration.
/// * `turn` - Current game turn.
///
/// # Returns
/// `OccupationTurnResult` with the updated state.
pub fn process_occupation_turn(
    state: &mut OccupationState,
    config: &OccupationConfig,
    _turn: u32,
) -> OccupationTurnResult {
    let mut messages = Vec::new();

    if state.is_integrated {
        return OccupationTurnResult {
            region_id: state.region_id.clone(),
            rebellion_triggered: false,
            unrest_level: 0.0,
            integration_progress: 1.0,
            fully_integrated: true,
            messages,
        };
    }

    // Update unrest based on garrison sufficiency
    if state.is_garrison_insufficient() {
        state.unrest_level = (state.unrest_level + config.unrest_increase_per_turn)
            .min(config.max_unrest);
        let deficit = state.garrison_deficit();
        messages.push(format!(
            "[OCCUPATION] Region {} garrison deficit: {} (unrest +{:.2} → {:.2})",
            state.region_id, deficit, config.unrest_increase_per_turn, state.unrest_level
        ));
    } else {
        state.unrest_level = (state.unrest_level - config.unrest_decrease_per_turn).max(0.0);
        // Integration progresses when garrison is sufficient
        state.integration_progress = (state.integration_progress + config.integration_rate).min(1.0);
        messages.push(format!(
            "[OCCUPATION] Region {} garrison sufficient (unrest -{:.2} → {:.2}, integration +{:.2} → {:.2})",
            state.region_id, config.unrest_decrease_per_turn, state.unrest_level,
            config.integration_rate, state.integration_progress
        ));
    }

    // Check for rebellion
    let rebellion_triggered = state.unrest_level >= config.rebellion_threshold;
    if rebellion_triggered {
        messages.push(format!(
            "[OCCUPATION] Region {} rebellion triggered! Unrest {:.2} >= threshold {:.2}",
            state.region_id, state.unrest_level, config.rebellion_threshold
        ));
    }

    // Check for full integration
    let fully_integrated = state.integration_progress >= 1.0;
    if fully_integrated {
        state.is_integrated = true;
        state.unrest_level = 0.0;
        messages.push(format!(
            "[OCCUPATION] Region {} fully integrated into {}",
            state.region_id, state.occupier
        ));
    }

    OccupationTurnResult {
        region_id: state.region_id.clone(),
        rebellion_triggered,
        unrest_level: state.unrest_level,
        integration_progress: state.integration_progress,
        fully_integrated,
        messages,
    }
}

/// Creates occupation states for all newly-occupied regions in a front.
///
/// # Arguments
/// * `region_control` - Current region control map from the front.
/// * `occupier` - The occupying country name.
/// * `occupier_culture` - The occupier's cultural group.
/// * `region_cultures` - Map of region_id → dominant culture.
/// * `region_populations` - Map of region_id → population.
/// * `turn` - Current turn.
///
/// # Returns
/// Map of region_id → OccupationState for newly occupied regions.
pub fn create_occupation_states(
    region_control: &HashMap<String, RegionControl>,
    occupier: &str,
    occupier_culture: &str,
    region_cultures: &HashMap<String, String>,
    region_populations: &HashMap<String, i64>,
    turn: u32,
) -> HashMap<String, OccupationState> {
    let mut states = HashMap::new();

    for (region_id, control) in region_control {
        if let RegionControl::Occupied(occupier_name) = control {
            if occupier_name == occupier {
                let region_culture = region_cultures.get(region_id)
                    .map(|s| s.as_str())
                    .unwrap_or("unknown");
                let region_pop = region_populations.get(region_id).copied().unwrap_or(1000);
                let cultural_distance = compute_cultural_distance(occupier_culture, region_culture);

                let state = OccupationState::new(
                    occupier.to_string(),
                    region_id.clone(),
                    turn,
                    region_pop,
                    cultural_distance,
                );
                states.insert(region_id.clone(), state);
            }
        }
    }

    states
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_occupation_same_culture_instant_integration() {
        let state = OccupationState::new(
            "Occupier".to_string(),
            "region_1".to_string(),
            1,
            100_000,
            0.0, // Same culture
        );

        assert!(state.is_integrated);
        assert_eq!(state.garrison_required, 0);
        assert_eq!(state.unrest_level, 0.0);
        assert_eq!(state.integration_progress, 1.0);
    }

    #[test]
    fn test_occupation_foreign_culture_requires_garrison() {
        let state = OccupationState::new(
            "Occupier".to_string(),
            "region_1".to_string(),
            1,
            100_000,
            0.8, // High cultural distance
        );

        assert!(!state.is_integrated);
        assert!(state.garrison_required > 0);
        assert!(state.unrest_level > 0.0);
        assert_eq!(state.integration_progress, 0.0);
    }

    #[test]
    fn test_garrison_scales_with_population() {
        let small = OccupationState::new("O".to_string(), "r1".to_string(), 1, 10_000, 0.8);
        let large = OccupationState::new("O".to_string(), "r2".to_string(), 1, 1_000_000, 0.8);

        assert!(large.garrison_required > small.garrison_required,
            "Larger population must require more garrison");
    }

    #[test]
    fn test_garrison_scales_with_cultural_distance() {
        let close = OccupationState::new("O".to_string(), "r1".to_string(), 1, 100_000, 0.3);
        let far = OccupationState::new("O".to_string(), "r2".to_string(), 1, 100_000, 0.8);

        assert!(far.garrison_required > close.garrison_required,
            "Greater cultural distance must require more garrison");
    }

    #[test]
    fn test_garrison_insufficient_detection() {
        let mut state = OccupationState::new("O".to_string(), "r1".to_string(), 1, 100_000, 0.8);
        state.current_garrison = 10;
        assert!(state.is_garrison_insufficient());

        state.current_garrison = state.garrison_required;
        assert!(!state.is_garrison_insufficient());
    }

    #[test]
    fn test_garrison_deficit() {
        let mut state = OccupationState::new("O".to_string(), "r1".to_string(), 1, 100_000, 0.8);
        state.current_garrison = 100;
        let deficit = state.garrison_deficit();
        assert!(deficit > 0);
        assert_eq!(deficit, state.garrison_required - 100);
    }

    #[test]
    fn test_occupation_unrest_increases_with_insufficient_garrison() {
        let mut state = OccupationState::new("O".to_string(), "r1".to_string(), 1, 100_000, 0.8);
        state.current_garrison = 0; // No garrison

        let initial_unrest = state.unrest_level;
        let config = OccupationConfig::default();
        let result = process_occupation_turn(&mut state, &config, 2);

        assert!(state.unrest_level > initial_unrest, "Unrest must increase with insufficient garrison");
        assert!(!result.rebellion_triggered); // Should not trigger immediately
    }

    #[test]
    fn test_occupation_unrest_decreases_with_sufficient_garrison() {
        let mut state = OccupationState::new("O".to_string(), "r1".to_string(), 1, 100_000, 0.8);
        state.current_garrison = state.garrison_required * 2; // Over-garrisoned

        let initial_unrest = state.unrest_level;
        let config = OccupationConfig::default();
        let _result = process_occupation_turn(&mut state, &config, 2);

        assert!(state.unrest_level < initial_unrest, "Unrest must decrease with sufficient garrison");
    }

    #[test]
    fn test_occupation_integration_progresses_with_garrison() {
        let mut state = OccupationState::new("O".to_string(), "r1".to_string(), 1, 100_000, 0.8);
        state.current_garrison = state.garrison_required;

        let config = OccupationConfig::default();
        let _result = process_occupation_turn(&mut state, &config, 2);

        assert!(state.integration_progress > 0.0, "Integration must progress with sufficient garrison");
    }

    #[test]
    fn test_occupation_rebellion_triggered_at_threshold() {
        let mut state = OccupationState::new("O".to_string(), "r1".to_string(), 1, 100_000, 0.8);
        state.current_garrison = 0;
        state.unrest_level = 0.79; // Just below threshold

        let config = OccupationConfig {
            rebellion_threshold: 0.8,
            unrest_increase_per_turn: 0.05,
            ..Default::default()
        };

        let result = process_occupation_turn(&mut state, &config, 2);
        assert!(result.rebellion_triggered, "Rebellion must trigger when unrest exceeds threshold");
    }

    #[test]
    fn test_occupation_full_integration() {
        let mut state = OccupationState::new("O".to_string(), "r1".to_string(), 1, 100_000, 0.8);
        state.current_garrison = state.garrison_required;
        state.integration_progress = 0.99; // Almost integrated

        let config = OccupationConfig {
            integration_rate: 0.02,
            ..Default::default()
        };

        let result = process_occupation_turn(&mut state, &config, 100);
        assert!(result.fully_integrated, "Region must be fully integrated when progress reaches 1.0");
        assert!(state.is_integrated);
    }

    #[test]
    fn test_cultural_distance_same_culture() {
        let dist = compute_cultural_distance("slavic", "slavic");
        assert_eq!(dist, 0.0);
    }

    #[test]
    fn test_cultural_distance_same_group() {
        let dist = compute_cultural_distance("polish", "russian");
        assert!(dist > 0.0 && dist < 0.5, "Same group must have moderate distance");
    }

    #[test]
    fn test_cultural_distance_different_group() {
        let dist = compute_cultural_distance("slavic", "asian");
        assert!(dist >= 0.5, "Different groups must have high distance");
    }

    #[test]
    fn test_create_occupation_states() {
        let mut region_control = HashMap::new();
        region_control.insert("r1".to_string(), RegionControl::Occupied("Occupier".to_string()));
        region_control.insert("r2".to_string(), RegionControl::Owner);
        region_control.insert("r3".to_string(), RegionControl::Occupied("OtherCountry".to_string()));

        let mut cultures = HashMap::new();
        cultures.insert("r1".to_string(), "slavic".to_string());

        let mut populations = HashMap::new();
        populations.insert("r1".to_string(), 50_000);

        let states = create_occupation_states(
            &region_control,
            "Occupier",
            "germanic",
            &cultures,
            &populations,
            5,
        );

        assert_eq!(states.len(), 1, "Only one region occupied by 'Occupier'");
        assert!(states.contains_key("r1"));
    }

    #[test]
    fn test_integrated_occupation_no_changes() {
        let mut state = OccupationState::new("O".to_string(), "r1".to_string(), 1, 100_000, 0.0);
        let config = OccupationConfig::default();
        let result = process_occupation_turn(&mut state, &config, 2);

        assert!(result.fully_integrated);
        assert!(!result.rebellion_triggered);
        assert_eq!(result.unrest_level, 0.0);
    }
}

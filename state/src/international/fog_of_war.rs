//! Phase 66: Fog of War system for foreign nation intelligence.
//!
//! Implements a four-tier progressive confidence system:
//! `Unknown` → `BroadRange` → `NarrowRange` → `Exact`.
//!
//! The player cannot see exact foreign GDP, treasury, or military size
//! without spies/diplomats. The `apply_fog_of_war()` function physically
//! strips hidden data from the DTO before it leaves the Rust backend.

use crate::politics::vip_registry::DiplomaticPostType;
use crate::state::GameState;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Four-tier intelligence level for foreign nation data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum IntelLevel {
    /// No intelligence — all foreign stats are null/None in the DTO.
    #[default]
    Unknown,
    /// Broad range estimate — ±50% of true value.
    BroadRange,
    /// Narrow range estimate — ±15% of true value.
    NarrowRange,
    /// Exact values visible (high-level treaty or long-term spy).
    Exact,
}

impl IntelLevel {
    /// Returns the estimation error margin (as a fraction of true value) for this tier.
    pub fn error_margin(self) -> f64 {
        match self {
            IntelLevel::Unknown => 1.0,   // 100% error — no data
            IntelLevel::BroadRange => 0.50, // ±50%
            IntelLevel::NarrowRange => 0.15, // ±15%
            IntelLevel::Exact => 0.0,       // No error
        }
    }

    /// Returns true if this tier provides any data at all.
    pub fn has_data(self) -> bool {
        !matches!(self, IntelLevel::Unknown)
    }

    /// Returns the next higher tier, or None if already at Exact.
    pub fn upgrade(self) -> Option<IntelLevel> {
        match self {
            IntelLevel::Unknown => Some(IntelLevel::BroadRange),
            IntelLevel::BroadRange => Some(IntelLevel::NarrowRange),
            IntelLevel::NarrowRange => Some(IntelLevel::Exact),
            IntelLevel::Exact => None,
        }
    }

    /// Returns the next lower tier, or None if already at Unknown.
    pub fn downgrade(self) -> Option<IntelLevel> {
        match self {
            IntelLevel::Unknown => None,
            IntelLevel::BroadRange => Some(IntelLevel::Unknown),
            IntelLevel::NarrowRange => Some(IntelLevel::BroadRange),
            IntelLevel::Exact => Some(IntelLevel::NarrowRange),
        }
    }

    /// Human-readable label for UI display.
    pub fn as_str(self) -> &'static str {
        match self {
            IntelLevel::Unknown => "Unknown",
            IntelLevel::BroadRange => "Broad Range",
            IntelLevel::NarrowRange => "Narrow Range",
            IntelLevel::Exact => "Exact",
        }
    }
}

/// Intelligence data about a foreign country, stored per observer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ForeignIntelligence {
    /// Estimated GDP range: (low, high). Both None if Unknown.
    pub estimated_gdp: Option<(f64, f64)>,
    /// Estimated military size range: (low, high). Both None if Unknown.
    pub estimated_military: Option<(u32, u32)>,
    /// Estimated treasury reserves range: (low, high). None if Unknown.
    pub estimated_treasury: Option<(f64, f64)>,
    /// Whether the foreign country's government form is known.
    pub government_known: bool,
    /// Current intelligence level for this country.
    pub intel_level: IntelLevel,
    /// Turn when intelligence was last updated.
    pub last_intel_turn: u32,
}

impl ForeignIntelligence {
    /// Creates intelligence for an Unknown tier (no data).
    pub fn unknown() -> Self {
        Self {
            estimated_gdp: None,
            estimated_military: None,
            estimated_treasury: None,
            government_known: false,
            intel_level: IntelLevel::Unknown,
            last_intel_turn: 0,
        }
    }

    /// Updates the intelligence estimate based on the true values and the intel level.
    pub fn update_from_true_values(
        &mut self,
        true_gdp: f64,
        true_military: u32,
        true_treasury: f64,
        level: IntelLevel,
        turn: u32,
        rng: &mut impl Rng,
    ) {
        self.intel_level = level;
        self.last_intel_turn = turn;
        self.government_known = level.has_data();

        if !level.has_data() {
            self.estimated_gdp = None;
            self.estimated_military = None;
            self.estimated_treasury = None;
            return;
        }

        let margin = level.error_margin();
        // Add random noise within the margin so estimates aren't perfectly centered
        let noise = rng.gen_range(-0.1..=0.1) * margin;

        let gdp_low = (true_gdp * (1.0 - margin + noise)).max(0.0);
        let gdp_high = true_gdp * (1.0 + margin + noise);
        self.estimated_gdp = Some((gdp_low, gdp_high));

        let mil_low = ((true_military as f64) * (1.0 - margin + noise)).max(0.0) as u32;
        let mil_high = ((true_military as f64) * (1.0 + margin + noise)).max(0.0) as u32;
        self.estimated_military = Some((mil_low, mil_high));

        let treas_low = (true_treasury * (1.0 - margin + noise)).max(0.0);
        let treas_high = true_treasury * (1.0 + margin + noise);
        self.estimated_treasury = Some((treas_low, treas_high));
    }
}

/// Configuration for the Fog of War system. No magic numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FogOfWarConfig {
    /// Base intelligence level assigned to all foreign countries at game start.
    pub base_intelligence: IntelLevel,
    /// Rate at which spies reveal intelligence per turn (0.0 = never, 1.0 = always).
    pub spy_reveal_rate: f64,
    /// Intel bonus per ambassador posted in a foreign country (added to reveal rate).
    pub ambassador_intel_bonus: f64,
    /// Intel bonus per consul posted in a foreign country.
    pub consul_intel_bonus: f64,
    /// Base estimation error for BroadRange tier (fraction of true value).
    pub broad_range_error: f64,
    /// Base estimation error for NarrowRange tier (fraction of true value).
    pub narrow_range_error: f64,
    /// Turns required at BroadRange before auto-upgrading to NarrowRange (via sustained ambassador).
    pub turns_to_narrow: u32,
    /// Turns required at NarrowRange before auto-upgrading to Exact (via sustained ambassador + treaty).
    pub turns_to_exact: u32,
}

impl Default for FogOfWarConfig {
    fn default() -> Self {
        Self {
            base_intelligence: IntelLevel::Unknown,
            spy_reveal_rate: 0.15,
            ambassador_intel_bonus: 0.10,
            consul_intel_bonus: 0.05,
            broad_range_error: 0.50,
            narrow_range_error: 0.15,
            turns_to_narrow: 10,
            turns_to_exact: 30,
        }
    }
}

/// Configuration for diplomatic mechanics. No magic numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiplomaticConfig {
    /// Base risk per turn that a spy is discovered by host counter-intelligence.
    pub spy_discovery_risk: f64,
    /// Relation penalty applied when a spy is caught and expelled.
    pub spy_caught_relation_penalty: i64,
    /// Number of turns relations are frozen after a spy is caught.
    pub spy_caught_freeze_turns: u32,
    /// Base relation improvement rate per turn with an ambassador present.
    pub ambassador_relation_boost: i64,
    /// Relation damage from a border provocation.
    pub provocation_relation_penalty: i64,
    /// Turns relations are frozen after a border provocation.
    pub provocation_freeze_turns: u32,
    /// Minimum liquid reserves required to assign a new diplomat.
    pub diplomat_assignment_cost: f64,
    /// Minimum liquid reserves required to send economic aid.
    pub min_aid_amount: f64,
}

impl Default for DiplomaticConfig {
    fn default() -> Self {
        Self {
            spy_discovery_risk: 0.05,
            spy_caught_relation_penalty: 20,
            spy_caught_freeze_turns: 5,
            ambassador_relation_boost: 1,
            provocation_relation_penalty: 15,
            provocation_freeze_turns: 3,
            diplomat_assignment_cost: 50_000.0,
            min_aid_amount: 1_000.0,
        }
    }
}

/// Computes the intel level for a foreign country based on posted diplomats,
/// spies, trade volume, and treaty participation.
pub fn compute_intel_level(
    state: &GameState,
    observer: &str,
    target: &str,
    config: &FogOfWarConfig,
) -> IntelLevel {
    let Some(observer_country) = state.countries.get(observer) else {
        return IntelLevel::Unknown;
    };
    let Some(registry) = &observer_country.politics.vip_registry else {
        return config.base_intelligence;
    };

    let mut has_ambassador = false;
    let mut has_consul = false;
    let mut spy_count = 0;

    for vip in registry.vips.values() {
        if let Some(post) = &vip.diplomatic_post {
            if post.host_country == target {
                match post.post_type {
                    DiplomaticPostType::Ambassador => has_ambassador = true,
                    DiplomaticPostType::Consul => has_consul = true,
                    DiplomaticPostType::Spy => spy_count += 1,
                    DiplomaticPostType::MilitaryAttache => {}
                }
            }
        }
    }

    // Intel level progression:
    // - No diplomats → Unknown (or base_intelligence)
    // - Consul only → BroadRange
    // - Ambassador → NarrowRange
    // - Ambassador + Spy → Exact
    // - Spy only → BroadRange (spies gather intel but lack diplomatic cover)
    if has_ambassador && spy_count > 0 {
        IntelLevel::Exact
    } else if has_ambassador {
        IntelLevel::NarrowRange
    } else if has_consul || spy_count > 0 {
        IntelLevel::BroadRange
    } else {
        config.base_intelligence
    }
}

/// Applies Fog of War filtering to a foreign country's true values.
///
/// Returns `None` for all estimated fields if the intel level is `Unknown`.
/// This function is called by `build_global_snapshot()` to physically strip
/// hidden data before it reaches the frontend.
pub fn apply_fog_of_war(
    true_gdp: f64,
    true_military: u32,
    true_treasury: f64,
    intel: &ForeignIntelligence,
) -> FogOfWarResult {
    match intel.intel_level {
        IntelLevel::Unknown => FogOfWarResult {
            gdp: None,
            military: None,
            treasury: None,
            intel_level: IntelLevel::Unknown,
        },
        IntelLevel::Exact => FogOfWarResult {
            gdp: Some((true_gdp, true_gdp)),
            military: Some((true_military, true_military)),
            treasury: Some((true_treasury, true_treasury)),
            intel_level: IntelLevel::Exact,
        },
        IntelLevel::BroadRange | IntelLevel::NarrowRange => FogOfWarResult {
            gdp: intel.estimated_gdp,
            military: intel.estimated_military,
            treasury: intel.estimated_treasury,
            intel_level: intel.intel_level,
        },
    }
}

/// Result of applying Fog of War to a foreign country's stats.
/// Fields are `None` when the intel level is `Unknown`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FogOfWarResult {
    /// Estimated GDP range (low, high), or None if Unknown.
    pub gdp: Option<(f64, f64)>,
    /// Estimated military size range (low, high), or None if Unknown.
    pub military: Option<(u32, u32)>,
    /// Estimated treasury range (low, high), or None if Unknown.
    pub treasury: Option<(f64, f64)>,
    /// Current intel level for this country.
    pub intel_level: IntelLevel,
}

/// Processes intel updates for a single observer-target pair per turn.
///
/// Called from the turn processor. Updates the `ForeignIntelligence` entry
/// based on current diplomat postings and spy activity.
pub fn process_intel_turn(
    state: &GameState,
    observer: &str,
    target: &str,
    intelligence: &mut HashMap<String, HashMap<String, ForeignIntelligence>>,
    fog_config: &FogOfWarConfig,
    current_turn: u32,
) {
    let target_country = match state.countries.get(target) {
        Some(c) => c,
        None => return,
    };

    let true_gdp = target_country.budget.gdp;
    let true_military = target_country.order_of_battle.unit_count() as u32;
    let true_treasury = target_country.budget.liquid_reserves;

    let level = compute_intel_level(state, observer, target, fog_config);

    // Get or create the intelligence entry
    let observer_intel = intelligence
        .entry(observer.to_string())
        .or_default();
    let intel = observer_intel
        .entry(target.to_string())
        .or_insert_with(ForeignIntelligence::unknown);

    // Only update if the level changed or it's time for a refresh
    let should_update = intel.intel_level != level
        || (current_turn - intel.last_intel_turn) >= 5;

    if should_update {
        let mut rng = rand::thread_rng();
        intel.update_from_true_values(true_gdp, true_military, true_treasury, level, current_turn, &mut rng);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intel_level_progression() {
        assert_eq!(IntelLevel::Unknown.upgrade(), Some(IntelLevel::BroadRange));
        assert_eq!(IntelLevel::BroadRange.upgrade(), Some(IntelLevel::NarrowRange));
        assert_eq!(IntelLevel::NarrowRange.upgrade(), Some(IntelLevel::Exact));
        assert_eq!(IntelLevel::Exact.upgrade(), None);
    }

    #[test]
    fn test_intel_level_downgrade() {
        assert_eq!(IntelLevel::Exact.downgrade(), Some(IntelLevel::NarrowRange));
        assert_eq!(IntelLevel::Unknown.downgrade(), None);
    }

    #[test]
    fn test_intel_level_error_margin() {
        assert_eq!(IntelLevel::Unknown.error_margin(), 1.0);
        assert_eq!(IntelLevel::BroadRange.error_margin(), 0.50);
        assert_eq!(IntelLevel::NarrowRange.error_margin(), 0.15);
        assert_eq!(IntelLevel::Exact.error_margin(), 0.0);
    }

    #[test]
    fn test_intel_level_has_data() {
        assert!(!IntelLevel::Unknown.has_data());
        assert!(IntelLevel::BroadRange.has_data());
        assert!(IntelLevel::NarrowRange.has_data());
        assert!(IntelLevel::Exact.has_data());
    }

    #[test]
    fn test_foreign_intelligence_unknown() {
        let intel = ForeignIntelligence::unknown();
        assert_eq!(intel.intel_level, IntelLevel::Unknown);
        assert!(intel.estimated_gdp.is_none());
        assert!(intel.estimated_military.is_none());
        assert!(intel.estimated_treasury.is_none());
        assert!(!intel.government_known);
    }

    #[test]
    fn test_foreign_intelligence_update_broad_range() {
        let mut intel = ForeignIntelligence::unknown();
        let mut rng = rand::thread_rng();
        intel.update_from_true_values(1_000_000.0, 500, 50_000.0, IntelLevel::BroadRange, 10, &mut rng);

        assert_eq!(intel.intel_level, IntelLevel::BroadRange);
        assert!(intel.estimated_gdp.is_some());
        let (low, high) = intel.estimated_gdp.unwrap();
        // BroadRange = ±50%, so range should be roughly 500k-1.5M
        assert!(low > 0.0 && low < 1_000_000.0, "low={} should be below true", low);
        assert!(high > 1_000_000.0, "high={} should be above true", high);
        assert!(intel.government_known);
    }

    #[test]
    fn test_foreign_intelligence_update_exact() {
        let mut intel = ForeignIntelligence::unknown();
        let mut rng = rand::thread_rng();
        intel.update_from_true_values(1_000_000.0, 500, 50_000.0, IntelLevel::Exact, 10, &mut rng);

        assert_eq!(intel.intel_level, IntelLevel::Exact);
        // Exact should have zero margin
        let (low, high) = intel.estimated_gdp.unwrap();
        assert!((low - 1_000_000.0).abs() < 1.0);
        assert!((high - 1_000_000.0).abs() < 1.0);
    }

    #[test]
    fn test_foreign_intelligence_update_unknown_strips_data() {
        let mut intel = ForeignIntelligence::unknown();
        let mut rng = rand::thread_rng();
        // First set to BroadRange
        intel.update_from_true_values(1_000_000.0, 500, 50_000.0, IntelLevel::BroadRange, 10, &mut rng);
        assert!(intel.estimated_gdp.is_some());
        // Then downgrade to Unknown
        intel.update_from_true_values(1_000_000.0, 500, 50_000.0, IntelLevel::Unknown, 11, &mut rng);
        assert!(intel.estimated_gdp.is_none());
        assert!(intel.estimated_military.is_none());
        assert!(intel.estimated_treasury.is_none());
    }

    #[test]
    fn test_apply_fog_of_war_unknown() {
        let intel = ForeignIntelligence::unknown();
        let result = apply_fog_of_war(1_000_000.0, 500, 50_000.0, &intel);
        assert!(result.gdp.is_none());
        assert!(result.military.is_none());
        assert!(result.treasury.is_none());
        assert_eq!(result.intel_level, IntelLevel::Unknown);
    }

    #[test]
    fn test_apply_fog_of_war_exact() {
        let mut intel = ForeignIntelligence::unknown();
        let mut rng = rand::thread_rng();
        intel.update_from_true_values(1_000_000.0, 500, 50_000.0, IntelLevel::Exact, 10, &mut rng);
        let result = apply_fog_of_war(1_000_000.0, 500, 50_000.0, &intel);
        assert_eq!(result.gdp, Some((1_000_000.0, 1_000_000.0)));
        assert_eq!(result.military, Some((500, 500)));
        assert_eq!(result.intel_level, IntelLevel::Exact);
    }

    #[test]
    fn test_compute_intel_level_no_diplomats() {
        let state = GameState::default();
        let config = FogOfWarConfig::default();
        let level = compute_intel_level(&state, "A", "B", &config);
        assert_eq!(level, IntelLevel::Unknown);
    }

    #[test]
    fn test_diplomatic_config_defaults() {
        let config = DiplomaticConfig::default();
        assert!(config.spy_discovery_risk > 0.0 && config.spy_discovery_risk < 1.0);
        assert!(config.spy_caught_relation_penalty > 0);
        assert!(config.diplomat_assignment_cost > 0.0);
    }

    #[test]
    fn test_fog_of_war_config_defaults() {
        let config = FogOfWarConfig::default();
        assert_eq!(config.base_intelligence, IntelLevel::Unknown);
        assert!(config.spy_reveal_rate > 0.0);
        assert!(config.ambassador_intel_bonus > 0.0);
    }
}

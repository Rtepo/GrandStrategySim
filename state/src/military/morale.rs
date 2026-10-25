//! Phase 72: Homefront morale system.
//!
//! High casualties reduce `war_morale` and `mental_health` of demographic
//! classes. Low war morale leads to factory strikes (reduced production) and
//! military desertions (manpower loss).
//!
//! # Mechanics
//! - When casualties are routed back to demographics, `war_morale` drops
//!   proportionally to casualties relative to total population.
//! - `war_morale` below `strike_threshold` → factory strikes (production
//!   output reduced).
//! - `war_morale` below `desertion_threshold` → military desertions
//!   (units lose manpower).
//! - `war_morale` recovers naturally at `morale_recovery_rate` per turn
//!   when no new casualties occur.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::military::fronts::Casualties;
use crate::society::geography::{ClassDemographics, RuralClass};

// ============================================================================
// MORALE CONFIG
// ============================================================================

/// Configuration for the homefront morale system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MoraleConfig {
    /// Morale drop per 1000 casualties (relative to total population).
    pub casualty_morale_impact_per_1000: f64,
    /// War morale below this threshold triggers factory strikes.
    pub strike_threshold: f64,
    /// War morale below this threshold triggers military desertions.
    pub desertion_threshold: f64,
    /// Natural morale recovery rate per turn (when no new casualties).
    pub morale_recovery_rate: f64,
    /// Baseline war morale for new populations.
    pub baseline_war_morale: f64,
    /// Baseline mental health for new populations.
    pub baseline_mental_health: f64,
    /// Strike production reduction factor (0.0 = no production, 1.0 = full).
    /// Applied when war_morale is below strike_threshold.
    pub strike_production_reduction: f64,
    /// Desertion rate per turn when war_morale is below desertion_threshold
    /// (fraction of military manpower that deserts).
    pub desertion_rate: f64,
}

impl Default for MoraleConfig {
    fn default() -> Self {
        Self {
            casualty_morale_impact_per_1000: 5.0,
            strike_threshold: 30.0,
            desertion_threshold: 15.0,
            morale_recovery_rate: 1.0,
            baseline_war_morale: 70.0,
            baseline_mental_health: 70.0,
            strike_production_reduction: 0.5,
            desertion_rate: 0.05,
        }
    }
}

// ============================================================================
// MORALE IMPACT FROM CASUALTIES
// ============================================================================

/// Result of applying casualty impact to morale.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoraleImpactResult {
    /// Total morale drop applied.
    pub total_morale_drop: f64,
    /// Total mental health drop applied.
    pub total_mental_health_drop: f64,
    /// Whether strikes are now active.
    pub strikes_active: bool,
    /// Whether desertions are now active.
    pub desertions_active: bool,
    /// Log messages.
    pub messages: Vec<String>,
}

/// Applies casualty impact to a demographic class's war morale and mental health.
///
/// Morale drops proportionally to casualties relative to total population.
///
/// # Arguments
/// * `demographics` - Mutable demographics for the affected class.
/// * `casualties` - Casualties suffered by this class.
/// * `config` - Morale configuration.
///
/// # Returns
/// `MoraleImpactResult` with the impact details.
pub fn apply_casualty_morale_impact(
    demographics: &mut ClassDemographics,
    casualties: &Casualties,
    config: &MoraleConfig,
) -> MoraleImpactResult {
    let mut result = MoraleImpactResult::default();

    let total_casualties = casualties.total();
    if total_casualties <= 0 || demographics.population <= 0 {
        return result;
    }

    // Morale drop: proportional to casualties relative to population
    // Scale: casualty_morale_impact_per_1000 per 1000 casualties per 100k population
    let casualty_ratio = total_casualties as f64 / demographics.population as f64;
    let morale_drop = (casualty_ratio * 1000.0 * config.casualty_morale_impact_per_1000).min(50.0);

    demographics.war_morale = (demographics.war_morale - morale_drop).max(0.0);
    result.total_morale_drop = morale_drop;

    // Mental health also drops (but less than war morale)
    let mental_health_drop = morale_drop * 0.5;
    demographics.mental_health = (demographics.mental_health - mental_health_drop).max(0.0);
    result.total_mental_health_drop = mental_health_drop;

    // Check thresholds
    result.strikes_active = demographics.war_morale < config.strike_threshold;
    result.desertions_active = demographics.war_morale < config.desertion_threshold;

    if result.strikes_active {
        result.messages.push(format!(
            "[MORALE] Strikes active! War morale {:.1} < threshold {:.1}",
            demographics.war_morale, config.strike_threshold
        ));
    }
    if result.desertions_active {
        result.messages.push(format!(
            "[MORALE] Desertions active! War morale {:.1} < threshold {:.1}",
            demographics.war_morale, config.desertion_threshold
        ));
    }

    result
}

/// Applies casualty impact to all demographic classes based on the casualty
/// demographic breakdown.
///
/// # Arguments
/// * `rural_classes` - Mutable map of class name → demographics.
/// * `casualties` - Casualties with demographic breakdown.
/// * `config` - Morale configuration.
///
/// # Returns
/// Combined `MoraleImpactResult` across all classes.
pub fn apply_casualty_morale_to_classes(
    rural_classes: &mut BTreeMap<RuralClass, ClassDemographics>,
    casualties: &Casualties,
    config: &MoraleConfig,
) -> MoraleImpactResult {
    let mut combined = MoraleImpactResult::default();

    for (class_key, demographics) in rural_classes.iter_mut() {
        // Get casualties for this class from the demographic breakdown
        let class_casualties = casualties
            .demographic_breakdown
            .iter()
            .filter_map(|(rc, &count)| {
                if rc == class_key {
                    Some(count)
                } else {
                    None
                }
            })
            .sum::<i64>();

        if class_casualties > 0 {
            let class_casualties_struct = Casualties {
                dead: (class_casualties as f64 * 0.5) as i64,
                wounded: (class_casualties as f64 * 0.35) as i64,
                deserters: class_casualties
                    - (class_casualties as f64 * 0.5) as i64
                    - (class_casualties as f64 * 0.35) as i64,
                demographic_breakdown: HashMap::new(),
            };

            let result =
                apply_casualty_morale_impact(demographics, &class_casualties_struct, config);
            combined.total_morale_drop += result.total_morale_drop;
            combined.total_mental_health_drop += result.total_mental_health_drop;
            combined.strikes_active |= result.strikes_active;
            combined.desertions_active |= result.desertions_active;
            combined.messages.extend(result.messages);
        }
    }

    combined
}

// ============================================================================
// MORALE RECOVERY
// ============================================================================

/// Recovers war morale and mental health for a demographic class.
///
/// Called each turn when no new casualties occur. Morale recovers at
/// `morale_recovery_rate` per turn, capped at baseline.
///
/// # Arguments
/// * `demographics` - Mutable demographics.
/// * `config` - Morale configuration.
pub fn recover_morale(demographics: &mut ClassDemographics, config: &MoraleConfig) {
    demographics.war_morale =
        (demographics.war_morale + config.morale_recovery_rate).min(config.baseline_war_morale);
    demographics.mental_health = (demographics.mental_health + config.morale_recovery_rate * 0.5)
        .min(config.baseline_mental_health);
}

/// Recovers morale for all demographic classes in a region.
pub fn recover_morale_for_classes(
    rural_classes: &mut BTreeMap<RuralClass, ClassDemographics>,
    config: &MoraleConfig,
) {
    for demographics in rural_classes.values_mut() {
        recover_morale(demographics, config);
    }
}

// ============================================================================
// STRIKE AND DESERTION EFFECTS
// ============================================================================

/// Returns the production reduction factor due to strikes.
///
/// # Arguments
/// * `war_morale` - Current war morale.
/// * `config` - Morale configuration.
///
/// # Returns
/// 1.0 = full production, <1.0 = reduced production due to strikes.
pub fn strike_production_factor(war_morale: f64, config: &MoraleConfig) -> f64 {
    if war_morale >= config.strike_threshold {
        return 1.0;
    }
    // Linear interpolation: at threshold → 1.0, at 0.0 → (1.0 - strike_production_reduction)
    let below_threshold = config.strike_threshold - war_morale;
    let fraction = below_threshold / config.strike_threshold;
    1.0 - (config.strike_production_reduction * fraction)
}

/// Calculates the number of desertions from military units based on war morale.
///
/// # Arguments
/// * `manpower` - Total military manpower.
/// * `war_morale` - Current war morale (averaged across classes).
/// * `config` - Morale configuration.
///
/// # Returns
/// Number of soldiers who desert this turn.
pub fn calculate_desertions(manpower: i64, war_morale: f64, config: &MoraleConfig) -> i64 {
    if war_morale >= config.desertion_threshold || manpower <= 0 {
        return 0;
    }
    let below_threshold = config.desertion_threshold - war_morale;
    let fraction = below_threshold / config.desertion_threshold;
    (manpower as f64 * config.desertion_rate * fraction) as i64
}

/// Initializes morale fields for a new demographic class.
pub fn initialize_morale(demographics: &mut ClassDemographics, config: &MoraleConfig) {
    demographics.war_morale = config.baseline_war_morale;
    demographics.mental_health = config.baseline_mental_health;
    demographics.morale_initialized = true;
}

// ============================================================================
// v4.22: DYNAMIC HOMEFRONT MORALE (target-seeking update)
// ============================================================================

/// Country-level inputs that shape every class's morale target.
///
/// All inputs are real simulation state: no constants beyond the configured
/// weights inside [`MoraleConfig`]. Derived per country in the military turn
/// from its active fronts (`war_exhaustion`, `at_war`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HomefrontContext {
    /// Country war exhaustion (0.0–100.0), max over the country's fronts.
    pub war_exhaustion: f64,
    /// Whether the country is currently at war (has fronts / war list).
    pub at_war: bool,
    /// Current global turn (for YoY driver snapshots).
    pub current_turn: u32,
}

/// Computes the morale target for a class from real per-class inputs.
///
/// Inputs (all read off [`ClassDemographics`] — no external lookups):
/// * `economic_status` — Prosperous/Stable/Struggling/Destitute.
/// * Real wage vs expected — YoY real-wage growth via `calculate_yoy_drivers`
///   (the class's own QoL ring buffer, fed with its last stamped real wage).
/// * Consumption fulfillment — `savings_per_capita` vs
///   `habitual_savings_per_capita` (the habitual standard the class tries to
///   hold); persistent shortfall means unfilled consumption.
/// * Class unemployment — `allocated_fte` / `available_fte` gap.
/// * War exhaustion + active war (`HomefrontContext`).
/// * Repression/unrest — `political_sentiment.radicals` share.
/// * `health_status` — Critical/Poor classes carry a morale penalty.
///
/// # Returns
/// `(war_morale_target, mental_health_target)` in `[5.0, 95.0]`.
pub fn compute_morale_targets(
    demographics: &ClassDemographics,
    ctx: &HomefrontContext,
    config: &MoraleConfig,
) -> (f64, f64) {
    let base = config.baseline_war_morale;

    // Economic status anchor: peacetime prosperity sits high; genuine
    // destitution is the primary path below the strike threshold.
    let status_mod = match demographics.economic_status {
        crate::society::geography::EconomicStatus::Prosperous => 8.0,
        crate::society::geography::EconomicStatus::Stable => 0.0,
        crate::society::geography::EconomicStatus::Struggling => -10.0,
        crate::society::geography::EconomicStatus::Destitute => -22.0,
    };

    // Real wage vs expected: YoY real-wage growth clamped to ±50% → ±15 pts.
    // Uses the class's own QoL history; the last stamped real wage is the
    // country average wage recorded by the sentiment pass last turn.
    let current_real_wage = demographics.previous_turn_qol.real_wage_yoy;
    let drivers = demographics.calculate_yoy_drivers(
        ctx.current_turn,
        demographics.savings_per_capita,
        current_real_wage,
        0.0,
    );
    let wage_mod = drivers.real_wage_growth.clamp(-0.5, 0.5) * 30.0;

    // Consumption fulfillment: live savings/capita vs habitual standard.
    // A class running below its habitual standard is having consumption
    // squeezed — unfilled subsistence/discretionary demand drags morale.
    let consumption_mod = if demographics.habitual_savings_per_capita > 0.0 {
        let fill_ratio = (demographics.savings_per_capita
            / demographics.habitual_savings_per_capita)
            .min(1.0);
        (fill_ratio - 1.0) * 25.0 // 0 at full fill, → -25 as fill → 0
    } else {
        0.0
    };

    // Class unemployment: share of offered FTE with no allocation.
    let unemployment_ratio = if demographics.available_fte > 0.0 {
        ((demographics.available_fte - demographics.allocated_fte).max(0.0)
            / demographics.available_fte)
            .min(1.0)
    } else {
        0.0
    };
    let unemployment_mod = -unemployment_ratio * 20.0;

    // War pressure: baseline −5 when at war, plus up to −30 scaled by
    // exhaustion (0–100). Casualty shocks apply separately via
    // `apply_casualty_morale_impact`.
    let war_mod = if ctx.at_war { -5.0 } else { 0.0 } - ctx.war_exhaustion * 0.30;

    // Repression/unrest: radicalized classes are already disaffected.
    let radical_mod = -demographics.political_sentiment.radicals * 15.0;

    // Health: sick/critical populations are demoralized.
    let health_mod = match demographics.health_status {
        crate::society::geography::HealthStatus::Critical => -8.0,
        crate::society::geography::HealthStatus::Poor => -4.0,
        _ => 0.0,
    };

    let war_target = base
        + status_mod
        + wage_mod
        + consumption_mod
        + unemployment_mod
        + war_mod
        + radical_mod
        + health_mod;

    // Mental health tracks the same inputs but damped — psychological
    // well-being moves slower and shallower than war sentiment.
    let mental_target = config.baseline_mental_health
        + status_mod * 0.6
        + wage_mod * 0.5
        + consumption_mod * 0.6
        + unemployment_mod * 0.8
        + (war_mod * 0.7)
        + radical_mod * 0.5
        + health_mod * 1.5;

    (war_target.clamp(5.0, 95.0), mental_target.clamp(5.0, 95.0))
}

/// Per-turn homefront morale update for a single demographic class.
///
/// Morale is EWMA-seeked toward `compute_morale_targets`:
/// * `war_morale` converges at `morale_seek_rate` (≈5-turn half-life).
/// * `mental_health` converges at half that rate (slower dynamics).
///
/// Self-healing seed: classes whose `morale_initialized` flag is `false`
/// (legacy saves, `or_default()` spawns — the historical cause of the pinned
/// `war_morale: 0.0` state) are seeded straight to the computed target so
/// peacetime morale starts in the natural 60–90 band instead of climbing
/// +1.0/turn from zero.
///
/// Casualty-driven drops are still applied by `apply_casualty_morale_impact`
/// during battle resolution — this function only steers the slow signal back
/// toward the target implied by living conditions, so a devastated class
/// converges to a *low* target, not instantly to baseline.
pub fn update_homefront_morale(
    demographics: &mut ClassDemographics,
    ctx: &HomefrontContext,
    config: &MoraleConfig,
) {
    let (war_target, mental_target) = compute_morale_targets(demographics, ctx, config);

    if !demographics.morale_initialized {
        demographics.war_morale = war_target;
        demographics.mental_health = mental_target;
        demographics.morale_initialized = true;
        return;
    }

    const WAR_SEEK_RATE: f64 = 0.20;
    const MENTAL_SEEK_RATE: f64 = 0.10;
    demographics.war_morale +=
        (war_target - demographics.war_morale) * WAR_SEEK_RATE;
    demographics.mental_health +=
        (mental_target - demographics.mental_health) * MENTAL_SEEK_RATE;
    demographics.war_morale = demographics.war_morale.clamp(0.0, 100.0);
    demographics.mental_health = demographics.mental_health.clamp(0.0, 100.0);
}

/// Per-turn homefront morale update for all classes in a keyed map.
///
/// Generic over the class key type so it serves both `rural_classes`
/// (`BTreeMap<RuralClass, _>`) and `urban_classes` (`BTreeMap<UrbanClass, _>`).
/// O(classes) — single pass, no per-entity scans.
pub fn update_homefront_morale_for_classes<K: Ord>(
    classes: &mut BTreeMap<K, ClassDemographics>,
    ctx: &HomefrontContext,
    config: &MoraleConfig,
) {
    for demographics in classes.values_mut() {
        update_homefront_morale(demographics, ctx, config);
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_demographics(population: i64) -> ClassDemographics {
        let mut d = ClassDemographics::default();
        d.population = population;
        d.war_morale = 70.0;
        d.mental_health = 70.0;
        d
    }

    fn make_casualties(total: i64) -> Casualties {
        Casualties {
            dead: (total as f64 * 0.5) as i64,
            wounded: (total as f64 * 0.35) as i64,
            deserters: total - (total as f64 * 0.5) as i64 - (total as f64 * 0.35) as i64,
            demographic_breakdown: HashMap::new(),
        }
    }

    #[test]
    fn test_casualty_morale_impact() {
        let mut demo = make_demographics(100_000);
        let casualties = make_casualties(5_000); // 5% casualties
        let config = MoraleConfig::default();

        let initial_morale = demo.war_morale;
        let result = apply_casualty_morale_impact(&mut demo, &casualties, &config);

        assert!(
            demo.war_morale < initial_morale,
            "War morale must drop after casualties"
        );
        assert!(result.total_morale_drop > 0.0);
    }

    #[test]
    fn test_zero_casualties_no_impact() {
        let mut demo = make_demographics(100_000);
        let casualties = make_casualties(0);
        let config = MoraleConfig::default();

        let initial_morale = demo.war_morale;
        let result = apply_casualty_morale_impact(&mut demo, &casualties, &config);

        assert_eq!(demo.war_morale, initial_morale);
        assert_eq!(result.total_morale_drop, 0.0);
    }

    #[test]
    fn test_strikes_activate_below_threshold() {
        let mut demo = make_demographics(100_000);
        demo.war_morale = 25.0; // Below strike_threshold (30.0)
        let casualties = make_casualties(100);
        let config = MoraleConfig::default();

        let result = apply_casualty_morale_impact(&mut demo, &casualties, &config);

        assert!(
            result.strikes_active,
            "Strikes must activate below threshold"
        );
    }

    #[test]
    fn test_desertions_activate_below_threshold() {
        let mut demo = make_demographics(100_000);
        demo.war_morale = 10.0; // Below desertion_threshold (15.0)
        let casualties = make_casualties(100);
        let config = MoraleConfig::default();

        let result = apply_casualty_morale_impact(&mut demo, &casualties, &config);

        assert!(
            result.desertions_active,
            "Desertions must activate below threshold"
        );
    }

    #[test]
    fn test_morale_recovery() {
        let mut demo = make_demographics(100_000);
        demo.war_morale = 40.0;
        demo.mental_health = 50.0;
        let config = MoraleConfig::default();

        recover_morale(&mut demo, &config);

        assert!(demo.war_morale > 40.0, "War morale must recover");
        assert!(demo.mental_health > 50.0, "Mental health must recover");
    }

    #[test]
    fn test_morale_recovery_capped_at_baseline() {
        let mut demo = make_demographics(100_000);
        demo.war_morale = 69.5;
        let config = MoraleConfig::default();

        recover_morale(&mut demo, &config);

        assert!(
            demo.war_morale <= config.baseline_war_morale,
            "War morale must not exceed baseline"
        );
    }

    #[test]
    fn test_strike_production_factor() {
        let config = MoraleConfig::default();

        // Above threshold → full production
        assert_eq!(strike_production_factor(50.0, &config), 1.0);

        // Below threshold → reduced production
        let factor = strike_production_factor(15.0, &config);
        assert!(factor < 1.0, "Production must be reduced during strikes");
        assert!(factor > 0.0, "Production must not be zero");
    }

    #[test]
    fn test_calculate_desertions() {
        let config = MoraleConfig::default();

        // Above threshold → no desertions
        assert_eq!(calculate_desertions(10_000, 50.0, &config), 0);

        // Below threshold → desertions
        let desertions = calculate_desertions(10_000, 10.0, &config);
        assert!(desertions > 0, "Desertions must occur below threshold");
    }

    #[test]
    fn test_initialize_morale() {
        let mut demo = ClassDemographics::default();
        demo.war_morale = 0.0;
        demo.mental_health = 0.0;
        let config = MoraleConfig::default();

        initialize_morale(&mut demo, &config);

        assert_eq!(demo.war_morale, config.baseline_war_morale);
        assert_eq!(demo.mental_health, config.baseline_mental_health);
    }

    #[test]
    fn test_apply_casualty_morale_to_classes() {
        let mut classes = BTreeMap::new();
        classes.insert(RuralClass::FreePeasant, make_demographics(50_000));
        classes.insert(RuralClass::LandlessLaborer, make_demographics(30_000));

        let mut breakdown = HashMap::new();
        breakdown.insert(RuralClass::FreePeasant, 2000);
        let casualties = Casualties {
            dead: 1000,
            wounded: 700,
            deserters: 300,
            demographic_breakdown: breakdown,
        };

        let config = MoraleConfig::default();
        let _result = apply_casualty_morale_to_classes(&mut classes, &casualties, &config);

        // FreePeasant should have lower morale (they had casualties)
        let peasant = classes.get(&RuralClass::FreePeasant).unwrap();
        assert!(peasant.war_morale < 70.0, "FreePeasant morale must drop");

        // LandlessLaborer should have unchanged morale (no casualties)
        let laborer = classes.get(&RuralClass::LandlessLaborer).unwrap();
        assert_eq!(
            laborer.war_morale, 70.0,
            "LandlessLaborer morale must not change"
        );
    }

    #[test]
    fn test_mental_health_drops_less_than_war_morale() {
        let mut demo = make_demographics(100_000);
        let casualties = make_casualties(10_000);
        let config = MoraleConfig::default();

        let initial_war = demo.war_morale;
        let initial_mental = demo.mental_health;
        apply_casualty_morale_impact(&mut demo, &casualties, &config);

        let war_drop = initial_war - demo.war_morale;
        let mental_drop = initial_mental - demo.mental_health;

        assert!(
            mental_drop < war_drop,
            "Mental health drop must be less than war morale drop"
        );
    }
}

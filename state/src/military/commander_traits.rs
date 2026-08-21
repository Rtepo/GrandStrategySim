//! Phase 70.5: VIP Commander Tactics — MilitaryTacticModifiers.
//!
//! Implements the `MilitaryTacticModifiers` struct, parallel to
//! `MarketBehaviorModifiers`, that derives combat-relevant tactical
//! modifiers from VIP traits.
//!
//! # Trait → Tactic Mapping
//!
//! | Trait | Effect |
//! |-------|--------|
//! | Aggressive | +50% attack power, low casualty tolerance threshold |
//! | Cautious | -30% attack power, +20% defense, will retreat |
//! | Cunning | +30% maneuver bonus |
//! | Brave | No retreat, +10% organization |
//! | Cowardly | Will retreat, -20% attack |
//! | Reckless | +80% attack, no retreat, high supply burn |
//! | Methodical | +15% defense, will retreat, low supply burn |
//! | Loyal | No retreat, +10% organization |
//!
//! These modifiers are applied during `resolve_land_combat()` to the
//! highest-ranking commander's units.

use serde::{Deserialize, Serialize};

// ============================================================================
// AIR DOCTRINE
// ============================================================================

/// Air doctrine preference for a commander.
///
/// Determines how AirForce units are employed when under this commander's control.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AirDoctrine {
    /// Prioritize achieving air superiority (fighter sweeps, interceptor missions).
    Superiority,
    /// Prioritize ground support (close air support, tactical bombing).
    #[default]
    GroundSupport,
    /// Defensive air doctrine (preserve air assets, defensive patrols).
    Defensive,
}

// ============================================================================
// MILITARY TACTIC MODIFIERS
// ============================================================================

/// Strongly-typed tactical modifiers derived from VIP traits.
///
/// Parallel to `MarketBehaviorModifiers`, but for combat behavior.
/// All combat functions consume this struct, never raw trait strings.
///
/// # Defaults
/// All multipliers default to 1.0 (baseline). `will_retreat` defaults to false.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MilitaryTacticModifiers {
    /// Scales attack power (1.0 = baseline, >1 = aggressive, <1 = cautious).
    pub aggression_multiplier: f64,

    /// Fraction of manpower loss before retreat is considered (0.0–1.0).
    /// Higher = tolerates more losses before retreating.
    /// Aggressive = 0.5, Cautious = 0.2.
    pub casualty_tolerance: f64,

    /// Supply burn rate multiplier (1.0 = baseline).
    /// High = fast push (burns more ammo/fuel), Low = slow attrition.
    pub supply_burn_rate: f64,

    /// Maneuver capability bonus (0.0 = baseline, >0 = bonus).
    /// Affects flanking and encirclement tactics.
    pub maneuver_bonus: f64,

    /// Defender power multiplier (1.0 = baseline, >1 = defensive bias).
    pub defensive_bias: f64,

    /// Air doctrine preference for this commander.
    pub air_doctrine: AirDoctrine,

    /// Whether this commander will order a strategic retreat when the
    /// combat power ratio is catastrophic.
    ///
    /// Derived from traits:
    /// - Cautious, Methodical, Cowardly → true
    /// - Aggressive, Reckless, Brave, Loyal → false
    /// - Default (no relevant trait) → false
    pub will_retreat: bool,

    /// Organization bonus/penalty (1.0 = baseline).
    /// Affects unit organization recovery and resistance to attrition.
    pub organization_multiplier: f64,
}

impl Default for MilitaryTacticModifiers {
    fn default() -> Self {
        Self {
            aggression_multiplier: 1.0,
            casualty_tolerance: 0.3, // 30% losses before considering retreat
            supply_burn_rate: 1.0,
            maneuver_bonus: 0.0,
            defensive_bias: 1.0,
            air_doctrine: AirDoctrine::default(),
            will_retreat: false,
            organization_multiplier: 1.0,
        }
    }
}

// ============================================================================
// TRAIT → MODIFIER MAPPING
// ============================================================================

/// Centralized evaluation: traits → `MilitaryTacticModifiers`.
///
/// This is the SINGLE source of truth for trait → military tactic mapping.
/// No other module may inspect raw trait strings for combat behavior.
///
/// # Arguments
/// * `traits` - Slice of trait string IDs from a VIP's `traits` field.
///
/// # Returns
/// A `MilitaryTacticModifiers` struct with all modifier fields set based on
/// the combined effect of all traits. Multiple traits accumulate.
pub fn evaluate_military_tactics(traits: &[String]) -> MilitaryTacticModifiers {
    let mut mods = MilitaryTacticModifiers::default();

    for trait_id in traits {
        apply_military_trait(&mut mods, trait_id);
    }

    mods
}

/// Apply a single trait's military modifiers to the accumulator.
///
/// This is the canonical mapping — no other function may perform
/// trait string checks for military behavior.
fn apply_military_trait(mods: &mut MilitaryTacticModifiers, trait_id: &str) {
    let t = trait_id.to_lowercase();

    match t.as_str() {
        "aggressive" => {
            mods.aggression_multiplier *= 1.5;
            mods.casualty_tolerance = mods.casualty_tolerance.max(0.5);
            mods.supply_burn_rate *= 1.3;
            mods.will_retreat = false;
        }
        "cautious" => {
            mods.aggression_multiplier *= 0.7;
            mods.defensive_bias *= 1.2;
            mods.casualty_tolerance = mods.casualty_tolerance.min(0.2);
            mods.supply_burn_rate *= 0.8;
            mods.will_retreat = true;
        }
        "cunning" => {
            mods.maneuver_bonus += 0.3;
        }
        "brave" => {
            mods.organization_multiplier *= 1.1;
            mods.will_retreat = false;
        }
        "cowardly" => {
            mods.aggression_multiplier *= 0.8;
            mods.will_retreat = true;
        }
        "reckless" => {
            mods.aggression_multiplier *= 1.8;
            mods.casualty_tolerance = mods.casualty_tolerance.max(0.6);
            mods.supply_burn_rate *= 1.5;
            mods.will_retreat = false;
        }
        "methodical" => {
            mods.defensive_bias *= 1.15;
            mods.supply_burn_rate *= 0.85;
            mods.will_retreat = true;
        }
        "loyal" => {
            mods.organization_multiplier *= 1.1;
            mods.will_retreat = false;
        }
        _ => {
            // No military-relevant trait — no modifier
        }
    }
}

// ============================================================================
// COMBAT POWER MODIFICATION
// ============================================================================

/// Applies tactic modifiers to a unit's attack power.
///
/// # Arguments
/// * `base_attack` - The unit's base attack power.
/// * `tactics` - The commander's tactical modifiers.
///
/// # Returns
/// Modified attack power.
pub fn apply_attack_modifier(base_attack: f64, tactics: &MilitaryTacticModifiers) -> f64 {
    base_attack * tactics.aggression_multiplier
}

/// Applies tactic modifiers to a unit's defense power.
///
/// # Arguments
/// * `base_defense` - The unit's base defense power.
/// * `tactics` - The commander's tactical modifiers.
///
/// # Returns
/// Modified defense power.
pub fn apply_defense_modifier(base_defense: f64, tactics: &MilitaryTacticModifiers) -> f64 {
    base_defense * tactics.defensive_bias
}

/// Applies tactic modifiers to a unit's organization.
///
/// # Arguments
/// * `base_organization` - The unit's base organization.
/// * `tactics` - The commander's tactical modifiers.
///
/// # Returns
/// Modified organization.
pub fn apply_organization_modifier(base_organization: f64, tactics: &MilitaryTacticModifiers) -> f64 {
    base_organization * tactics.organization_multiplier
}

/// Converts `MilitaryTacticModifiers` to a `CommanderRetraitProfile` for
/// use with the retreat evaluation system.
pub fn to_retreat_profile(tactics: &MilitaryTacticModifiers, commander_name: &str) -> crate::military::retreat::CommanderRetraitProfile {
    crate::military::retreat::CommanderRetraitProfile {
        will_retreat: tactics.will_retreat,
        commander_name: commander_name.to_string(),
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_modifiers() {
        let mods = MilitaryTacticModifiers::default();
        assert_eq!(mods.aggression_multiplier, 1.0);
        assert_eq!(mods.defensive_bias, 1.0);
        assert!(!mods.will_retreat);
        assert_eq!(mods.maneuver_bonus, 0.0);
    }

    #[test]
    fn test_aggressive_trait() {
        let traits = vec!["Aggressive".to_string()];
        let mods = evaluate_military_tactics(&traits);

        assert!((mods.aggression_multiplier - 1.5).abs() < 0.001);
        assert!(!mods.will_retreat, "Aggressive commanders do not retreat");
        assert!(mods.casualty_tolerance >= 0.5, "Aggressive commanders tolerate more casualties");
    }

    #[test]
    fn test_cautious_trait() {
        let traits = vec!["Cautious".to_string()];
        let mods = evaluate_military_tactics(&traits);

        assert!((mods.aggression_multiplier - 0.7).abs() < 0.001);
        assert!(mods.will_retreat, "Cautious commanders will retreat");
        assert!((mods.defensive_bias - 1.2).abs() < 0.001);
    }

    #[test]
    fn test_cunning_trait() {
        let traits = vec!["Cunning".to_string()];
        let mods = evaluate_military_tactics(&traits);

        assert!((mods.maneuver_bonus - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_brave_trait() {
        let traits = vec!["Brave".to_string()];
        let mods = evaluate_military_tactics(&traits);

        assert!(!mods.will_retreat, "Brave commanders do not retreat");
        assert!((mods.organization_multiplier - 1.1).abs() < 0.001);
    }

    #[test]
    fn test_cowardly_trait() {
        let traits = vec!["Cowardly".to_string()];
        let mods = evaluate_military_tactics(&traits);

        assert!(mods.will_retreat, "Cowardly commanders will retreat");
        assert!((mods.aggression_multiplier - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_reckless_trait() {
        let traits = vec!["Reckless".to_string()];
        let mods = evaluate_military_tactics(&traits);

        assert!(!mods.will_retreat, "Reckless commanders do not retreat");
        assert!(mods.aggression_multiplier > 1.5, "Reckless commanders are very aggressive");
        assert!(mods.supply_burn_rate > 1.0, "Reckless commanders burn more supplies");
    }

    #[test]
    fn test_methodical_trait() {
        let traits = vec!["Methodical".to_string()];
        let mods = evaluate_military_tactics(&traits);

        assert!(mods.will_retreat, "Methodical commanders will retreat");
        assert!(mods.defensive_bias > 1.0, "Methodical commanders have defensive bias");
        assert!(mods.supply_burn_rate < 1.0, "Methodical commanders burn less supplies");
    }

    #[test]
    fn test_loyal_trait() {
        let traits = vec!["Loyal".to_string()];
        let mods = evaluate_military_tactics(&traits);

        assert!(!mods.will_retreat, "Loyal commanders do not retreat");
        assert!((mods.organization_multiplier - 1.1).abs() < 0.001);
    }

    #[test]
    fn test_multiple_traits_accumulate() {
        let traits = vec!["Aggressive".to_string(), "Cunning".to_string()];
        let mods = evaluate_military_tactics(&traits);

        // Aggressive: aggression *= 1.5, Cunning: maneuver += 0.3
        assert!((mods.aggression_multiplier - 1.5).abs() < 0.001);
        assert!((mods.maneuver_bonus - 0.3).abs() < 0.001);
        // Aggressive sets will_retreat = false, Cunning doesn't change it
        assert!(!mods.will_retreat);
    }

    #[test]
    fn test_no_relevant_trait() {
        let traits = vec!["Charismatic".to_string(), "Paranoid".to_string()];
        let mods = evaluate_military_tactics(&traits);

        // These traits have no military effect — defaults should be preserved
        assert_eq!(mods.aggression_multiplier, 1.0);
        assert_eq!(mods.defensive_bias, 1.0);
        assert!(!mods.will_retreat);
    }

    #[test]
    fn test_apply_attack_modifier() {
        let mods = MilitaryTacticModifiers {
            aggression_multiplier: 1.5,
            ..Default::default()
        };
        assert_eq!(apply_attack_modifier(100.0, &mods), 150.0);
    }

    #[test]
    fn test_apply_defense_modifier() {
        let mods = MilitaryTacticModifiers {
            defensive_bias: 1.2,
            ..Default::default()
        };
        assert_eq!(apply_defense_modifier(100.0, &mods), 120.0);
    }

    #[test]
    fn test_to_retreat_profile() {
        let mods = MilitaryTacticModifiers {
            will_retreat: true,
            ..Default::default()
        };
        let profile = to_retreat_profile(&mods, "General Cautious");
        assert!(profile.will_retreat);
        assert_eq!(profile.commander_name, "General Cautious");
    }

    #[test]
    fn test_trait_case_insensitive() {
        let traits = vec!["AGGRESSIVE".to_string()];
        let mods = evaluate_military_tactics(&traits);
        assert!((mods.aggression_multiplier - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_air_doctrine_default() {
        let mods = MilitaryTacticModifiers::default();
        assert_eq!(mods.air_doctrine, AirDoctrine::GroundSupport);
    }
}

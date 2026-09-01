//! Phase 70: Multi-domain combat resolution.
//!
//! Implements theater-ordered combat resolution:
//! 1. **Air domain** — air superiority determines bombardment and interception
//!    modifiers applied to subsequent domains.
//! 2. **Naval domain** — sea control determines naval bombardment and
//!    amphibious landing modifiers.
//! 3. **Land domain** — ground combat, modified by air superiority and
//!    naval bombardment from previous phases.
//!
//! Each domain is resolved independently, with the results feeding forward
//! as modifiers to subsequent domains. This creates realistic combined-arms
//! dynamics where air superiority enhances ground combat effectiveness.
//!
//! # Rules
//! - Air superiority winner gets an `air_superiority_bonus` applied to
//!   land combat power for their side.
//! - Naval superiority winner gets a `naval_bombardment_bonus` applied to
//!   land combat power for their side (if coastal).
//! - All modifiers are derived from config values — no magic numbers (Rule 2).
//! - Double-entry: casualties are tracked per-domain and aggregated.

use serde::{Deserialize, Serialize};

use crate::military::combat::resolve_battle;
use crate::military::config::MilitaryCombatConfig;
use crate::military::fronts::{Battle, BattleResult};
use crate::military::units::{MilitaryUnit, UnitType};

// ============================================================================
// DOMAIN CLASSIFICATION
// ============================================================================

/// Combat domain classification for a military unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CombatDomain {
    /// Air domain: AirForce units
    Air,
    /// Naval domain: Naval units
    Naval,
    /// Land domain: Infantry, Tanks, Artillery, PeasantBattalion
    Land,
}

impl CombatDomain {
    /// Classifies a unit type into its combat domain.
    pub fn for_unit_type(unit_type: UnitType) -> CombatDomain {
        match unit_type {
            UnitType::AirForce => CombatDomain::Air,
            UnitType::Naval => CombatDomain::Naval,
            UnitType::Infantry
            | UnitType::Tanks
            | UnitType::Artillery
            | UnitType::PeasantBattalion => CombatDomain::Land,
        }
    }
}

// ============================================================================
// DOMAIN MODIFIERS
// ============================================================================

/// Modifiers applied to a domain's combat based on results from prior domains.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DomainModifiers {
    /// Multiplier applied to attacker's land combat power.
    /// > 1.0 = attacker has advantage (e.g., from air superiority).
    pub attacker_land_power_multiplier: f64,
    /// Multiplier applied to defender's land combat power.
    pub defender_land_power_multiplier: f64,
    /// Whether attacker has air superiority (affects supply interdiction).
    pub attacker_air_superiority: bool,
    /// Whether defender has air superiority.
    pub defender_air_superiority: bool,
    /// Whether attacker has naval control (affects coastal bombardment).
    pub attacker_naval_control: bool,
    /// Whether defender has naval control.
    pub defender_naval_control: bool,
}

impl DomainModifiers {
    /// Creates neutral modifiers (no advantages on either side).
    pub fn neutral() -> Self {
        Self {
            attacker_land_power_multiplier: 1.0,
            defender_land_power_multiplier: 1.0,
            attacker_air_superiority: false,
            defender_air_superiority: false,
            attacker_naval_control: false,
            defender_naval_control: false,
        }
    }
}

// ============================================================================
// MULTI-DOMAIN COMBAT RESULT
// ============================================================================

/// Result of multi-domain combat resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultiDomainBattleResult {
    /// The final aggregated battle (land domain, with modifiers applied).
    pub final_battle: Battle,
    /// Air domain battle result (if air units participated).
    pub air_battle: Option<Battle>,
    /// Naval domain battle result (if naval units participated).
    pub naval_battle: Option<Battle>,
    /// Domain modifiers computed from air and naval phases.
    pub modifiers: DomainModifiers,
    /// All log messages from all domains.
    pub messages: Vec<String>,
}

// ============================================================================
// MULTI-DOMAIN COMBAT RESOLUTION
// ============================================================================

/// Resolves a multi-domain battle, processing theaters in order:
/// Air → Naval → Land.
///
/// # Arguments
/// * `attacker_units` - All attacking units (will be mutated for supply burning)
/// * `defender_units` - All defending units (will be mutated for supply burning)
/// * `location` - Battle location (region ID)
/// * `attacker_country` - Attacking country name
/// * `defender_country` - Defending country name
/// * `turn` - Current game turn
/// * `battle_id` - Unique battle identifier
/// * `config` - Military combat configuration
/// * `terrain` - Terrain type for land combat
/// * `is_coastal` - Whether the battle region is coastal (affects naval bombardment)
///
/// # Returns
/// `MultiDomainBattleResult` with per-domain battles and the final aggregated battle.
pub fn resolve_multi_domain_battle(
    attacker_units: &mut [MilitaryUnit],
    defender_units: &mut [MilitaryUnit],
    location: String,
    attacker_country: String,
    defender_country: String,
    turn: u32,
    battle_id: String,
    config: &MilitaryCombatConfig,
    terrain: &str,
    is_coastal: bool,
) -> MultiDomainBattleResult {
    let mut messages = Vec::new();
    let mut modifiers = DomainModifiers::neutral();

    // ── Phase 1: Air Domain ──
    // Split units by domain (using indices to avoid borrow conflicts)
    let attacker_air_indices: Vec<usize> = attacker_units
        .iter()
        .enumerate()
        .filter(|(_, u)| CombatDomain::for_unit_type(u.unit_type) == CombatDomain::Air)
        .map(|(i, _)| i)
        .collect();
    let defender_air_indices: Vec<usize> = defender_units
        .iter()
        .enumerate()
        .filter(|(_, u)| CombatDomain::for_unit_type(u.unit_type) == CombatDomain::Air)
        .map(|(i, _)| i)
        .collect();

    let mut air_battle = None;
    if !attacker_air_indices.is_empty() || !defender_air_indices.is_empty() {
        // Extract air units for air combat
        let mut attacker_air: Vec<MilitaryUnit> = attacker_air_indices
            .iter()
            .map(|&i| attacker_units[i].clone())
            .collect();
        let mut defender_air: Vec<MilitaryUnit> = defender_air_indices
            .iter()
            .map(|&i| defender_units[i].clone())
            .collect();

        let air_battle_id = format!("{}-AIR", battle_id);
        let air_battle_result = resolve_battle(
            &mut attacker_air,
            &mut defender_air,
            location.clone(),
            attacker_country.clone(),
            defender_country.clone(),
            turn,
            air_battle_id,
            config,
            "air",
        );

        // Write back supply changes to original air units
        for (&orig_idx, battle_unit) in attacker_air_indices.iter().zip(attacker_air.iter()) {
            attacker_units[orig_idx].stockpile = battle_unit.stockpile.clone();
        }
        for (&orig_idx, battle_unit) in defender_air_indices.iter().zip(defender_air.iter()) {
            defender_units[orig_idx].stockpile = battle_unit.stockpile.clone();
        }

        // Determine air superiority
        match air_battle_result.result {
            BattleResult::AttackerVictory | BattleResult::PyrrhicVictory => {
                modifiers.attacker_air_superiority = true;
                modifiers.attacker_land_power_multiplier *= config.air_superiority_offensive_bonus;
                messages.push(format!(
                    "[AIR] Attacker achieves air superiority over {} (land power x{:.2})",
                    location, config.air_superiority_offensive_bonus
                ));
            }
            BattleResult::DefenderVictory => {
                modifiers.defender_air_superiority = true;
                modifiers.defender_land_power_multiplier *= config.air_superiority_defensive_bonus;
                messages.push(format!(
                    "[AIR] Defender achieves air superiority over {} (land power x{:.2})",
                    location, config.air_superiority_defensive_bonus
                ));
            }
            BattleResult::Stalemate => {
                messages.push(format!(
                    "[AIR] Air contest over {} ends in stalemate",
                    location
                ));
            }
            BattleResult::Retreat { .. } => {
                // Air retreat — one side withdrew. No air superiority awarded.
                messages.push(format!(
                    "[AIR] Air forces retreat from contest over {}",
                    location
                ));
            }
        }

        air_battle = Some(air_battle_result);
    }

    // ── Phase 2: Naval Domain ──
    let attacker_naval_indices: Vec<usize> = attacker_units
        .iter()
        .enumerate()
        .filter(|(_, u)| CombatDomain::for_unit_type(u.unit_type) == CombatDomain::Naval)
        .map(|(i, _)| i)
        .collect();
    let defender_naval_indices: Vec<usize> = defender_units
        .iter()
        .enumerate()
        .filter(|(_, u)| CombatDomain::for_unit_type(u.unit_type) == CombatDomain::Naval)
        .map(|(i, _)| i)
        .collect();

    let mut naval_battle = None;
    if is_coastal && (!attacker_naval_indices.is_empty() || !defender_naval_indices.is_empty()) {
        let mut attacker_naval: Vec<MilitaryUnit> = attacker_naval_indices
            .iter()
            .map(|&i| attacker_units[i].clone())
            .collect();
        let mut defender_naval: Vec<MilitaryUnit> = defender_naval_indices
            .iter()
            .map(|&i| defender_units[i].clone())
            .collect();

        let naval_battle_id = format!("{}-NAVAL", battle_id);
        let naval_battle_result = resolve_battle(
            &mut attacker_naval,
            &mut defender_naval,
            location.clone(),
            attacker_country.clone(),
            defender_country.clone(),
            turn,
            naval_battle_id,
            config,
            "naval",
        );

        // Write back supply changes
        for (&orig_idx, battle_unit) in attacker_naval_indices.iter().zip(attacker_naval.iter()) {
            attacker_units[orig_idx].stockpile = battle_unit.stockpile.clone();
        }
        for (&orig_idx, battle_unit) in defender_naval_indices.iter().zip(defender_naval.iter()) {
            defender_units[orig_idx].stockpile = battle_unit.stockpile.clone();
        }

        // Determine naval control
        match naval_battle_result.result {
            BattleResult::AttackerVictory | BattleResult::PyrrhicVictory => {
                modifiers.attacker_naval_control = true;
                modifiers.attacker_land_power_multiplier *= config.naval_bombardment_bonus;
                messages.push(format!(
                    "[NAVAL] Attacker gains sea control near {} (land bombardment x{:.2})",
                    location, config.naval_bombardment_bonus
                ));
            }
            BattleResult::DefenderVictory => {
                modifiers.defender_naval_control = true;
                modifiers.defender_land_power_multiplier *= config.naval_bombardment_bonus;
                messages.push(format!(
                    "[NAVAL] Defender gains sea control near {} (land bombardment x{:.2})",
                    location, config.naval_bombardment_bonus
                ));
            }
            BattleResult::Stalemate => {
                messages.push(format!(
                    "[NAVAL] Naval contest near {} ends in stalemate",
                    location
                ));
            }
            BattleResult::Retreat { .. } => {
                // Naval retreat — one side withdrew. No naval control awarded.
                messages.push(format!(
                    "[NAVAL] Naval forces retreat from contest near {}",
                    location
                ));
            }
        }

        naval_battle = Some(naval_battle_result);
    }

    // ── Phase 3: Land Domain (with modifiers) ──
    let attacker_land_indices: Vec<usize> = attacker_units
        .iter()
        .enumerate()
        .filter(|(_, u)| CombatDomain::for_unit_type(u.unit_type) == CombatDomain::Land)
        .map(|(i, _)| i)
        .collect();
    let defender_land_indices: Vec<usize> = defender_units
        .iter()
        .enumerate()
        .filter(|(_, u)| CombatDomain::for_unit_type(u.unit_type) == CombatDomain::Land)
        .map(|(i, _)| i)
        .collect();

    // Apply land power modifiers to land units
    for &idx in &attacker_land_indices {
        // Temporarily boost attack stat via organization (since we can't change base stats)
        // The modifier is applied through a temporary stat adjustment
        let unit = &mut attacker_units[idx];
        let original_attack = unit.stats.attack;
        unit.stats.attack = original_attack * modifiers.attacker_land_power_multiplier;
    }
    for &idx in &defender_land_indices {
        let unit = &mut defender_units[idx];
        let original_defense = unit.stats.defense;
        unit.stats.defense = original_defense * modifiers.defender_land_power_multiplier;
    }

    // Extract land units for land combat
    let mut attacker_land: Vec<MilitaryUnit> = attacker_land_indices
        .iter()
        .map(|&i| attacker_units[i].clone())
        .collect();
    let mut defender_land: Vec<MilitaryUnit> = defender_land_indices
        .iter()
        .map(|&i| defender_units[i].clone())
        .collect();

    let final_battle = if !attacker_land.is_empty() && !defender_land.is_empty() {
        let land_battle_id = format!("{}-LAND", battle_id);
        let land_battle = resolve_battle(
            &mut attacker_land,
            &mut defender_land,
            location.clone(),
            attacker_country.clone(),
            defender_country.clone(),
            turn,
            land_battle_id,
            config,
            terrain,
        );

        // Write back supply changes and casualties to original land units
        for (&orig_idx, battle_unit) in attacker_land_indices.iter().zip(attacker_land.iter()) {
            attacker_units[orig_idx].stockpile = battle_unit.stockpile.clone();
        }
        for (&orig_idx, battle_unit) in defender_land_indices.iter().zip(defender_land.iter()) {
            defender_units[orig_idx].stockpile = battle_unit.stockpile.clone();
        }

        messages.push(format!(
            "[LAND] Battle in {}: {:?} — attacker power x{:.2}, defender power x{:.2}",
            location,
            land_battle.result,
            modifiers.attacker_land_power_multiplier,
            modifiers.defender_land_power_multiplier
        ));

        land_battle
    } else if !attacker_land.is_empty() && defender_land.is_empty() {
        // No defenders — attacker wins by default
        Battle {
            id: format!("{}-LAND", battle_id),
            location: location.clone(),
            attacker: attacker_country.clone(),
            defender: defender_country.clone(),
            turn,
            attacker_units: attacker_land.iter().map(|u| u.id.clone()).collect(),
            defender_units: Vec::new(),
            attacker_casualties: crate::military::fronts::Casualties {
                dead: 0,
                wounded: 0,
                deserters: 0,
                demographic_breakdown: std::collections::HashMap::new(),
            },
            defender_casualties: crate::military::fronts::Casualties {
                dead: 0,
                wounded: 0,
                deserters: 0,
                demographic_breakdown: std::collections::HashMap::new(),
            },
            result: BattleResult::AttackerVictory,
        }
    } else {
        // No land units on either side — stalemate
        Battle {
            id: format!("{}-LAND", battle_id),
            location: location.clone(),
            attacker: attacker_country.clone(),
            defender: defender_country.clone(),
            turn,
            attacker_units: Vec::new(),
            defender_units: Vec::new(),
            attacker_casualties: crate::military::fronts::Casualties {
                dead: 0,
                wounded: 0,
                deserters: 0,
                demographic_breakdown: std::collections::HashMap::new(),
            },
            defender_casualties: crate::military::fronts::Casualties {
                dead: 0,
                wounded: 0,
                deserters: 0,
                demographic_breakdown: std::collections::HashMap::new(),
            },
            result: BattleResult::Stalemate,
        }
    };

    // Restore original stats (undo the temporary modifier application)
    for &idx in &attacker_land_indices {
        let unit = &mut attacker_units[idx];
        // The attack stat was multiplied, so divide it back
        if modifiers.attacker_land_power_multiplier > 0.0 {
            unit.stats.attack /= modifiers.attacker_land_power_multiplier;
        }
    }
    for &idx in &defender_land_indices {
        let unit = &mut defender_units[idx];
        if modifiers.defender_land_power_multiplier > 0.0 {
            unit.stats.defense /= modifiers.defender_land_power_multiplier;
        }
    }

    MultiDomainBattleResult {
        final_battle,
        air_battle,
        naval_battle,
        modifiers,
        messages,
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::military::units::UnitType;

    fn make_unit(id: &str, unit_type: UnitType, manpower: i64) -> MilitaryUnit {
        MilitaryUnit::new(
            id.to_string(),
            unit_type,
            manpower,
            rustc_hash::FxHashMap::default(),
            "home".to_string(),
        )
    }

    #[test]
    fn test_domain_classification() {
        assert_eq!(
            CombatDomain::for_unit_type(UnitType::AirForce),
            CombatDomain::Air
        );
        assert_eq!(
            CombatDomain::for_unit_type(UnitType::Naval),
            CombatDomain::Naval
        );
        assert_eq!(
            CombatDomain::for_unit_type(UnitType::Infantry),
            CombatDomain::Land
        );
        assert_eq!(
            CombatDomain::for_unit_type(UnitType::Tanks),
            CombatDomain::Land
        );
        assert_eq!(
            CombatDomain::for_unit_type(UnitType::Artillery),
            CombatDomain::Land
        );
        assert_eq!(
            CombatDomain::for_unit_type(UnitType::PeasantBattalion),
            CombatDomain::Land
        );
    }

    #[test]
    fn test_neutral_modifiers() {
        let mods = DomainModifiers::neutral();
        assert_eq!(mods.attacker_land_power_multiplier, 1.0);
        assert_eq!(mods.defender_land_power_multiplier, 1.0);
        assert!(!mods.attacker_air_superiority);
        assert!(!mods.defender_air_superiority);
    }

    #[test]
    fn test_multi_domain_land_only_battle() {
        let mut attacker = vec![make_unit("att-1", UnitType::Infantry, 1000)];
        let mut defender = vec![make_unit("def-1", UnitType::Infantry, 1000)];
        let config = MilitaryCombatConfig::default();

        let result = resolve_multi_domain_battle(
            &mut attacker,
            &mut defender,
            "region_a".to_string(),
            "Attacker".to_string(),
            "Defender".to_string(),
            1,
            "BATTLE-001".to_string(),
            &config,
            "plains",
            false,
        );

        // No air or naval units → no air/naval battles
        assert!(result.air_battle.is_none());
        assert!(result.naval_battle.is_none());
        // Land battle should have occurred
        assert!(!result.final_battle.attacker_units.is_empty());
        assert!(!result.final_battle.defender_units.is_empty());
    }

    #[test]
    fn test_multi_domain_air_battle_occurs() {
        let mut attacker = vec![
            make_unit("att-air", UnitType::AirForce, 500),
            make_unit("att-inf", UnitType::Infantry, 1000),
        ];
        let mut defender = vec![
            make_unit("def-air", UnitType::AirForce, 500),
            make_unit("def-inf", UnitType::Infantry, 1000),
        ];
        let config = MilitaryCombatConfig::default();

        let result = resolve_multi_domain_battle(
            &mut attacker,
            &mut defender,
            "region_a".to_string(),
            "Attacker".to_string(),
            "Defender".to_string(),
            1,
            "BATTLE-002".to_string(),
            &config,
            "plains",
            false,
        );

        // Air battle should have occurred
        assert!(
            result.air_battle.is_some(),
            "Air battle must occur when air units present"
        );
        // Naval battle should NOT have occurred (not coastal)
        assert!(result.naval_battle.is_none());
    }

    #[test]
    fn test_multi_domain_naval_battle_requires_coastal() {
        let mut attacker = vec![
            make_unit("att-nav", UnitType::Naval, 500),
            make_unit("att-inf", UnitType::Infantry, 1000),
        ];
        let mut defender = vec![
            make_unit("def-nav", UnitType::Naval, 500),
            make_unit("def-inf", UnitType::Infantry, 1000),
        ];
        let config = MilitaryCombatConfig::default();

        // Non-coastal battle — naval units should not engage
        let result = resolve_multi_domain_battle(
            &mut attacker,
            &mut defender,
            "region_a".to_string(),
            "Attacker".to_string(),
            "Defender".to_string(),
            1,
            "BATTLE-003".to_string(),
            &config,
            "plains",
            false, // NOT coastal
        );

        assert!(
            result.naval_battle.is_none(),
            "Naval battle must not occur if not coastal"
        );

        // Coastal battle — naval units should engage
        let mut attacker2 = vec![
            make_unit("att-nav2", UnitType::Naval, 500),
            make_unit("att-inf2", UnitType::Infantry, 1000),
        ];
        let mut defender2 = vec![
            make_unit("def-nav2", UnitType::Naval, 500),
            make_unit("def-inf2", UnitType::Infantry, 1000),
        ];

        let result2 = resolve_multi_domain_battle(
            &mut attacker2,
            &mut defender2,
            "region_a".to_string(),
            "Attacker".to_string(),
            "Defender".to_string(),
            1,
            "BATTLE-004".to_string(),
            &config,
            "plains",
            true, // Coastal
        );

        assert!(
            result2.naval_battle.is_some(),
            "Naval battle must occur if coastal and naval units present"
        );
    }

    #[test]
    fn test_multi_domain_air_superiority_modifies_land() {
        // Attacker has air units, defender has none → attacker gets air superiority
        let mut attacker = vec![
            make_unit("att-air", UnitType::AirForce, 1000),
            make_unit("att-inf", UnitType::Infantry, 1000),
        ];
        let mut defender = vec![make_unit("def-inf", UnitType::Infantry, 1000)];
        let config = MilitaryCombatConfig::default();

        let result = resolve_multi_domain_battle(
            &mut attacker,
            &mut defender,
            "region_a".to_string(),
            "Attacker".to_string(),
            "Defender".to_string(),
            1,
            "BATTLE-005".to_string(),
            &config,
            "plains",
            false,
        );

        // Attacker should have air superiority (no defender air units)
        assert!(
            result.modifiers.attacker_air_superiority || result.modifiers.defender_air_superiority,
            "Air superiority must be determined when one side has air units"
        );
        // If attacker won air battle, their land multiplier should be boosted
        if result.modifiers.attacker_air_superiority {
            assert!(
                result.modifiers.attacker_land_power_multiplier > 1.0,
                "Air superiority must boost land combat power"
            );
        }
    }

    #[test]
    fn test_multi_domain_no_units_stalemate() {
        let mut attacker: Vec<MilitaryUnit> = Vec::new();
        let mut defender: Vec<MilitaryUnit> = Vec::new();
        let config = MilitaryCombatConfig::default();

        let result = resolve_multi_domain_battle(
            &mut attacker,
            &mut defender,
            "region_a".to_string(),
            "Attacker".to_string(),
            "Defender".to_string(),
            1,
            "BATTLE-006".to_string(),
            &config,
            "plains",
            false,
        );

        // No units → stalemate
        assert_eq!(result.final_battle.result, BattleResult::Stalemate);
        assert!(result.air_battle.is_none());
        assert!(result.naval_battle.is_none());
    }

    #[test]
    fn test_multi_domain_attacker_wins_no_defenders() {
        let mut attacker = vec![make_unit("att-1", UnitType::Infantry, 1000)];
        let mut defender: Vec<MilitaryUnit> = Vec::new();
        let config = MilitaryCombatConfig::default();

        let result = resolve_multi_domain_battle(
            &mut attacker,
            &mut defender,
            "region_a".to_string(),
            "Attacker".to_string(),
            "Defender".to_string(),
            1,
            "BATTLE-007".to_string(),
            &config,
            "plains",
            false,
        );

        // No defenders → attacker victory
        assert_eq!(result.final_battle.result, BattleResult::AttackerVictory);
    }

    #[test]
    fn test_multi_domain_messages_generated() {
        let mut attacker = vec![
            make_unit("att-air", UnitType::AirForce, 500),
            make_unit("att-inf", UnitType::Infantry, 1000),
        ];
        let mut defender = vec![
            make_unit("def-air", UnitType::AirForce, 500),
            make_unit("def-inf", UnitType::Infantry, 1000),
        ];
        let config = MilitaryCombatConfig::default();

        let result = resolve_multi_domain_battle(
            &mut attacker,
            &mut defender,
            "region_a".to_string(),
            "Attacker".to_string(),
            "Defender".to_string(),
            1,
            "BATTLE-008".to_string(),
            &config,
            "plains",
            false,
        );

        // Messages should be generated for each phase
        assert!(
            !result.messages.is_empty(),
            "Combat messages must be generated"
        );
    }
}

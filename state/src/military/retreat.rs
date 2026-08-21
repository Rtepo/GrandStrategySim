//! Phase 70.4b: Strategic retreat — VIP tactics.
//!
//! Implements the strategic retreat mechanic for combat. When the combat power
//! ratio is catastrophic (one side is overwhelmingly stronger), the weaker
//! side's commander may order a retreat instead of fighting to the death.
//!
//! Whether a retreat is ordered depends on the commander's `will_retreat`
//! trait, which is derived from their `MilitaryTacticModifiers`:
//! - `Cautious`, `Methodical`, `Cowardly` → will retreat
//! - `Aggressive`, `Reckless`, `Brave`, `Loyal` → will NOT retreat
//! - Default (no relevant trait) → will NOT retreat (fight it out)
//!
//! # Effects of Retreat
//! - The retreating side cedes `RegionControl` to the attacker.
//! - The retreating side takes drastically reduced casualties
//!   (`retreat_casualty_ratio` vs `max_loser_casualty_ratio`).
//! - The opposing side takes minimal casualties
//!   (`retreat_attacker_casualty_ratio`).
//! - The retreating side loses a fraction of equipment
//!   (`retreat_equipment_loss_rate`), which flows to the victor as
//!   captured materiel (added to victor's `military_stockpile`).

use std::collections::HashMap;

use crate::military::config::MilitaryCombatConfig;
use crate::military::fronts::{BattleResult, Casualties};
use crate::military::units::{MilitaryUnit, EquipmentReserve};
use crate::registries::enums::Commodity;
use crate::society::geography::RuralClass;

// ============================================================================
// COMMANDER RETREAT DECISION
// ============================================================================

/// Represents a commander's tactical decision-making profile.
///
/// Derived from VIP traits. The `will_retreat` field determines whether
/// the commander will order a strategic retreat when the combat power ratio
/// is catastrophic.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommanderRetraitProfile {
    /// Whether this commander will order a retreat when the power ratio
    /// is catastrophic.
    pub will_retreat: bool,
    /// The commander's name (for logging).
    pub commander_name: String,
}

impl CommanderRetraitProfile {
    /// Creates a profile for a commander who will retreat (Cautious, Methodical, Cowardly).
    pub fn retreating(name: String) -> Self {
        Self {
            will_retreat: true,
            commander_name: name,
        }
    }

    /// Creates a profile for a commander who will NOT retreat (Aggressive, Reckless, Brave, Loyal).
    pub fn fighting(name: String) -> Self {
        Self {
            will_retreat: false,
            commander_name: name,
        }
    }

    /// Creates a profile for a commander with no commander assigned (default: no retreat).
    pub fn no_commander() -> Self {
        Self {
            will_retreat: false,
            commander_name: "none".to_string(),
        }
    }
}

// ============================================================================
// RETREAT EVALUATION
// ============================================================================

/// Result of evaluating whether a retreat should occur.
#[derive(Debug, Clone, PartialEq)]
pub enum RetreatEvaluation {
    /// No retreat — combat proceeds normally.
    NoRetreat,
    /// Defender retreats — cedes region control to attacker.
    DefenderRetreats,
    /// Attacker retreats — aborts the attack, no region control change.
    AttackerRetreats,
}

/// Evaluates whether a strategic retreat should occur based on the combat
/// power ratio and the commanders' tactical profiles.
///
/// # Arguments
/// * `attacker_power` - Total combat power of the attacking side.
/// * `defender_power` - Total combat power of the defending side.
/// * `attacker_commander` - The attacker's highest-ranking commander's profile.
/// * `defender_commander` - The defender's highest-ranking commander's profile.
/// * `config` - Military combat configuration (contains `catastrophic_power_ratio`).
///
/// # Returns
/// `RetreatEvaluation` indicating whether a retreat should occur.
///
/// # Rules
/// - If `defender_power < attacker_power * catastrophic_power_ratio`
///   (defender is overwhelmed) AND defender commander's `will_retreat` is true:
///   → `DefenderRetreats`
/// - If `attacker_power < defender_power * catastrophic_power_ratio`
///   (attacker walked into a meat grinder) AND attacker commander's `will_retreat` is true:
///   → `AttackerRetreats`
/// - Otherwise: `NoRetreat`
pub fn evaluate_retreat(
    attacker_power: f64,
    defender_power: f64,
    attacker_commander: &CommanderRetraitProfile,
    defender_commander: &CommanderRetraitProfile,
    config: &MilitaryCombatConfig,
) -> RetreatEvaluation {
    let catastrophic = config.catastrophic_power_ratio;

    // Check if defender is overwhelmed
    if defender_power < attacker_power * catastrophic {
        if defender_commander.will_retreat {
            return RetreatEvaluation::DefenderRetreats;
        }
    }

    // Check if attacker walked into a meat grinder
    if attacker_power < defender_power * catastrophic {
        if attacker_commander.will_retreat {
            return RetreatEvaluation::AttackerRetreats;
        }
    }

    RetreatEvaluation::NoRetreat
}

// ============================================================================
// RETREAT CASUALTIES AND EQUIPMENT LOSS
// ============================================================================

/// Result of processing a strategic retreat.
#[derive(Debug, Clone, PartialEq)]
pub struct RetreatResult {
    /// The battle result (Retreat variant with retreating side identified).
    pub battle_result: BattleResult,
    /// Casualties suffered by the retreating side.
    pub retreating_casualties: Casualties,
    /// Casualties suffered by the opposing (victorious) side.
    pub victor_casualties: Casualties,
    /// Equipment abandoned by the retreating side, captured by the victor.
    /// Maps commodity → quantity captured.
    pub captured_equipment: HashMap<Commodity, f64>,
    /// Log messages.
    pub messages: Vec<String>,
}

/// Processes a strategic retreat, calculating reduced casualties and
/// equipment loss.
///
/// This function:
/// 1. Calculates reduced casualties for the retreating side
///    (`retreat_casualty_ratio` instead of `max_loser_casualty_ratio`).
/// 2. Calculates minimal casualties for the victorious side
///    (`retreat_attacker_casualty_ratio`).
/// 3. Strips equipment from the retreating side's units
///    (`retreat_equipment_loss_rate` fraction of ToE equipment).
/// 4. Returns the captured equipment for the victor to add to their stockpile.
///
/// # Arguments
/// * `retreating_units` - Units of the retreating side (will be mutated to
///   strip equipment).
/// * `victor_units` - Units of the victorious side (not mutated, used for
///   casualty calculation).
/// * `retreating_side_name` - Name of the retreating country.
/// * `victor_side_name` - Name of the victorious country.
/// * `config` - Military combat configuration.
///
/// # Returns
/// `RetreatResult` with casualties and captured equipment.
pub fn process_retreat(
    retreating_units: &mut [MilitaryUnit],
    victor_units: &[MilitaryUnit],
    retreating_side_name: &str,
    victor_side_name: &str,
    config: &MilitaryCombatConfig,
) -> RetreatResult {
    let mut messages = Vec::new();
    let mut captured_equipment: HashMap<Commodity, f64> = HashMap::new();

    // Calculate casualties for the retreating side (reduced)
    let retreating_casualties = calculate_retreat_casualties(
        retreating_units,
        config.retreat_casualty_ratio,
        config,
    );

    // Calculate casualties for the victorious side (minimal)
    let victor_casualties = calculate_retreat_casualties(
        victor_units,
        config.retreat_attacker_casualty_ratio,
        config,
    );

    // Strip equipment from retreating units
    let equipment_loss_rate = config.retreat_equipment_loss_rate;
    for unit in retreating_units.iter_mut() {
        for reserve in &mut unit.equipment_reserves {
            let lost_qty = reserve.current_quantity * equipment_loss_rate;
            if lost_qty > 0.0 {
                *captured_equipment.entry(reserve.commodity).or_insert(0.0) += lost_qty;
                reserve.current_quantity -= lost_qty;
            }
        }
    }

    messages.push(format!(
        "[RETREAT] {} retreats from battle. {} casualties (reduced), {} equipment captured by {}.",
        retreating_side_name,
        retreating_casualties.total(),
        captured_equipment.values().map(|v| *v as i64).sum::<i64>(),
        victor_side_name
    ));

    RetreatResult {
        battle_result: BattleResult::Retreat {
            retreating_side: retreating_side_name.to_string(),
        },
        retreating_casualties,
        victor_casualties,
        captured_equipment,
        messages,
    }
}

/// Calculates casualties for a retreat (reduced ratio).
///
/// Same structure as `calculate_casualties` in combat.rs but with a
/// custom casualty ratio (not derived from battle power).
fn calculate_retreat_casualties(
    units: &[MilitaryUnit],
    casualty_ratio: f64,
    config: &MilitaryCombatConfig,
) -> Casualties {
    let total_manpower: i64 = units.iter().map(|u| u.manpower).sum();
    let base_casualties = (total_manpower as f64 * casualty_ratio) as i64;

    let dead = (base_casualties as f64 * config.dead_ratio) as i64;
    let wounded = (base_casualties as f64 * config.wounded_ratio) as i64;
    let deserters = base_casualties - dead - wounded;

    // Aggregate demographic breakdown
    let mut demographic_breakdown: HashMap<RuralClass, i64> = HashMap::new();
    for unit in units {
        for (rural_class, &count) in &unit.manpower_origin {
            *demographic_breakdown.entry(rural_class.clone()).or_insert(0) += count;
        }
    }

    // Scale demographic breakdown to match total casualties
    let total_origin: i64 = demographic_breakdown.values().sum();
    if total_origin > 0 {
        let scale = base_casualties as f64 / total_origin as f64;
        for count in demographic_breakdown.values_mut() {
            *count = (*count as f64 * scale) as i64;
        }
    }

    Casualties {
        dead,
        wounded,
        deserters,
        demographic_breakdown,
    }
}

/// Applies captured equipment to the victor's military stockpile.
///
/// This is a physical commodity transfer — no fiat cash is created (Rule 1 & 3).
/// The retreating side abandoned physical equipment, and the victor recovers it.
///
/// # Arguments
/// * `stockpile` - The victor's military stockpile (will be credited).
/// * `captured_equipment` - Equipment captured from the retreating side.
pub fn apply_captured_equipment_to_stockpile(
    stockpile: &mut HashMap<Commodity, f64>,
    captured_equipment: &HashMap<Commodity, f64>,
) {
    for (commodity, qty) in captured_equipment {
        if *qty > 0.0 {
            *stockpile.entry(*commodity).or_insert(0.0) += qty;
        }
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
        let mut unit = MilitaryUnit::new(
            id.to_string(),
            unit_type,
            manpower,
            std::collections::HashMap::new(),
            "home".to_string(),
        );
        // Add some equipment for retreat loss testing
        unit.equipment_reserves = vec![
            EquipmentReserve {
                commodity: Commodity::Rifles,
                toe_quantity: 1000.0,
                current_quantity: 900.0,
                condition: 0.8,
                depreciation_rate: 0.01,
            },
            EquipmentReserve {
                commodity: Commodity::Ammunition,
                toe_quantity: 500.0,
                current_quantity: 400.0,
                condition: 0.9,
                depreciation_rate: 0.02,
            },
        ];
        unit
    }

    fn make_config() -> MilitaryCombatConfig {
        MilitaryCombatConfig::default()
    }

    #[test]
    fn test_retreat_evaluation_defender_overwhelmed_cautious() {
        let config = make_config();
        let attacker_cmd = CommanderRetraitProfile::fighting("AggressiveGen".to_string());
        let defender_cmd = CommanderRetraitProfile::retreating("CautiousGen".to_string());

        // Defender is overwhelmed: 100 < 1000 * 0.333 = 333
        let result = evaluate_retreat(1000.0, 100.0, &attacker_cmd, &defender_cmd, &config);

        assert_eq!(result, RetreatEvaluation::DefenderRetreats);
    }

    #[test]
    fn test_retreat_evaluation_defender_overwhelmed_aggressive_no_retreat() {
        let config = make_config();
        let attacker_cmd = CommanderRetraitProfile::fighting("AggressiveGen".to_string());
        let defender_cmd = CommanderRetraitProfile::fighting("BraveGen".to_string());

        // Defender is overwhelmed but commander is aggressive — no retreat
        let result = evaluate_retreat(1000.0, 100.0, &attacker_cmd, &defender_cmd, &config);

        assert_eq!(result, RetreatEvaluation::NoRetreat);
    }

    #[test]
    fn test_retreat_evaluation_attacker_meatgrinder_cautious() {
        let config = make_config();
        let attacker_cmd = CommanderRetraitProfile::retreating("CautiousGen".to_string());
        let defender_cmd = CommanderRetraitProfile::fighting("AggressiveGen".to_string());

        // Attacker walked into a meat grinder: 100 < 1000 * 0.333 = 333
        let result = evaluate_retreat(100.0, 1000.0, &attacker_cmd, &defender_cmd, &config);

        assert_eq!(result, RetreatEvaluation::AttackerRetreats);
    }

    #[test]
    fn test_retreat_evaluation_balanced_no_retreat() {
        let config = make_config();
        let attacker_cmd = CommanderRetraitProfile::retreating("CautiousGen".to_string());
        let defender_cmd = CommanderRetraitProfile::retreating("CautiousGen".to_string());

        // Balanced forces — no retreat even with cautious commanders
        let result = evaluate_retreat(500.0, 500.0, &attacker_cmd, &defender_cmd, &config);

        assert_eq!(result, RetreatEvaluation::NoRetreat);
    }

    #[test]
    fn test_retreat_evaluation_no_commander_no_retreat() {
        let config = make_config();
        let attacker_cmd = CommanderRetraitProfile::no_commander();
        let defender_cmd = CommanderRetraitProfile::no_commander();

        // No commander → default is no retreat
        let result = evaluate_retreat(1000.0, 100.0, &attacker_cmd, &defender_cmd, &config);

        assert_eq!(result, RetreatEvaluation::NoRetreat);
    }

    #[test]
    fn test_process_retreat_reduces_casualties() {
        let config = make_config();
        let mut retreating = vec![make_unit("ret-1", UnitType::Infantry, 1000)];
        let victor = vec![make_unit("vic-1", UnitType::Infantry, 1000)];

        let result = process_retreat(
            &mut retreating,
            &victor,
            "DefenderCountry",
            "AttackerCountry",
            &config,
        );

        // Retreating side should have very low casualties (5% of 1000 = 50)
        let retreating_total = result.retreating_casualties.total();
        assert!(retreating_total <= 55, "Retreating casualties must be low (5%), got {}", retreating_total);
        assert!(retreating_total > 0, "Retreating casualties must be > 0");

        // Victor should have minimal casualties (2% of 1000 = 20)
        let victor_total = result.victor_casualties.total();
        assert!(victor_total <= 25, "Victor casualties must be minimal (2%), got {}", victor_total);
    }

    #[test]
    fn test_process_retreat_captures_equipment() {
        let config = make_config();
        let mut retreating = vec![make_unit("ret-1", UnitType::Infantry, 1000)];
        let victor = vec![make_unit("vic-1", UnitType::Infantry, 1000)];

        let original_rifles = retreating[0].equipment_reserves.iter()
            .find(|r| r.commodity == Commodity::Rifles)
            .map(|r| r.current_quantity)
            .unwrap();

        let result = process_retreat(
            &mut retreating,
            &victor,
            "DefenderCountry",
            "AttackerCountry",
            &config,
        );

        // Equipment should be captured (15% loss rate)
        let captured_rifles = result.captured_equipment.get(&Commodity::Rifles).copied().unwrap_or(0.0);
        assert!(captured_rifles > 0.0, "Rifles must be captured during retreat");

        // Retreating unit should have lost equipment
        let remaining_rifles = retreating[0].equipment_reserves.iter()
            .find(|r| r.commodity == Commodity::Rifles)
            .map(|r| r.current_quantity)
            .unwrap();
        assert!(remaining_rifles < original_rifles, "Retreating unit must lose equipment");

        // Captured amount should equal the loss
        let expected_loss = original_rifles * config.retreat_equipment_loss_rate;
        assert!((captured_rifles - expected_loss).abs() < 0.01,
            "Captured equipment must equal lost equipment");
    }

    #[test]
    fn test_process_retreat_returns_retreat_battle_result() {
        let config = make_config();
        let mut retreating = vec![make_unit("ret-1", UnitType::Infantry, 1000)];
        let victor = vec![make_unit("vic-1", UnitType::Infantry, 1000)];

        let result = process_retreat(
            &mut retreating,
            &victor,
            "DefenderCountry",
            "AttackerCountry",
            &config,
        );

        match &result.battle_result {
            BattleResult::Retreat { retreating_side } => {
                assert_eq!(retreating_side, "DefenderCountry");
            }
            _ => panic!("Battle result must be Retreat variant"),
        }
    }

    #[test]
    fn test_apply_captured_equipment_to_stockpile() {
        let mut stockpile = HashMap::new();
        let mut captured = HashMap::new();
        captured.insert(Commodity::Rifles, 135.0);
        captured.insert(Commodity::Ammunition, 60.0);

        apply_captured_equipment_to_stockpile(&mut stockpile, &captured);

        assert_eq!(stockpile.get(&Commodity::Rifles), Some(&135.0));
        assert_eq!(stockpile.get(&Commodity::Ammunition), Some(&60.0));
    }

    #[test]
    fn test_retreat_captures_equipment_not_cash() {
        // Verify that retreat captures physical commodities, not fiat cash.
        // The captured_equipment map is HashMap<Commodity, f64> — structurally
        // guaranteed to be physical commodities, not cash.
        let config = make_config();
        let mut retreating = vec![make_unit("ret-1", UnitType::Infantry, 1000)];
        let victor = vec![make_unit("vic-1", UnitType::Infantry, 1000)];

        let result = process_retreat(
            &mut retreating,
            &victor,
            "DefenderCountry",
            "AttackerCountry",
            &config,
        );

        // All captured items must be Commodity variants (physical goods)
        for (commodity, qty) in &result.captured_equipment {
            assert!(*qty > 0.0, "Captured quantity must be positive");
            // commodity is a Commodity enum variant — structurally physical
            let _ = commodity; // Verify it's a Commodity
        }
    }

    #[test]
    fn test_commander_profiles() {
        let retreating = CommanderRetraitProfile::retreating("Cautious".to_string());
        assert!(retreating.will_retreat);

        let fighting = CommanderRetraitProfile::fighting("Aggressive".to_string());
        assert!(!fighting.will_retreat);

        let no_cmd = CommanderRetraitProfile::no_commander();
        assert!(!no_cmd.will_retreat);
    }

    #[test]
    fn test_retreat_preserves_manpower() {
        // The retreating army survives to fight another day.
        // Manpower should be preserved (minus the small retreat casualties).
        let config = make_config();
        let mut retreating = vec![make_unit("ret-1", UnitType::Infantry, 1000)];
        let victor = vec![make_unit("vic-1", UnitType::Infantry, 1000)];

        let original_manpower: i64 = retreating.iter().map(|u| u.manpower).sum();

        let result = process_retreat(
            &mut retreating,
            &victor,
            "DefenderCountry",
            "AttackerCountry",
            &config,
        );

        // The retreating side's casualties should be much less than a decisive defeat
        let total_casualties = result.retreating_casualties.total();
        let decisive_defeat_casualties = (original_manpower as f64 * config.max_loser_casualty_ratio) as i64;

        assert!(total_casualties < decisive_defeat_casualties,
            "Retreat casualties ({}) must be less than decisive defeat casualties ({})",
            total_casualties, decisive_defeat_casualties);
    }
}

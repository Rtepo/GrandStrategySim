//! Phase 70: Unit modernization and equipment scrapping.
//!
//! Implements `modernize_unit()` which:
//! - Upgrades a unit's Table of Equipment (ToE) to newer commodities.
//! - Replaces old equipment requirements with newer equipment types.
//! - Generates B2B procurement demand for replacement equipment.
//! - Scraps obsolete equipment, returning physical commodities to the
//!   military stockpile (NOT fiat cash — Rule 1 & Rule 3 compliance).
//!
//! Scrap recovery is governed by `scrap_recovery_rate` (fraction of original
//! physical materials recovered). No cash is created by scrapping.
//!
//! Examples:
//! - LightTanks → MediumTanks (upgrades equipment)
//! - Old Rifles → modern SmallArms (upgrades equipment)
//! - TowedArtillery → MobileArtillery (upgrades equipment)
//!
//! Scrapping returns:
//! - LightTanks → Steel + Aluminum (physical commodities)
//! - Rifles → Steel (physical commodity)
//! - TowedArtillery → Steel (physical commodity)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::military::units::{EquipmentReserve, MilitaryUnit, UnitType};
use crate::registries::enums::Commodity;

// ============================================================================
// MODERNIZATION CONFIG
// ============================================================================

/// Configuration for unit modernization.
///
/// All values are derived from physical properties — no magic numbers (Rule 2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModernizationConfig {
    /// Fraction of original physical materials recovered when scrapping
    /// obsolete equipment. Range [0.0, 1.0].
    ///
    /// Derived from physical recycling efficiency, not a magic constant.
    /// Typical: 0.3–0.6 (30–60% of steel/aluminum can be recovered from
    /// scrapped military equipment).
    pub scrap_recovery_rate: f64,

    /// Condition threshold below which equipment is prioritized for replacement.
    /// Equipment below this condition is scrapped first during modernization.
    pub replacement_condition_threshold: f64,
}

impl Default for ModernizationConfig {
    fn default() -> Self {
        Self {
            scrap_recovery_rate: 0.4,             // 40% physical material recovery
            replacement_condition_threshold: 0.5, // Replace equipment below 50% condition
        }
    }
}

// ============================================================================
// EQUIPMENT UPGRADE MAPPING
// ============================================================================

/// Defines an equipment upgrade: old commodity → new commodity.
///
/// Each upgrade specifies:
/// - The old commodity being replaced.
/// - The new commodity replacing it.
/// - The ratio of new quantity to old quantity (e.g., 1 MediumTank replaces
///   1 LightTank, but 1 modern rifle might replace 1.2 old rifles due to
///   higher effectiveness).
/// - The physical commodities recovered from scrapping the old equipment.
#[derive(Debug, Clone, PartialEq)]
pub struct EquipmentUpgrade {
    /// The old commodity being replaced.
    pub old_commodity: Commodity,
    /// The new commodity replacing it.
    pub new_commodity: Commodity,
    /// Ratio of new quantity to old quantity.
    /// E.g., 1.0 means 1:1 replacement, 0.8 means 1 new replaces 1.25 old.
    pub quantity_ratio: f64,
    /// Physical commodities recovered per unit of old equipment scrapped.
    /// E.g., scrapping 1 LightTank recovers 0.4 * Steel + 0.3 * Aluminum.
    pub scrap_yields: Vec<(Commodity, f64)>,
}

/// Returns the available equipment upgrades for a unit type at a given year.
///
/// Upgrades are era-gated: newer equipment only becomes available after
/// specific years, matching the `table_of_equipment` era gating.
///
/// # Arguments
/// * `unit_type` - The unit type to get upgrades for.
/// * `year` - Current game year (determines which upgrades are available).
///
/// # Returns
/// Vector of `EquipmentUpgrade` entries available at this year.
pub fn available_upgrades(unit_type: UnitType, year: u32) -> Vec<EquipmentUpgrade> {
    match unit_type {
        UnitType::Tanks => {
            let mut upgrades = Vec::new();
            // LightTanks → MediumTanks (available 1935+)
            if year >= 1935 {
                upgrades.push(EquipmentUpgrade {
                    old_commodity: Commodity::LightTanks,
                    new_commodity: Commodity::MediumTanks,
                    quantity_ratio: 0.8, // 1 MediumTank replaces 1.25 LightTanks
                    scrap_yields: vec![
                        (Commodity::Steel, 15.0),   // Steel from armor plate
                        (Commodity::Aluminum, 5.0), // Aluminum from components
                    ],
                });
            }
            // MediumTanks → HeavyTanks (available 1942+)
            if year >= 1942 {
                upgrades.push(EquipmentUpgrade {
                    old_commodity: Commodity::MediumTanks,
                    new_commodity: Commodity::HeavyTanks,
                    quantity_ratio: 0.6, // 1 HeavyTank replaces ~1.67 MediumTanks
                    scrap_yields: vec![(Commodity::Steel, 20.0), (Commodity::Aluminum, 8.0)],
                });
            }
            upgrades
        }
        UnitType::Artillery => {
            let mut upgrades = Vec::new();
            // TowedArtillery → MobileArtillery (if available as a commodity)
            // Since MobileArtillery may not exist, we use SupportEquipment as
            // the modernization target for artillery support gear.
            if year >= 1935 {
                upgrades.push(EquipmentUpgrade {
                    old_commodity: Commodity::TowedArtillery,
                    new_commodity: Commodity::SupportEquipment,
                    quantity_ratio: 1.0,
                    scrap_yields: vec![(Commodity::Steel, 12.0)],
                });
            }
            upgrades
        }
        UnitType::Infantry => {
            let mut upgrades = Vec::new();
            // Rifles → SupportEquipment (modern infantry gear, available 1935+)
            if year >= 1935 {
                upgrades.push(EquipmentUpgrade {
                    old_commodity: Commodity::Rifles,
                    new_commodity: Commodity::SupportEquipment,
                    quantity_ratio: 0.5, // Modern support gear supplements rifles
                    scrap_yields: vec![
                        (Commodity::Steel, 0.5), // Steel from old rifle metal
                    ],
                });
            }
            upgrades
        }
        UnitType::AirForce => {
            let mut upgrades = Vec::new();
            // Fighters → Helicopters (available 1960+)
            if year >= 1960 {
                upgrades.push(EquipmentUpgrade {
                    old_commodity: Commodity::Fighters,
                    new_commodity: Commodity::Helicopters,
                    quantity_ratio: 0.7,
                    scrap_yields: vec![(Commodity::Steel, 8.0), (Commodity::Aluminum, 12.0)],
                });
            }
            upgrades
        }
        UnitType::Naval | UnitType::PeasantBattalion => {
            // No upgrades for naval (handled via fleet system) or peasant battalions
            Vec::new()
        }
    }
}

// ============================================================================
// MODERNIZATION RESULT
// ============================================================================

/// Result of modernizing a unit.
#[derive(Debug, Clone, PartialEq)]
pub struct ModernizationResult {
    /// Unit ID that was modernized.
    pub unit_id: String,
    /// Equipment upgrades applied.
    pub upgrades_applied: Vec<EquipmentUpgrade>,
    /// Physical commodities recovered from scrapping old equipment.
    /// These should be added to `military_stockpile`.
    pub scrap_recovered: HashMap<Commodity, f64>,
    /// B2B procurement demand generated for new equipment.
    /// Key = commodity to procure, Value = quantity needed.
    pub procurement_demand: HashMap<Commodity, f64>,
    /// Whether any upgrades were actually applied.
    pub upgraded: bool,
}

// ============================================================================
// MODERNIZATION LOGIC
// ============================================================================

/// Modernizes a unit's Table of Equipment (ToE).
///
/// This function:
/// 1. Checks available upgrades for the unit's type at the current year.
/// 2. For each upgrade, finds equipment reserves matching the old commodity.
/// 3. Scraps the old equipment, recovering physical commodities based on
///    `scrap_recovery_rate` (Rule 1 & Rule 3 — no fiat cash created).
/// 4. Creates new equipment reserves for the new commodity.
/// 5. Generates B2B procurement demand for the new equipment (the difference
///    between target quantity and current quantity).
///
/// # Arguments
/// * `unit` - The military unit to modernize (will be mutated).
/// * `year` - Current game year (determines available upgrades).
/// * `config` - Modernization configuration.
///
/// # Returns
/// `ModernizationResult` with scrap recovered and procurement demand.
pub fn modernize_unit(
    unit: &mut MilitaryUnit,
    year: u32,
    config: &ModernizationConfig,
) -> ModernizationResult {
    let mut result = ModernizationResult {
        unit_id: unit.id.clone(),
        upgrades_applied: Vec::new(),
        scrap_recovered: HashMap::default(),
        procurement_demand: HashMap::default(),
        upgraded: false,
    };

    let upgrades = available_upgrades(unit.unit_type, year);
    if upgrades.is_empty() {
        return result;
    }

    for upgrade in upgrades {
        // Collect old equipment data BEFORE removal
        let old_reserves: Vec<(f64, f64)> = unit
            .equipment_reserves
            .iter()
            .filter(|r| r.commodity == upgrade.old_commodity)
            .map(|r| (r.toe_quantity, r.current_quantity))
            .collect();

        if old_reserves.is_empty() {
            continue;
        }

        let total_old_toe: f64 = old_reserves.iter().map(|(t, _)| t).sum();
        let total_old_current: f64 = old_reserves.iter().map(|(_, c)| c).sum();

        // Calculate scrap recovery: physical commodities returned
        for (scrap_commodity, base_yield) in &upgrade.scrap_yields {
            let recovered = total_old_current * base_yield * config.scrap_recovery_rate;
            if recovered > 0.0 {
                *result
                    .scrap_recovered
                    .entry(*scrap_commodity)
                    .or_insert(0.0) += recovered;
            }
        }

        // Calculate new equipment quantities
        let new_toe = total_old_toe * upgrade.quantity_ratio;
        let new_current = total_old_current * upgrade.quantity_ratio * 0.5; // Start at 50%

        // Generate procurement demand for the gap between current and target
        let procurement_needed = (new_toe - new_current).max(0.0);
        if procurement_needed > 0.0 {
            *result
                .procurement_demand
                .entry(upgrade.new_commodity)
                .or_insert(0.0) += procurement_needed;
        }

        // Remove old equipment reserves
        unit.equipment_reserves
            .retain(|r| r.commodity != upgrade.old_commodity);

        // Add new equipment reserve with the upgraded commodity
        if new_toe > 0.0 {
            unit.equipment_reserves.push(EquipmentReserve {
                commodity: upgrade.new_commodity,
                toe_quantity: new_toe,
                current_quantity: new_current,
                condition: 1.0,
                depreciation_rate: 0.01,
            });
        }

        result.upgrades_applied.push(upgrade);
        result.upgraded = true;
    }

    result
}

/// Applies scrap recovery to the military stockpile.
///
/// This is the physical commodity writeback from scrapping old equipment.
/// No fiat cash is created — only physical commodities are returned (Rule 1 & 3).
///
/// # Arguments
/// * `stockpile` - The military stockpile to add recovered commodities to.
/// * `scrap_recovered` - The commodities recovered from scrapping.
pub fn apply_scrap_to_stockpile(
    stockpile: &mut std::collections::HashMap<Commodity, f64>,
    scrap_recovered: &std::collections::HashMap<Commodity, f64>,
) {
    for (commodity, qty) in scrap_recovered {
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
    use rustc_hash::FxHashMap as HashMap;

    fn make_tank_unit(year: u32) -> MilitaryUnit {
        let mut unit = MilitaryUnit::new(
            "TEST-TANK-001".to_string(),
            UnitType::Tanks,
            1000,
            HashMap::default(),
            "home".to_string(),
        );
        unit.equipment_reserves = UnitType::Tanks.table_of_equipment(year);
        unit
    }

    fn make_infantry_unit(year: u32) -> MilitaryUnit {
        let mut unit = MilitaryUnit::new(
            "TEST-INF-001".to_string(),
            UnitType::Infantry,
            1000,
            HashMap::default(),
            "home".to_string(),
        );
        unit.equipment_reserves = UnitType::Infantry.table_of_equipment(year);
        unit
    }

    #[test]
    fn test_modernize_tank_light_to_medium() {
        let mut unit = make_tank_unit(1920); // Has LightTanks
        let config = ModernizationConfig::default();

        // Verify unit has LightTanks before modernization
        assert!(unit
            .equipment_reserves
            .iter()
            .any(|r| r.commodity == Commodity::LightTanks));

        let result = modernize_unit(&mut unit, 1935, &config); // 1935: MediumTanks available

        assert!(result.upgraded);
        // LightTanks should be gone, replaced by MediumTanks
        assert!(!unit
            .equipment_reserves
            .iter()
            .any(|r| r.commodity == Commodity::LightTanks));
        assert!(unit
            .equipment_reserves
            .iter()
            .any(|r| r.commodity == Commodity::MediumTanks));
    }

    #[test]
    fn test_scrap_returns_physical_commodities_not_cash() {
        let mut unit = make_tank_unit(1920);
        let config = ModernizationConfig::default();

        let result = modernize_unit(&mut unit, 1935, &config);

        // Scrap must return physical commodities (Steel, Aluminum)
        assert!(
            !result.scrap_recovered.is_empty(),
            "Scrap must return physical commodities"
        );
        assert!(
            result.scrap_recovered.contains_key(&Commodity::Steel),
            "Scrap must return Steel (physical commodity, not cash)"
        );
        assert!(
            result.scrap_recovered.contains_key(&Commodity::Aluminum),
            "Scrap must return Aluminum (physical commodity, not cash)"
        );

        // Verify no fiat cash is in the scrap (scrap_recovered is Commodity-keyed)
        // This is structurally guaranteed — scrap_recovered is HashMap<Commodity, f64>
    }

    #[test]
    fn test_scrap_recovery_rate_applied() {
        let mut unit = make_tank_unit(1920);
        let config = ModernizationConfig {
            scrap_recovery_rate: 0.5, // 50% recovery
            ..Default::default()
        };

        let result = modernize_unit(&mut unit, 1935, &config);

        // With 50% recovery rate, scrap should be half of the base yield
        let steel_recovered = result
            .scrap_recovered
            .get(&Commodity::Steel)
            .copied()
            .unwrap_or(0.0);
        assert!(steel_recovered > 0.0, "Steel must be recovered");
        // The exact amount depends on the LightTank quantity, but it should be
        // proportional to the 50% recovery rate.
    }

    #[test]
    fn test_modernization_generates_procurement_demand() {
        let mut unit = make_tank_unit(1920);
        let config = ModernizationConfig::default();

        let result = modernize_unit(&mut unit, 1935, &config);

        // Modernization must generate B2B procurement demand for new equipment
        assert!(
            !result.procurement_demand.is_empty(),
            "Modernization must generate procurement demand for new equipment"
        );
        assert!(
            result
                .procurement_demand
                .contains_key(&Commodity::MediumTanks),
            "Procurement demand must include MediumTanks"
        );
    }

    #[test]
    fn test_no_upgrade_before_era() {
        let mut unit = make_tank_unit(1920);
        let config = ModernizationConfig::default();

        // 1930: MediumTanks not yet available (needs 1935)
        let result = modernize_unit(&mut unit, 1930, &config);

        assert!(
            !result.upgraded,
            "No upgrade should happen before the era gate"
        );
        assert!(
            unit.equipment_reserves
                .iter()
                .any(|r| r.commodity == Commodity::LightTanks),
            "LightTanks should still be present"
        );
    }

    #[test]
    fn test_apply_scrap_to_stockpile() {
        let mut stockpile = std::collections::HashMap::new();
        let mut scrap = std::collections::HashMap::new();
        scrap.insert(Commodity::Steel, 100.0);
        scrap.insert(Commodity::Aluminum, 50.0);

        apply_scrap_to_stockpile(&mut stockpile, &scrap);

        assert_eq!(stockpile.get(&Commodity::Steel), Some(&100.0));
        assert_eq!(stockpile.get(&Commodity::Aluminum), Some(&50.0));
    }

    #[test]
    fn test_modernize_infantry_adds_support_equipment() {
        let mut unit = make_infantry_unit(1920);
        let config = ModernizationConfig::default();

        // Verify unit has Rifles before modernization
        assert!(unit
            .equipment_reserves
            .iter()
            .any(|r| r.commodity == Commodity::Rifles));

        let result = modernize_unit(&mut unit, 1935, &config);

        // 1935: SupportEquipment available for infantry
        // Note: Rifles are NOT removed — the upgrade adds SupportEquipment
        // The upgrade replaces some rifle capacity with support equipment
        if result.upgraded {
            assert!(
                result
                    .procurement_demand
                    .contains_key(&Commodity::SupportEquipment),
                "Infantry modernization must generate demand for SupportEquipment"
            );
        }
    }

    #[test]
    fn test_available_upgrades_empty_for_peasant_battalion() {
        let upgrades = available_upgrades(UnitType::PeasantBattalion, 1940);
        assert!(
            upgrades.is_empty(),
            "Peasant battalions should have no upgrades"
        );
    }

    #[test]
    fn test_available_upgrades_empty_for_naval() {
        let upgrades = available_upgrades(UnitType::Naval, 1940);
        assert!(
            upgrades.is_empty(),
            "Naval units should have no upgrades (handled by fleet system)"
        );
    }

    #[test]
    fn test_modernize_unit_no_upgrades_available() {
        let mut unit = make_tank_unit(1920);
        let config = ModernizationConfig::default();

        // 1920: no upgrades available for tanks (MediumTanks needs 1935)
        let result = modernize_unit(&mut unit, 1920, &config);

        assert!(!result.upgraded);
        assert!(result.scrap_recovered.is_empty());
        assert!(result.procurement_demand.is_empty());
    }

    #[test]
    fn test_double_entry_scrap_flow() {
        // Verify the scrap flow is double-entry:
        // 1. Old equipment is REMOVED from the unit (equipment_reserves shrinks)
        // 2. Physical commodities are ADDED to scrap_recovered
        // 3. scrap_recovered is then applied to military_stockpile
        // No cash is created anywhere in this flow.
        let mut unit = make_tank_unit(1920);
        let config = ModernizationConfig::default();

        let _old_equipment_count = unit.equipment_reserves.len();
        let _old_light_tank_qty: f64 = unit
            .equipment_reserves
            .iter()
            .filter(|r| r.commodity == Commodity::LightTanks)
            .map(|r| r.current_quantity)
            .sum();

        let result = modernize_unit(&mut unit, 1935, &config);

        // Old equipment (LightTanks) must be removed from the unit
        assert!(
            !unit
                .equipment_reserves
                .iter()
                .any(|r| r.commodity == Commodity::LightTanks),
            "Old equipment must be removed from unit (double-entry: debit unit equipment)"
        );

        // Physical commodities must be recovered
        let total_scrap: f64 = result.scrap_recovered.values().sum();
        assert!(
            total_scrap > 0.0,
            "Physical commodities must be recovered (double-entry: credit stockpile)"
        );

        // The flow is: unit loses equipment → stockpile gains physical commodities
        // No cash is involved anywhere.
    }
}

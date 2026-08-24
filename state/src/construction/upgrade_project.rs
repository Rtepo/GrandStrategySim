//! Phase 81 Wave 2: Upgrade project for consumption-method transitions.
//!
//! Models the partial-delivery accumulation of CAPEX commodities required to
//! upgrade a building's consumption method (lighting, heating, ventilation,
//! power generation). The active method string ONLY flips when `is_complete()`
//! returns true — partial B2C/B2B fulfillments carry over across turns.
//!
//! Modeled on the established `ConstructionProject` accumulation pattern in
//! `state/src/construction/projects.rs`, but simplified for consumption-method
//! upgrades (no contractor linkage, no OHS, no network targets).

use crate::registries::enums::Commodity;
use crate::registries::production_methods::MethodSlot;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Pending consumption-method upgrade for a building.
///
/// Accumulates partial CAPEX deliveries over multiple turns. The active method
/// string (e.g., `active_lighting`) ONLY flips to `target_method` when
/// `is_complete()` returns true. Only one upgrade per building at a time.
///
/// # Lifecycle
/// 1. **Initiation**: Created when a building owner decides to upgrade. The
///    `required_materials` are the target method's CAPEX BOM, scaled by the
///    building's physical capacity (Flaw 1 correction).
/// 2. **Accumulation**: Each turn, after B2C/B2B clearing, fulfilled CAPEX
///    quantities are accumulated via `accumulate_delivery()`. The building
///    continues using its current (old) method during accumulation.
/// 3. **Completion**: When `is_complete()` returns true, the caller flips the
///    active method string and removes the `UpgradeProject`.
/// 4. **Cancellation**: If cancelled (building demolished, owner bankrupt),
///    partially-delivered CAPEX is lost. No refund (Rule 8: rational actors
///    bear the cost of their decisions).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UpgradeProject {
    /// Target MethodSlot (Lighting, Heating, Ventilation, PowerGeneration).
    pub target_slot: MethodSlot,

    /// Target method name (e.g., "LED Lighting", "Heat Pump").
    pub target_method: String,

    /// Required CAPEX commodities (Commodity -> total quantity needed).
    /// Scaled by building capacity at initiation (Flaw 1 correction).
    #[serde(default)]
    pub required_materials: BTreeMap<Commodity, f64>,

    /// CAPEX commodities delivered so far (accumulates across turns).
    #[serde(default)]
    pub delivered_materials: BTreeMap<Commodity, f64>,

    /// Progress 0.0-1.0, computed as min(delivered/required) across all
    /// materials. Updated by `accumulate_delivery()` and `compute_progress()`.
    #[serde(default)]
    pub progress: f64,

    /// Turn the upgrade was initiated.
    #[serde(default)]
    pub start_turn: u32,
}

impl UpgradeProject {
    /// Compute progress as the minimum fulfillment ratio across all required
    /// materials. Identical algorithm to `ConstructionProject::compute_progress()`.
    ///
    /// # Rules
    /// * If any required material has zero delivery, progress is 0 for that material.
    /// * Overall progress = min(delivered[mat] / required[mat]) across all materials.
    /// * If `required_materials` is empty, progress is 1.0 (no materials needed).
    pub fn compute_progress(&self) -> f64 {
        if self.required_materials.is_empty() {
            return 1.0;
        }
        let mut min_ratio = f64::MAX;
        for (&commodity, &required) in &self.required_materials {
            if required <= 0.0 {
                continue;
            }
            let delivered = self.delivered_materials.get(&commodity).copied().unwrap_or(0.0);
            let ratio = (delivered / required).min(1.0);
            if ratio < min_ratio {
                min_ratio = ratio;
            }
        }
        if min_ratio == f64::MAX {
            1.0
        } else {
            min_ratio
        }
    }

    /// Check if all required CAPEX has been delivered (100% of every BOM
    /// component). The active method string should only flip when this
    /// returns true.
    pub fn is_complete(&self) -> bool {
        self.compute_progress() >= 1.0
    }

    /// Accumulate a partial CAPEX delivery from B2C/B2B clearing.
    ///
    /// # Arguments
    /// * `commodity` - The CAPEX commodity being delivered.
    /// * `quantity` - The quantity delivered this turn (may be partial).
    ///
    /// # Returns
    /// The quantity actually accumulated (capped at the remaining requirement).
    /// Any excess over the requirement is NOT accumulated — the caller must
    /// handle overflow (e.g., leave it in the building's inventory or B2C
    /// fulfillment pool).
    ///
    /// # Rules
    /// * Accumulation is capped at `required - already_delivered` for each
    ///   commodity, so over-deliveries do not carry forward.
    /// * Updates `progress` after accumulation.
    pub fn accumulate_delivery(&mut self, commodity: Commodity, quantity: f64) -> f64 {
        let required = self.required_materials.get(&commodity).copied().unwrap_or(0.0);
        if required <= 0.0 {
            return 0.0;
        }
        let already = self.delivered_materials.get(&commodity).copied().unwrap_or(0.0);
        let remaining = (required - already).max(0.0);
        if remaining <= 0.0 {
            return 0.0;
        }
        let to_accumulate = quantity.min(remaining);
        if to_accumulate <= 0.0 {
            return 0.0;
        }
        *self.delivered_materials.entry(commodity).or_insert(0.0) += to_accumulate;
        self.progress = self.compute_progress();
        to_accumulate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_requirements_is_complete() {
        let project = UpgradeProject {
            target_slot: MethodSlot::Lighting,
            target_method: "LED Lighting".to_string(),
            required_materials: BTreeMap::new(),
            delivered_materials: BTreeMap::new(),
            progress: 0.0,
            start_turn: 0,
        };
        assert!(project.is_complete());
    }

    #[test]
    fn test_partial_delivery_not_complete() {
        let mut required = BTreeMap::new();
        required.insert(Commodity::Glass, 10.0);
        required.insert(Commodity::ElectronicComponents, 5.0);
        let project = UpgradeProject {
            target_slot: MethodSlot::Lighting,
            target_method: "LED Lighting".to_string(),
            required_materials: required,
            delivered_materials: BTreeMap::new(),
            progress: 0.0,
            start_turn: 0,
        };
        assert!(!project.is_complete());
        assert_eq!(project.compute_progress(), 0.0);
    }

    #[test]
    fn test_accumulate_partial_delivery() {
        let mut required = BTreeMap::new();
        required.insert(Commodity::Glass, 10.0);
        let mut project = UpgradeProject {
            target_slot: MethodSlot::Lighting,
            target_method: "Incandescent Bulbs".to_string(),
            required_materials: required,
            delivered_materials: BTreeMap::new(),
            progress: 0.0,
            start_turn: 0,
        };
        // Deliver 3.0 out of 10.0
        let accumulated = project.accumulate_delivery(Commodity::Glass, 3.0);
        assert!((accumulated - 3.0).abs() < 1e-9);
        assert!((project.progress - 0.3).abs() < 1e-9);
        assert!(!project.is_complete());
    }

    #[test]
    fn test_accumulate_multiple_turns_to_completion() {
        let mut required = BTreeMap::new();
        required.insert(Commodity::Glass, 10.0);
        required.insert(Commodity::ElectronicComponents, 5.0);
        let mut project = UpgradeProject {
            target_slot: MethodSlot::Lighting,
            target_method: "LED Lighting".to_string(),
            required_materials: required,
            delivered_materials: BTreeMap::new(),
            progress: 0.0,
            start_turn: 0,
        };
        // Turn 1: deliver 3.0 Glass
        project.accumulate_delivery(Commodity::Glass, 3.0);
        assert!(!project.is_complete());
        // Turn 2: deliver 7.0 Glass (complete)
        project.accumulate_delivery(Commodity::Glass, 7.0);
        assert!(!project.is_complete()); // Still need ElectronicComponents
        // Turn 3: deliver 5.0 ElectronicComponents (complete)
        project.accumulate_delivery(Commodity::ElectronicComponents, 5.0);
        assert!(project.is_complete());
        assert!((project.progress - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_over_delivery_capped() {
        let mut required = BTreeMap::new();
        required.insert(Commodity::Glass, 10.0);
        let mut project = UpgradeProject {
            target_slot: MethodSlot::Lighting,
            target_method: "Incandescent Bulbs".to_string(),
            required_materials: required,
            delivered_materials: BTreeMap::new(),
            progress: 0.0,
            start_turn: 0,
        };
        // Deliver 15.0 but only 10.0 is needed
        let accumulated = project.accumulate_delivery(Commodity::Glass, 15.0);
        assert!((accumulated - 10.0).abs() < 1e-9);
        assert!(project.is_complete());
    }

    #[test]
    fn test_min_ratio_across_materials() {
        let mut required = BTreeMap::new();
        required.insert(Commodity::Glass, 10.0);
        required.insert(Commodity::ElectronicComponents, 5.0);
        let mut project = UpgradeProject {
            target_slot: MethodSlot::Lighting,
            target_method: "LED Lighting".to_string(),
            required_materials: required,
            delivered_materials: BTreeMap::new(),
            progress: 0.0,
            start_turn: 0,
        };
        // Fully deliver Glass but not ElectronicComponents
        project.accumulate_delivery(Commodity::Glass, 10.0);
        project.accumulate_delivery(Commodity::ElectronicComponents, 2.5);
        // Progress = min(1.0, 0.5) = 0.5
        assert!((project.progress - 0.5).abs() < 1e-9);
        assert!(!project.is_complete());
    }
}

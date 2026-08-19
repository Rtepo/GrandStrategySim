//! Phase 19: Configuration for generative investment goods, blueprints, fixed
//! asset cohorts, maintenance-as-a-service, technological obsolescence, and
//! quality-driven markets.
//!
//! All tuning knobs for Phase 19 live here so there are no magic numbers in the
//! simulation. The config is attached to `Country` with `#[serde(default)]` for
//! backward save-file compatibility.

use crate::registries::enums::WealthBracket;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Configuration for the entire Phase 19 generative-goods subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerativeGoodsConfig {
    // ── Blueprint design (19A) ──────────────────────────────────────────────
    /// R&D budget a company must spend to design one `ProductBlueprint`.
    #[serde(default = "default_blueprint_design_cost")]
    pub blueprint_design_cost: f64,
    /// Maximum number of blueprints a single company may hold.
    #[serde(default = "default_max_blueprints_per_company")]
    pub max_blueprints_per_company: usize,
    /// Royalty VWAP ratio applied to a blueprint's output when no explicit ratio
    /// is set on the design (fallback).
    #[serde(default = "default_blueprint_royalty_ratio")]
    pub default_blueprint_royalty_ratio: f64,
    /// Patent-style expiry in turns for a blueprint license.
    #[serde(default = "default_blueprint_patent_turns")]
    pub blueprint_patent_turns: u32,
    /// FX rate applied when crediting a foreign licensor (1.0 = par).
    #[serde(default = "default_cross_border_royalty_fx")]
    pub cross_border_royalty_fx: f64,

    // ── Fixed asset cohorts (19B) ───────────────────────────────────────────
    /// Hard cap on the number of `FixedAssetCohort`s per building before
    /// compaction merges them. Guarantees RAM predictability.
    #[serde(default = "default_max_fixed_cohorts_per_building")]
    pub max_fixed_cohorts_per_building: usize,
    /// Hard cap on `InventoryCohort`s per commodity before compaction.
    #[serde(default = "default_max_inventory_cohorts_per_commodity")]
    pub max_inventory_cohorts_per_commodity: usize,
    /// Capacity contributed by one pristine machine of unit quality
    /// (machinery_factor = 1.0 + Σ count × quality × condition × obsolescence × this).
    #[serde(default = "default_machine_unit_capacity")]
    pub machine_unit_capacity: f64,
    /// Extra per-turn stress multiplier on cohort degradation when the host
    /// building's shell condition is poor (0.0 = no extra stress).
    #[serde(default = "default_degradation_stress_weight")]
    pub degradation_stress_weight: f64,
    /// Phase 19C: Quality/durability premium multiplier on desired willingness-
    /// to-pay for fixed-asset purchases. Companies *want* premium assets at
    /// `ref_price × this`, but are clamped by available cash.
    #[serde(default = "default_asset_quality_wtp_multiplier")]
    pub asset_quality_wtp_multiplier: f64,
    /// Phase 19C: Starvation ratio — if a company's affordable WTP falls below
    /// `ref_price × this`, it skips the asset purchase entirely (goes without
    /// new machinery this turn rather than buying extremely low-quality junk).
    #[serde(default = "default_asset_purchase_starvation_ratio")]
    pub asset_purchase_starvation_ratio: f64,

    // ── Technological obsolescence (19B) ───────────────────────────────────
    /// Aggressiveness of the TechnologicalGap penalty. Higher = faster
    /// obsolescence. With `k ≈ 2.0` a tech 50 years behind the frontier
    /// contributes ~0 to capacity.
    #[serde(default = "default_obsolescence_aggressiveness")]
    pub obsolescence_aggressiveness: f64,

    // ── Maintenance as a B2B service (19B) ─────────────────────────────────
    /// Units of `MaintenanceServices` required to restore one unit of condition
    /// across one machine in a cohort.
    #[serde(default = "default_maintenance_per_condition_point")]
    pub maintenance_per_condition_point: f64,
    /// Maximum condition restorable per cohort per turn (caps the repair rate).
    #[serde(default = "default_max_restore_per_turn")]
    pub max_restore_per_turn: f64,

    // ── B2B affordability (19C) ────────────────────────────────────────────
    /// When true, a company may use credit-line headroom (via `Borrower`) in
    /// addition to liquid cash when bidding on fixed assets.
    #[serde(default)]
    pub allow_asset_purchase_on_credit: bool,
    /// Exponent applied to durability when computing B2B willingness-to-pay.
    #[serde(default = "default_b2b_durability_exponent")]
    pub b2b_durability_exponent: f64,

    // ── B2C quality segmentation (19C) ─────────────────────────────────────
    /// Per-wealth-tier exponent `α` in `utility = quality^α / price + inertia`.
    /// Higher `α` = more quality-loving. Loaded from a map so it is data-driven.
    #[serde(default = "default_quality_weights")]
    pub quality_weights: HashMap<WealthBracket, f64>,
    /// Default quality assumed for an offer that has no blueprint (legacy goods).
    #[serde(default = "default_legacy_quality")]
    pub legacy_quality: f64,

    /// Catch-all for forward-compatible extra fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_blueprint_design_cost() -> f64 {
    50000.0
}
fn default_max_blueprints_per_company() -> usize {
    8
}
fn default_blueprint_royalty_ratio() -> f64 {
    0.04
}
fn default_blueprint_patent_turns() -> u32 {
    240
}
fn default_cross_border_royalty_fx() -> f64 {
    1.0
}
fn default_max_fixed_cohorts_per_building() -> usize {
    12
}
fn default_max_inventory_cohorts_per_commodity() -> usize {
    8
}
fn default_machine_unit_capacity() -> f64 {
    0.05
}
fn default_asset_quality_wtp_multiplier() -> f64 {
    1.5 // Companies want 50% premium for quality/durability, clamped by cash.
}
fn default_asset_purchase_starvation_ratio() -> f64 {
    0.3 // Below 30% of ref price, skip the purchase (junk not worth buying).
}
fn default_degradation_stress_weight() -> f64 {
    1.0
}
fn default_obsolescence_aggressiveness() -> f64 {
    2.0
}
fn default_maintenance_per_condition_point() -> f64 {
    1.0
}
fn default_max_restore_per_turn() -> f64 {
    0.25
}
fn default_b2b_durability_exponent() -> f64 {
    0.5
}
fn default_legacy_quality() -> f64 {
    1.0
}
fn default_quality_weights() -> HashMap<WealthBracket, f64> {
    let mut m = HashMap::new();
    m.insert(WealthBracket::VeryHigh, 2.0);
    m.insert(WealthBracket::High, 1.5);
    m.insert(WealthBracket::Medium, 1.0);
    m.insert(WealthBracket::Low, 0.5);
    m
}

impl Default for GenerativeGoodsConfig {
    fn default() -> Self {
        Self {
            blueprint_design_cost: default_blueprint_design_cost(),
            max_blueprints_per_company: default_max_blueprints_per_company(),
            default_blueprint_royalty_ratio: default_blueprint_royalty_ratio(),
            blueprint_patent_turns: default_blueprint_patent_turns(),
            cross_border_royalty_fx: default_cross_border_royalty_fx(),
            max_fixed_cohorts_per_building: default_max_fixed_cohorts_per_building(),
            max_inventory_cohorts_per_commodity: default_max_inventory_cohorts_per_commodity(),
            machine_unit_capacity: default_machine_unit_capacity(),
            degradation_stress_weight: default_degradation_stress_weight(),
            asset_quality_wtp_multiplier: default_asset_quality_wtp_multiplier(),
            asset_purchase_starvation_ratio: default_asset_purchase_starvation_ratio(),
            obsolescence_aggressiveness: default_obsolescence_aggressiveness(),
            maintenance_per_condition_point: default_maintenance_per_condition_point(),
            max_restore_per_turn: default_max_restore_per_turn(),
            allow_asset_purchase_on_credit: false,
            b2b_durability_exponent: default_b2b_durability_exponent(),
            quality_weights: default_quality_weights(),
            legacy_quality: default_legacy_quality(),
            extra: Map::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = GenerativeGoodsConfig::default();
        assert!(c.blueprint_design_cost > 0.0);
        assert!(c.max_fixed_cohorts_per_building >= 4);
        assert!(c.obsolescence_aggressiveness > 0.0);
        assert!(c.maintenance_per_condition_point > 0.0);
        assert_eq!(c.quality_weights.len(), 4);
    }

    #[test]
    fn quality_weights_default_is_quality_stratified() {
        let c = GenerativeGoodsConfig::default();
        let vh = c.quality_weights[&WealthBracket::VeryHigh];
        let lo = c.quality_weights[&WealthBracket::Low];
        // Rich tiers must be more quality-loving than poor tiers.
        assert!(vh > lo);
    }

    #[test]
    fn serde_roundtrip() {
        let c = GenerativeGoodsConfig::default();
        let s = serde_json::to_string(&c).unwrap();
        let back: GenerativeGoodsConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }
}

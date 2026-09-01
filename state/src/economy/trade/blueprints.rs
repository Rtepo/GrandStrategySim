//! Phase 19A: Generative product blueprints and intellectual property.
//!
//! A `ProductBlueprint` is a product design created by a company from a known
//! Commercial technology and a chosen bill of materials (with generative
//! substitutes). It carries the computed `quality` and `durability` that every
//! unit produced under this blueprint inherits.
//!
//! # Lifecycle
//! 1. **Design** (`design_blueprint`): a company with a patented-or-licensed
//!    Commercial `base_tech` spends R&D budget to enumerate material choices per
//!    `BlueprintSpec` role and picks the bundle maximizing a margin-weighted
//!    target. The result is stored on `Company.blueprints`.
//! 2. **Production** (`ActiveProductionMethod.active_blueprint`): a building
//!    producing the blueprint's output commodity tags each output batch with the
//!    blueprint's quality (→ `InventoryCohort` in Phase 19C).
//! 3. **Licensing** (`LicensedBlueprint` on `Company.licensed_blueprints`):
//!    other companies (domestic or foreign) license the blueprint and pay
//!    royalties via `TransferSettler` (see `economy/royalties.rs`).
//!
//! # Generative substitutes
//! Substitution is encoded entirely by `BlueprintSpec.roles[].substitutes`.
//! The design search (`design_blueprint`) is what makes it generative — the
//! designer trades production cost vs quality/durability. No hardcoded recipes.

use crate::economy::generative_goods_config::GenerativeGoodsConfig;
use crate::registries::blueprint_specs::{blueprint_specs, BlueprintSpec, MaterialChoice};
use crate::registries::enums::Commodity;
use crate::registries::production_methods::MethodSlot;
use crate::registries::tech_tree::TechId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// A product design created by a company from known technologies + chosen materials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProductBlueprint {
    /// Deterministic id: hash of (owner + output + base_tech + chosen inputs + granted_turn).
    pub id: String,
    /// Designer / licensor company id.
    pub owner_company_id: String,
    /// Output commodity this blueprint produces (e.g. IndustrialMachinery, Cars).
    pub output_commodity: Commodity,
    /// Underlying Commercial technology (must be patented or licensed by the designer).
    pub base_tech: TechId,
    /// Year of `base_tech` (cached from `TechNode.year` for obsolescence penalties).
    pub base_tech_year: u32,
    /// Chosen bill of materials (incl. substitutes), as per-1k-worker quantities.
    pub inputs: BTreeMap<Commodity, f64>,
    /// Production-method slot the blueprint applies to (Production or Automation).
    pub required_slot: MethodSlot,
    /// Computed quality (0.0..~2.0). Higher = better need satisfaction for B2C
    /// and better capacity contribution for fixed assets.
    pub quality: f64,
    /// Expected lifespan in turns before condition reaches 0 (maintenance cadence).
    pub durability: f64,
    /// Royalty fee = `qty × royalty_vwap_ratio × last_turn_vwap(output)`.
    pub royalty_vwap_ratio: f64,
    /// Turn the blueprint was designed.
    pub granted_turn: u32,
    /// Turn the blueprint patent expires (`granted_turn + patent_turns`).
    pub expires_turn: u32,
}

impl ProductBlueprint {
    /// Returns `true` if the blueprint patent has expired by `current_turn`.
    pub fn is_expired(&self, current_turn: u32) -> bool {
        current_turn >= self.expires_turn
    }

    /// Returns `true` if `company_id` owns this blueprint.
    pub fn is_owned_by(&self, company_id: &str) -> bool {
        self.owner_company_id == company_id
    }
}

/// A blueprint a company has licensed from another (domestic or foreign).
///
/// # Rules
/// * `licensor_company_id == "STATE"` for state-designed blueprints.
/// * Royalties are paid each turn the licensee produces output under this blueprint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LicensedBlueprint {
    /// The licensed blueprint's id.
    pub blueprint_id: String,
    /// Owner of the blueprint (the licensor). `"STATE"` for state designs.
    pub licensor_company_id: String,
    /// Country name of the licensor (for cross-border settlement routing).
    pub licensor_country: String,
    /// Turn the license was granted.
    pub licensed_turn: u32,
}

/// The material choices made during a single blueprint design.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignChoices {
    /// One `MaterialChoice` per role in the `BlueprintSpec`, in role order.
    pub choices: Vec<MaterialChoice>,
}

/// Score a candidate design by margin-weighted `quality × durability / cost`.
///
/// This is the deterministic objective the design search maximizes. A cheaper
/// bill of materials (`cost`) raises the score; lower quality or durability
/// lowers it. Substitutes that are cheap *enough* can beat ideal materials.
///
/// # Rules
/// * `cost` must be > 0.0 (avoids div-by-zero).
/// * Returns 0.0 if quality or durability is ≤ 0 (a worthless design).
pub fn design_score(quality: f64, durability: f64, cost: f64) -> f64 {
    if quality <= 0.0 || durability <= 0.0 || cost <= 0.0 {
        return 0.0;
    }
    (quality * durability) / cost
}

/// Design a `ProductBlueprint` by searching over material choices per role.
///
/// # Arguments
/// * `owner_company_id` — the designing company.
/// * `output_commodity` — the commodity to design (must be blueprint-eligible).
/// * `base_tech` / `base_tech_year` — the underlying Commercial technology.
/// * `required_slot` — the production-method slot this blueprint applies to.
/// * `material_costs` — current B2B market prices per material commodity (used
///   to compute the bill-of-materials cost for each candidate bundle).
/// * `config` — Phase 19 config (provides default royalty ratio + patent turns).
/// * `granted_turn` — the current turn.
///
/// # Returns
/// * `Some(ProductBlueprint)` if the commodity has a `BlueprintSpec` and the
///   search produced a positive score.
/// * `None` if the commodity is not blueprint-eligible (no spec exists).
///
/// # Rules
/// * The search enumerates the cartesian product of per-role choices
///   (ideal + each substitute). Role count is small (≤4) and substitutes per
///   role are few (≤3), so worst case ~`4×4 = 256` candidates — cheap.
/// * Ties are broken by preferring ideal materials (lower substitute index sum).
/// * The chosen bill of materials is stored in `ProductBlueprint.inputs` as
///   per-1k-worker quantities (the role's `share` × a unit scale of 1.0).
/// * Blueprint id is a deterministic concatenation of its defining fields.
pub fn design_blueprint(
    owner_company_id: &str,
    output_commodity: Commodity,
    base_tech: TechId,
    base_tech_year: u32,
    required_slot: MethodSlot,
    material_costs: &HashMap<Commodity, f64>,
    config: &GenerativeGoodsConfig,
    granted_turn: u32,
) -> Option<ProductBlueprint> {
    let specs = blueprint_specs();
    let spec = specs.get(&output_commodity)?;
    let choices = search_best_choices(spec, material_costs);
    let (quality, durability) = spec.compute_stats(&choices);
    if quality <= 0.0 || durability <= 0.0 {
        return None;
    }
    let bom = spec.bill_of_materials(&choices);
    let mut inputs: BTreeMap<Commodity, f64> = BTreeMap::new();
    for (commodity, share) in bom {
        // Per-1k-worker quantity = share (role fraction) × unit scale 1.0.
        // Production methods later scale this by employment.
        inputs.insert(commodity, share);
    }
    let id = blueprint_id(
        owner_company_id,
        output_commodity,
        &base_tech,
        &inputs,
        granted_turn,
    );
    let expires_turn = granted_turn + config.blueprint_patent_turns;
    Some(ProductBlueprint {
        id,
        owner_company_id: owner_company_id.to_string(),
        output_commodity,
        base_tech,
        base_tech_year,
        inputs,
        required_slot,
        quality,
        durability,
        royalty_vwap_ratio: config.default_blueprint_royalty_ratio,
        granted_turn,
        expires_turn,
    })
}

/// Enumerate all candidate choice bundles and return the highest-scoring one.
///
/// Ties on score are broken by preferring fewer substitutions (ideal materials).
fn search_best_choices(
    spec: &BlueprintSpec,
    material_costs: &HashMap<Commodity, f64>,
) -> Vec<MaterialChoice> {
    let mut best: Option<(Vec<MaterialChoice>, f64, usize)> = None;
    enumerate_choices(spec, &mut Vec::new(), 0, material_costs, &mut best);
    best.map(|(c, _, _)| c)
        .unwrap_or_else(|| vec![MaterialChoice::Ideal; spec.roles.len()])
}

/// Recursively enumerate the cartesian product of per-role choices.
fn enumerate_choices(
    spec: &BlueprintSpec,
    current: &mut Vec<MaterialChoice>,
    role_idx: usize,
    material_costs: &HashMap<Commodity, f64>,
    best: &mut Option<(Vec<MaterialChoice>, f64, usize)>,
) {
    if role_idx == spec.roles.len() {
        let (quality, durability) = spec.compute_stats(current);
        let bom = spec.bill_of_materials(current);
        let cost: f64 = bom
            .iter()
            .map(|(c, qty)| material_costs.get(c).copied().unwrap_or(1.0) * qty)
            .sum();
        let score = design_score(quality, durability, cost);
        // Tie-break: fewer total substitute indices (prefer ideal materials).
        let subst_count: usize = current
            .iter()
            .map(|c| match c {
                MaterialChoice::Substitute(_) => 1,
                _ => 0,
            })
            .sum();
        let is_better = match best {
            None => true,
            Some((_, best_score, best_subst)) => {
                score > *best_score + 1e-12
                    || (score > *best_score - 1e-12 && subst_count < *best_subst)
            }
        };
        if is_better {
            *best = Some((current.clone(), score, subst_count));
        }
        return;
    }
    let role = &spec.roles[role_idx];
    // Try ideal first (so ties resolve to ideal via the subst_count tiebreak).
    current.push(MaterialChoice::Ideal);
    enumerate_choices(spec, current, role_idx + 1, material_costs, best);
    current.pop();
    for i in 0..role.substitutes.len() {
        current.push(MaterialChoice::Substitute(i));
        enumerate_choices(spec, current, role_idx + 1, material_costs, best);
        current.pop();
    }
}

/// Deterministic blueprint id from its defining fields.
fn blueprint_id(
    owner: &str,
    output: Commodity,
    base_tech: &str,
    inputs: &BTreeMap<Commodity, f64>,
    granted_turn: u32,
) -> String {
    let inputs_str: String = inputs
        .iter()
        .map(|(c, q)| format!("{:?}:{:.4}", c, q))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "bp_{}_{:?}_{}_{}_{}",
        owner, output, base_tech, granted_turn, inputs_str
    )
}

/// Compute the royalty fee for a single licensee-producer this turn.
///
/// # Arguments
/// * `blueprint` — the licensed blueprint.
/// * `actual_output_qty` — units of `output_commodity` produced this turn.
/// * `last_turn_vwap` — last turn's VWAP for `output_commodity`.
///
/// # Returns
/// * `fee = actual_output_qty × blueprint.royalty_vwap_ratio × last_turn_vwap`.
pub fn compute_blueprint_royalty_fee(
    blueprint: &ProductBlueprint,
    actual_output_qty: f64,
    last_turn_vwap: f64,
) -> f64 {
    (actual_output_qty * blueprint.royalty_vwap_ratio * last_turn_vwap).max(0.0)
}

/// A pending cross-border royalty credit (emitted by the licensee country,
/// consumed by the sequential post-parallel crediting pass).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrossBorderRoyaltyQueueEntry {
    /// The foreign licensor's company id.
    pub licensor_company_id: String,
    /// The foreign licensor's country name.
    pub licensor_country: String,
    /// Amount (in licensor-country currency) to credit.
    pub amount: f64,
    /// The blueprint that earned the royalty (for audit/logging).
    pub blueprint_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::generative_goods_config::GenerativeGoodsConfig;
    use std::collections::HashMap;

    fn free_material_costs() -> HashMap<Commodity, f64> {
        // Equal costs → ideal materials win (no cost advantage to substitute).
        let mut m = HashMap::new();
        m.insert(Commodity::Steel, 1.0);
        m.insert(Commodity::Iron, 1.0);
        m.insert(Commodity::Aluminum, 1.0);
        m.insert(Commodity::ElectronicComponents, 1.0);
        m.insert(Commodity::MechanicalComponents, 1.0);
        m
    }

    #[test]
    fn design_industrial_machinery_prefers_ideal_when_costs_equal() {
        let cfg = GenerativeGoodsConfig::default();
        let bp = design_blueprint(
            "ACME",
            Commodity::IndustrialMachinery,
            "tech_cnc".to_string(),
            1990,
            MethodSlot::Production,
            &free_material_costs(),
            &cfg,
            100,
        )
        .expect("industrial machinery is blueprint-eligible");
        assert_eq!(bp.output_commodity, Commodity::IndustrialMachinery);
        // With equal costs, ideal wins → quality == base_quality, durability == base.
        assert!((bp.quality - 1.2).abs() < 1e-9);
        assert!((bp.durability - 240.0).abs() < 1e-9);
        assert!(bp.id.starts_with("bp_ACME_"));
        assert_eq!(bp.expires_turn, 100 + cfg.blueprint_patent_turns);
    }

    #[test]
    fn design_picks_substitute_when_iron_is_much_cheaper() {
        let cfg = GenerativeGoodsConfig::default();
        // Iron is 10× cheaper than Steel and Aluminum.
        let mut costs = free_material_costs();
        costs.insert(Commodity::Iron, 0.1);
        costs.insert(Commodity::Steel, 1.0);
        costs.insert(Commodity::Aluminum, 1.0);
        let bp = design_blueprint(
            "ACME",
            Commodity::IndustrialMachinery,
            "tech_cnc".to_string(),
            1990,
            MethodSlot::Production,
            &costs,
            &cfg,
            100,
        )
        .expect("design succeeds");
        // The cheaper Iron substitute should appear in the bill of materials.
        assert!(bp.inputs.contains_key(&Commodity::Iron));
        // And quality/durability should be lower than ideal.
        assert!(bp.quality < 1.2, "Iron substitute must lower quality");
        assert!(
            bp.durability < 240.0,
            "Iron substitute must lower durability"
        );
    }

    #[test]
    fn design_returns_none_for_non_blueprint_commodity() {
        let cfg = GenerativeGoodsConfig::default();
        let bp = design_blueprint(
            "ACME",
            Commodity::Steel, // raw material — no BlueprintSpec
            "tech_x".to_string(),
            1900,
            MethodSlot::Production,
            &free_material_costs(),
            &cfg,
            100,
        );
        assert!(bp.is_none());
    }

    #[test]
    fn royalty_fee_is_qty_times_ratio_times_vwap() {
        let bp = ProductBlueprint {
            id: "bp_test".to_string(),
            owner_company_id: "ACME".to_string(),
            output_commodity: Commodity::IndustrialMachinery,
            base_tech: "tech".to_string(),
            base_tech_year: 1990,
            inputs: BTreeMap::new(),
            required_slot: MethodSlot::Production,
            quality: 1.2,
            durability: 240.0,
            royalty_vwap_ratio: 0.05,
            granted_turn: 100,
            expires_turn: 340,
        };
        // 1000 units × 0.05 × 200 vwap = 10000.
        let fee = compute_blueprint_royalty_fee(&bp, 1000.0, 200.0);
        assert!((fee - 10000.0).abs() < 1e-6);
    }

    #[test]
    fn expiry_check_works() {
        let bp = ProductBlueprint {
            id: "bp".to_string(),
            owner_company_id: "o".to_string(),
            output_commodity: Commodity::Cars,
            base_tech: "t".to_string(),
            base_tech_year: 1990,
            inputs: BTreeMap::new(),
            required_slot: MethodSlot::Production,
            quality: 1.0,
            durability: 100.0,
            royalty_vwap_ratio: 0.05,
            granted_turn: 100,
            expires_turn: 200,
        };
        assert!(!bp.is_expired(199));
        assert!(bp.is_expired(200));
        assert!(bp.is_expired(300));
    }

    #[test]
    fn design_score_penalizes_high_cost() {
        let s1 = design_score(1.0, 100.0, 1.0);
        let s2 = design_score(1.0, 100.0, 2.0);
        assert!(s1 > s2, "higher cost must lower score");
    }

    #[test]
    fn design_score_zero_for_nonpositive_inputs() {
        assert_eq!(design_score(0.0, 100.0, 1.0), 0.0);
        assert_eq!(design_score(1.0, 0.0, 1.0), 0.0);
        assert_eq!(design_score(1.0, 100.0, 0.0), 0.0);
    }
}

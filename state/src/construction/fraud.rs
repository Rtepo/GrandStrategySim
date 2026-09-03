//! Phase 22B: Construction fraud — material substitution and OHS cutting.
//!
//! Material fraud: the contractor buys cheaper substitute materials on the
//! B2B market, naturally retaining the cash difference. The project
//! accumulates `structural_defect` points.
//!
//! OHS fraud: the contractor refuses to submit HealthCapacity/EducationSlots
//! B2B buy bids. The unspent cash remains in `available_cash`. Accident
//! probability increases with uncovered OHS.

use crate::construction::projects::ConstructionProject;
use crate::registries::enums::Commodity;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// A material substitution fraud event (hidden until inspected).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaterialSubstitution {
    /// The original BOM commodity (e.g. Steel).
    pub original_commodity: Commodity,
    /// The cheaper substitute used (e.g. Timber).
    pub substitute_commodity: Commodity,
    /// Quantity substituted (tons-equivalent).
    pub quantity_substituted: f64,
    /// Cash retained by the contractor = (orig_price - subst_price) * qty.
    /// Naturally kept in `available_cash` — no synthetic transfer.
    pub cash_retained: f64,
    /// Structural defect points added to the project.
    pub defect_added: f64,
}

/// Quality tier for commodities used in defect calculation.
/// Higher = stronger. Derived from commodity type.
pub fn commodity_quality(commodity: Commodity) -> f64 {
    match commodity {
        Commodity::Steel => 1.0,
        Commodity::Cement => 0.7,
        Commodity::Bricks => 0.6,
        Commodity::Timber | Commodity::Planks => 0.4,
        Commodity::Glass => 0.5,
        Commodity::Stone => 0.8,
        Commodity::ConstructionMachinery => 1.0,
        _ => 0.5,
    }
}

/// Find a cheaper substitute for a BOM commodity.
/// Returns `Some(substitute)` if a cheaper alternative exists, `None` otherwise.
pub fn find_cheaper_substitute(commodity: Commodity) -> Option<Commodity> {
    match commodity {
        Commodity::Steel => Some(Commodity::Timber),
        Commodity::Cement => Some(Commodity::Clay),
        Commodity::Bricks => Some(Commodity::Timber),
        Commodity::Glass => Some(Commodity::Timber),
        Commodity::Stone => Some(Commodity::Bricks),
        _ => None,
    }
}

/// Base accident rate per turn at zero OHS coverage.
pub const BASE_ACCIDENT_RATE: f64 = 0.02;

/// Phase 25: OHS compensation multiplier — compensation per casualty is
/// `COMPENSATION_WAGE_MULTIPLIER × average_wage`. This scales with
/// inflation/deflation automatically. 100.0 = ~100 years of wages.
/// This replaces the hardcoded `50_000.0` constant.
pub const COMPENSATION_WAGE_MULTIPLIER: f64 = 100.0;

/// Decide whether the contractor commits material fraud on a project.
///
/// # Arguments
/// * `project` - The active construction project (mutated: defect added).
/// * `reputation_score` - Contractor's reputation (0–100).
/// * `justice_coverage` - Justice system coverage ratio (0–1).
/// * `inspection_probability` - Probability of being inspected this turn (0–1).
/// * `rng` - Random number generator.
///
/// # Returns
/// `Some(MaterialSubstitution)` if fraud was committed, `None` otherwise.
///
/// # Rules
/// * Probability scales with: low reputation, low justice coverage, low inspection.
/// * If fraud occurs, the project's `structural_defect` is increased.
/// * The cash benefit is naturally retained (lower B2B outflow) — no transfer.
pub fn try_material_fraud(
    project: &mut ConstructionProject,
    reputation_score: f64,
    justice_coverage: f64,
    inspection_probability: f64,
    rng: &mut impl Rng,
    market_prices: &std::collections::HashMap<Commodity, f64>,
) -> Option<MaterialSubstitution> {
    // Find a substitutable commodity in the BOM
    let substitutable: Vec<Commodity> = project
        .required_materials
        .keys()
        .filter(|c| find_cheaper_substitute(**c).is_some())
        .copied()
        .collect();

    if substitutable.is_empty() {
        return None;
    }

    // Fraud probability: low reputation + low justice + low inspection → high fraud
    let reputation_factor = (1.0 - reputation_score / 100.0).max(0.0);
    let impunity_factor = (1.0 - justice_coverage).max(0.0);
    let inspection_evasion = (1.0 - inspection_probability).max(0.0);
    let fraud_chance = reputation_factor * 0.3 * impunity_factor * inspection_evasion;

    if rng.gen::<f64>() >= fraud_chance {
        return None;
    }

    // Pick a random substitutable commodity
    let original = substitutable[rng.gen_range(0..substitutable.len())];
    let substitute = find_cheaper_substitute(original)?;
    let required = project
        .required_materials
        .get(&original)
        .copied()
        .unwrap_or(0.0);
    if required <= 0.0 {
        return None;
    }

    // Substitute 30–80% of the required quantity
    let fraction = 0.3 + rng.gen::<f64>() * 0.5;
    let quantity_substituted = required * fraction;

    let orig_quality = commodity_quality(original);
    let subst_quality = commodity_quality(substitute);
    let defect_added = quantity_substituted * (1.0 - subst_quality / orig_quality);

    // Normalize defect to project scale (relative to total BOM mass)
    let total_bom_mass: f64 = project.required_materials.values().sum();
    let normalized_defect = if total_bom_mass > 0.0 {
        (defect_added / total_bom_mass).min(1.0)
    } else {
        0.0
    };

    project.structural_defect = (project.structural_defect + normalized_defect).min(1.0);

    // D.4.5: Cash retained = market price difference × quantity.
    // The contractor saves the difference between the original commodity's
    // market price and the substitute's market price, multiplied by the
    // quantity substituted. Falls back to quality-based estimate if prices
    // are not available.
    let orig_price = market_prices.get(&original).copied().unwrap_or(0.0);
    let subst_price = market_prices.get(&substitute).copied().unwrap_or(0.0);
    let cash_retained = if orig_price > 0.0 && subst_price > 0.0 {
        quantity_substituted * (orig_price - subst_price).max(0.0)
    } else {
        // Fallback: quality-based estimate scaled by avg_wage proxy
        quantity_substituted * (orig_quality - subst_quality) * 1000.0
    };

    Some(MaterialSubstitution {
        original_commodity: original,
        substitute_commodity: substitute,
        quantity_substituted,
        cash_retained,
        defect_added: normalized_defect,
    })
}

/// Decide whether the contractor cuts OHS spending on a project.
///
/// # Arguments
/// * `project` - The active construction project (mutated: coverage reduced).
/// * `reputation_score` - Contractor's reputation (0–100).
/// * `justice_coverage` - Justice system coverage ratio (0–1).
/// * `inspection_probability` - Probability of PIP inspection this turn (0–1).
/// * `rng` - Random number generator.
///
/// # Returns
/// `true` if OHS was cut (coverage reduced), `false` otherwise.
///
/// # Rules
/// * Probability scales with: low reputation, low justice, low inspection.
/// * If cut, `ohs_coverage_ratio` is reduced (fewer/no B2B OHS bids submitted).
/// * The unspent cash naturally remains in `available_cash`.
pub fn try_ohs_cut(
    project: &mut ConstructionProject,
    reputation_score: f64,
    justice_coverage: f64,
    inspection_probability: f64,
    rng: &mut impl Rng,
) -> bool {
    // No OHS requirement → nothing to cut
    if project.ohs_health_required <= 0.0 && project.ohs_education_required <= 0.0 {
        return false;
    }

    let reputation_factor = (1.0 - reputation_score / 100.0).max(0.0);
    let impunity_factor = (1.0 - justice_coverage).max(0.0);
    let inspection_evasion = (1.0 - inspection_probability).max(0.0);
    let cut_chance = reputation_factor * 0.3 * impunity_factor * inspection_evasion;

    if rng.gen::<f64>() >= cut_chance {
        return false;
    }

    // Cut OHS coverage to 10–50% of what it should be
    let cut_level = 0.1 + rng.gen::<f64>() * 0.4;
    project.ohs_coverage_ratio = project.ohs_coverage_ratio.min(cut_level);

    true
}

/// Check if a workplace accident occurs this turn.
///
/// # Arguments
/// * `project` - The active construction project.
/// * `rng` - Random number generator.
///
/// # Returns
/// `Some(casualty_count)` if an accident occurs, `None` otherwise.
///
/// # Rules
/// * `accident_chance = BASE_ACCIDENT_RATE * (1.0 - ohs_coverage_ratio) * (1.0 + progress)`
/// * Full coverage → zero chance.
/// * More progress + less coverage → higher chance.
pub fn check_workplace_accident(project: &ConstructionProject, rng: &mut impl Rng) -> Option<u32> {
    let accident_chance =
        BASE_ACCIDENT_RATE * (1.0 - project.ohs_coverage_ratio) * (1.0 + project.progress);

    if accident_chance <= 0.0 || rng.gen::<f64>() >= accident_chance {
        return None;
    }

    // 1–5 casualties per accident
    let casualties = 1 + rng.gen_range(0..5);
    Some(casualties)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::projects::ConstructionProjectType;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn make_project() -> ConstructionProject {
        let mut required = std::collections::BTreeMap::new();
        required.insert(Commodity::Steel, 500.0);
        required.insert(Commodity::Cement, 800.0);
        ConstructionProject {
            id: "p1".to_string(),
            project_type: ConstructionProjectType::Factory,
            micro_region_id: "r1".to_string(),
            target_building_type: "Steel Mill".to_string(),
            required_materials: required,
            delivered_materials: std::collections::BTreeMap::new(),
            target_capacity_increase: 100,
            target_capital_increase: 1_000_000.0,
            is_new_building: true,
            total_cost: 500_000.0,
            cost_spent: 0.0,
            investor_cash_debited: 500_000.0,
            tranches_paid: 0.0,
            duration_turns: 10,
            turns_elapsed: 0,
            progress: 0.0,
            on_hold: false,
            consecutive_hold_turns: 0,
            hold_reason: None,
            investor_id: "inv1".to_string(),
            main_contractor_id: "c1".to_string(),
            subcontractors: Vec::new(),
            tranches: Vec::new(),
            paid_tranches: 0,
            contract_price: 500_000.0,
            contractor_margin: 0.15,
            structural_defect: 0.0,
            ohs_health_required: 10.0,
            ohs_education_required: 5.0,
            ohs_health_delivered: 10.0,
            ohs_education_delivered: 5.0,
            ohs_coverage_ratio: 1.0,
            ohs_accidents: 0,
            network_link_target: None,
            network_target_level: None,
            weather_productivity: 1.0,
            disaster_material_loss: 0.0,
            parcel_id: String::new(),
        }
    }

    #[test]
    fn test_commodity_quality() {
        assert_eq!(commodity_quality(Commodity::Steel), 1.0);
        assert_eq!(commodity_quality(Commodity::Timber), 0.4);
        assert!(commodity_quality(Commodity::Steel) > commodity_quality(Commodity::Timber));
    }

    #[test]
    fn test_find_cheaper_substitute() {
        assert_eq!(
            find_cheaper_substitute(Commodity::Steel),
            Some(Commodity::Timber)
        );
        assert_eq!(
            find_cheaper_substitute(Commodity::Cement),
            Some(Commodity::Clay)
        );
        assert_eq!(find_cheaper_substitute(Commodity::Energy), None);
    }

    #[test]
    fn test_fraud_increases_defect() {
        let mut project = make_project();
        let mut rng = StdRng::seed_from_u64(42);
        // Low reputation, low justice, low inspection → high fraud chance
        let result = try_material_fraud(&mut project, 10.0, 0.1, 0.1, &mut rng, &std::collections::HashMap::new());
        if let Some(fraud) = result {
            assert!(fraud.defect_added > 0.0);
            assert!(project.structural_defect > 0.0);
        }
    }

    #[test]
    fn test_ohs_cut_reduces_coverage() {
        let mut project = make_project();
        let mut rng = StdRng::seed_from_u64(42);
        let cut = try_ohs_cut(&mut project, 10.0, 0.1, 0.1, &mut rng);
        if cut {
            assert!(project.ohs_coverage_ratio < 1.0);
        }
    }

    #[test]
    fn test_accident_zero_at_full_coverage() {
        let project = make_project();
        let mut rng = StdRng::seed_from_u64(42);
        // Full coverage → zero accident chance
        for _ in 0..100 {
            assert!(check_workplace_accident(&project, &mut rng).is_none());
        }
    }

    #[test]
    fn test_accident_nonzero_at_low_coverage() {
        let mut project = make_project();
        project.ohs_coverage_ratio = 0.0;
        project.progress = 0.8;
        let mut rng = StdRng::seed_from_u64(42);
        let mut accident_occurred = false;
        for _ in 0..1000 {
            if check_workplace_accident(&project, &mut rng).is_some() {
                accident_occurred = true;
                break;
            }
        }
        assert!(
            accident_occurred,
            "Expected at least one accident with zero coverage"
        );
    }
}

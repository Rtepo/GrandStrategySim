//! Phase 19B: Fixed asset cohorts, degradation, technological obsolescence,
//! maintenance-as-a-service, and cohort compaction.
//!
//! A `FixedAssetCohort` aggregates N identical machines (same blueprint, same
//! acquire turn, average condition) so we never track individual items — this
//! is the memory-safety guarantee (see the blueprint doc's "MEMORY SAFETY"
//! section).
//!
//! # Lifecycle per turn
//! 1. **Degradation** (`degrade_cohorts`): each cohort's `condition` drops by
//!    `1.0 / durability × stress_factor`. Cohorts reaching `condition ≤ 0` are
//!    scrapped.
//! 2. **Maintenance** (`restore_cohort_condition`): consuming `MaintenanceServices`
//!    (bought B2B from `Sector::MaintenanceWorkshops` buildings) restores
//!    condition. The service is ephemeral — consumed on delivery, not stockpiled.
//! 3. **Obsolescence** (`obsolescence_factor`): a cohort's efficiency contribution
//!    drops toward 0 as its `base_tech_year` falls behind the domestic tech
//!    frontier. This forces scrap-and-renew investment cycles.
//! 4. **Capacity** (`machinery_factor`): `1.0 + Σ count × quality × condition ×
//!    obsolescence × machine_unit_capacity`. The `1.0` baseline = manual mode
//!    for empty cohorts (no save breakage).
//! 5. **Compaction** (`compact_cohorts`): merge cohorts when count exceeds the
//!    configured cap to bound RAM.
//!
//! # No circular dependency
//! `MaintenanceServices` is produced by `Sector::MaintenanceWorkshops` buildings
//! that consume only generic raw materials (Steel, MechanicalComponents,
//! ElectronicComponents, Energy, Fuels) — never machinery or MaintenanceServices.
//! A cold-start world can always bootstrap maintenance from basic raw materials.

use crate::economy::generative_goods_config::GenerativeGoodsConfig;
use crate::registries::enums::Commodity;
use crate::registries::tech_tree::TechId;
use serde::{Deserialize, Serialize};

/// A cohort of identical fixed-asset machines (memory-bounded aggregate).
///
/// # Rules
/// * One cohort = N machines sharing `blueprint_id` + `acquired_turn` + an
///   *average* `condition` (not per-machine state).
/// * `condition` is in `[0.0, 1.0]`; degradation applies to the scalar.
/// * `quality` / `durability` / `base_tech` / `base_tech_year` are cached from
///   the blueprint at install time so the cohort is self-contained for capacity
///   and obsolescence calculations (no registry lookup needed per turn).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FixedAssetCohort {
    /// Blueprint id this cohort was produced under.
    pub blueprint_id: String,
    /// Commodity of the machines (IndustrialMachinery, Trucks, Cars, ...).
    pub commodity: Commodity,
    /// Number of machines in this cohort (cohort size).
    pub count: f64,
    /// Average condition in `[0.0, 1.0]`.
    pub condition: f64,
    /// Cached blueprint quality (affects capacity + degradation).
    pub quality: f64,
    /// Durability in turns (turns-to-fully-degrade from condition 1.0 → 0.0).
    pub durability: f64,
    /// Cached blueprint base technology id (for obsolescence penalty).
    pub base_tech: TechId,
    /// Cached `TechNode.year` of `base_tech` (for obsolescence penalty).
    pub base_tech_year: u32,
    /// Turn the cohort was acquired/installed.
    pub acquired_turn: u32,
}

impl FixedAssetCohort {
    /// Create a freshly installed cohort (condition = 1.0).
    pub fn new(
        blueprint_id: String,
        commodity: Commodity,
        count: f64,
        quality: f64,
        durability: f64,
        base_tech: TechId,
        base_tech_year: u32,
        acquired_turn: u32,
    ) -> Self {
        Self {
            blueprint_id,
            commodity,
            count,
            condition: 1.0,
            quality,
            durability,
            base_tech,
            base_tech_year,
            acquired_turn,
        }
    }

    /// Returns `true` if the cohort is scrapped (condition ≤ 0).
    pub fn is_scrapped(&self) -> bool {
        self.condition <= 0.0 || self.count <= 0.0
    }
}

/// Compute the obsolescence factor for a cohort given the current tech frontier.
///
/// `obsolescence_factor = clamp(1.0 - k × (frontier_year - base_tech_year) / frontier_year, 0.0, 1.0)`
///
/// # Rules
/// * `frontier_year` = the highest `TechNode.year` among currently-patented-or-
///   known technologies for this cohort's sector in the domestic market.
/// * `k` is the obsolescence aggressiveness knob (default ~2.0).
/// * A cohort at the frontier (`base_tech_year == frontier_year`) → factor 1.0.
/// * A cohort 50 years behind with `k = 2.0` → factor ≈ 0 (forces scrap-and-renew).
/// * If `frontier_year` is 0 (no techs known), returns 1.0 (no penalty — cold start).
pub fn obsolescence_factor(base_tech_year: u32, frontier_year: u32, k: f64) -> f64 {
    if frontier_year == 0 {
        return 1.0;
    }
    let gap = (frontier_year as f64) - (base_tech_year as f64);
    if gap <= 0.0 {
        return 1.0;
    }
    let penalty = k * gap / (frontier_year as f64);
    (1.0 - penalty).max(0.0).min(1.0)
}

/// Compute the machinery capacity factor for a building's installed cohorts.
///
/// `machinery_factor = 1.0 + Σ_cohort(count × quality × condition × obsolescence × machine_unit_capacity)`
///
/// # Rules
/// * The `1.0` baseline = manual mode for buildings with empty `fixed_assets`
///   → identical to pre-Phase-19 behavior (no save breakage, no GDP cliff).
/// * Scrapped cohorts (condition ≤ 0 or count ≤ 0) contribute 0.
/// * `frontier_year` is the per-sector domestic tech frontier (caller computes).
pub fn machinery_factor(
    cohorts: &[FixedAssetCohort],
    frontier_year: u32,
    config: &GenerativeGoodsConfig,
) -> f64 {
    let mut factor = 1.0;
    for cohort in cohorts {
        if cohort.is_scrapped() {
            continue;
        }
        let obs = obsolescence_factor(cohort.base_tech_year, frontier_year, config.obsolescence_aggressiveness);
        factor += cohort.count * cohort.quality * cohort.condition * obs * config.machine_unit_capacity;
    }
    factor
}

/// Degrade all cohorts in place by one turn.
///
/// `cohort.condition -= (1.0 / cohort.durability) × stress_factor`
///
/// # Rules
/// * `stress_factor` scales degradation when the host building is in poor
///   condition (`stress_factor = 1.0 + stress_weight × (1.0 - building_condition)`).
/// * Condition is clamped to `[0.0, 1.0]`.
/// * Cohorts reaching `condition ≤ 0` are marked scrapped (caller removes them).
/// * High-durability blueprints degrade slower (durability is per-blueprint).
///
/// # Returns
/// The list of indices of cohorts that reached `condition ≤ 0` (to be scrapped).
pub fn degrade_cohorts(
    cohorts: &mut [FixedAssetCohort],
    building_condition: f64,
    config: &GenerativeGoodsConfig,
) -> Vec<usize> {
    let stress = 1.0 + config.degradation_stress_weight * (1.0 - building_condition).max(0.0);
    let mut scrapped = Vec::new();
    for (i, cohort) in cohorts.iter_mut().enumerate() {
        if cohort.is_scrapped() {
            continue;
        }
        if cohort.durability > 0.0 {
            cohort.condition -= (1.0 / cohort.durability) * stress;
        }
        if cohort.condition <= 0.0 {
            cohort.condition = 0.0;
            scrapped.push(i);
        } else if cohort.condition > 1.0 {
            cohort.condition = 1.0;
        }
    }
    scrapped
}

/// Remove scrapped cohorts (condition ≤ 0 or count ≤ 0) from the vector.
pub fn remove_scrapped(cohorts: &mut Vec<FixedAssetCohort>) {
    cohorts.retain(|c| !c.is_scrapped());
}

/// Compute the total `MaintenanceServices` quantity needed to fully restore all
/// cohorts to condition 1.0 this turn.
///
/// `needed = Σ_cohort count × (1.0 - condition) × maintenance_per_condition_point`
///
/// This is the derived demand a factory submits as a Buy Bid on the B2B market.
pub fn maintenance_services_needed(cohorts: &[FixedAssetCohort], config: &GenerativeGoodsConfig) -> f64 {
    cohorts
        .iter()
        .filter(|c| !c.is_scrapped())
        .map(|c| c.count * (1.0 - c.condition) * config.maintenance_per_condition_point)
        .sum()
}

/// Restore cohort condition by consuming a quantity of `MaintenanceServices`.
///
/// # Rules
/// * The `services_available` quantity is distributed proportionally across
///   cohorts by their condition deficit (`count × (1.0 - condition)`).
/// * Each cohort's restoration is capped at `max_restore_per_turn` (config).
/// * Condition is clamped to `[0.0, 1.0]`.
/// * Returns the actual quantity of services consumed (≤ `services_available`).
pub fn restore_cohort_condition(
    cohorts: &mut [FixedAssetCohort],
    services_available: f64,
    config: &GenerativeGoodsConfig,
) -> f64 {
    if services_available <= 0.0 {
        return 0.0;
    }
    let total_deficit: f64 = cohorts
        .iter()
        .filter(|c| !c.is_scrapped())
        .map(|c| c.count * (1.0 - c.condition))
        .sum();
    if total_deficit <= 0.0 {
        return 0.0;
    }
    let mut consumed = 0.0;
    for cohort in cohorts.iter_mut() {
        if cohort.is_scrapped() {
            continue;
        }
        let deficit = cohort.count * (1.0 - cohort.condition);
        if deficit <= 0.0 {
            continue;
        }
        let share = deficit / total_deficit;
        let allocated = services_available * share;
        let max_restorable = cohort.count * config.max_restore_per_turn * config.maintenance_per_condition_point;
        let used = allocated.min(max_restorable);
        if used > 0.0 && config.maintenance_per_condition_point > 0.0 {
            let condition_gain = used / (cohort.count * config.maintenance_per_condition_point);
            cohort.condition = (cohort.condition + condition_gain).min(1.0);
            consumed += used;
        }
    }
    consumed
}

/// Install a fixed-asset cohort onto a building's cohort vector, then compact.
///
/// # Rules
/// * Appends the cohort, then runs `compact_cohorts` to respect the cap.
/// * Use this instead of `push` so the cap is always enforced.
pub fn install_fixed_asset(cohorts: &mut Vec<FixedAssetCohort>, cohort: FixedAssetCohort, config: &GenerativeGoodsConfig) {
    cohorts.push(cohort);
    compact_cohorts(cohorts, config);
}

/// Compact a cohort vector to respect `max_fixed_cohorts_per_building`.
///
/// # Rules
/// * If `len ≤ cap`, no-op.
/// * Prefer merging cohorts with the same `blueprint_id` (sum `count`,
///   condition-/count-weighted average `condition` and `quality`).
/// * If still over the cap, merge the two cohorts with the closest `condition`
///   (weighted average) until under the cap.
/// * This guarantees RAM predictability — see the blueprint's "MEMORY SAFETY".
pub fn compact_cohorts(cohorts: &mut Vec<FixedAssetCohort>, config: &GenerativeGoodsConfig) {
    let cap = config.max_fixed_cohorts_per_building;
    if cohorts.len() <= cap {
        return;
    }
    // Pass 1: merge same-blueprint cohorts.
    merge_same_blueprint(cohorts);
    if cohorts.len() <= cap {
        return;
    }
    // Pass 2: merge closest-condition cohorts until under cap.
    while cohorts.len() > cap {
        merge_closest_condition(cohorts);
    }
}

/// Merge all cohorts sharing the same `blueprint_id` (in place).
fn merge_same_blueprint(cohorts: &mut Vec<FixedAssetCohort>) {
    if cohorts.len() <= 1 {
        return;
    }
    // Sort by blueprint_id so identical blueprints are adjacent.
    cohorts.sort_by(|a, b| a.blueprint_id.cmp(&b.blueprint_id));
    let mut merged: Vec<FixedAssetCohort> = Vec::with_capacity(cohorts.len());
    for cohort in cohorts.drain(..) {
        if let Some(last) = merged.last_mut() {
            if last.blueprint_id == cohort.blueprint_id
                && last.commodity == cohort.commodity
                && last.base_tech_year == cohort.base_tech_year
            {
                let total_count = last.count + cohort.count;
                if total_count > 0.0 {
                    last.condition = (last.condition * last.count + cohort.condition * cohort.count) / total_count;
                    last.quality = (last.quality * last.count + cohort.quality * cohort.count) / total_count;
                    last.count = total_count;
                }
                continue;
            }
        }
        merged.push(cohort);
    }
    *cohorts = merged;
}

/// Merge the two cohorts with the closest `condition` (in place).
fn merge_closest_condition(cohorts: &mut Vec<FixedAssetCohort>) {
    if cohorts.len() < 2 {
        return;
    }
    let mut best_i = 0;
    let mut best_j = 1;
    let mut best_diff = (cohorts[0].condition - cohorts[1].condition).abs();
    for i in 0..cohorts.len() {
        for j in (i + 1)..cohorts.len() {
            let diff = (cohorts[i].condition - cohorts[j].condition).abs();
            if diff < best_diff {
                best_diff = diff;
                best_i = i;
                best_j = j;
            }
        }
    }
    // Merge j into i, remove j.
    let (lo, hi) = if best_i < best_j { (best_i, best_j) } else { (best_j, best_i) };
    let to_merge = cohorts.remove(hi);
    let target = &mut cohorts[lo];
    let total_count = target.count + to_merge.count;
    if total_count > 0.0 {
        target.condition = (target.condition * target.count + to_merge.condition * to_merge.count) / total_count;
        target.quality = (target.quality * target.count + to_merge.quality * to_merge.count) / total_count;
        target.count = total_count;
    }
}

// ── Phase 23A: Draft Animal Maintenance ──

/// Compute Fodder + Water needed to sustain draft-animal cohorts for one turn.
///
/// Only counts cohorts where `commodity == Commodity::DraftAnimals`.
/// Draft animals require Fodder and Water instead of MaintenanceServices.
///
/// # Returns
/// A `BTreeMap<Commodity, f64>` with `Fodder` and `Water` quantities.
pub fn draft_animal_maintenance_needed(
    cohorts: &[FixedAssetCohort],
    config: &GenerativeGoodsConfig,
) -> std::collections::BTreeMap<Commodity, f64> {
    let mut needed = std::collections::BTreeMap::new();
    for cohort in cohorts.iter().filter(|c| !c.is_scrapped()) {
        if cohort.commodity != Commodity::DraftAnimals {
            continue;
        }
        // Fodder scales with count and condition deficit (hungrier when recovering).
        let fodder = cohort.count * config.maintenance_per_condition_point;
        let water = cohort.count * config.maintenance_per_condition_point * 0.5;
        *needed.entry(Commodity::Fodder).or_insert(0.0) += fodder;
        *needed.entry(Commodity::Water).or_insert(0.0) += water;
    }
    needed
}

/// Restore draft-animal cohort condition by consuming Fodder + Water.
///
/// Mirrors `restore_cohort_condition` but with animal feed inputs instead of
/// MaintenanceServices. Fodder and Water are consumed proportionally across
/// all draft-animal cohorts by their condition deficit.
///
/// # Rules
/// * Only affects cohorts where `commodity == Commodity::DraftAnimals`.
/// * Fodder and Water must both be available; the limiting input caps restoration.
/// * Condition is clamped to `[0.0, 1.0]`.
///
/// # Returns
/// `(fodder_consumed, water_consumed)` — actual quantities used.
pub fn feed_draft_animals(
    cohorts: &mut [FixedAssetCohort],
    fodder_available: f64,
    water_available: f64,
    config: &GenerativeGoodsConfig,
) -> (f64, f64) {
    if fodder_available <= 0.0 || water_available <= 0.0 {
        return (0.0, 0.0);
    }

    // Total condition deficit across all draft-animal cohorts.
    let total_deficit: f64 = cohorts
        .iter()
        .filter(|c| !c.is_scrapped() && c.commodity == Commodity::DraftAnimals)
        .map(|c| c.count * (1.0 - c.condition))
        .sum();

    if total_deficit <= 0.0 {
        return (0.0, 0.0);
    }

    // Fodder is the primary feed; Water is needed at half the rate.
    // The limiting factor is whichever runs out first relative to need.
    let fodder_needed: f64 = cohorts
        .iter()
        .filter(|c| !c.is_scrapped() && c.commodity == Commodity::DraftAnimals)
        .map(|c| c.count * (1.0 - c.condition) * config.maintenance_per_condition_point)
        .sum();
    let water_needed = fodder_needed * 0.5;

    // Scale factor = min(fodder_available / fodder_needed, water_available / water_needed, 1.0)
    let fodder_scale = if fodder_needed > 0.0 {
        (fodder_available / fodder_needed).min(1.0)
    } else {
        1.0
    };
    let water_scale = if water_needed > 0.0 {
        (water_available / water_needed).min(1.0)
    } else {
        1.0
    };
    let scale = fodder_scale.min(water_scale);

    let mut fodder_consumed = 0.0;
    let mut water_consumed = 0.0;

    for cohort in cohorts.iter_mut() {
        if cohort.is_scrapped() || cohort.commodity != Commodity::DraftAnimals {
            continue;
        }
        let deficit = cohort.count * (1.0 - cohort.condition);
        if deficit <= 0.0 {
            continue;
        }
        let share = deficit / total_deficit;
        let fodder_allocated = fodder_needed * share * scale;
        let water_allocated = water_needed * share * scale;

        if config.maintenance_per_condition_point > 0.0 {
            let condition_gain = fodder_allocated / (cohort.count * config.maintenance_per_condition_point);
            cohort.condition = (cohort.condition + condition_gain).min(1.0);
        }
        fodder_consumed += fodder_allocated;
        water_consumed += water_allocated;
    }

    (fodder_consumed, water_consumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::generative_goods_config::GenerativeGoodsConfig;
    use crate::registries::enums::Commodity;

    fn cfg() -> GenerativeGoodsConfig {
        GenerativeGoodsConfig::default()
    }

    fn cohort(condition: f64, count: f64, quality: f64, durability: f64, tech_year: u32) -> FixedAssetCohort {
        FixedAssetCohort {
            blueprint_id: "bp_test".to_string(),
            commodity: Commodity::IndustrialMachinery,
            count,
            condition,
            quality,
            durability,
            base_tech: "tech".to_string(),
            base_tech_year: tech_year,
            acquired_turn: 100,
        }
    }

    #[test]
    fn machinery_factor_baseline_is_one_for_empty_cohorts() {
        let c = cfg();
        assert!((machinery_factor(&[], 2000, &c) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn machinery_factor_scales_with_cohort() {
        let c = cfg();
        let cohorts = vec![cohort(1.0, 100.0, 1.0, 200.0, 2000)];
        // factor = 1.0 + 100 × 1.0 × 1.0 × 1.0 × machine_unit_capacity
        let expected = 1.0 + 100.0 * c.machine_unit_capacity;
        assert!((machinery_factor(&cohorts, 2000, &c) - expected).abs() < 1e-9);
    }

    #[test]
    fn obsolescence_factor_is_one_at_frontier() {
        let f = obsolescence_factor(2000, 2000, 2.0);
        assert!((f - 1.0).abs() < 1e-9);
    }

    #[test]
    fn obsolescence_factor_drops_aggressively_for_old_tech() {
        // 50 years behind frontier, k=2.0: factor = 1 - 2×50/2000 = 1 - 0.05 = 0.95
        // 500 years behind: factor = 1 - 2×500/2000 = 1 - 0.5 = 0.5
        let f_50 = obsolescence_factor(1950, 2000, 2.0);
        assert!((f_50 - 0.95).abs() < 1e-9);
        let f_500 = obsolescence_factor(1500, 2000, 2.0);
        assert!((f_500 - 0.5).abs() < 1e-9);
        // 1000 years behind: factor = 1 - 2×1000/2000 = 0.0 → clamped to 0
        let f_1000 = obsolescence_factor(1000, 2000, 2.0);
        assert!((f_1000 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn obsolescence_factor_is_one_for_zero_frontier_cold_start() {
        let f = obsolescence_factor(1900, 0, 2.0);
        assert!((f - 1.0).abs() < 1e-9);
    }

    #[test]
    fn degraded_old_cohort_contributes_near_zero_capacity() {
        let c = cfg();
        // 1000-year-old tech → obsolescence 0.0 → contributes nothing.
        let cohorts = vec![cohort(1.0, 100.0, 1.0, 200.0, 1000)];
        let factor = machinery_factor(&cohorts, 2000, &c);
        assert!((factor - 1.0).abs() < 1e-9, "obsolete cohort must not add capacity");
    }

    #[test]
    fn degradation_lowers_condition_proportional_to_inverse_durability() {
        let c = cfg();
        let mut cohorts = vec![cohort(1.0, 10.0, 1.0, 100.0, 2000)];
        degrade_cohorts(&mut cohorts, 1.0, &c);
        // stress = 1.0 + 0 × (1-1) = 1.0; condition -= 1/100 × 1.0 = 0.01
        assert!((cohorts[0].condition - 0.99).abs() < 1e-9);
    }

    #[test]
    fn degradation_scraps_cohort_at_zero_condition() {
        let c = cfg();
        let mut cohorts = vec![cohort(0.005, 10.0, 1.0, 100.0, 2000)];
        let scrapped = degrade_cohorts(&mut cohorts, 1.0, &c);
        assert_eq!(scrapped, vec![0]);
        assert!(cohorts[0].is_scrapped());
    }

    #[test]
    fn maintenance_needed_scales_with_condition_deficit() {
        let c = cfg();
        let cohorts = vec![
            cohort(0.5, 10.0, 1.0, 100.0, 2000), // deficit = 10 × 0.5 = 5
            cohort(0.8, 10.0, 1.0, 100.0, 2000), // deficit = 10 × 0.2 = 2
        ];
        let needed = maintenance_services_needed(&cohorts, &c);
        assert!((needed - 7.0).abs() < 1e-9);
    }

    #[test]
    fn restore_cohort_condition_restores_proportionally() {
        let c = cfg();
        let mut cohorts = vec![
            cohort(0.5, 10.0, 1.0, 100.0, 2000),
            cohort(0.8, 10.0, 1.0, 100.0, 2000),
        ];
        let consumed = restore_cohort_condition(&mut cohorts, 7.0, &c);
        assert!(consumed > 0.0);
        // Both cohorts should have gained condition.
        assert!(cohorts[0].condition > 0.5);
        assert!(cohorts[1].condition > 0.8);
    }

    #[test]
    fn restore_capped_at_max_restore_per_turn() {
        let mut c = cfg();
        c.max_restore_per_turn = 0.1; // very low cap
        let mut cohorts = vec![cohort(0.0, 10.0, 1.0, 100.0, 2000)];
        let _consumed = restore_cohort_condition(&mut cohorts, 1000.0, &c);
        // Condition gain capped at max_restore_per_turn = 0.1.
        assert!(cohorts[0].condition <= 0.1 + 1e-9);
    }

    #[test]
    fn compact_cohorts_merges_same_blueprint() {
        let mut c = cfg();
        c.max_fixed_cohorts_per_building = 2; // force compaction
        let mut cohorts = vec![
            FixedAssetCohort {
                blueprint_id: "bp_A".to_string(),
                commodity: Commodity::IndustrialMachinery,
                count: 50.0,
                condition: 0.8,
                quality: 1.0,
                durability: 200.0,
                base_tech: "t".to_string(),
                base_tech_year: 2000,
                acquired_turn: 100,
            },
            FixedAssetCohort {
                blueprint_id: "bp_A".to_string(), // same blueprint → merge with above
                commodity: Commodity::IndustrialMachinery,
                count: 50.0,
                condition: 0.6,
                quality: 1.0,
                durability: 200.0,
                base_tech: "t".to_string(),
                base_tech_year: 2000,
                acquired_turn: 100,
            },
            FixedAssetCohort {
                blueprint_id: "bp_B".to_string(), // different blueprint → stays separate
                commodity: Commodity::IndustrialMachinery,
                count: 30.0,
                condition: 0.9,
                quality: 1.0,
                durability: 200.0,
                base_tech: "t".to_string(),
                base_tech_year: 2000,
                acquired_turn: 100,
            },
        ];
        compact_cohorts(&mut cohorts, &c);
        assert_eq!(cohorts.len(), 2, "two same-blueprint cohorts must merge, leaving 2");
        // The merged cohort should have count 100.
        let merged = cohorts.iter().find(|co| co.count > 50.0).unwrap();
        assert!((merged.count - 100.0).abs() < 1e-9);
    }

    #[test]
    fn compact_cohorts_respects_cap() {
        let mut c = cfg();
        c.max_fixed_cohorts_per_building = 3;
        // 5 distinct blueprints → must compact to 3.
        let mut cohorts: Vec<FixedAssetCohort> = (0..5)
            .map(|i| FixedAssetCohort {
                blueprint_id: format!("bp_{}", i),
                commodity: Commodity::IndustrialMachinery,
                count: 10.0,
                condition: 0.5 + i as f64 * 0.1,
                quality: 1.0,
                durability: 200.0,
                base_tech: "t".to_string(),
                base_tech_year: 2000,
                acquired_turn: 100,
            })
            .collect();
        compact_cohorts(&mut cohorts, &c);
        assert!(cohorts.len() <= 3);
    }

    #[test]
    fn install_fixed_asset_enforces_cap() {
        let mut c = cfg();
        c.max_fixed_cohorts_per_building = 2;
        let mut cohorts: Vec<FixedAssetCohort> = Vec::new();
        for i in 0..5 {
            install_fixed_asset(
                &mut cohorts,
                FixedAssetCohort {
                    blueprint_id: format!("bp_{}", i),
                    commodity: Commodity::IndustrialMachinery,
                    count: 10.0,
                    condition: 0.9,
                    quality: 1.0,
                    durability: 200.0,
                    base_tech: "t".to_string(),
                    base_tech_year: 2000,
                    acquired_turn: 100,
                },
                &c,
            );
        }
        assert!(cohorts.len() <= 2);
    }

    #[test]
    fn remove_scrapped_drops_zero_condition_cohorts() {
        let _c = cfg();
        let mut cohorts = vec![
            cohort(0.0, 10.0, 1.0, 200.0, 2000), // scrapped
            cohort(0.5, 10.0, 1.0, 200.0, 2000), // alive
        ];
        remove_scrapped(&mut cohorts);
        assert_eq!(cohorts.len(), 1);
        assert!((cohorts[0].condition - 0.5).abs() < 1e-9);
    }

    #[test]
    fn cold_start_no_circular_dependency_maintenance_uses_no_machinery() {
        // The MaintenanceWorkshops invariant: MaintenanceServices is produced
        // from generic raw materials only — never machinery. This test verifies
        // the cohort math doesn't introduce any machinery dependency.
        let c = cfg();
        // A factory with degraded cohorts needs MaintenanceServices.
        let cohorts = vec![cohort(0.2, 50.0, 1.0, 100.0, 2000)];
        let needed = maintenance_services_needed(&cohorts, &c);
        assert!(needed > 0.0);
        // The factory has no machinery in inventory (cold start) — but
        // maintenance only needs the *service*, which workshops produce from
        // raw materials. No deadlock.
        let mut restored = cohorts.clone();
        restore_cohort_condition(&mut restored, needed, &c);
        assert!(restored[0].condition > cohorts[0].condition);
    }

    // ── Phase 23A: Draft Animal tests ──

    fn draft_cohort(condition: f64, count: f64) -> FixedAssetCohort {
        FixedAssetCohort {
            blueprint_id: "bp_draft".to_string(),
            commodity: Commodity::DraftAnimals,
            count,
            condition,
            quality: 1.0,
            durability: 80.0, // animals age faster than machinery
            base_tech: "t".to_string(),
            base_tech_year: 1850,
            acquired_turn: 100,
        }
    }

    #[test]
    fn draft_animal_maintenance_needed_counts_only_draft_animals() {
        let c = cfg();
        let cohorts = vec![
            draft_cohort(0.5, 10.0),  // deficit = 10 * 0.5 = 5
            cohort(0.5, 10.0, 1.0, 200.0, 2000), // IndustrialMachinery — ignored
        ];
        let needed = draft_animal_maintenance_needed(&cohorts, &c);
        let fodder = needed.get(&Commodity::Fodder).copied().unwrap_or(0.0);
        let water = needed.get(&Commodity::Water).copied().unwrap_or(0.0);
        // Fodder = count * maintenance_per_condition_point = 10 * mcp
        // (maintenance_per_condition_point default = 0.1)
        assert!(fodder > 0.0);
        assert!(water > 0.0);
        assert!(water < fodder); // water is half the fodder rate
    }

    #[test]
    fn feed_draft_animals_restores_condition() {
        let c = cfg();
        let mut cohorts = vec![draft_cohort(0.5, 10.0)];
        let needed = draft_animal_maintenance_needed(&cohorts, &c);
        let fodder = needed.get(&Commodity::Fodder).copied().unwrap_or(0.0);
        let water = needed.get(&Commodity::Water).copied().unwrap_or(0.0);
        let (fc, wc) = feed_draft_animals(&mut cohorts, fodder, water, &c);
        assert!(fc > 0.0);
        assert!(wc > 0.0);
        assert!(cohorts[0].condition > 0.5, "condition should improve");
    }

    #[test]
    fn feed_draft_animals_no_fodder_no_restoration() {
        let c = cfg();
        let mut cohorts = vec![draft_cohort(0.3, 10.0)];
        let (fc, wc) = feed_draft_animals(&mut cohorts, 0.0, 100.0, &c);
        assert_eq!(fc, 0.0);
        assert_eq!(wc, 0.0);
        assert!((cohorts[0].condition - 0.3).abs() < 1e-9, "condition unchanged without fodder");
    }

    #[test]
    fn feed_draft_animals_ignores_machinery_cohorts() {
        let c = cfg();
        let mut cohorts = vec![
            draft_cohort(0.5, 10.0),
            cohort(0.5, 10.0, 1.0, 200.0, 2000), // IndustrialMachinery
        ];
        let (fc, _wc) = feed_draft_animals(&mut cohorts, 100.0, 100.0, &c);
        assert!(fc > 0.0);
        // Machinery cohort should be unchanged.
        assert!((cohorts[1].condition - 0.5).abs() < 1e-9);
    }
}

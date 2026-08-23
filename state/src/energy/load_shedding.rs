//! Phase 81: Tiered load shedding system.
//!
//! Implements escalating load shedding from minor commercial cuts to total
//! blackout, with government priority policies determining which sectors
//! are shed first.

#![allow(missing_docs)]

use crate::entities::Building;
use crate::registries::enums::Sector;
use crate::society::geography::Region;
use crate::society::housing::CommercialBuilding;
use crate::energy::types::*;

use std::collections::HashMap;

/// Calculate the load shedding tier based on supply/demand deficit.
///
/// # Arguments
/// * `supply_mw` - Available supply after HV balancing and curtailment.
/// * `demand_mw` - Total regional demand.
/// * `lv_grid_capacity` - LV grid capacity (supply is capped by this).
/// * `priority` - Government priority policy (affects which sectors are cut).
///
/// # Returns
/// `LoadShedTier` indicating the severity of shedding.
pub fn calculate_load_shed_tier(
    supply_mw: f64,
    demand_mw: f64,
    lv_grid_capacity: f64,
    _priority: GridPriority,
) -> LoadShedTier {
    let effective_supply = supply_mw.min(lv_grid_capacity);
    let deficit_ratio = if demand_mw > 0.0 {
        1.0 - (effective_supply / demand_mw)
    } else {
        0.0
    };
    match deficit_ratio {
        d if d <= 0.0 => LoadShedTier::Normal,
        d if d <= 0.05 => LoadShedTier::Tier1,
        d if d <= 0.15 => LoadShedTier::Tier2,
        d if d <= 0.30 => LoadShedTier::Tier3,
        d if d <= 0.50 => LoadShedTier::Tier4,
        _ => LoadShedTier::Blackout,
    }
}

/// Apply load shedding penalties to buildings based on tier and priority.
///
/// Penalties are written to the `building_efficiency_penalties` map as
/// positive values (reducing production). The existing `execute_production_cycle`
/// applies these as `fulfillment_ratio *= (1.0 - penalty)`.
///
/// # Arguments
/// * `region_id` - Region being shed.
/// * `tier` - Load shedding tier.
/// * `priority` - Government priority (determines cut order).
/// * `buildings` - All buildings (filtered by region and sector).
/// * `regions` - All regions (for mapping micro_region_id to region).
/// * `commercial_buildings` - Commercial buildings (for commercial shedding).
/// * `penalties` - Output map of building ID → penalty.
pub fn apply_load_shedding(
    region_id: &str,
    tier: LoadShedTier,
    priority: GridPriority,
    buildings: &[Building],
    regions: &[Region],
    commercial_buildings: &[CommercialBuilding],
    penalties: &mut HashMap<String, f64>,
) {
    if tier == LoadShedTier::Normal {
        return;
    }

    let reduction = tier.reduction_factor();
    if reduction <= 0.0 {
        return;
    }

    // Determine sector cut order based on priority.
    let (first_cut, second_cut, third_cut) = match priority {
        GridPriority::Peacetime => (
            vec![Sector::LocalServices, Sector::Hospitality, Sector::MediaAndEntertainment],
            vec![Sector::HeavyIndustry, Sector::LightIndustry],
            vec![Sector::ArmamentsIndustry, Sector::MedicalServices, Sector::EducationalServices],
        ),
        GridPriority::Wartime => (
            vec![Sector::LocalServices, Sector::Hospitality],
            vec![Sector::LightIndustry, Sector::MediaAndEntertainment],
            vec![Sector::HeavyIndustry, Sector::ArmamentsIndustry],
        ),
        GridPriority::WinterCrisis => (
            vec![Sector::HeavyIndustry, Sector::LightIndustry],
            vec![Sector::LocalServices, Sector::Hospitality],
            vec![Sector::MedicalServices, Sector::EducationalServices],
        ),
        GridPriority::Industrial => (
            vec![Sector::LocalServices, Sector::Hospitality, Sector::MediaAndEntertainment],
            vec![Sector::MedicalServices, Sector::EducationalServices],
            vec![Sector::HeavyIndustry, Sector::LightIndustry],
        ),
    };

    // Apply penalties to buildings in the region.
    for building in buildings {
        if building.region_id != region_id {
            continue;
        }

        let sector = &building.sector;
        let penalty = if first_cut.contains(sector) {
            reduction
        } else if second_cut.contains(sector) && tier >= LoadShedTier::Tier2 {
            reduction * 0.8
        } else if third_cut.contains(sector) && tier >= LoadShedTier::Tier3 {
            reduction * 0.5
        } else if tier == LoadShedTier::Blackout {
            1.0 // Total blackout affects everything.
        } else {
            continue;
        };

        let existing = penalties.get(&building.id).copied().unwrap_or(0.0);
        // Take the maximum penalty (worst case wins).
        penalties.insert(building.id.clone(), existing.max(penalty));
    }

    // Apply penalties to commercial buildings (mapped via micro_region_id).
    for cb in commercial_buildings {
        // Check if this commercial building belongs to the affected region.
        let belongs_to_region = regions
            .iter()
            .any(|r| r.id == region_id && r.micro_regions.contains_key(&cb.micro_region_id));
        if !belongs_to_region {
            continue;
        }
        let penalty = if tier == LoadShedTier::Blackout {
            1.0
        } else if first_cut.contains(&Sector::LocalServices) {
            reduction
        } else {
            continue;
        };
        let existing = penalties.get(&cb.id).copied().unwrap_or(0.0);
        penalties.insert(cb.id.clone(), existing.max(penalty));
    }
}

/// Check if grid condition is low enough to risk cascading failure.
///
/// Returns true if the grid condition is below 0.5, indicating
/// a risk of line trips and sudden supply loss.
pub fn check_cascading_failure_risk(grid: &PowerGridState, region_id: &str) -> bool {
    let lv_cond = grid
        .region_lv_condition
        .get(region_id)
        .copied()
        .unwrap_or(1.0);
    let mv_cond = grid
        .region_mv_condition
        .get(region_id)
        .copied()
        .unwrap_or(1.0);
    lv_cond < 0.5 || mv_cond < 0.5
}

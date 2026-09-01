//! Capacity-Based Infrastructure Model
//!
//! This module implements the capacity-based infrastructure system for public services,
//! healthcare, education, and care facilities. Buildings generate "Capacity" (beds/seats per turn)
//! instead of tradable commodities.

use serde::{Deserialize, Serialize};

// Phase A.2.1: `CapacityType` definition moved to `registries::enums` to break
// a circular dependency (`ProductionMethod` needs `CapacityType` for its typed
// `seat_type` field). Re-exported here so all existing
// `crate::infrastructure::CapacityType` references compile unchanged.
pub use crate::registries::enums::CapacityType;

/// Per-turn capacity generation by an infrastructure building
/// This replaces commodity output for infrastructure companies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapacityOutput {
    /// Type of capacity generated
    pub capacity_type: CapacityType,

    /// Base capacity per turn
    pub base_capacity: f64,

    /// Capacity per worker (efficiency multiplier)
    pub capacity_per_worker: f64,

    /// Current utilization (0.0-1.0)
    #[serde(default)]
    pub utilization: f64,
}

pub mod building_condition;
pub mod care;
pub mod cultural;
pub mod education;
pub mod effects;
pub mod healthcare;
pub mod heritage;
pub mod maritime;
pub mod pricing;

/// Phase A.1: Sync `region.capacity_pool` from live education/healthcare buildings.
///
/// For each building whose `active_method.seat_type` is `Some(...) ` and whose
/// `active_method.outputs` contains a service-capacity commodity (EducationSlots,
/// HealthCapacity), accumulate physical seats into the region's `capacity_pool`.
///
/// This replaces the previous approach where `capacity_pool` was only populated
/// by clergy domains (`factional_domains.rs`) or left empty. Now the pool
/// reflects real institutional capacity from buildings.
///
/// # Rules
/// * Physical seats = `worker_capacity * capacity_per_worker_factor` where the
///   factor is the method's output coefficient (Rule 15: physical scaling).
/// * The entry is additive across buildings (Rule 20: natural cap = sum).
/// * No negative values (Rule 20: clamping).
/// * Called before `apply_infrastructure_effects` so utilization is computed
///   from real capacity (Rule 16: temporal causality).
pub fn sync_education_capacity_pool(
    buildings: &[crate::entities::Building],
    regions: &mut [crate::society::geography::Region],
) {
    use crate::registries::enums::{Commodity, Sector};

    // Build a map from region_id → index for O(1) region lookup.
    // Collect into an owned map to avoid holding an immutable borrow of `regions`.
    let region_idx: std::collections::HashMap<String, usize> = regions
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id.clone(), i))
        .collect();

    for building in buildings {
        // Only process education and healthcare sector buildings.
        if building.sector != Sector::EducationalServices
            && building.sector != Sector::MedicalServices
        {
            continue;
        }

        // Read the typed seat_type field (A.2.2) — no string heuristics.
        let seat_type = match building.active_method.seat_type {
            Some(st) => st,
            None => continue,
        };

        // Check if the method outputs a service-capacity commodity.
        let has_capacity_output = building
            .active_method
            .outputs
            .keys()
            .any(|c| *c == Commodity::EducationSlots || *c == Commodity::HealthCapacity);

        if !has_capacity_output {
            continue;
        }

        // Get the region for this building.
        let idx = match region_idx.get(&building.region_id) {
            Some(i) => *i,
            None => continue,
        };

        // Compute physical seats: worker_capacity * output coefficient.
        // The output coefficient is the per-1000-worker output quantity from
        // the method. We scale by actual worker_capacity / 1000.0 (Rule 15).
        let capacity_factor = building
            .active_method
            .outputs
            .values()
            .copied()
            .find(|v| *v > 0.0)
            .unwrap_or(1.0);

        let seats = building.worker_capacity as f64 * capacity_factor / 1000.0;
        let seats = seats.max(0.0); // Rule 20: clamp non-negative

        // Accumulate into the region's capacity_pool.
        let region = &mut regions[idx];
        *region.capacity_pool.entry(seat_type).or_insert(0.0) += seats;
    }
}

pub use building_condition::{
    calculate_degradation_rate, calculate_maintenance_bom, calculate_opex_multiplier,
    calculate_renovation_bom, BuildingConditionConfig, RenovationError, RenovationResult,
};
pub use cultural::{
    collect_cultural_donations, deliver_relief_goods, distribute_cash_relief,
    refund_unfilled_cultural_bids, submit_relief_b2b_orders, CulturalBuilding,
    CulturalBuildingType, CulturalFunding, CulturalReliefConfig, CulturalTemplate,
    EndowmentDonationRates, VoluntaryDonationRates,
};
pub use heritage::{
    apply_heritage_effects, apply_heritage_subsidy, can_demolish, can_upgrade_technology,
    check_heritage_eligibility, process_heritage_effects, HeritageBuilding, HeritageError, Market,
};
pub use maritime::{
    advance_shipyard_projects, process_ports_turn, process_shipyard_maintenance,
    refund_unfilled_shipyard_bids, submit_shipyard_construction_orders, total_port_throughput,
    Dock, MaritimeConfig, MaritimeInfrastructure, Port, ShipConstructionProject, ShipType,
    Shipyard,
};

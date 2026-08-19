//! Phase 22C: Fleet-based inspectorate operational range.
//!
//! Inspectorate capacity is no longer a free national-pool scalar. It is
//! constrained by the vehicles (`FixedAssetCohort` of Cars/Trucks) installed
//! at the inspectorate building and the employment at that building.
//! An inspectorate with zero vehicles or zero staff produces zero capacity.

use crate::economy::fixed_assets::FixedAssetCohort;
use crate::entities::Building;
use crate::registries::enums::Commodity;

/// Kilometers of inspection range per vehicle-staff unit.
pub const KM_PER_VEHICLE_UNIT: f64 = 50.0;

/// Compute the effective inspection range (in km) for an inspectorate building.
///
/// # Rules
/// * Range scales with `min(vehicles, staff)` — you need both drivers and cars.
/// * Broken-down vehicles (condition ≤ 0) don't count.
/// * Zero staff → zero range (no drivers).
/// * Zero vehicles → zero range (no cars).
pub fn inspectorate_fleet_range(building: &Building) -> f64 {
    let vehicles: f64 = building
        .fixed_assets
        .iter()
        .filter(|c| c.commodity == Commodity::Cars || c.commodity == Commodity::Trucks)
        .map(|c| c.count * c.condition)
        .sum();

    let staff = building.current_employment as f64;

    let effective_units = vehicles.min(staff);
    effective_units * KM_PER_VEHICLE_UNIT
}

/// Check if a target building is within range of any inspectorate.
///
/// # Arguments
/// * `inspectorate_ranges` - Slice of (building_id, region_id, range_km) tuples.
/// * `target_region_id` - Region of the target building.
/// * `region_distance` - Function that returns distance in km between two regions.
///
/// # Returns
/// `true` if any inspectorate can reach the target this turn.
pub fn is_within_inspection_range<F>(
    inspectorate_ranges: &[(String, String, f64)],
    target_region_id: &str,
    region_distance: F,
) -> bool
where
    F: Fn(&str, &str) -> f64,
{
    inspectorate_ranges.iter().any(|(_, insp_region, range)| {
        let distance = region_distance(insp_region, target_region_id);
        distance <= *range
    })
}

/// Compute fleet ranges for all inspectorate buildings of a given capacity commodity.
///
/// # Arguments
/// * `buildings` - All buildings.
/// * `capacity_commodity` - The inspection capacity commodity to filter by.
///
/// # Returns
/// Vector of (building_id, region_id, range_km) for inspectorates with range > 0.
pub fn compute_inspectorate_fleet_ranges(
    buildings: &[Building],
    capacity_commodity: Commodity,
) -> Vec<(String, String, f64)> {
    buildings
        .iter()
        .filter(|b| {
            b.last_production
                .get(&capacity_commodity)
                .copied()
                .unwrap_or(0.0)
                > 0.0
        })
        .filter_map(|b| {
            let range = inspectorate_fleet_range(b);
            if range > 0.0 {
                Some((b.id.clone(), b.region_id.clone(), range))
            } else {
                None
            }
        })
        .collect()
}

/// Simple region distance: returns 0.0 if same region, 100.0 otherwise.
/// This is a fallback when no geographic distance data is available.
pub fn simple_region_distance(region_a: &str, region_b: &str) -> f64 {
    if region_a == region_b {
        0.0
    } else {
        100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::fixed_assets::FixedAssetCohort;
    use crate::entities::Building;
    use crate::registries::enums::Commodity;
    use crate::registries::tech_tree::TechId;

    fn make_building_with_vehicles(id: &str, region: &str, cars: f64, condition: f64, staff: u32) -> Building {
        let mut b = Building::default();
        b.id = id.to_string();
        b.region_id = region.to_string();
        b.current_employment = staff;
        if cars > 0.0 {
            b.fixed_assets.push(FixedAssetCohort {
                blueprint_id: "car_bp".to_string(),
                commodity: Commodity::Cars,
                count: cars,
                condition,
                quality: 1.0,
                durability: 100.0,
                base_tech: TechId::default(),
                base_tech_year: 2000,
                acquired_turn: 0,
            });
        }
        b
    }

    #[test]
    fn test_fleet_range_zero_vehicles() {
        let b = make_building_with_vehicles("b1", "r1", 0.0, 1.0, 10);
        assert_eq!(inspectorate_fleet_range(&b), 0.0);
    }

    #[test]
    fn test_fleet_range_zero_staff() {
        let b = make_building_with_vehicles("b1", "r1", 5.0, 1.0, 0);
        assert_eq!(inspectorate_fleet_range(&b), 0.0);
    }

    #[test]
    fn test_fleet_range_min_vehicles_staff() {
        // 5 vehicles, 3 staff → effective = 3 → range = 150
        let b = make_building_with_vehicles("b1", "r1", 5.0, 1.0, 3);
        assert!((inspectorate_fleet_range(&b) - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_fleet_range_broken_vehicles() {
        // Vehicles with condition 0 don't count
        let b = make_building_with_vehicles("b1", "r1", 5.0, 0.0, 10);
        assert_eq!(inspectorate_fleet_range(&b), 0.0);
    }

    #[test]
    fn test_within_range_same_region() {
        let ranges = vec![("insp1".to_string(), "r1".to_string(), 50.0)];
        assert!(is_within_inspection_range(&ranges, "r1", simple_region_distance));
    }

    #[test]
    fn test_out_of_range_different_region() {
        let ranges = vec![("insp1".to_string(), "r1".to_string(), 50.0)];
        // Different region → distance 100 > range 50
        assert!(!is_within_inspection_range(&ranges, "r2", simple_region_distance));
    }

    #[test]
    fn test_in_range_different_region_sufficient_range() {
        let ranges = vec![("insp1".to_string(), "r1".to_string(), 150.0)];
        // Different region → distance 100 < range 150
        assert!(is_within_inspection_range(&ranges, "r2", simple_region_distance));
    }
}

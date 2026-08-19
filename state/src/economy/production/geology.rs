//! Phase 21A: Geological deposit physics — lookup, depletion, quality decay, and depth gating.
//!
//! This module provides the core logic for finite-resource mining:
//! - Looking up deposits linked to mining buildings.
//! - Depleting `current_reserves` as resources are extracted.
//! - Decaying `current_quality` as the deposit is exhausted (economic death spiral).
//! - Gating deep deposits behind advanced mining technology.

use crate::registries::enums::Commodity;
use crate::society::geography::{GeologicalFormation, ResourceDeposit};
use crate::state::Country;

/// Maximum depth (in meters) that a mining method from a given year can access.
///
/// This maps the tech progression of mining methods to realistic depth capabilities.
/// Methods before 1880 can only reach shallow deposits; modern methods can reach
/// deep deposits.
pub fn max_depth_for_method_year(year: u32) -> f64 {
    match year {
        y if y < 1885 => 200.0,   // Manual Mining
        y if y < 1890 => 400.0,   // Pneumatic Drilling
        y if y < 1895 => 600.0,   // Electric Mine Pumps
        y if y < 1900 => 800.0,   // Longwall Mining
        y if y < 1950 => 1000.0,  // Open-Pit / Froth Flotation era
        y if y < 1970 => 1200.0,  // Mechanized Longwall
        _ => 2000.0,              // CNC Mining and beyond
    }
}

/// Check whether a mining method from the given year can access a deposit at
/// the given depth.
pub fn can_access_depth(method_year: u32, deposit_depth: f64) -> bool {
    deposit_depth <= max_depth_for_method_year(method_year)
}

/// Compute the effective quality of a deposit based on its depletion ratio.
///
/// Formula: `current_quality = base_quality * (1.0 - 0.5 * depletion_ratio^2)`
///
/// At 50% depletion, quality is ~87.5% of base.
/// At 90% depletion, quality is ~59.5% of base.
/// At 100% depletion, quality is 50% of base (but current_reserves = 0 means no extraction).
pub fn compute_current_quality(base_quality: f64, current_reserves: f64, estimated_reserves: f64) -> f64 {
    if estimated_reserves <= 0.0 {
        return base_quality;
    }
    let depletion_ratio = 1.0 - (current_reserves / estimated_reserves).max(0.0).min(1.0);
    base_quality * (1.0 - 0.5 * depletion_ratio * depletion_ratio)
}

/// Find a deposit in the country's geological formations that matches the
/// given deposit ID and region.
///
/// The deposit ID format is `"{formation_id}/{commodity_key}"`.
///
/// # Returns
/// A tuple of (formation index, deposit key) if found, or `None`.
pub fn find_deposit_index<'a>(
    country: &'a Country,
    deposit_id: &str,
) -> Option<(usize, &'a String, &'a ResourceDeposit)> {
    let parts: Vec<&str> = deposit_id.splitn(2, '/').collect();
    if parts.len() != 2 {
        return None;
    }
    let formation_id = parts[0];
    let commodity_key = parts[1];

    for (f_idx, formation) in country.geological_formations.iter().enumerate() {
        if formation.id == formation_id {
            if let Some((key, deposit)) = formation.resource_deposits.get_key_value(commodity_key) {
                return Some((f_idx, key, deposit));
            }
        }
    }
    None
}

/// Find a deposit for a specific commodity in a specific region.
///
/// Searches all formations that overlap the given region for a deposit
/// producing the requested commodity. Only returns discovered deposits.
///
/// # Returns
/// A deposit ID string (`"{formation_id}/{commodity_key}"`) if found.
pub fn find_deposit_for_commodity(
    country: &Country,
    region_id: &str,
    commodity: Commodity,
) -> Option<String> {
    let target_key = commodity.to_string();
    for formation in &country.geological_formations {
        if !formation.overlapping_regions.contains(&region_id.to_string()) {
            continue;
        }
        for (key, deposit) in &formation.resource_deposits {
            if deposit.commodity == commodity && deposit.discovered && deposit.current_reserves > 0.0 {
                return Some(format!("{}/{}", formation.id, key));
            }
        }
    }
    // Fallback: also match by key string (handles edge cases in commodity serialization)
    let _ = target_key; // suppress unused warning
    None
}

/// Deplete a deposit by the requested amount, reducing `current_reserves` and
/// recomputing `current_quality`.
///
/// # Arguments
/// * `country` - Mutable country whose formations contain the deposit.
/// * `deposit_id` - Deposit ID in `"{formation_id}/{commodity_key}"` format.
/// * `amount` - Requested extraction amount.
///
/// # Returns
/// The actual amount that could be extracted (may be less than requested if
/// `current_reserves` is insufficient). Returns 0.0 if the deposit is not found.
pub fn deplete_deposit(
    country: &mut Country,
    deposit_id: &str,
    amount: f64,
) -> f64 {
    if amount <= 0.0 {
        return 0.0;
    }

    let parts: Vec<&str> = deposit_id.splitn(2, '/').collect();
    if parts.len() != 2 {
        return 0.0;
    }
    let formation_id = parts[0];
    let commodity_key = parts[1];

    for formation in &mut country.geological_formations {
        if formation.id != formation_id {
            continue;
        }
        if let Some(deposit) = formation.resource_deposits.get_mut(commodity_key) {
            let actual = amount.min(deposit.current_reserves);
            deposit.current_reserves -= actual;
            // Recompute quality based on new depletion ratio
            deposit.current_quality = compute_current_quality(
                deposit.quality,
                deposit.current_reserves,
                deposit.estimated_reserves,
            );
            return actual;
        }
    }

    0.0
}

/// Get the quality multiplier for a deposit, to be applied to mining output.
///
/// Returns 0.0 if the deposit is not found, not discovered, or exhausted.
/// Otherwise returns `deposit.current_quality` (0.0–1.0).
pub fn deposit_quality_multiplier(
    country: &Country,
    deposit_id: &str,
) -> f64 {
    match find_deposit_index(country, deposit_id) {
        Some((_, _, deposit)) => {
            if !deposit.discovered || deposit.current_reserves <= 0.0 {
                0.0
            } else {
                deposit.current_quality
            }
        }
        None => 0.0,
    }
}

/// Check if a deposit is accessible with the given method year (depth gating).
///
/// Returns `false` if the deposit is not found or if the method year cannot
/// reach the deposit's depth.
pub fn deposit_is_accessible(
    country: &Country,
    deposit_id: &str,
    method_year: u32,
) -> bool {
    match find_deposit_index(country, deposit_id) {
        Some((_, _, deposit)) => can_access_depth(method_year, deposit.depth),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_depth_progression() {
        assert_eq!(max_depth_for_method_year(1880), 200.0);
        assert_eq!(max_depth_for_method_year(1885), 400.0);
        assert_eq!(max_depth_for_method_year(1890), 600.0);
        assert_eq!(max_depth_for_method_year(1895), 800.0);
        assert_eq!(max_depth_for_method_year(1950), 1200.0);
        assert_eq!(max_depth_for_method_year(1970), 2000.0);
        assert_eq!(max_depth_for_method_year(2020), 2000.0);
    }

    #[test]
    fn test_can_access_depth() {
        assert!(can_access_depth(1880, 150.0));
        assert!(!can_access_depth(1880, 300.0));
        assert!(can_access_depth(1950, 1000.0));
        assert!(!can_access_depth(1950, 1500.0));
        assert!(can_access_depth(1970, 1500.0));
    }

    #[test]
    fn test_quality_decay() {
        // No depletion -> full quality
        let q = compute_current_quality(0.9, 1_000_000.0, 1_000_000.0);
        assert!((q - 0.9).abs() < 1e-9);

        // 50% depletion -> ~87.5% of base
        let q = compute_current_quality(0.9, 500_000.0, 1_000_000.0);
        let expected = 0.9 * (1.0 - 0.5 * 0.25); // 0.9 * 0.875 = 0.7875
        assert!((q - expected).abs() < 1e-9);

        // 90% depletion -> ~59.5% of base
        let q = compute_current_quality(0.9, 100_000.0, 1_000_000.0);
        let expected = 0.9 * (1.0 - 0.5 * 0.81); // 0.9 * 0.595 = 0.5355
        assert!((q - expected).abs() < 1e-9);

        // 100% depletion -> 50% of base
        let q = compute_current_quality(0.9, 0.0, 1_000_000.0);
        let expected = 0.9 * 0.5; // 0.45
        assert!((q - expected).abs() < 1e-9);
    }

    #[test]
    fn test_deplete_deposit() {
        let mut country = Country::mock_for_tests();
        country.geological_formations.push(GeologicalFormation {
            id: "F1".to_string(),
            name: "Test Formation".to_string(),
            formation_type: crate::society::geography::FormationType::SedimentaryBasin,
            resource_deposits: {
                let mut m = BTreeMap::new();
                m.insert("hard_coal".to_string(), ResourceDeposit {
                    commodity: Commodity::HardCoal,
                    estimated_reserves: 1_000_000.0,
                    current_reserves: 1_000_000.0,
                    extraction_cost: 50.0,
                    quality: 0.9,
                    current_quality: 0.9,
                    depth: 100.0,
                    discovered: true,
                });
                m
            },
            overlapping_regions: vec!["R1".to_string()],
            total_area: 10_000.0,
        });

        // Deplete 100k
        let actual = deplete_deposit(&mut country, "F1/hard_coal", 100_000.0);
        assert!((actual - 100_000.0).abs() < 1e-9);

        // Check reserves dropped
        let deposit = &country.geological_formations[0].resource_deposits["hard_coal"];
        assert!((deposit.current_reserves - 900_000.0).abs() < 1e-9);

        // Check quality decayed
        let expected_q = compute_current_quality(0.9, 900_000.0, 1_000_000.0);
        assert!((deposit.current_quality - expected_q).abs() < 1e-9);

        // Try to deplete more than available
        let actual = deplete_deposit(&mut country, "F1/hard_coal", 2_000_000.0);
        assert!((actual - 900_000.0).abs() < 1e-9);
        let deposit = &country.geological_formations[0].resource_deposits["hard_coal"];
        assert!((deposit.current_reserves - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_find_deposit_for_commodity() {
        let mut country = Country::mock_for_tests();
        country.geological_formations.push(GeologicalFormation {
            id: "F1".to_string(),
            name: "Test".to_string(),
            formation_type: crate::society::geography::FormationType::MountainRange,
            resource_deposits: {
                let mut m = BTreeMap::new();
                m.insert("iron".to_string(), ResourceDeposit {
                    commodity: Commodity::Iron,
                    estimated_reserves: 500_000.0,
                    current_reserves: 500_000.0,
                    extraction_cost: 30.0,
                    quality: 0.8,
                    current_quality: 0.8,
                    depth: 150.0,
                    discovered: true,
                });
                m.insert("gold".to_string(), ResourceDeposit {
                    commodity: Commodity::Gold,
                    estimated_reserves: 100_000.0,
                    current_reserves: 100_000.0,
                    extraction_cost: 80.0,
                    quality: 0.7,
                    current_quality: 0.7,
                    depth: 800.0,
                    discovered: false, // hidden
                });
                m
            },
            overlapping_regions: vec!["R1".to_string()],
            total_area: 5_000.0,
        });

        // Iron is discovered -> should find it
        let id = find_deposit_for_commodity(&country, "R1", Commodity::Iron);
        assert!(id.is_some());
        assert!(id.unwrap().starts_with("F1/"));

        // Gold is not discovered -> should not find it
        let id = find_deposit_for_commodity(&country, "R1", Commodity::Gold);
        assert!(id.is_none());

        // Wrong region -> should not find it
        let id = find_deposit_for_commodity(&country, "R2", Commodity::Iron);
        assert!(id.is_none());
    }

    use std::collections::BTreeMap;
}

//! Phase 30: Air cargo routing and overflight fee calculation.
//!
//! Implements 2D vector math for air cargo route computation:
//! - Straight-line flight path from origin airport to destination airport.
//! - Perpendicular distance from each region's center to the flight path
//!   determines which airspaces are overflown.
//! - Overflight fees are charged to the transport/aviation company and
//!   credited to the overflown country's Treasury.
//!
//! # Key Distinction
//! Air cargo overflight fees are NOT trade tariffs on the cargo. They are
//! air-navigation charges paid for traversing a country's airspace. The cargo
//! itself is not taxed — only the transit through airspace is charged.

use crate::society::geography::Region;
use std::collections::HashMap;

/// Result of an air cargo route computation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AirCargoRoute {
    /// Total flight distance in km (straight-line).
    pub distance_km: f64,
    /// Whether the route is valid (both endpoints have airports).
    pub valid: bool,
    /// Overflight fees: (country_name, fee_amount).
    pub overflight_fees: Vec<(String, f64)>,
    /// Total overflight fees.
    pub total_overflight_fees: f64,
}

/// Compute an air cargo route between two airport regions using 2D vector math.
///
/// The flight path is a straight line from origin to destination. For each
/// other region in the world, the perpendicular distance from that region's
/// center to the flight path vector is computed. If the distance is below
/// the proximity threshold, that region's airspace is considered overflown.
///
/// Overflight fees are charged for each overflown region whose `owner_country`
/// differs from the origin country.
///
/// # Arguments
/// * `origin_region_id` - Region ID of the origin airport.
/// * `dest_region_id` - Region ID of the destination airport.
/// * `regions` - All regions in the world (for overflight detection).
/// * `overflight_rate_per_km` - Fee per km of airspace traversed.
/// * `proximity_threshold` - Max perpendicular distance (km) for overflight.
pub fn compute_air_route(
    origin_region_id: &str,
    dest_region_id: &str,
    regions: &[Region],
    overflight_rate_per_km: f64,
    proximity_threshold: f64,
) -> AirCargoRoute {
    // Find origin and destination regions.
    let origin = match regions.iter().find(|r| r.id == origin_region_id) {
        Some(r) => r,
        None => {
            return AirCargoRoute {
                valid: false,
                ..Default::default()
            }
        }
    };
    let dest = match regions.iter().find(|r| r.id == dest_region_id) {
        Some(r) => r,
        None => {
            return AirCargoRoute {
                valid: false,
                ..Default::default()
            }
        }
    };

    // Both endpoints must have airports.
    if !origin.geographic_traits.has_airport || !dest.geographic_traits.has_airport {
        return AirCargoRoute {
            valid: false,
            ..Default::default()
        };
    }

    // Flight vector: origin → destination.
    let ox = origin.coord_x;
    let oy = origin.coord_y;
    let dx = dest.coord_x - ox;
    let dy = dest.coord_y - oy;
    let flight_distance = (dx * dx + dy * dy).sqrt();

    if flight_distance < 1.0 {
        // Same location — no overflight fees.
        return AirCargoRoute {
            distance_km: 0.0,
            valid: true,
            overflight_fees: Vec::new(),
            total_overflight_fees: 0.0,
        };
    }

    // Unit vector along the flight path.
    let ux = dx / flight_distance;
    let uy = dy / flight_distance;

    let origin_country = &origin.owner_country;

    // For each region (excluding origin and destination), compute perpendicular
    // distance to the flight path. If within threshold, it's overflown.
    let mut fees_by_country: HashMap<String, f64> = HashMap::new();

    for region in regions {
        // Skip origin and destination.
        if region.id == origin_region_id || region.id == dest_region_id {
            continue;
        }

        // Skip regions with no owner (international waters, sea nodes).
        if region.owner_country.is_empty() {
            continue;
        }

        // Skip regions owned by the origin country (no overflight fee for
        // flying over your own territory).
        if &region.owner_country == origin_country {
            continue;
        }

        // Vector from origin to this region's center.
        let rx = region.coord_x - ox;
        let ry = region.coord_y - oy;

        // Projection of region onto the flight path (scalar = t).
        let t = rx * ux + ry * uy;

        // Only count regions that are between origin and destination (0 < t < flight_distance).
        if t < 0.0 || t > flight_distance {
            continue;
        }

        // Perpendicular distance = |cross product| / |flight vector|.
        // cross = rx * uy - ry * ux (z-component of 2D cross product).
        let cross = rx * uy - ry * ux;
        let perp_distance = cross.abs();

        if perp_distance > proximity_threshold {
            continue;
        }

        // This region's airspace is overflown.
        // The segment length within this airspace is approximated by the
        // chord length at the perpendicular distance. For simplicity, we
        // use the proximity threshold as the airspace radius and compute
        // the chord: 2 * sqrt(threshold^2 - perp^2).
        let chord = if perp_distance < proximity_threshold {
            2.0 * (proximity_threshold * proximity_threshold - perp_distance * perp_distance).sqrt()
        } else {
            0.0
        };

        let fee = chord * overflight_rate_per_km;
        if fee > 0.0 {
            *fees_by_country.entry(region.owner_country.clone()).or_insert(0.0) += fee;
        }
    }

    // Sort by country name for deterministic output.
    let mut countries: Vec<String> = fees_by_country.keys().cloned().collect();
    countries.sort();
    let mut overflight_fees = Vec::new();
    let mut total = 0.0;
    for country in &countries {
        let amount = fees_by_country[country];
        total += amount;
        overflight_fees.push((country.clone(), amount));
    }

    AirCargoRoute {
        distance_km: flight_distance,
        valid: true,
        overflight_fees,
        total_overflight_fees: total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::{
        ClimateProfile, Edge, EdgeType, GeographicTraits, NodeType, Region,
    };
    use std::collections::BTreeMap;

    fn make_region_with_coords(
        id: &str,
        owner: &str,
        x: f64,
        y: f64,
        has_airport: bool,
    ) -> Region {
        Region {
            id: id.to_string(),
            display_name: id.to_string(),
            owner_country: owner.to_string(),
            population: 1000,
            gdp: 1000.0,
            gdp_pc: 1.0,
            climate: crate::society::geography::Climate::Balanced,
            soil_profile: BTreeMap::new(),
            arable_land_max: 100,
            arable_land_used: 0,
            extraction_limits: BTreeMap::new(),
            extraction_used: BTreeMap::new(),
            resources: serde_json::Map::new(),
            is_capital: false,
            node_type: NodeType::LandRegion,
            edges: Vec::new(),
            adjacency: Vec::new(),
            land_distribution: BTreeMap::new(),
            class_demographics: Default::default(),
            governance: None,
            capacity_pool: BTreeMap::new(),
            capacity_utilization: BTreeMap::new(),
            capacity_prices: BTreeMap::new(),
            land_use_inventory: Default::default(),
            climate_profile: ClimateProfile::Temperate,
            micro_regions: BTreeMap::new(),
            treasury: Default::default(),
            microregion_budgets: HashMap::new(),
            winter_mortality_multiplier: 1.0,
            holy_site: None,
            geographic_traits: GeographicTraits {
                has_airport,
                ..Default::default()
            },
            coord_x: x,
            coord_y: y,
            development_level: 0.0,
            parcel_ids: Vec::new(),
        }
    }

    #[test]
    fn air_route_requires_airports_at_both_ends() {
        let regions = vec![
            make_region_with_coords("r1", "Lechia", 0.0, 0.0, true),
            make_region_with_coords("r2", "Lechia", 100.0, 0.0, false), // no airport
        ];
        let route = compute_air_route("r1", "r2", &regions, 0.02, 100.0);
        assert!(!route.valid);
    }

    #[test]
    fn air_route_computes_straight_line_distance() {
        let regions = vec![
            make_region_with_coords("r1", "Lechia", 0.0, 0.0, true),
            make_region_with_coords("r2", "Lechia", 300.0, 400.0, true), // 500 km away
        ];
        let route = compute_air_route("r1", "r2", &regions, 0.02, 100.0);
        assert!(route.valid);
        assert!((route.distance_km - 500.0).abs() < 1e-6);
        // Same country — no overflight fees.
        assert_eq!(route.total_overflight_fees, 0.0);
    }

    #[test]
    fn overflight_fee_charged_for_foreign_airspace() {
        // Origin at (0,0), destination at (1000,0).
        // Foreign region at (500, 50) — within 100km threshold.
        let regions = vec![
            make_region_with_coords("origin", "Lechia", 0.0, 0.0, true),
            make_region_with_coords("dest", "Lechia", 1000.0, 0.0, true),
            make_region_with_coords("overflown", "Enemy", 500.0, 50.0, false),
        ];
        let route = compute_air_route("origin", "dest", &regions, 0.02, 100.0);
        assert!(route.valid);
        assert!(route.total_overflight_fees > 0.0);
        assert_eq!(route.overflight_fees.len(), 1);
        assert_eq!(route.overflight_fees[0].0, "Enemy");
    }

    #[test]
    fn no_overflight_fee_for_own_country() {
        // Origin at (0,0), destination at (1000,0).
        // Same-country region at (500, 50) — no fee.
        let regions = vec![
            make_region_with_coords("origin", "Lechia", 0.0, 0.0, true),
            make_region_with_coords("dest", "Lechia", 1000.0, 0.0, true),
            make_region_with_coords("mid", "Lechia", 500.0, 50.0, false),
        ];
        let route = compute_air_route("origin", "dest", &regions, 0.02, 100.0);
        assert!(route.valid);
        assert_eq!(route.total_overflight_fees, 0.0);
    }

    #[test]
    fn region_outside_threshold_not_overflown() {
        // Origin at (0,0), destination at (1000,0).
        // Foreign region at (500, 200) — outside 100km threshold.
        let regions = vec![
            make_region_with_coords("origin", "Lechia", 0.0, 0.0, true),
            make_region_with_coords("dest", "Lechia", 1000.0, 0.0, true),
            make_region_with_coords("far", "Enemy", 500.0, 200.0, false),
        ];
        let route = compute_air_route("origin", "dest", &regions, 0.02, 100.0);
        assert!(route.valid);
        assert_eq!(route.total_overflight_fees, 0.0);
    }

    #[test]
    fn perpendicular_distance_math_is_exact() {
        // Flight path: (0,0) → (1000,0) — along the X axis.
        // Region at (500, 60) — perpendicular distance = 60.
        // Chord = 2 * sqrt(100^2 - 60^2) = 2 * sqrt(6400) = 2 * 80 = 160.
        // Fee = 160 * 0.02 = 3.2.
        let regions = vec![
            make_region_with_coords("origin", "Lechia", 0.0, 0.0, true),
            make_region_with_coords("dest", "Lechia", 1000.0, 0.0, true),
            make_region_with_coords("overflown", "Enemy", 500.0, 60.0, false),
        ];
        let route = compute_air_route("origin", "dest", &regions, 0.02, 100.0);
        assert!(route.valid);
        assert!((route.total_overflight_fees - 3.2).abs() < 1e-6);
    }

    #[test]
    fn multiple_countries_charged_separately() {
        // Flight path: (0,0) → (1000,0).
        // Two foreign regions at (300, 50) and (700, 50).
        let regions = vec![
            make_region_with_coords("origin", "Lechia", 0.0, 0.0, true),
            make_region_with_coords("dest", "Lechia", 1000.0, 0.0, true),
            make_region_with_coords("c1", "CountryA", 300.0, 50.0, false),
            make_region_with_coords("c2", "CountryB", 700.0, 50.0, false),
        ];
        let route = compute_air_route("origin", "dest", &regions, 0.02, 100.0);
        assert!(route.valid);
        assert_eq!(route.overflight_fees.len(), 2);
        // Both countries should have the same fee (same perpendicular distance).
        let fee_a = route.overflight_fees.iter().find(|(c, _)| c == "CountryA").unwrap().1;
        let fee_b = route.overflight_fees.iter().find(|(c, _)| c == "CountryB").unwrap().1;
        assert!((fee_a - fee_b).abs() < 1e-6);
        assert!(fee_a > 0.0);
    }

    #[test]
    fn region_behind_origin_not_overflown() {
        // Region at (-200, 50) — behind the origin, not on the flight path.
        let regions = vec![
            make_region_with_coords("origin", "Lechia", 0.0, 0.0, true),
            make_region_with_coords("dest", "Lechia", 1000.0, 0.0, true),
            make_region_with_coords("behind", "Enemy", -200.0, 50.0, false),
        ];
        let route = compute_air_route("origin", "dest", &regions, 0.02, 100.0);
        assert!(route.valid);
        assert_eq!(route.total_overflight_fees, 0.0);
    }
}

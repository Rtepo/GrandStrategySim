//! Phase 23C: Commuting and municipal passenger transport.
//!
//! Implements the commuter-eligible labor pool system. Workers can accept
//! jobs in directly adjacent regions (one edge hop) if they can afford the
//! `PassengerTransport` ticket. This module builds the commute map; the
//! actual B2C ticket clearing happens in `b2c_services.rs`, and the labor
//! market integration happens in `labor_market.rs`.
//!
//! # Rules
//! * Commuting is single-hop only (directly adjacent regions via `edges`).
//! * `commute_cost_units` = distance × capacity_per_km × frequency.
//! * Affordability is per-class: a class can commute if its savings cover
//!   the ticket price. If unaffordable, FTE is excluded from the commuter
//!   pool → localized labor shortage in the host region.

use crate::economy::transport_networks::TransportNetworkOverlay;
use crate::society::geography::{EdgeType, Region};
use serde::{Deserialize, Serialize};

/// Phase 23C: Transport ownership law.
///
/// Controls whether passenger transport is operated by the JST (subsidized)
/// or by private companies (market pricing). Privatization raises ticket
/// prices and may exclude lower-class workers from commuting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TransportLaw {
    /// Ownership model for transport operators.
    pub ownership: TransportOwnership,
    /// Subsidy fraction for public transport (0.0–1.0 of ticket price).
    /// Set to 0.0 under privatization; up to 1.0 under public ownership.
    pub public_subsidy_fraction: f64,
}

/// Transport ownership model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransportOwnership {
    /// JST operates transport; tickets are subsidized.
    #[default]
    Public,
    /// Transport is sold to private operators; market pricing.
    Privatized,
}

/// A worker's commute eligibility to a target region.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommuteOption {
    /// Home region ID (where the worker lives).
    pub home_region_id: String,
    /// Target region ID (where the worker would work).
    pub target_region_id: String,
    /// Distance in km (single-hop edge distance).
    pub distance_km: f64,
    /// PassengerTransport units required per FTE per turn.
    pub commute_cost_units: f64,
    /// Cash ticket price per FTE per turn.
    pub ticket_price: f64,
}

/// Configuration for commuting mechanics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommutingConfig {
    /// PassengerTransport units required per km of commute per FTE.
    pub capacity_per_km: f64,
    /// Average commute frequency per turn (e.g., 2 = round trip daily).
    pub commute_frequency: f64,
    /// Base ticket price per PassengerTransport unit (before subsidy).
    pub base_ticket_price: f64,
    /// Fraction of ticket price subsidized by JST for public transport (0.0–1.0).
    pub public_subsidy_fraction: f64,
}

impl Default for CommutingConfig {
    fn default() -> Self {
        Self {
            capacity_per_km: 0.1,
            commute_frequency: 2.0,
            base_ticket_price: 5.0,
            public_subsidy_fraction: 0.8, // 80% subsidized by default
        }
    }
}

/// Commuter FTE pool entry — tracks how much FTE a home region can send
/// to a target region, and how much was actually allocated.
#[derive(Debug, Clone, Default)]
pub struct CommuterFteEntry {
    /// Home region of the commuters.
    pub home_region_id: String,
    /// Target (work) region of the commuters.
    pub target_region_id: String,
    /// FTE available from the home region for commuting.
    pub available_fte: f64,
    /// FTE actually allocated to jobs in the target region.
    pub allocated_fte: f64,
    /// Wages earned by commuters (for remittance to home region).
    pub earned_wages: f64,
    /// PassengerTransport units consumed by these commuters.
    pub transport_units_consumed: f64,
}

/// Build commute options for all directly-adjacent region pairs.
///
/// Iterates each region's `edges` and creates a `CommuteOption` for each
/// `LandBorder` edge (workers don't commute via sea). The ticket price
/// accounts for the public subsidy fraction.
///
/// # Rules
/// * Only `LandBorder` edges are considered for commuting (no sea hops).
/// * Bidirectional: if region A borders B, options are created for both
///   A→B and B→A.
/// * The ticket price = `base_ticket_price × (1 - public_subsidy_fraction)`
///   for the subsidized portion.
pub fn build_commute_map(
    regions: &[Region],
    _overlay: &TransportNetworkOverlay,
    config: &CommutingConfig,
) -> Vec<CommuteOption> {
    let mut options: Vec<CommuteOption> = Vec::new();

    for region in regions {
        for edge in &region.edges {
            // Only land borders for commuting (no sea/lane hops).
            if edge.edge_type != EdgeType::LandBorder {
                continue;
            }
            // Skip if the target node is not a land region (could be a sea node).
            if !regions.iter().any(|r| r.id == edge.target_node) {
                continue;
            }

            let commute_cost = edge.distance * config.capacity_per_km * config.commute_frequency;
            let ticket_price = config.base_ticket_price
                * config.commute_frequency
                * (1.0 - config.public_subsidy_fraction);

            options.push(CommuteOption {
                home_region_id: region.id.clone(),
                target_region_id: edge.target_node.clone(),
                distance_km: edge.distance,
                commute_cost_units: commute_cost,
                ticket_price,
            });
        }
    }

    options
}

/// Check if a class can afford the commute ticket.
///
/// # Rules
/// * A class can commute if its per-capita savings >= ticket_price.
/// * Subsidized tickets (public transport) are cheaper, enabling lower-class
///   commuting. Privatized transport with 0% subsidy may exclude them.
pub fn can_afford_commute(class_savings_per_capita: f64, ticket_price: f64) -> bool {
    class_savings_per_capita >= ticket_price
}

/// Compute the total commute demand (PassengerTransport units) for a region
/// as a target (workers wanting to commute INTO this region).
///
/// This is used by `clear_passenger_transport_b2c` to determine how much
/// transport capacity is needed.
pub fn compute_commute_demand_for_target(
    commute_map: &[CommuteOption],
    target_region_id: &str,
    commuter_fte: f64,
) -> f64 {
    commute_map
        .iter()
        .filter(|c| c.target_region_id == target_region_id)
        .map(|c| c.commute_cost_units * commuter_fte)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::{ClimateProfile, Edge, NodeType, Region};
    use std::collections::BTreeMap;
    use std::collections::HashMap;

    fn make_region(id: &str, edges: Vec<Edge>) -> Region {
        Region {
            id: id.to_string(),
            display_name: id.to_string(),
            owner_country: "test".to_string(),
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
            edges,
            land_distribution: BTreeMap::new(),
            class_demographics: Default::default(),
            education: crate::state::macro_data::Education::default(),
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
            geographic_traits: Default::default(),
            coord_x: 0.0,
            coord_y: 0.0,
            development_level: 0.0,
            parcel_ids: Vec::new(),
            is_autonomous_republic: false,
            elevation_difference_m: 0.0,
            thermal_grid: Default::default(),
            local_pollution: Default::default(),
            water_reserves: Default::default(),
            water_network: Default::default(),
            sewer_network: Default::default(),
            waste_grid: Default::default(),
            city_metadata: None,
        }
    }

    fn edge(target: &str, et: EdgeType, dist: f64) -> Edge {
        Edge {
            target_node: target.to_string(),
            edge_type: et,
            distance: dist,
            is_navigable: false,
            territorial_owner: None,
        }
    }

    #[test]
    fn build_commute_map_creates_bidirectional_options() {
        let config = CommutingConfig::default();
        let overlay = TransportNetworkOverlay::default();
        let regions = vec![
            make_region("r1", vec![edge("r2", EdgeType::LandBorder, 50.0)]),
            make_region("r2", vec![edge("r1", EdgeType::LandBorder, 50.0)]),
        ];
        let map = build_commute_map(&regions, &overlay, &config);
        // Should have r1→r2 and r2→r1.
        assert_eq!(map.len(), 2);
        assert!(map
            .iter()
            .any(|c| c.home_region_id == "r1" && c.target_region_id == "r2"));
        assert!(map
            .iter()
            .any(|c| c.home_region_id == "r2" && c.target_region_id == "r1"));
    }

    #[test]
    fn build_commute_map_skips_sea_edges() {
        let config = CommutingConfig::default();
        let overlay = TransportNetworkOverlay::default();
        let regions = vec![
            make_region("r1", vec![edge("r2", EdgeType::Coastline, 200.0)]),
            make_region("r2", vec![edge("r1", EdgeType::Coastline, 200.0)]),
        ];
        let map = build_commute_map(&regions, &overlay, &config);
        assert!(
            map.is_empty(),
            "sea edges should not create commute options"
        );
    }

    #[test]
    fn subsidized_ticket_is_cheaper() {
        let mut config = CommutingConfig::default();
        config.public_subsidy_fraction = 0.8;
        let overlay = TransportNetworkOverlay::default();
        let regions = vec![
            make_region("r1", vec![edge("r2", EdgeType::LandBorder, 50.0)]),
            make_region("r2", vec![edge("r1", EdgeType::LandBorder, 50.0)]),
        ];
        let map = build_commute_map(&regions, &overlay, &config);
        let ticket = map[0].ticket_price;
        // With 80% subsidy, ticket = 5.0 * 2.0 * (1 - 0.8) = 2.0
        assert!((ticket - 2.0).abs() < 1e-9);

        // Now privatized (0% subsidy)
        config.public_subsidy_fraction = 0.0;
        let map2 = build_commute_map(&regions, &overlay, &config);
        let ticket2 = map2[0].ticket_price;
        // Full price: 5.0 * 2.0 * 1.0 = 10.0
        assert!((ticket2 - 10.0).abs() < 1e-9);
        assert!(
            ticket2 > ticket,
            "privatized tickets should be more expensive"
        );
    }

    #[test]
    fn can_afford_commute_checks_savings() {
        assert!(can_afford_commute(100.0, 10.0));
        assert!(!can_afford_commute(5.0, 10.0));
    }
}

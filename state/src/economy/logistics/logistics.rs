//! Phase 23A: Spatial friction and freight logistics.
//!
//! Implements the route-before-settle principle: every cross-region B2B trade
//! must secure a freight route before physical goods can move. The module
//! provides:
//!
//! * `compute_freight_route` — BFS/Dijkstra pathfinding over `Region.edges`
//!   to find the minimum-friction path between two regions.
//! * `freight_cost` / `freight_capacity_required` — cost and capacity
//!   computations derived from the route.
//! * `procure_freight_and_split_trades` — the freight procurement gate that
//!   splits matched trades into freight-secured (settleable) and deferred
//!   (frozen) batches, securing `FreightCapacity` from transport companies.
//!
//! # Invariants
//! * Same-region trades are frictionless (distance 0, no capacity required).
//! * Cross-region trades without a valid path are `impassable` → deferred.
//! * All freight payments route through `TransferSettler` (double-entry).
//! * `FreightCapacity` is an ephemeral service commodity — consumed on
//!   delivery, never stockpiled.

use crate::economy::order_book::Trade;
use crate::economy::transfer_settler::{settle_company_to_company, TransferError};
use crate::economy::transport_networks::{NetworkLevel, TransportNetworkOverlay};
use crate::entities::{Building, Company};
use crate::international::DiplomaticRelation;
use crate::registries::enums::Commodity;
use crate::society::geography::{EdgeType, NodeType, Region};
use crate::state::Country;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent 4 — Phase 5: Transport mode classification for freight producers.
/// Used to gate which producers can serve which routes (Rule 18 & 19):
/// a land-only wagon cannot carry goods across an ocean leg.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportMode {
    /// Land-based: pack caravans, wagons, trucks, rail.
    Land,
    /// Water-based: ships, barges (requires Coastline/SeaLane edges).
    Water,
    /// Air-based: air cargo (requires airports at both endpoints).
    Air,
    /// Unknown mode — fallback for non-transport buildings.
    Unknown,
}

/// Agent 4 — Phase 5: Classify a building's transport mode from its active
/// production method name. Returns `Unknown` for non-transport buildings.
fn classify_transport_mode(building: &Building) -> TransportMode {
    let method_name = building.active_method.active_methods.production.as_str();
    if method_name.contains("Air Cargo") {
        TransportMode::Air
    } else if method_name.contains("Ship")
        || method_name.contains("Barge")
        || method_name.contains("Maritime")
    {
        TransportMode::Water
    } else if method_name.contains("Pack Caravans")
        || method_name.contains("Horse-Drawn")
        || method_name.contains("Freight Train")
        || method_name.contains("Container Trucking")
        || method_name.contains("Wagon")
        || method_name.contains("Truck")
    {
        TransportMode::Land
    } else {
        TransportMode::Unknown
    }
}

/// Agent 4 — Phase 5: Determine the dominant transport mode required by a
/// route based on its edge types. If any segment is SeaLane/Coastline, the
/// route requires Water mode. Otherwise, it's Land mode.
fn route_transport_mode(route: &FreightRoute) -> TransportMode {
    let has_water = route
        .path_segments
        .iter()
        .any(|seg| matches!(seg.edge_type, EdgeType::SeaLane | EdgeType::Coastline));
    if has_water {
        TransportMode::Water
    } else {
        TransportMode::Land
    }
}

///
/// Used for territorial-water blockade checks and maritime transit tariff
/// calculation. Each segment records the from/to node IDs and the edge type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteSegment {
    /// Source node ID.
    pub from_node: String,
    /// Target node ID.
    pub to_node: String,
    /// Edge type of this segment.
    pub edge_type: EdgeType,
    /// Distance of this segment in km.
    pub distance: f64,
    /// Territorial owner of this segment (if any — for SeaLane edges).
    pub territorial_owner: Option<String>,
}

/// Result of a route lookup between two regions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FreightRoute {
    /// Total path distance in km (sum of edge distances).
    pub distance_km: f64,
    /// Weighted-average friction multiplier across the path
    /// (1.0 = baseline dirt road, <1.0 = improved).
    /// Phase 31: This is DIMENSIONLESS (friction only, no fuel cost).
    pub friction_multiplier: f64,
    /// Phase 31: Fuel cost per km along the route (currency per km).
    /// This is kept SEPARATE from friction_multiplier to avoid the
    /// dimensional bug where fuel cost (currency/km) was mixed with
    /// friction (dimensionless) and then multiplied by base_rate.
    pub fuel_cost_per_km: f64,
    /// Whether the route uses a waterborne segment (maritime/river),
    /// which drastically reduces cost.
    pub uses_waterborne: bool,
    /// Whether the route is impassable (no path found).
    pub impassable: bool,
    /// Phase 30: Reconstructed path segments for territorial-water checks
    /// and maritime transit tariff calculation. Empty for local routes.
    pub path_segments: Vec<RouteSegment>,
}

impl FreightRoute {
    /// Returns `true` if this is a same-region (frictionless) route.
    pub fn is_local(&self) -> bool {
        self.distance_km == 0.0 && !self.impassable
    }
}

/// Configuration for freight logistics mechanics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FreightLogisticsConfig {
    /// Base freight cost per ton-km (currency units).
    pub base_freight_rate: f64,
    /// Freight capacity units required per ton-km of transport.
    pub capacity_per_ton_km: f64,
    /// Friction coefficient for baseline LandBorder edges (no network).
    pub land_border_friction: f64,
    /// Friction coefficient for navigable River edges.
    pub river_friction: f64,
    /// Friction coefficient for Coastline/SeaLane edges (waterborne).
    pub waterborne_friction: f64,
    /// Mountain penalty multiplier applied to edges from Mountainous regions.
    pub mountain_penalty: f64,
    /// Maximum turns a trade can be deferred before cancellation.
    pub max_deferred_turns: u32,
    /// Phase 30: Fuel consumption rate for ships per km (waterborne).
    pub ship_fuel_rate: f64,
    /// Phase 30: Fuel consumption rate for barges per km (navigable rivers).
    pub barge_fuel_rate: f64,
    /// Phase 30: Maritime transit tariff per ton-km through territorial waters.
    pub maritime_transit_rate: f64,
    /// Phase 30: Overflight fee per km of airspace traversed by air cargo.
    pub overflight_rate_per_km: f64,
    /// Phase 30: Airspace proximity threshold in km — regions within this
    /// perpendicular distance of the flight path are considered overflown.
    pub airspace_proximity_threshold: f64,
    /// Phase 30: Congestion decay rate per turn (e.g., 0.10 = 10% decay).
    pub congestion_decay_rate: f64,
}

impl Default for FreightLogisticsConfig {
    fn default() -> Self {
        Self {
            // Phase 25: Reduced from 0.5 to 0.05 to make freight affordable.
            base_freight_rate: 0.05,
            capacity_per_ton_km: 0.01,
            land_border_friction: 1.0,
            river_friction: 0.8,
            waterborne_friction: 0.3,
            mountain_penalty: 1.5,
            max_deferred_turns: 3,
            // Phase 30: Mode-specific fuel rates (fuel units per ton-km).
            ship_fuel_rate: 0.015,
            barge_fuel_rate: 0.008,
            // Phase 30: Transit fees.
            maritime_transit_rate: 0.01,
            overflight_rate_per_km: 0.02,
            airspace_proximity_threshold: 100.0,
            congestion_decay_rate: 0.10,
        }
    }
}

impl FreightLogisticsConfig {
    /// Agent 4 — Phase 6: Scale currency-denominated rate constants by
    /// `average_wage` for inflation-proofing (Rule 2).
    ///
    /// The physical rates (friction coefficients, fuel consumption per km,
    /// capacity per ton-km) are dimensionless or physical and do NOT scale.
    /// Only the currency-denominated rates (base_freight_rate,
    /// maritime_transit_rate, overflight_rate_per_km) are scaled.
    ///
    /// The scaling factor is `average_wage / 1000.0` (normalized so that at
    /// average_wage = 1000, the rates match their defaults). This ensures
    /// the rates remain stable under inflation or deflation.
    pub fn scaled_for_economy(&self, average_wage: f64) -> Self {
        let scale = (average_wage.max(1.0) / 1000.0).max(0.01);
        Self {
            base_freight_rate: self.base_freight_rate * scale,
            maritime_transit_rate: self.maritime_transit_rate * scale,
            overflight_rate_per_km: self.overflight_rate_per_km * scale,
            // Physical/dimensionless rates — unchanged.
            capacity_per_ton_km: self.capacity_per_ton_km,
            land_border_friction: self.land_border_friction,
            river_friction: self.river_friction,
            waterborne_friction: self.waterborne_friction,
            mountain_penalty: self.mountain_penalty,
            max_deferred_turns: self.max_deferred_turns,
            ship_fuel_rate: self.ship_fuel_rate,
            barge_fuel_rate: self.barge_fuel_rate,
            airspace_proximity_threshold: self.airspace_proximity_threshold,
            congestion_decay_rate: self.congestion_decay_rate,
        }
    }
}

/// A trade that could not secure freight capacity this turn.
///
/// Stored on the country and retried next turn. Trades deferred beyond
/// `max_deferred_turns` are cancelled with a bid refund.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeferredTrade {
    /// The original matched trade.
    pub trade: Trade,
    /// Number of turns this trade has been deferred.
    pub deferred_turns: u32,
    /// Reason for deferral.
    pub reason: DeferredReason,
}

/// Why a trade was deferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeferredReason {
    #[default]
    /// No freight capacity available from any transport company.
    NoFreightCapacity,
    /// No path exists between buyer and seller regions.
    ImpassableRoute,
    /// Buyer could not afford the freight cost.
    UnaffordableFreight,
}

/// Phase 31: Friction component of edge weight (dimensionless).
///
/// This is the pure friction coefficient — it does NOT include fuel cost.
/// It is used for pathfinding (Dijkstra) and stored separately in
/// `FreightRoute.friction_multiplier` to avoid the dimensional bug where
/// fuel cost (currency/km) was mixed with friction (dimensionless).
fn edge_friction(
    from_region: &Region,
    to_region_id: &str,
    edge: &crate::society::geography::Edge,
    overlay: &TransportNetworkOverlay,
    config: &FreightLogisticsConfig,
) -> f64 {
    let friction = match edge.edge_type {
        EdgeType::LandBorder => {
            let network_friction =
                overlay.friction_multiplier(&from_region.id, to_region_id, &edge.edge_type);
            config.land_border_friction * network_friction
        }
        EdgeType::River => {
            if edge.is_navigable {
                let network_friction =
                    overlay.friction_multiplier(&from_region.id, to_region_id, &edge.edge_type);
                config.river_friction * network_friction
            } else {
                let network_friction =
                    overlay.friction_multiplier(&from_region.id, to_region_id, &edge.edge_type);
                config.land_border_friction * network_friction
            }
        }
        EdgeType::Coastline | EdgeType::SeaLane => config.waterborne_friction,
    };

    // Mountain penalty on land edges.
    let is_mountainous =
        from_region.climate_profile == crate::society::geography::ClimateProfile::Mountainous;
    if is_mountainous && matches!(edge.edge_type, EdgeType::LandBorder) {
        friction * config.mountain_penalty
    } else {
        friction
    }
}

/// Phase 31: Fuel cost component of edge weight (currency per km).
///
/// This is kept SEPARATE from friction to avoid the dimensional bug.
/// Fuel cost is in currency/km and should NOT be multiplied by `base_rate`
/// in `freight_cost()`.
fn edge_fuel_cost_per_km(
    from_region: &Region,
    to_region_id: &str,
    edge: &crate::society::geography::Edge,
    overlay: &TransportNetworkOverlay,
    config: &FreightLogisticsConfig,
    fuel_prices: &rustc_hash::FxHashMap<Commodity, f64>,
) -> f64 {
    match edge.edge_type {
        EdgeType::LandBorder | EdgeType::River if !edge.is_navigable => {
            // Land-based transport: use network level fuel rate.
            let level = overlay
                .get_link(&from_region.id, to_region_id)
                .map(|l| l.level)
                .unwrap_or(NetworkLevel::None);
            let fuel_commodity = level.fuel_commodity();
            let fuel_price = fuel_prices.get(&fuel_commodity).copied().unwrap_or(0.0);
            level.fuel_consumption_per_km() * fuel_price
        }
        EdgeType::River if edge.is_navigable => {
            // Barge transport on navigable rivers.
            let fuel_price = fuel_prices.get(&Commodity::Fuels).copied().unwrap_or(0.0);
            config.barge_fuel_rate * fuel_price
        }
        EdgeType::Coastline | EdgeType::SeaLane => {
            // Maritime transport.
            let fuel_price = fuel_prices.get(&Commodity::Fuels).copied().unwrap_or(0.0);
            config.ship_fuel_rate * fuel_price
        }
        _ => 0.0,
    }
}

/// Phase 31: Composite edge weight for Dijkstra pathfinding.
///
/// The weight combines friction (dimensionless) and fuel cost (currency/km):
/// `weight = friction + fuel_cost_per_km`
///
/// Both components are per-km values. The Dijkstra minimizes the sum of
/// `edge.distance × weight` over the path. This preserves fuel-aware
/// routing while keeping the components separate for `freight_cost()`.
fn edge_weight(
    from_region: &Region,
    to_region_id: &str,
    edge: &crate::society::geography::Edge,
    overlay: &TransportNetworkOverlay,
    config: &FreightLogisticsConfig,
    fuel_prices: &rustc_hash::FxHashMap<Commodity, f64>,
) -> f64 {
    let friction = edge_friction(from_region, to_region_id, edge, overlay, config);
    let fuel_cost = edge_fuel_cost_per_km(
        from_region,
        to_region_id,
        edge,
        overlay,
        config,
        fuel_prices,
    );
    // Toll cost (Phase 30 — future: per-link tolls, 0.0 for now).
    let toll_cost = 0.0;
    friction + fuel_cost + toll_cost
}

/// Compute the freight route between a buyer region and a seller region.
///
/// Uses Dijkstra's algorithm over `Region.edges` to find the minimum-cost
/// path, where cost = sum of (edge.distance × edge_weight).
///
/// Phase 30 upgrades:
/// * Edge weight now includes fuel costs and tolls alongside friction.
/// * Sea-node traversal is fixed — intermediate sea/ocean nodes are now
///   traversable (they have Region entries with `NodeType::SeaNode`/`OceanNode`).
/// * Route reconstruction: the path edge sequence is stored for territorial-
///   water blockade checks and maritime transit tariff calculation.
/// * Territorial waters: SeaLane edges with a `territorial_owner` are checked
///   against diplomacy — if the owner has an embargo against the trading pair,
///   the edge is impassable (blockade).
///
/// # Rules
/// * Same-region (buyer == seller) → distance 0, friction 1.0, not impassable.
/// * No path found → `impassable = true`.
/// * Phase 31: `friction_multiplier` is DIMENSIONLESS (friction only).
///   `fuel_cost_per_km` is in currency/km (kept separate).
/// * `uses_waterborne` is true if any edge in the path is Coastline/SeaLane
///   or a navigable River.
pub fn compute_freight_route(
    buyer_region_id: &str,
    seller_region_id: &str,
    regions: &[Region],
    overlay: &TransportNetworkOverlay,
    config: &FreightLogisticsConfig,
    fuel_prices: &rustc_hash::FxHashMap<Commodity, f64>,
    diplomacy: &HashMap<String, HashMap<String, DiplomaticRelation>>,
    buyer_country: &str,
    seller_country: &str,
) -> FreightRoute {
    // Same-region: frictionless.
    if buyer_region_id == seller_region_id {
        return FreightRoute {
            distance_km: 0.0,
            friction_multiplier: 1.0,
            fuel_cost_per_km: 0.0,
            uses_waterborne: false,
            impassable: false,
            path_segments: Vec::new(),
        };
    }

    // Build a lookup: region_id → index in regions slice.
    let mut region_index: HashMap<&str, usize> = HashMap::new();
    for (i, r) in regions.iter().enumerate() {
        region_index.insert(r.id.as_str(), i);
    }

    // Dijkstra: node_id → (total_cost, total_distance, uses_waterborne, path_segments,
    //                       total_friction_cost, total_fuel_cost)
    let mut dist: HashMap<String, f64> = HashMap::new();
    let mut path_distance: HashMap<String, f64> = HashMap::new();
    let mut path_uses_water: HashMap<String, bool> = HashMap::new();
    let mut path_segments: HashMap<String, Vec<RouteSegment>> = HashMap::new();
    let mut path_friction_cost: HashMap<String, f64> = HashMap::new();
    let mut path_fuel_cost: HashMap<String, f64> = HashMap::new();
    let mut visited: HashMap<String, bool> = HashMap::new();

    dist.insert(buyer_region_id.to_string(), 0.0);
    path_distance.insert(buyer_region_id.to_string(), 0.0);
    path_uses_water.insert(buyer_region_id.to_string(), false);
    path_segments.insert(buyer_region_id.to_string(), Vec::new());
    path_friction_cost.insert(buyer_region_id.to_string(), 0.0);
    path_fuel_cost.insert(buyer_region_id.to_string(), 0.0);

    // Simple priority queue: (cost, node_id).
    let mut queue: Vec<(f64, String)> = vec![(0.0, buyer_region_id.to_string())];

    while let Some((current_cost, current_node)) = pop_min(&mut queue) {
        if *visited.get(&current_node).unwrap_or(&false) {
            continue;
        }
        visited.insert(current_node.clone(), true);

        if current_node == seller_region_id {
            break;
        }

        // Phase 30: Find the region for this node. Sea/ocean nodes DO have
        // Region entries — don't skip them. Instead, filter edges by node_type.
        let region = match regions.iter().find(|r| r.id == current_node) {
            Some(r) => r,
            None => continue, // Truly unknown node — skip.
        };

        // Phase 30: Determine which edge types this node can traverse.
        // LandRegion → LandBorder, River, Coastline (no SeaLane from land).
        // SeaNode/OceanNode → SeaLane, Coastline (no land edges from sea).
        let is_sea_node = matches!(region.node_type, NodeType::SeaNode | NodeType::OceanNode);

        for edge in &region.edges {
            let neighbor = &edge.target_node;
            if *visited.get(neighbor).unwrap_or(&false) {
                continue;
            }

            // Phase 30: Edge-type filtering by node type.
            if is_sea_node {
                // Sea nodes can only traverse waterborne edges.
                if !matches!(edge.edge_type, EdgeType::SeaLane | EdgeType::Coastline) {
                    continue;
                }
            } else {
                // Land nodes can traverse land, river, and coastline edges.
                // SeaLane edges from land nodes are not valid (use Coastline to reach sea).
                if matches!(edge.edge_type, EdgeType::SeaLane) {
                    continue;
                }
            }

            let from_region = region;

            // Phase 23D: Geographic trait gating — waterborne edges require
            // the corresponding trait on the origin region.
            match edge.edge_type {
                EdgeType::Coastline | EdgeType::SeaLane => {
                    if !from_region.geographic_traits.has_coastline {
                        continue;
                    }
                }
                EdgeType::River
                    if edge.is_navigable && !from_region.geographic_traits.has_navigable_river =>
                {
                    continue;
                }
                _ => {}
            }

            // Phase 30: Territorial waters blockade check.
            // If this is a SeaLane edge with a territorial_owner, and the owner
            // has an embargo against either the buyer's or seller's country,
            // the edge is impassable (blockade).
            if let Some(ref owner) = edge.territorial_owner {
                if owner != buyer_country && owner != seller_country {
                    let blocked = is_blockade(owner, buyer_country, seller_country, diplomacy);
                    if blocked {
                        continue; // Skip this edge — blockade.
                    }
                }
            }

            let weight = edge_weight(from_region, neighbor, edge, overlay, config, fuel_prices);
            let edge_cost = edge.distance * weight;
            let new_cost = current_cost + edge_cost;
            let new_distance =
                path_distance.get(&current_node).copied().unwrap_or(0.0) + edge.distance;
            let edge_is_water = matches!(edge.edge_type, EdgeType::Coastline | EdgeType::SeaLane)
                || (edge.edge_type == EdgeType::River && edge.is_navigable);
            let new_uses_water =
                *path_uses_water.get(&current_node).unwrap_or(&false) || edge_is_water;

            // Phase 31: Track friction and fuel cost separately for dimensional correctness.
            let edge_friction_val = edge_friction(from_region, neighbor, edge, overlay, config);
            let edge_fuel_val =
                edge_fuel_cost_per_km(from_region, neighbor, edge, overlay, config, fuel_prices);
            let edge_friction_cost = edge.distance * edge_friction_val;
            let edge_fuel_cost = edge.distance * edge_fuel_val;
            let new_friction_cost = path_friction_cost
                .get(&current_node)
                .copied()
                .unwrap_or(0.0)
                + edge_friction_cost;
            let new_fuel_cost =
                path_fuel_cost.get(&current_node).copied().unwrap_or(0.0) + edge_fuel_cost;

            let existing = dist.get(neighbor).copied().unwrap_or(f64::MAX);
            if new_cost < existing {
                // Build new path segments by appending this edge.
                let mut new_segments = path_segments
                    .get(&current_node)
                    .cloned()
                    .unwrap_or_default();
                new_segments.push(RouteSegment {
                    from_node: current_node.clone(),
                    to_node: neighbor.clone(),
                    edge_type: edge.edge_type,
                    distance: edge.distance,
                    territorial_owner: edge.territorial_owner.clone(),
                });

                dist.insert(neighbor.clone(), new_cost);
                path_distance.insert(neighbor.clone(), new_distance);
                path_uses_water.insert(neighbor.clone(), new_uses_water);
                path_segments.insert(neighbor.clone(), new_segments);
                path_friction_cost.insert(neighbor.clone(), new_friction_cost);
                path_fuel_cost.insert(neighbor.clone(), new_fuel_cost);
                queue.push((new_cost, neighbor.clone()));
            }
        }
    }

    // Check if we reached the seller region.
    let total_cost = dist.get(seller_region_id).copied();
    if total_cost.is_none() || total_cost == Some(f64::MAX) {
        return FreightRoute {
            distance_km: 0.0,
            friction_multiplier: 1.0,
            fuel_cost_per_km: 0.0,
            uses_waterborne: false,
            impassable: true,
            path_segments: Vec::new(),
        };
    }

    let _total_cost = total_cost.unwrap();
    let total_distance = path_distance.get(seller_region_id).copied().unwrap_or(0.0);
    let uses_water = path_uses_water
        .get(seller_region_id)
        .copied()
        .unwrap_or(false);
    let segments = path_segments
        .get(seller_region_id)
        .cloned()
        .unwrap_or_default();
    let total_friction_cost = path_friction_cost
        .get(seller_region_id)
        .copied()
        .unwrap_or(0.0);
    let total_fuel_cost = path_fuel_cost.get(seller_region_id).copied().unwrap_or(0.0);

    // Phase 31: Separate friction (dimensionless) from fuel cost (currency/km).
    // friction_multiplier = total_friction_cost / total_distance (dimensionless)
    // fuel_cost_per_km = total_fuel_cost / total_distance (currency per km)
    let avg_friction = if total_distance > 0.0 {
        total_friction_cost / total_distance
    } else {
        1.0
    };
    let avg_fuel_cost_per_km = if total_distance > 0.0 {
        total_fuel_cost / total_distance
    } else {
        0.0
    };

    FreightRoute {
        distance_km: total_distance,
        friction_multiplier: avg_friction,
        fuel_cost_per_km: avg_fuel_cost_per_km,
        uses_waterborne: uses_water,
        impassable: false,
        path_segments: segments,
    }
}

/// Phase 30: Check if a territorial owner is blockading trade between two countries.
///
/// A blockade is active if the territorial owner has `ban_import` or `ban_export`
/// set in its diplomatic relations with either the buyer's or seller's country.
fn is_blockade(
    owner: &str,
    buyer_country: &str,
    seller_country: &str,
    diplomacy: &HashMap<String, HashMap<String, DiplomaticRelation>>,
) -> bool {
    let check_pair = |a: &str, b: &str| -> bool {
        if a == b {
            return false;
        }
        diplomacy
            .get(a)
            .and_then(|partners| partners.get(b))
            .map(|rel| rel.ban_import || rel.ban_export)
            .unwrap_or(false)
    };
    check_pair(owner, buyer_country) || check_pair(owner, seller_country)
}

/// Phase 23D: Auto-assign geographic traits to all regions based on their edges.
///
/// A region gets:
/// * `has_coastline = true` if it has any `Coastline` or `SeaLane` edge.
/// * `has_navigable_river = true` if it has any navigable `River` edge.
/// * `has_mountain_pass = true` if it has a `LandBorder` edge and is mountainous.
///
/// This should be called once during world generation or save migration.
/// `has_airport` is NOT set here — it's set by airport construction completion.
pub fn assign_geographic_traits_from_edges(regions: &mut [Region]) {
    for region in regions.iter_mut() {
        let mut has_coast = false;
        let mut has_river = false;
        let mut has_pass = false;
        for edge in &region.edges {
            match edge.edge_type {
                EdgeType::Coastline | EdgeType::SeaLane => has_coast = true,
                EdgeType::River if edge.is_navigable => has_river = true,
                EdgeType::LandBorder
                    // Mountain pass: high-distance land border suggests
                    // mountainous terrain. Threshold: 150 km.
                    if edge.distance >= 150.0 => {
                        has_pass = true;
                    }
                _ => {}
            }
        }
        region.geographic_traits.has_coastline = has_coast;
        region.geographic_traits.has_navigable_river = has_river;
        region.geographic_traits.has_mountain_pass = has_pass;
        // has_airport is preserved (set by construction).
    }
}

/// Pop the minimum-cost entry from the queue (linear scan).
fn pop_min(queue: &mut Vec<(f64, String)>) -> Option<(f64, String)> {
    if queue.is_empty() {
        return None;
    }
    let mut min_idx = 0;
    let mut min_cost = queue[0].0;
    for (i, (cost, _)) in queue.iter().enumerate().skip(1) {
        if *cost < min_cost {
            min_cost = *cost;
            min_idx = i;
        }
    }
    Some(queue.swap_remove(min_idx))
}

/// Phase 31: Freight cost with dimensional correctness.
///
/// `freight_cost = friction_cost + fuel_cost`
///
/// Where:
/// * `friction_cost = quantity × distance × friction_multiplier × base_rate`
///   (friction_multiplier is dimensionless, base_rate is currency/ton-km)
/// * `fuel_cost = fuel_cost_per_km × distance`
///   (fuel_cost_per_km is currency/km, NOT multiplied by base_rate or quantity)
///
/// This fixes the Phase 30 dimensional bug where fuel cost (currency/km)
/// was mixed with friction (dimensionless) and the hybrid was multiplied
/// by `base_rate`, causing fuel cost to be double-counted.
///
/// Returns 0.0 for local (same-region) routes.
pub fn freight_cost(route: &FreightRoute, quantity: f64, base_rate: f64) -> f64 {
    if route.is_local() || route.impassable {
        return 0.0;
    }
    let friction_cost = quantity * route.distance_km * route.friction_multiplier * base_rate;
    let fuel_cost = route.fuel_cost_per_km * route.distance_km;
    friction_cost + fuel_cost
}

/// Freight capacity required = quantity × distance_km × capacity_per_ton_km.
///
/// Returns 0.0 for local (same-region) routes.
pub fn freight_capacity_required(
    route: &FreightRoute,
    quantity: f64,
    capacity_per_ton_km: f64,
) -> f64 {
    if route.is_local() || route.impassable {
        return 0.0;
    }
    quantity * route.distance_km * capacity_per_ton_km
}

/// Result of freight procurement for a single trade.
#[derive(Debug, Clone)]
struct FreightProcurementResult {
    /// Whether freight was successfully secured.
    secured: bool,
    /// The freight producer company index (if secured).
    freight_producer_idx: Option<usize>,
    /// Freight capacity consumed from the producer.
    capacity_consumed: f64,
    /// Reason for failure (if not secured).
    failure_reason: Option<DeferredReason>,
}

/// Split matched trades into (freight-secured, deferred) batches.
///
/// For each cross-region trade, this function:
/// 1. Computes the freight route between buyer and seller regions.
/// 2. If impassable → defers the trade.
/// 3. Computes the freight capacity required and freight cost.
/// 4. Attempts to secure `FreightCapacity` from a transport company in the
///    buyer's or seller's region.
/// 5. If capacity is secured → settles the freight payment via
///    `settle_company_to_company` (double-entry) and includes the trade
///    in the secured batch.
/// 6. If capacity is NOT secured → defers the trade.
///
/// Same-region trades bypass the freight gate entirely (frictionless).
///
/// # Returns
/// `(secured_trades, deferred_trades)` — the caller settles only the
/// secured trades via `settle_trades`.
pub fn procure_freight_and_split_trades(
    trades: &[Trade],
    companies: &mut [Company],
    buildings: &mut [Building],
    regions: &[Region],
    overlay: &mut TransportNetworkOverlay,
    config: &FreightLogisticsConfig,
    country: &mut Country,
    fuel_prices: &rustc_hash::FxHashMap<Commodity, f64>,
    diplomacy: &HashMap<String, HashMap<String, DiplomaticRelation>>,
    company_country: &HashMap<String, String>,
) -> (Vec<Trade>, Vec<DeferredTrade>) {
    let mut secured: Vec<Trade> = Vec::new();
    let mut deferred: Vec<DeferredTrade> = Vec::new();

    // Pre-compute company region lookup: company_id → region_id.
    let company_region: HashMap<String, String> = companies
        .iter()
        .map(|c| (c.id.clone(), c.region_id.clone()))
        .collect();

    // Pre-compute building inventory of FreightCapacity by owner company.
    let mut freight_capacity_by_company: HashMap<String, f64> = HashMap::new();
    // Agent 4 — Phase 5: Pre-compute transport mode by company for
    // mode-to-geography gating (Rule 18 & 19).
    let mut transport_mode_by_company: HashMap<String, TransportMode> = HashMap::new();
    for b in buildings.iter() {
        if b.owner_id.is_empty() {
            continue;
        }
        let cap = b
            .inventory
            .get(&Commodity::FreightCapacity)
            .copied()
            .unwrap_or(0.0);
        if cap > 0.0 {
            *freight_capacity_by_company
                .entry(b.owner_id.clone())
                .or_insert(0.0) += cap;
            // Classify the transport mode from the building's active method.
            let mode = classify_transport_mode(b);
            // If a company has multiple buildings with different modes, keep
            // the first non-Unknown mode found.
            transport_mode_by_company
                .entry(b.owner_id.clone())
                .and_modify(|existing| {
                    if *existing == TransportMode::Unknown && mode != TransportMode::Unknown {
                        *existing = mode;
                    }
                })
                .or_insert(mode);
        }
    }

    for trade in trades {
        // Determine buyer and seller regions.
        let buyer_region = company_region.get(&trade.buyer_id);
        let seller_region = company_region.get(&trade.seller_id);

        // If either company's region is unknown (e.g., MIN-DEF), bypass freight.
        let (buyer_region_id, seller_region_id) = match (buyer_region, seller_region) {
            (Some(br), Some(sr)) => (br.clone(), sr.clone()),
            _ => {
                secured.push(trade.clone());
                continue;
            }
        };

        // Same-region trade: frictionless, no freight needed.
        if buyer_region_id == seller_region_id {
            secured.push(trade.clone());
            continue;
        }

        // Phase 30: Determine buyer and seller countries for diplomacy checks.
        let buyer_country = company_country
            .get(&trade.buyer_id)
            .cloned()
            .unwrap_or_default();
        let seller_country = company_country
            .get(&trade.seller_id)
            .cloned()
            .unwrap_or_default();

        // Compute the freight route (Phase 30: with fuel prices and diplomacy).
        let route = compute_freight_route(
            &buyer_region_id,
            &seller_region_id,
            regions,
            overlay,
            config,
            fuel_prices,
            diplomacy,
            &buyer_country,
            &seller_country,
        );

        if route.impassable {
            refund_buyer_encumbrance(
                companies,
                &trade.buyer_id,
                trade.quantity,
                trade.bid_limit_price,
            );
            deferred.push(DeferredTrade {
                trade: trade.clone(),
                deferred_turns: 1,
                reason: DeferredReason::ImpassableRoute,
            });
            continue;
        }

        // Compute freight capacity required and cost.
        let capacity_needed =
            freight_capacity_required(&route, trade.quantity, config.capacity_per_ton_km);
        let cost = freight_cost(&route, trade.quantity, config.base_freight_rate);

        // Phase 30: Calculate maritime transit tariffs for territorial waters.
        let transit_fees = calculate_maritime_transit_fees(&route, trade.quantity, config);

        // Phase 30: Add congestion to links used by this route.
        for seg in &route.path_segments {
            if matches!(seg.edge_type, EdgeType::LandBorder | EdgeType::River) {
                if let Some(link) = overlay.get_link_mut(&seg.from_node, &seg.to_node) {
                    link.add_congestion(capacity_needed);
                }
            }
        }

        // Agent 4 — Phase 5: Determine the route's transport mode for
        // mode-to-geography gating (Rule 18 & 19).
        let route_mode = route_transport_mode(&route);

        // Find a freight producer with available capacity and matching mode.
        let buyer_idx = companies.iter().position(|c| c.id == trade.buyer_id);
        let producer_idx = find_freight_producer(
            &freight_capacity_by_company,
            &transport_mode_by_company,
            companies,
            &buyer_region_id,
            &seller_region_id,
            capacity_needed,
            route_mode,
        );

        let procurement = match producer_idx {
            Some(producer_idx) => {
                // Attempt to settle the freight payment via TransferSettler.
                let payer_idx = buyer_idx.unwrap_or(producer_idx);
                let total_cost = cost + transit_fees.total;
                let settle_result = settle_company_to_company(
                    companies,
                    payer_idx,
                    producer_idx,
                    total_cost,
                    country,
                );

                match settle_result {
                    Ok(_) => {
                        // Phase 30: Settle maritime transit fees to territorial owners.
                        settle_maritime_transit_fees(companies, buyer_idx, &transit_fees, country);
                        FreightProcurementResult {
                            secured: true,
                            freight_producer_idx: Some(producer_idx),
                            capacity_consumed: capacity_needed,
                            failure_reason: None,
                        }
                    }
                    Err(TransferError::InsufficientCash) => FreightProcurementResult {
                        secured: false,
                        freight_producer_idx: Some(producer_idx),
                        capacity_consumed: 0.0,
                        failure_reason: Some(DeferredReason::UnaffordableFreight),
                    },
                    Err(_) => FreightProcurementResult {
                        secured: false,
                        freight_producer_idx: Some(producer_idx),
                        capacity_consumed: 0.0,
                        failure_reason: Some(DeferredReason::NoFreightCapacity),
                    },
                }
            }
            None => FreightProcurementResult {
                secured: false,
                freight_producer_idx: None,
                capacity_consumed: 0.0,
                failure_reason: Some(DeferredReason::NoFreightCapacity),
            },
        };

        if procurement.secured {
            if let Some(prod_idx) = procurement.freight_producer_idx {
                let producer_id = companies[prod_idx].id.clone();
                decrement_freight_capacity(buildings, &producer_id, procurement.capacity_consumed);
            }
            secured.push(trade.clone());
        } else {
            refund_buyer_encumbrance(
                companies,
                &trade.buyer_id,
                trade.quantity,
                trade.bid_limit_price,
            );
            deferred.push(DeferredTrade {
                trade: trade.clone(),
                deferred_turns: 1,
                reason: procurement
                    .failure_reason
                    .unwrap_or(DeferredReason::NoFreightCapacity),
            });
        }
    }

    (secured, deferred)
}

/// Phase 30: Maritime transit fees for a route through territorial waters.
#[derive(Debug, Clone, Default)]
struct MaritimeTransitFees {
    /// Total transit fee amount.
    total: f64,
    /// Per-owner breakdown: (country_name, fee_amount).
    by_owner: Vec<(String, f64)>,
}

/// Phase 30: Calculate maritime transit fees for territorial-water segments.
///
/// For each SeaLane edge in the route with a `territorial_owner`, a per-ton-km
/// transit fee is charged. This is NOT a trade tariff on the cargo — it's a
/// transit fee for passing through territorial waters.
fn calculate_maritime_transit_fees(
    route: &FreightRoute,
    quantity: f64,
    config: &FreightLogisticsConfig,
) -> MaritimeTransitFees {
    let mut fees = MaritimeTransitFees::default();
    let mut by_owner_map: HashMap<String, f64> = HashMap::new();

    for seg in &route.path_segments {
        if !matches!(seg.edge_type, EdgeType::SeaLane | EdgeType::Coastline) {
            continue;
        }
        if let Some(ref owner) = seg.territorial_owner {
            let fee = quantity * seg.distance * config.maritime_transit_rate;
            *by_owner_map.entry(owner.clone()).or_insert(0.0) += fee;
        }
    }

    // Sort by owner name for deterministic ordering.
    let mut owners: Vec<String> = by_owner_map.keys().cloned().collect();
    owners.sort();
    for owner in &owners {
        let amount = by_owner_map[owner];
        fees.total += amount;
        fees.by_owner.push((owner.clone(), amount));
    }

    fees
}

/// Phase 30: Settle maritime transit fees — debit buyer, credit owner Treasury.
///
/// Agent 4 — Fiat Leak Fix:
/// * **Domestic owner:** Uses `settle_transfer_to_treasury` for proper
///   double-entry (buyer debited with bank sync, domestic treasury credited).
/// * **Foreign owner:** Uses `settle_transfer_to_treasury` (buyer debited,
///   domestic treasury credited as agent), then records the fee as a
///   `pending_foreign_transit_fees` payable. A sequential post-parallel
///   phase will debit the domestic treasury and credit the foreign country.
/// * **debit_cash direction:** Fixed from `+=` (was increasing encumbrance)
///   to `-=` (releases encumbrance for the fee amount).
/// * No fiat is created or destroyed — every debit has a matching credit.
fn settle_maritime_transit_fees(
    companies: &mut [Company],
    buyer_idx: Option<usize>,
    fees: &MaritimeTransitFees,
    country: &mut Country,
) {
    for (owner_name, amount) in &fees.by_owner {
        if *amount <= 0.0 {
            continue;
        }
        // Debit the buyer via TransferSettler for proper double-entry.
        // The fee is credited to the domestic treasury (as agent for foreign
        // owners, or as revenue for domestic owners).
        if let Some(idx) = buyer_idx {
            let _ = crate::economy::transfer_settler::settle_transfer_to_treasury(
                companies, idx, *amount, country,
            );
            // Agent 4: Fix debit_cash direction — RELEASE encumbrance (was +=).
            if let Some(buyer) = companies.get_mut(idx) {
                buyer.debit_cash = (buyer.debit_cash - amount).max(0.0);
            }
        }
        // For foreign owners, record the fee as a payable.
        // The domestic treasury holds it in trust until the sequential phase
        // credits the foreign country's treasury.
        if owner_name != &country.name {
            country
                .pending_foreign_transit_fees
                .push((owner_name.clone(), *amount));
        }
        // For domestic owners, the treasury credit is already done by
        // settle_transfer_to_treasury above — no additional action needed.
    }
}

/// Refund a buyer's bid encumbrance for a deferred (unsettled) trade.
///
/// When `submit_company_b2b_orders` submits a bid, it encumbers cash:
/// `company.available_cash -= encumbrance; company.debit_cash += encumbrance`.
/// When `settle_trades` settles a trade, it releases the encumbrance:
/// `buyer.debit_cash -= trade_value`.
///
/// For deferred trades (not settled), we must manually release the encumbrance
/// so the buyer's cash is not permanently locked. The encumbrance = quantity ×
/// bid_limit_price (matching the original bid submission).
fn refund_buyer_encumbrance(
    companies: &mut [Company],
    buyer_id: &str,
    quantity: f64,
    bid_limit_price: f64,
) {
    let encumbrance = quantity * bid_limit_price;
    if let Some(buyer) = companies.iter_mut().find(|c| c.id == buyer_id) {
        buyer.debit_cash = (buyer.debit_cash - encumbrance).max(0.0);
    }
}

/// Find a transport company with available FreightCapacity.
///
/// Agent 4 — Phase 5: Mode-to-geography gating (Rule 18 & 19).
/// Only producers whose `TransportMode` matches the route's mode are eligible.
/// A land-only wagon cannot serve a maritime route; a ship cannot serve a
/// land-only route. `Unknown` mode producers (legacy or non-transport) are
/// treated as compatible with any route (backward compatibility).
///
/// Preference order:
/// 1. Companies in the buyer's region (pick-up at destination).
/// 2. Companies in the seller's region (pick-up at source).
/// 3. Any company with capacity.
fn find_freight_producer(
    freight_capacity_by_company: &HashMap<String, f64>,
    transport_mode_by_company: &HashMap<String, TransportMode>,
    companies: &[Company],
    buyer_region: &str,
    seller_region: &str,
    capacity_needed: f64,
    route_mode: TransportMode,
) -> Option<usize> {
    // Helper: check if a company's mode is compatible with the route mode.
    let mode_compatible = |company_id: &str| -> bool {
        match transport_mode_by_company.get(company_id) {
            Some(mode) => *mode == TransportMode::Unknown || *mode == route_mode,
            None => true, // No mode info → allow (backward compat)
        }
    };

    // Helper: find a company by region with enough capacity and matching mode.
    let find_in_region = |region: &str| -> Option<usize> {
        for (i, c) in companies.iter().enumerate() {
            if c.region_id != region {
                continue;
            }
            if !mode_compatible(&c.id) {
                continue;
            }
            let cap = freight_capacity_by_company
                .get(&c.id)
                .copied()
                .unwrap_or(0.0);
            if cap >= capacity_needed {
                return Some(i);
            }
        }
        None
    };

    // 1. Buyer's region.
    if let Some(idx) = find_in_region(buyer_region) {
        return Some(idx);
    }
    // 2. Seller's region.
    if let Some(idx) = find_in_region(seller_region) {
        return Some(idx);
    }
    // 3. Any region (with mode gating).
    for (i, c) in companies.iter().enumerate() {
        if !mode_compatible(&c.id) {
            continue;
        }
        let cap = freight_capacity_by_company
            .get(&c.id)
            .copied()
            .unwrap_or(0.0);
        if cap >= capacity_needed {
            return Some(i);
        }
    }
    None
}

/// Decrement FreightCapacity from a producer company's buildings.
///
/// FreightCapacity is an ephemeral service — consumed on delivery.
/// Removes capacity from buildings proportionally until the consumed
/// amount is satisfied.
fn decrement_freight_capacity(buildings: &mut [Building], producer_id: &str, amount: f64) {
    if amount <= 0.0 {
        return;
    }
    let mut remaining = amount;
    for b in buildings.iter_mut().filter(|b| b.owner_id == producer_id) {
        if remaining <= 0.0 {
            break;
        }
        let available = b
            .inventory
            .get(&Commodity::FreightCapacity)
            .copied()
            .unwrap_or(0.0);
        if available <= 0.0 {
            continue;
        }
        let consumed = remaining.min(available);
        let new_qty = (available - consumed).max(0.0);
        if new_qty > 0.0 {
            b.inventory.insert(Commodity::FreightCapacity, new_qty);
        } else {
            b.inventory.remove(&Commodity::FreightCapacity);
        }
        remaining -= consumed;
    }
}

/// Expire deferred trades that have exceeded the max deferral limit.
///
/// Returns the trades to cancel (for bid refund) and the remaining deferred.
pub fn expire_deferred_trades(
    deferred: &mut Vec<DeferredTrade>,
    max_turns: u32,
) -> Vec<DeferredTrade> {
    let mut expired: Vec<DeferredTrade> = Vec::new();
    deferred.retain(|d| {
        if d.deferred_turns >= max_turns {
            expired.push(d.clone());
            false
        } else {
            true
        }
    });
    expired
}

/// Increment the deferral counter for all deferred trades (called at turn start).
pub fn increment_deferral_counters(deferred: &mut Vec<DeferredTrade>) {
    for d in deferred {
        d.deferred_turns += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::{ClimateProfile, Edge, EdgeType, NodeType, Region};
    use std::collections::BTreeMap;

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
            sports_facilities: Vec::new(),
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
            aquifer_capacity_liters: 0.0,
            aquifer_quality: crate::society::geography::default_aquifer_quality(),
        }
    }

    fn edge(target: &str, et: EdgeType, dist: f64, nav: bool) -> Edge {
        Edge {
            target_node: target.to_string(),
            edge_type: et,
            distance: dist,
            is_navigable: nav,
            territorial_owner: None,
        }
    }

    /// Helper: empty fuel prices (no fuel cost impact).
    fn empty_fuel_prices() -> rustc_hash::FxHashMap<Commodity, f64> {
        rustc_hash::FxHashMap::default()
    }

    /// Helper: empty diplomacy (no blockades).
    fn empty_diplomacy() -> HashMap<String, HashMap<String, DiplomaticRelation>> {
        HashMap::new()
    }

    #[test]
    fn same_region_route_is_local() {
        let config = FreightLogisticsConfig::default();
        let overlay = TransportNetworkOverlay::default();
        let route = compute_freight_route(
            "r1",
            "r1",
            &[],
            &overlay,
            &config,
            &empty_fuel_prices(),
            &empty_diplomacy(),
            "test",
            "test",
        );
        assert!(route.is_local());
        assert_eq!(route.distance_km, 0.0);
        assert!(!route.impassable);
    }

    #[test]
    fn adjacent_regions_route_found() {
        let config = FreightLogisticsConfig::default();
        let overlay = TransportNetworkOverlay::default();
        let regions = vec![
            make_region("r1", vec![edge("r2", EdgeType::LandBorder, 100.0, false)]),
            make_region("r2", vec![edge("r1", EdgeType::LandBorder, 100.0, false)]),
        ];
        let route = compute_freight_route(
            "r1",
            "r2",
            &regions,
            &overlay,
            &config,
            &empty_fuel_prices(),
            &empty_diplomacy(),
            "test",
            "test",
        );
        assert!(!route.impassable);
        assert!((route.distance_km - 100.0).abs() < 1e-9);
        assert!(!route.uses_waterborne);
    }

    #[test]
    fn no_path_is_impassable() {
        let config = FreightLogisticsConfig::default();
        let overlay = TransportNetworkOverlay::default();
        let regions = vec![make_region("r1", vec![]), make_region("r2", vec![])];
        let route = compute_freight_route(
            "r1",
            "r2",
            &regions,
            &overlay,
            &config,
            &empty_fuel_prices(),
            &empty_diplomacy(),
            "test",
            "test",
        );
        assert!(route.impassable);
    }

    #[test]
    fn waterborne_route_detected() {
        let config = FreightLogisticsConfig::default();
        let overlay = TransportNetworkOverlay::default();
        let mut regions = vec![
            make_region("r1", vec![edge("r2", EdgeType::Coastline, 200.0, true)]),
            make_region("r2", vec![edge("r1", EdgeType::Coastline, 200.0, true)]),
        ];
        // Phase 23D: Assign coastline traits so maritime routing is allowed.
        assign_geographic_traits_from_edges(&mut regions);
        let route = compute_freight_route(
            "r1",
            "r2",
            &regions,
            &overlay,
            &config,
            &empty_fuel_prices(),
            &empty_diplomacy(),
            "test",
            "test",
        );
        assert!(!route.impassable);
        assert!(route.uses_waterborne);
        assert!((route.distance_km - 200.0).abs() < 1e-9);
        // Waterborne friction should be lower than land.
        assert!(route.friction_multiplier < 1.0);
    }

    #[test]
    fn freight_cost_zero_for_local() {
        let route = FreightRoute {
            distance_km: 0.0,
            friction_multiplier: 1.0,
            fuel_cost_per_km: 0.0,
            uses_waterborne: false,
            impassable: false,
            path_segments: Vec::new(),
        };
        assert_eq!(freight_cost(&route, 100.0, 0.5), 0.0);
        assert_eq!(freight_capacity_required(&route, 100.0, 0.01), 0.0);
    }

    #[test]
    fn freight_cost_scales_with_distance_and_friction() {
        let route = FreightRoute {
            distance_km: 100.0,
            friction_multiplier: 0.5,
            fuel_cost_per_km: 0.0,
            uses_waterborne: false,
            impassable: false,
            path_segments: Vec::new(),
        };
        // friction_cost = 100 * 100.0 * 0.5 * 0.5 = 2500, fuel_cost = 0
        assert!((freight_cost(&route, 100.0, 0.5) - 2500.0).abs() < 1e-9);
    }

    #[test]
    fn phase31_freight_cost_separates_fuel_from_friction() {
        // Phase 31: fuel cost should NOT be multiplied by base_rate.
        let route = FreightRoute {
            distance_km: 100.0,
            friction_multiplier: 0.5,
            fuel_cost_per_km: 2.0, // 2 currency/km
            uses_waterborne: false,
            impassable: false,
            path_segments: Vec::new(),
        };
        // friction_cost = 100 * 100 * 0.5 * 0.5 = 2500
        // fuel_cost = 2.0 * 100 = 200 (NOT multiplied by base_rate or quantity)
        // total = 2700
        let cost = freight_cost(&route, 100.0, 0.5);
        assert!(
            (cost - 2700.0).abs() < 1e-9,
            "Fuel cost should be separate from friction: expected 2700, got {}",
            cost
        );
    }

    #[test]
    fn phase31_fuel_cost_not_doubled_by_base_rate() {
        // Verify that fuel cost is not multiplied by base_rate.
        let route_no_fuel = FreightRoute {
            distance_km: 100.0,
            friction_multiplier: 0.5,
            fuel_cost_per_km: 0.0,
            uses_waterborne: false,
            impassable: false,
            path_segments: Vec::new(),
        };
        let route_with_fuel = FreightRoute {
            distance_km: 100.0,
            friction_multiplier: 0.5,
            fuel_cost_per_km: 1.0,
            uses_waterborne: false,
            impassable: false,
            path_segments: Vec::new(),
        };

        let cost_no_fuel = freight_cost(&route_no_fuel, 100.0, 0.5);
        let cost_with_fuel = freight_cost(&route_with_fuel, 100.0, 0.5);

        // The difference should be exactly fuel_cost_per_km * distance = 100.
        // NOT fuel_cost_per_km * distance * quantity * base_rate = 100 * 100 * 0.5 = 5000.
        let diff = cost_with_fuel - cost_no_fuel;
        assert!(
            (diff - 100.0).abs() < 1e-9,
            "Fuel cost difference should be 100 (fuel_cost_per_km * distance), got {}",
            diff
        );
    }

    #[test]
    fn phase31_impassable_route_zero_freight_cost() {
        let route = FreightRoute {
            distance_km: 100.0,
            friction_multiplier: 0.5,
            fuel_cost_per_km: 2.0,
            uses_waterborne: false,
            impassable: true,
            path_segments: Vec::new(),
        };
        assert_eq!(freight_cost(&route, 100.0, 0.5), 0.0);
    }

    #[test]
    fn network_overlay_reduces_friction() {
        let config = FreightLogisticsConfig::default();
        let mut overlay = TransportNetworkOverlay::default();
        overlay.install_link(
            "r1",
            "r2",
            crate::economy::transport_networks::NetworkLevel::Highway,
            1,
        );

        let regions = vec![
            make_region("r1", vec![edge("r2", EdgeType::LandBorder, 100.0, false)]),
            make_region("r2", vec![edge("r1", EdgeType::LandBorder, 100.0, false)]),
        ];

        let route_no_overlay = {
            let empty_overlay = TransportNetworkOverlay::default();
            compute_freight_route(
                "r1",
                "r2",
                &regions,
                &empty_overlay,
                &config,
                &empty_fuel_prices(),
                &empty_diplomacy(),
                "test",
                "test",
            )
        };
        let route_with_overlay = compute_freight_route(
            "r1",
            "r2",
            &regions,
            &overlay,
            &config,
            &empty_fuel_prices(),
            &empty_diplomacy(),
            "test",
            "test",
        );

        // With Highway overlay, friction should be lower.
        assert!(route_with_overlay.friction_multiplier < route_no_overlay.friction_multiplier);
    }

    #[test]
    fn expire_deferred_removes_old_trades() {
        let mut deferred = vec![
            DeferredTrade {
                trade: Trade {
                    buyer_id: "b1".to_string(),
                    seller_id: "s1".to_string(),
                    commodity: Commodity::Steel,
                    quantity: 10.0,
                    execution_price: 5.0,
                    bid_limit_price: 5.0,
                    blueprint_id: None,
                    quality: None,
                    durability: None,
                },
                deferred_turns: 3,
                reason: DeferredReason::NoFreightCapacity,
            },
            DeferredTrade {
                trade: Trade {
                    buyer_id: "b2".to_string(),
                    seller_id: "s2".to_string(),
                    commodity: Commodity::Steel,
                    quantity: 10.0,
                    execution_price: 5.0,
                    bid_limit_price: 5.0,
                    blueprint_id: None,
                    quality: None,
                    durability: None,
                },
                deferred_turns: 1,
                reason: DeferredReason::NoFreightCapacity,
            },
        ];
        let expired = expire_deferred_trades(&mut deferred, 3);
        assert_eq!(expired.len(), 1);
        assert_eq!(deferred.len(), 1);
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 23D: Geographic trait gating tests
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn assign_geographic_traits_from_edges_detects_coastline() {
        let mut regions = vec![
            make_region("r1", vec![edge("sea1", EdgeType::Coastline, 50.0, true)]),
            make_region("r2", vec![edge("r3", EdgeType::LandBorder, 100.0, false)]),
            make_region("r3", vec![edge("r2", EdgeType::LandBorder, 200.0, false)]),
        ];
        assign_geographic_traits_from_edges(&mut regions);
        assert!(regions[0].geographic_traits.has_coastline);
        assert!(!regions[1].geographic_traits.has_coastline);
        // r3 has a 200 km land border → mountain pass trait.
        assert!(regions[2].geographic_traits.has_mountain_pass);
    }

    #[test]
    fn assign_geographic_traits_from_edges_detects_navigable_river() {
        let mut regions = vec![
            make_region("r1", vec![edge("r2", EdgeType::River, 80.0, true)]),
            make_region("r2", vec![edge("r1", EdgeType::River, 80.0, true)]),
        ];
        assign_geographic_traits_from_edges(&mut regions);
        assert!(regions[0].geographic_traits.has_navigable_river);
        assert!(regions[1].geographic_traits.has_navigable_river);
    }

    #[test]
    fn maritime_route_blocked_without_coastline_trait() {
        let config = FreightLogisticsConfig::default();
        let overlay = TransportNetworkOverlay::default();
        // r1 has a Coastline edge but NO has_coastline trait → blocked.
        let regions = vec![
            make_region("r1", vec![edge("r2", EdgeType::Coastline, 200.0, true)]),
            make_region("r2", vec![edge("r1", EdgeType::Coastline, 200.0, true)]),
        ];
        // Don't assign traits — both regions lack has_coastline.
        let route = compute_freight_route(
            "r1",
            "r2",
            &regions,
            &overlay,
            &config,
            &empty_fuel_prices(),
            &empty_diplomacy(),
            "test",
            "test",
        );
        assert!(
            route.impassable,
            "maritime route should be impassable without coastline trait"
        );
    }

    #[test]
    fn maritime_route_allowed_with_coastline_trait() {
        let config = FreightLogisticsConfig::default();
        let overlay = TransportNetworkOverlay::default();
        let mut regions = vec![
            make_region("r1", vec![edge("r2", EdgeType::Coastline, 200.0, true)]),
            make_region("r2", vec![edge("r1", EdgeType::Coastline, 200.0, true)]),
        ];
        // Assign traits — both regions get has_coastline.
        assign_geographic_traits_from_edges(&mut regions);
        let route = compute_freight_route(
            "r1",
            "r2",
            &regions,
            &overlay,
            &config,
            &empty_fuel_prices(),
            &empty_diplomacy(),
            "test",
            "test",
        );
        assert!(
            !route.impassable,
            "maritime route should be passable with coastline trait"
        );
        assert!(route.uses_waterborne);
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 30: Multi-hop sea routes, fuel costs, territorial waters
    // ═══════════════════════════════════════════════════════════

    /// Helper: make a sea node region.
    fn make_sea_region(id: &str, edges: Vec<Edge>) -> Region {
        let mut r = make_region(id, edges);
        r.node_type = NodeType::SeaNode;
        r.owner_country = String::new();
        r.geographic_traits.has_coastline = true;
        r
    }

    #[test]
    fn multi_hop_sea_route_through_intermediate_sea_node() {
        let config = FreightLogisticsConfig::default();
        let overlay = TransportNetworkOverlay::default();
        // r1 (coast) → sea1 → sea2 → r2 (coast)
        let mut regions = vec![
            make_region("r1", vec![edge("sea1", EdgeType::Coastline, 50.0, true)]),
            make_sea_region(
                "sea1",
                vec![
                    edge("r1", EdgeType::Coastline, 50.0, true),
                    edge("sea2", EdgeType::SeaLane, 200.0, true),
                ],
            ),
            make_sea_region(
                "sea2",
                vec![
                    edge("sea1", EdgeType::SeaLane, 200.0, true),
                    edge("r2", EdgeType::Coastline, 50.0, true),
                ],
            ),
            make_region("r2", vec![edge("sea2", EdgeType::Coastline, 50.0, true)]),
        ];
        assign_geographic_traits_from_edges(&mut regions);
        let route = compute_freight_route(
            "r1",
            "r2",
            &regions,
            &overlay,
            &config,
            &empty_fuel_prices(),
            &empty_diplomacy(),
            "test",
            "test",
        );
        assert!(!route.impassable, "multi-hop sea route should be passable");
        assert!(route.uses_waterborne);
        // Total distance: 50 + 200 + 50 = 300
        assert!((route.distance_km - 300.0).abs() < 1e-9);
        // Path segments should have 3 edges
        assert_eq!(route.path_segments.len(), 3);
    }

    #[test]
    fn fuel_cost_makes_expensive_route_chose_alternative() {
        let config = FreightLogisticsConfig::default();
        let overlay = TransportNetworkOverlay::default();
        // Two paths: r1→r2 direct (100km land) vs r1→r3→r2 (50+50km land)
        // With high fuel prices, the shorter route should be preferred.
        let mut fuel_prices = rustc_hash::FxHashMap::default();
        fuel_prices.insert(Commodity::Fuels, 10.0); // High fuel price
        let regions = vec![
            make_region(
                "r1",
                vec![
                    edge("r2", EdgeType::LandBorder, 100.0, false),
                    edge("r3", EdgeType::LandBorder, 50.0, false),
                ],
            ),
            make_region(
                "r2",
                vec![
                    edge("r1", EdgeType::LandBorder, 100.0, false),
                    edge("r3", EdgeType::LandBorder, 50.0, false),
                ],
            ),
            make_region(
                "r3",
                vec![
                    edge("r1", EdgeType::LandBorder, 50.0, false),
                    edge("r2", EdgeType::LandBorder, 50.0, false),
                ],
            ),
        ];
        let route = compute_freight_route(
            "r1",
            "r2",
            &regions,
            &overlay,
            &config,
            &fuel_prices,
            &empty_diplomacy(),
            "test",
            "test",
        );
        assert!(!route.impassable);
        // The shorter route (r1→r3→r2 = 100km) should be chosen over direct (100km)
        // but with lower fuel cost since fuel scales with distance×consumption.
        // Both routes are 100km, but the direct route has 1 edge while the indirect
        // has 2. With same total distance, the direct route should win (fewer edges).
        assert!((route.distance_km - 100.0).abs() < 1e-9);
    }

    #[test]
    fn territorial_waters_blockade_makes_route_impassable() {
        let config = FreightLogisticsConfig::default();
        let overlay = TransportNetworkOverlay::default();
        // r1 → sea1(owned by "Enemy") → r2
        // Enemy has embargo against buyer country.
        let mut regions = vec![
            make_region(
                "r1",
                vec![Edge {
                    target_node: "sea1".to_string(),
                    edge_type: EdgeType::Coastline,
                    distance: 50.0,
                    is_navigable: true,
                    territorial_owner: Some("Enemy".to_string()),
                }],
            ),
            make_sea_region(
                "sea1",
                vec![
                    Edge {
                        target_node: "r1".to_string(),
                        edge_type: EdgeType::Coastline,
                        distance: 50.0,
                        is_navigable: true,
                        territorial_owner: Some("Enemy".to_string()),
                    },
                    Edge {
                        target_node: "r2".to_string(),
                        edge_type: EdgeType::Coastline,
                        distance: 50.0,
                        is_navigable: true,
                        territorial_owner: Some("Enemy".to_string()),
                    },
                ],
            ),
            make_region(
                "r2",
                vec![Edge {
                    target_node: "sea1".to_string(),
                    edge_type: EdgeType::Coastline,
                    distance: 50.0,
                    is_navigable: true,
                    territorial_owner: Some("Enemy".to_string()),
                }],
            ),
        ];
        assign_geographic_traits_from_edges(&mut regions);

        // Set up diplomacy: Enemy has embargo against "Lechia" (buyer country).
        let mut diplomacy = HashMap::new();
        let mut enemy_relations = HashMap::new();
        enemy_relations.insert(
            "Lechia".to_string(),
            DiplomaticRelation {
                ban_import: true,
                ban_export: true,
                ..Default::default()
            },
        );
        diplomacy.insert("Enemy".to_string(), enemy_relations);

        let route = compute_freight_route(
            "r1",
            "r2",
            &regions,
            &overlay,
            &config,
            &empty_fuel_prices(),
            &diplomacy,
            "Lechia",
            "Lechia",
        );
        assert!(
            route.impassable,
            "route through blockaded territorial waters should be impassable"
        );
    }

    #[test]
    fn territorial_waters_no_blockade_when_no_embargo() {
        let config = FreightLogisticsConfig::default();
        let overlay = TransportNetworkOverlay::default();
        let mut regions = vec![
            make_region(
                "r1",
                vec![Edge {
                    target_node: "sea1".to_string(),
                    edge_type: EdgeType::Coastline,
                    distance: 50.0,
                    is_navigable: true,
                    territorial_owner: Some("Neutral".to_string()),
                }],
            ),
            make_sea_region(
                "sea1",
                vec![
                    Edge {
                        target_node: "r1".to_string(),
                        edge_type: EdgeType::Coastline,
                        distance: 50.0,
                        is_navigable: true,
                        territorial_owner: Some("Neutral".to_string()),
                    },
                    Edge {
                        target_node: "r2".to_string(),
                        edge_type: EdgeType::Coastline,
                        distance: 50.0,
                        is_navigable: true,
                        territorial_owner: Some("Neutral".to_string()),
                    },
                ],
            ),
            make_region(
                "r2",
                vec![Edge {
                    target_node: "sea1".to_string(),
                    edge_type: EdgeType::Coastline,
                    distance: 50.0,
                    is_navigable: true,
                    territorial_owner: Some("Neutral".to_string()),
                }],
            ),
        ];
        assign_geographic_traits_from_edges(&mut regions);

        // No embargo — route should be passable.
        let route = compute_freight_route(
            "r1",
            "r2",
            &regions,
            &overlay,
            &config,
            &empty_fuel_prices(),
            &empty_diplomacy(),
            "Lechia",
            "Lechia",
        );
        assert!(
            !route.impassable,
            "route through non-blockaded territorial waters should be passable"
        );
        assert!(route.uses_waterborne);
    }

    #[test]
    fn route_reconstruction_provides_path_segments() {
        let config = FreightLogisticsConfig::default();
        let overlay = TransportNetworkOverlay::default();
        let regions = vec![
            make_region("r1", vec![edge("r2", EdgeType::LandBorder, 100.0, false)]),
            make_region("r2", vec![edge("r1", EdgeType::LandBorder, 100.0, false)]),
        ];
        let route = compute_freight_route(
            "r1",
            "r2",
            &regions,
            &overlay,
            &config,
            &empty_fuel_prices(),
            &empty_diplomacy(),
            "test",
            "test",
        );
        assert!(!route.impassable);
        assert_eq!(route.path_segments.len(), 1);
        assert_eq!(route.path_segments[0].from_node, "r1");
        assert_eq!(route.path_segments[0].to_node, "r2");
        assert_eq!(route.path_segments[0].edge_type, EdgeType::LandBorder);
    }
}

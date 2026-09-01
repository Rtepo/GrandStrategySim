//! Phase 81: Grid topology, transmission physics, and power flow distribution.
//!
//! Implements the three-tier grid model:
//! - HV lines with explicit inter-region topology and DC flow balancing.
//! - MV/LV as abstract regional capacity limits.
//! - Transmission losses based on distance and line condition.
//! - Overproduction handling (industrial buff, curtailment, grid damage).
//! - Load shedding integration.

#![allow(missing_docs)]

use crate::economy::production::weather::get_region_weather_modifier;
use crate::economy::production::weather::WeatherState;
use crate::energy::generation::{compute_marginal_cost, weather_output_multiplier};
use crate::energy::types::*;
use crate::entities::Building;
use crate::registries::enums::{Commodity, Sector};
use crate::society::geography::{EdgeType, Region};
use crate::society::housing::{CommercialBuilding, HousingBuilding};
use crate::state::{Country, Season};

use rand::Rng;
use std::collections::HashMap;

/// Base loss rate per km for HV transmission lines.
/// 5% per 1000 km at perfect condition — physically realistic for ACSR
/// conductors at 220-400 kV.
const HV_BASE_LOSS_RATE: f64 = 0.00005;

/// Calculate transmission loss for an HV line based on distance and condition.
///
/// At condition=1.0: 5% per 1000 km.
/// At condition=0.5: 10% per 1000 km (degraded lines lose proportionally more).
/// Capped at 50% for extremely degraded long-distance lines.
pub fn transmission_loss(line: &GridLine) -> f64 {
    let condition_factor = 1.0 / line.condition.max(0.1);
    let base_loss = line.distance_km * HV_BASE_LOSS_RATE * condition_factor;
    base_loss.min(0.50)
}

/// Initialize the power grid during world generation.
///
/// Creates HV lines between adjacent regions based on the start year (era-scaled
/// topology) and calculates LV/MV capacities from actual connected building demand.
///
/// # Arguments
/// * `country` - Mutable country (regions and power_grid_state updated).
/// * `housing_buildings` - Housing buildings (for electricity demand aggregation).
/// * `commercial_buildings` - Commercial buildings (for electricity demand aggregation).
/// * `start_year` - Scenario start year, gating HV topology era.
/// * `rng` - Random number generator for condition variance.
///
/// # Rules
/// * Pre-1920: No HV lines (island grids).
/// * 1920-1950: HV only from capital to neighbors (hub-and-spoke).
/// * 1950-1975: HV between all adjacent regions.
/// * Post-1975: Full HV mesh with higher capacities.
/// * Bugfix Sprint (5B): LV/MV capacities are derived from the actual connected
///   housing + commercial electricity demand (kWh/turn → MW) generated during
///   world seeding, multiplied by a 1.2× engineering headroom factor. This
///   replaces the old `pop * dev * wage / 500_000` magic formula (Rule 2/15).
pub fn init_power_grid(
    country: &mut Country,
    housing_buildings: &[HousingBuilding],
    commercial_buildings: &[CommercialBuilding],
    start_year: u32,
    rng: &mut impl Rng,
) {
    let grid = &mut country.power_grid_state;

    // Clear any existing grid state.
    grid.hv_lines.clear();
    grid.region_lv_capacity.clear();
    grid.region_mv_capacity.clear();
    grid.region_lv_condition.clear();
    grid.region_mv_condition.clear();
    grid.spot_prices.clear();
    grid.load_shed_tiers.clear();
    grid.overproduction_tiers.clear();

    // Sort regions by ID for deterministic iteration.
    let mut sorted_regions: Vec<&Region> = country.regions.iter().collect();
    sorted_regions.sort_by(|a, b| a.id.cmp(&b.id));

    // Bugfix Sprint (5B): Aggregate actual connected electricity demand per
    // region from housing + commercial buildings. electricity_capacity is in
    // kWh/turn; convert to MW (1 MWh = 1000 kWh), matching grid.rs:296.
    let mut regional_demand_mw: HashMap<String, f64> = HashMap::new();
    for region in &sorted_regions {
        regional_demand_mw.insert(region.id.clone(), 0.0);
    }
    for hb in housing_buildings {
        if let Some(rid) = sorted_regions
            .iter()
            .find(|r| r.micro_regions.contains_key(&hb.micro_region_id))
            .map(|r| r.id.clone())
        {
            let demand_mw = hb.utility_connections.electricity_capacity / 1000.0;
            *regional_demand_mw.get_mut(&rid).unwrap() += demand_mw;
        }
    }
    for cb in commercial_buildings {
        if let Some(rid) = sorted_regions
            .iter()
            .find(|r| r.micro_regions.contains_key(&cb.micro_region_id))
            .map(|r| r.id.clone())
        {
            let demand_mw = cb.utility_connections.electricity_capacity / 1000.0;
            *regional_demand_mw.get_mut(&rid).unwrap() += demand_mw;
        }
    }

    // Calculate LV/MV capacities for each region from actual demand.
    // Engineering headroom factor: 1.2 (20% safety margin — standard grid
    // design practice, not a magic number).
    const LV_HEADROOM_FACTOR: f64 = 1.2;
    for region in &sorted_regions {
        let actual_demand_mw = regional_demand_mw.get(&region.id).copied().unwrap_or(0.0);
        // LV capacity = max(actual_demand * headroom, 0.1) — floor of 0.1 MW
        // for regions with no connected buildings (pre-electrification).
        let lv_capacity = (actual_demand_mw * LV_HEADROOM_FACTOR).max(0.1);
        let mv_capacity = lv_capacity * 3.0;
        let lv_condition = 0.80 + rng.gen_range(0.0..0.20);
        let mv_condition = 0.80 + rng.gen_range(0.0..0.20);

        grid.region_lv_capacity
            .insert(region.id.clone(), lv_capacity);
        grid.region_mv_capacity
            .insert(region.id.clone(), mv_capacity);
        grid.region_lv_condition
            .insert(region.id.clone(), lv_condition);
        grid.region_mv_condition
            .insert(region.id.clone(), mv_condition);
        grid.spot_prices.insert(region.id.clone(), 0.0);
        grid.load_shed_tiers
            .insert(region.id.clone(), LoadShedTier::Normal);
        grid.overproduction_tiers
            .insert(region.id.clone(), OverproductionTier::Normal);
    }

    // Generate HV lines based on era.
    let (connectivity_mode, capacity_divisor, condition_base) = if start_year < 1920 {
        // Pre-1920: Island grids — no HV lines.
        return;
    } else if start_year < 1950 {
        // 1920-1950: Capital hub-and-spoke only.
        (HvConnectivityMode::CapitalHubSpoke, 50_000.0, 0.70)
    } else if start_year < 1975 {
        // 1950-1975: All adjacent regions.
        (HvConnectivityMode::AllAdjacent, 20_000.0, 0.80)
    } else {
        // Post-1975: Full mesh with higher capacity.
        (HvConnectivityMode::AllAdjacent, 10_000.0, 0.85)
    };

    let country_id = country.name.clone();
    let mut line_counter = 0u32;

    // Build a lookup of region ID → region for population lookups.
    let region_map: HashMap<String, &Region> =
        country.regions.iter().map(|r| (r.id.clone(), r)).collect();

    // Collect candidate edges deterministically (sorted by from_region, to_region).
    let mut candidate_edges: Vec<(String, String, f64)> = Vec::new();

    for region in &sorted_regions {
        if region.node_type != crate::society::geography::NodeType::LandRegion {
            continue;
        }

        match connectivity_mode {
            HvConnectivityMode::None => {
                continue;
            }
            HvConnectivityMode::CapitalHubSpoke => {
                // Only capital connects to its neighbors.
                if !region.is_capital {
                    continue;
                }
                for edge in &region.edges {
                    if edge.edge_type != EdgeType::LandBorder {
                        continue;
                    }
                    if let Some(target) = region_map.get(&edge.target_node) {
                        if target.node_type != crate::society::geography::NodeType::LandRegion {
                            continue;
                        }
                        candidate_edges.push((
                            region.id.clone(),
                            edge.target_node.clone(),
                            edge.distance,
                        ));
                    }
                }
            }
            HvConnectivityMode::AllAdjacent => {
                for edge in &region.edges {
                    if edge.edge_type != EdgeType::LandBorder {
                        continue;
                    }
                    if let Some(target) = region_map.get(&edge.target_node) {
                        if target.node_type != crate::society::geography::NodeType::LandRegion {
                            continue;
                        }
                        // Avoid duplicate edges (A→B and B→A).
                        let (from, to) = if region.id < edge.target_node {
                            (region.id.clone(), edge.target_node.clone())
                        } else {
                            (edge.target_node.clone(), region.id.clone())
                        };
                        let dist = edge.distance;
                        let key = (from.clone(), to.clone());
                        if !candidate_edges
                            .iter()
                            .any(|(f, t, _)| (f.clone(), t.clone()) == key)
                        {
                            candidate_edges.push((from, to, dist));
                        }
                    }
                }
            }
        }
    }

    // Sort candidate edges deterministically.
    candidate_edges.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));

    // Create HV lines from candidate edges.
    for (from_region, to_region, distance) in candidate_edges {
        let from_pop = region_map
            .get(&from_region)
            .map(|r| r.population.max(1) as f64)
            .unwrap_or(1.0);
        let to_pop = region_map
            .get(&to_region)
            .map(|r| r.population.max(1) as f64)
            .unwrap_or(1.0);
        let capacity = from_pop.min(to_pop) / capacity_divisor;
        let condition = condition_base + rng.gen_range(0.0..0.15);

        line_counter += 1;
        grid.hv_lines.push(GridLine {
            id: format!("hv_{}_{}", country_id, line_counter),
            from_region,
            to_region,
            tier: GridTier::Hv,
            capacity_mw: capacity.max(1.0),
            condition,
            distance_km: distance,
            is_interconnector: false,
            owner_country: country_id.clone(),
            current_flow_mw: 0.0,
        });
    }
}

/// HV connectivity mode for era-scaled grid initialization.
#[derive(Clone, Copy)]
#[allow(dead_code)]
enum HvConnectivityMode {
    /// No HV lines (pre-1920 island grids).
    None,
    /// Capital connects to neighbors only (1920-1950).
    CapitalHubSpoke,
    /// All adjacent regions connected (1950+).
    AllAdjacent,
}

/// Collect regional energy production and demand.
///
/// Returns (supply_mw, demand_mw, max_capacity_mw) per region ID.
/// Applies weather modifiers to power plant output based on plant type and cooling.
fn collect_regional_supply_demand(
    buildings: &[Building],
    housing_buildings: &[HousingBuilding],
    commercial_buildings: &[CommercialBuilding],
    regions: &[Region],
    _grid: &PowerGridState,
    weather_state: &crate::economy::production::weather::WeatherState,
) -> (
    HashMap<String, f64>,
    HashMap<String, f64>,
    HashMap<String, f64>,
) {
    let mut supply_mw: HashMap<String, f64> = HashMap::new();
    let mut demand_mw: HashMap<String, f64> = HashMap::new();
    let mut max_capacity_mw: HashMap<String, f64> = HashMap::new();

    // Initialize all regions.
    for region in regions {
        supply_mw.insert(region.id.clone(), 0.0);
        demand_mw.insert(region.id.clone(), 0.0);
        max_capacity_mw.insert(region.id.clone(), 0.0);
    }

    // Collect energy production from Sector::Energy buildings.
    // Apply weather modifiers based on plant type and cooling method.
    for building in buildings {
        if building.sector != Sector::Energy {
            continue;
        }
        let region_id = &building.region_id;
        let energy_in_inventory = building
            .inventory
            .get(&Commodity::Energy)
            .copied()
            .unwrap_or(0.0);

        // Get nameplate capacity from metadata if available.
        let (nameplate, weather_multiplier, has_metadata) =
            if let Some(meta) = get_plant_metadata(building) {
                let weather = get_region_weather_modifier(weather_state, region_id);
                let wm = weather_output_multiplier(
                    meta.plant_type,
                    meta.cooling_type,
                    meta.has_cooling_upgrade,
                    &weather,
                );
                (meta.nameplate_capacity_mw, wm, true)
            } else {
                // Bugfix Sprint (5C): Pre-Phase-81 buildings without metadata.
                // Supply is still estimated from employment so these buildings
                // contribute to grid supply, but they are EXCLUDED from
                // max_capacity_mw so regional and national capacity reconcile.
                (building.current_employment as f64 * 0.5, 1.0, false)
            };

        // Apply weather multiplier to actual output.
        // Bugfix Sprint (5A): Clamp supply to nameplate capacity — supply can
        // never exceed the plant's physical nameplate, preventing "matter from
        // the void" when inventory exceeds nameplate.
        let weather_adjusted_supply = (energy_in_inventory * weather_multiplier).min(nameplate);
        *supply_mw.get_mut(region_id).unwrap_or(&mut 0.0) += weather_adjusted_supply;
        // Bugfix Sprint (5C): Only count buildings with PowerPlantMetadata
        // toward max_capacity_mw, so regional and national capacity reconcile.
        if has_metadata {
            *max_capacity_mw.get_mut(region_id).unwrap_or(&mut 0.0) += nameplate;
        }
    }

    // Collect demand from housing buildings.
    // Housing buildings use micro_region_id, so we need to map them to regions.
    for hb in housing_buildings {
        // Find which region contains this micro_region.
        let region_id = regions
            .iter()
            .find(|r| r.micro_regions.contains_key(&hb.micro_region_id))
            .map(|r| r.id.clone());
        if let Some(rid) = region_id {
            // Electricity capacity is in kWh per turn. Convert to MW (1 MWh = 1000 kWh).
            let demand = hb.utility_connections.electricity_capacity / 1000.0;
            *demand_mw.get_mut(&rid).unwrap_or(&mut 0.0) += demand;
        }
    }

    // Collect demand from commercial buildings.
    for cb in commercial_buildings {
        let region_id = regions
            .iter()
            .find(|r| r.micro_regions.contains_key(&cb.micro_region_id))
            .map(|r| r.id.clone());
        if let Some(rid) = region_id {
            let demand = cb.utility_connections.electricity_capacity / 1000.0;
            *demand_mw.get_mut(&rid).unwrap_or(&mut 0.0) += demand;
        }
    }

    // Also collect demand from industrial buildings (they consume electricity).
    for building in buildings {
        if building.sector == Sector::Energy {
            continue; // Energy buildings produce, not consume (their own consumption is internal).
        }
        let region_id = &building.region_id;
        // Industrial electricity demand is proportional to employment.
        let demand = building.current_employment as f64 * 0.002; // 2 kW per worker
        *demand_mw.get_mut(region_id).unwrap_or(&mut 0.0) += demand;
    }

    (supply_mw, demand_mw, max_capacity_mw)
}

/// Extract `PowerPlantMetadata` from a building's `extra` map.
pub fn get_plant_metadata(building: &Building) -> Option<PowerPlantMetadata> {
    building
        .extra
        .get(PowerPlantMetadata::EXTRA_KEY)
        .and_then(PowerPlantMetadata::from_json)
}

/// Perform DC flow balancing over HV lines.
///
/// Iteratively transfers power from surplus regions to deficit regions via
/// HV lines. Uses sorted iteration for determinism.
///
/// Returns a map of (from_region, to_region) → actual flow in MW.
fn dc_flow_balancing(
    supply: &mut HashMap<String, f64>,
    demand: &HashMap<String, f64>,
    hv_lines: &[GridLine],
) -> HashMap<(String, String), f64> {
    let mut flows: HashMap<(String, String), f64> = HashMap::new();

    // Sort HV lines deterministically by (from_region, to_region).
    let mut sorted_lines: Vec<&GridLine> = hv_lines.iter().collect();
    sorted_lines
        .sort_by(|a, b| (&a.from_region, &a.to_region).cmp(&(&b.from_region, &b.to_region)));

    // Sort region IDs for deterministic iteration.
    let mut sorted_region_ids: Vec<String> = supply.keys().cloned().collect();
    sorted_region_ids.sort();

    // Iterate until convergence (max iterations = number of regions).
    let max_iterations = sorted_region_ids.len().max(1);
    for _ in 0..max_iterations {
        let mut any_transfer = false;

        for region_id in &sorted_region_ids {
            let &local_supply = supply.get(region_id).unwrap_or(&0.0);
            let &local_demand = demand.get(region_id).unwrap_or(&0.0);
            let surplus = local_supply - local_demand;

            if surplus <= 0.0 {
                continue; // No surplus to export.
            }

            // Find HV lines from this region to deficit neighbors.
            for line in &sorted_lines {
                let (from, to) = if line.from_region == *region_id {
                    (&line.from_region, &line.to_region)
                } else if line.to_region == *region_id {
                    (&line.to_region, &line.from_region)
                } else {
                    continue;
                };

                let _ = from; // Already confirmed == region_id.

                let &neighbor_supply = supply.get(to).unwrap_or(&0.0);
                let &neighbor_demand = demand.get(to).unwrap_or(&0.0);
                let neighbor_deficit = neighbor_demand - neighbor_supply;

                if neighbor_deficit <= 0.0 {
                    continue; // Neighbor also has surplus — no transfer (HV Black Hole prevention).
                }

                let loss = transmission_loss(line);
                let max_transfer = surplus.min(neighbor_deficit).min(line.capacity_mw);
                let transfer = max_transfer;
                let received = transfer * (1.0 - loss);

                // Update supply: sender loses transfer, receiver gains received.
                *supply.get_mut(region_id).unwrap() -= transfer;
                *supply.get_mut(to).unwrap_or(&mut 0.0) += received;

                // Record flow.
                let flow_key = (line.from_region.clone(), line.to_region.clone());
                let current = flows.get(&flow_key).copied().unwrap_or(0.0);
                // Flow direction: positive = from_region → to_region.
                let directed_flow = if line.from_region == *region_id {
                    current + transfer
                } else {
                    current - transfer // Reverse direction.
                };
                flows.insert(flow_key, directed_flow);

                any_transfer = true;
                break; // Move to next surplus region after one transfer.
            }
        }

        if !any_transfer {
            break; // Converged.
        }
    }

    flows
}

/// Main grid power distribution function.
///
/// Called in the turn loop after Wave 1 (energy production) and before
/// Wave 3 (general production). Performs:
/// 1. Collects regional supply and demand.
/// 2. DC flow balancing over HV lines.
/// 3. LV/MV capacity checks.
/// 4. Storage absorption.
/// 5. Overproduction handling (industrial buff, curtailment, grid damage).
/// 6. Load shedding calculation.
/// 7. Returns building_efficiency_penalties for Wave 3.
///
/// # Arguments
/// * `country` - Mutable country (power_grid_state updated).
/// * `buildings` - All buildings (read for supply/demand, written for curtailment).
/// * `housing_buildings` - Housing buildings (read for demand).
/// * `commercial_buildings` - Commercial buildings (read for demand).
/// * `season` - Current season (affects demand patterns).
///
/// # Returns
/// `GridDistributionResult` with penalties, supply/demand, and tier info.
pub fn distribute_grid_power(
    country: &mut Country,
    buildings: &mut [Building],
    housing_buildings: &[HousingBuilding],
    commercial_buildings: &[CommercialBuilding],
    _season: Season,
    fuel_prices: &HashMap<Commodity, f64>,
) -> GridDistributionResult {
    let mut result = GridDistributionResult::default();

    // Extract power_grid_state from country to avoid double mutable borrow.
    let mut grid = std::mem::take(&mut country.power_grid_state);

    // Phase 81 Wave 2: Capture average_wage for merit-order spot market clearing.
    let average_wage = country.macro_indicators.average_wage.max(1.0);

    // Phase 81 Wave 2: Clear previous turn's spot market state.
    grid.spot_market.marginal_costs.clear();
    grid.spot_market.clearing_prices.clear();
    grid.spot_market.dispatch_order.clear();
    grid.spot_market.revenue_distribution.clear();
    grid.spot_market.dispatched_mw.clear();

    // Step 1-2: Collect regional supply, demand, and max capacity.
    // Apply weather modifiers to power plant output.
    let weather_state = country.weather_state.clone();
    let (mut supply_mw, demand_mw, max_capacity_mw) = collect_regional_supply_demand(
        buildings,
        housing_buildings,
        commercial_buildings,
        &country.regions,
        &grid,
        &weather_state,
    );

    // Record initial values.
    for region in &country.regions {
        let s = supply_mw.get(&region.id).copied().unwrap_or(0.0);
        let d = demand_mw.get(&region.id).copied().unwrap_or(0.0);
        let m = max_capacity_mw.get(&region.id).copied().unwrap_or(0.0);
        result.region_max_capacity_mw.insert(region.id.clone(), m);
        // Phase 81: Store in grid state for snapshot access.
        grid.region_supply_mw.insert(region.id.clone(), s);
        grid.region_demand_mw.insert(region.id.clone(), d);
        grid.region_max_capacity_mw.insert(region.id.clone(), m);
    }

    // Step 3: DC flow balancing over HV lines.
    let flows = dc_flow_balancing(&mut supply_mw, &demand_mw, &grid.hv_lines);
    result.interconnector_flows = flows;

    // Step 4: Calculate storage absorption (pumped storage + batteries).
    let mut storage_absorbed: HashMap<String, f64> = HashMap::new();
    for building in buildings.iter() {
        if building.sector != Sector::Energy {
            continue;
        }
        if let Some(meta) = get_plant_metadata(building) {
            if meta.plant_type.is_storage() {
                let region_id = &building.region_id;
                let available_capacity = meta.nameplate_capacity_mw;
                let local_surplus = supply_mw.get(region_id).copied().unwrap_or(0.0)
                    - demand_mw.get(region_id).copied().unwrap_or(0.0);
                if local_surplus > 0.0 {
                    let absorbed = local_surplus.min(available_capacity);
                    *storage_absorbed.get_mut(region_id).unwrap_or(&mut 0.0) += absorbed;
                    *supply_mw.get_mut(region_id).unwrap() -= absorbed;
                }
            }
        }
    }

    // Step 5: LV/MV capacity checks and overproduction/load shedding.
    let priority = GridPriority::Peacetime; // TODO: derive from government policy.

    // Sort regions by ID for deterministic processing.
    let mut sorted_regions: Vec<&Region> = country.regions.iter().collect();
    sorted_regions.sort_by(|a, b| a.id.cmp(&b.id));

    for region in &sorted_regions {
        let region_id = &region.id;
        let supply = supply_mw.get(region_id).copied().unwrap_or(0.0);
        let demand = demand_mw.get(region_id).copied().unwrap_or(0.0);
        let lv_cap = grid
            .region_lv_capacity
            .get(region_id)
            .copied()
            .unwrap_or(0.0);
        let mv_cap = grid
            .region_mv_capacity
            .get(region_id)
            .copied()
            .unwrap_or(0.0);
        let storage_abs = storage_absorbed.get(region_id).copied().unwrap_or(0.0);

        // LV/MV capacity limit: supply can't exceed grid capacity.
        let grid_cap = lv_cap.min(mv_cap);
        let effective_supply = supply.min(grid_cap);

        // Calculate overproduction tier.
        let overprod_tier = calculate_overproduction_tier(effective_supply, demand, storage_abs);
        result
            .region_overproduction_tiers
            .insert(region_id.clone(), overprod_tier);
        grid.overproduction_tiers
            .insert(region_id.clone(), overprod_tier);

        // Calculate load shed tier.
        let shed_tier = crate::energy::load_shedding::calculate_load_shed_tier(
            effective_supply,
            demand,
            grid_cap,
            priority,
        );
        result
            .region_load_shed_tiers
            .insert(region_id.clone(), shed_tier);
        grid.load_shed_tiers.insert(region_id.clone(), shed_tier);

        // Apply overproduction effects.
        match overprod_tier {
            OverproductionTier::Normal => {}
            OverproductionTier::IndustrialBuff => {
                let surplus_ratio = if demand > 0.0 {
                    (effective_supply - demand - storage_abs) / demand
                } else {
                    0.0
                };
                apply_industrial_buff(
                    region_id,
                    surplus_ratio,
                    buildings,
                    &mut result.building_efficiency_penalties,
                );
            }
            OverproductionTier::Curtailment => {
                let curtailed = apply_curtailment(
                    region_id,
                    effective_supply - demand - storage_abs,
                    buildings,
                );
                result
                    .region_curtailed_mw
                    .insert(region_id.clone(), curtailed);
            }
            OverproductionTier::GridDamage => {
                let surplus_ratio = if demand > 0.0 {
                    (effective_supply - demand - storage_abs) / demand
                } else {
                    1.0
                };
                let curtailed = apply_curtailment(
                    region_id,
                    effective_supply - demand - storage_abs,
                    buildings,
                );
                result
                    .region_curtailed_mw
                    .insert(region_id.clone(), curtailed);
                apply_grid_damage(region_id, surplus_ratio, &mut grid);
            }
        }

        // Apply load shedding penalties.
        if shed_tier != LoadShedTier::Normal {
            crate::energy::load_shedding::apply_load_shedding(
                region_id,
                shed_tier,
                priority,
                buildings,
                &country.regions,
                commercial_buildings,
                &mut result.building_efficiency_penalties,
            );
        }

        // Phase 81 Wave 2: Calculate spot price using merit-order dispatch.
        // Build the merit order stack for this region and clear the spot market.
        let merit_stack = build_merit_order_stack(
            buildings,
            region_id,
            fuel_prices,
            average_wage,
            &weather_state,
        );
        let (spot_price, dispatch_results) = clear_spot_market(&merit_stack, demand, average_wage);

        grid.spot_prices.insert(region_id.clone(), spot_price);
        result
            .region_spot_prices
            .insert(region_id.clone(), spot_price);

        // Phase 81 Wave 2: Store merit-order results in spot_market state.
        grid.spot_market
            .clearing_prices
            .insert(region_id.clone(), spot_price);
        for (plant_id, marginal_cost, _) in &merit_stack {
            grid.spot_market
                .marginal_costs
                .insert(plant_id.clone(), *marginal_cost);
            grid.spot_market.dispatch_order.push(plant_id.clone());
        }
        for (plant_id, dispatched_mw) in &dispatch_results {
            grid.spot_market
                .dispatched_mw
                .insert(plant_id.clone(), *dispatched_mw);
            let revenue = dispatched_mw * spot_price;
            grid.spot_market
                .revenue_distribution
                .insert(plant_id.clone(), revenue);
        }

        // Record final supply/demand.
        result
            .region_supply_mw
            .insert(region_id.clone(), effective_supply);
        result.region_demand_mw.insert(region_id.clone(), demand);
        result
            .region_storage_absorbed_mw
            .insert(region_id.clone(), storage_abs);
    }

    // Consume Commodity::Energy from building inventories (it's been distributed).
    for building in buildings.iter_mut() {
        if building.sector != Sector::Energy {
            continue;
        }
        // Remove distributed energy from inventory.
        let energy = building
            .inventory
            .get(&Commodity::Energy)
            .copied()
            .unwrap_or(0.0);
        if energy > 0.0 {
            building.inventory.insert(Commodity::Energy, 0.0);
        }
    }

    // Restore power_grid_state into country.
    country.power_grid_state = grid;

    result
}

/// Calculate overproduction tier based on actual remaining surplus.
///
/// Uses ACTUAL remaining surplus (after HV exports and storage absorption),
/// NOT theoretical HV capacity. This prevents the "HV Black Hole" fallacy.
fn calculate_overproduction_tier(
    actual_supply_mw: f64,
    actual_demand_mw: f64,
    storage_absorbed_mw: f64,
) -> OverproductionTier {
    let remaining_surplus = actual_supply_mw - actual_demand_mw - storage_absorbed_mw;
    let surplus_ratio = if actual_demand_mw > 0.0 {
        remaining_surplus / actual_demand_mw
    } else if remaining_surplus > 0.0 {
        1.0
    } else {
        0.0
    };
    match surplus_ratio {
        s if s <= 0.0 => OverproductionTier::Normal,
        s if s <= 0.10 => OverproductionTier::IndustrialBuff,
        s if s <= 0.25 => OverproductionTier::Curtailment,
        _ => OverproductionTier::GridDamage,
    }
}

/// Apply industrial buff to HeavyIndustry and ArmamentsIndustry buildings.
///
/// The buff is a NEGATIVE efficiency penalty (increases production). It is
/// clamped by physical BOM availability in `execute_production_cycle` —
/// the factory can only overclock if it has the physical materials to do so.
fn apply_industrial_buff(
    region_id: &str,
    surplus_ratio: f64,
    buildings: &[Building],
    penalties: &mut HashMap<String, f64>,
) {
    let buff_multiplier = 1.0 + (surplus_ratio * 0.5);
    let buff_penalty = -(buff_multiplier - 1.0); // Negative = buff.

    for building in buildings {
        if building.region_id != region_id {
            continue;
        }
        if building.sector == Sector::HeavyIndustry || building.sector == Sector::ArmamentsIndustry
        {
            let existing = penalties.get(&building.id).copied().unwrap_or(0.0);
            // Only apply buff if there's no existing load shedding penalty.
            if existing <= 0.0 {
                penalties.insert(building.id.clone(), existing + buff_penalty);
            }
        }
    }
}

/// Apply curtailment to renewable and thermal plants.
///
/// Curtailment order: Storage (already absorbed) → Renewable curtailment → Thermal throttling.
/// Curtailed energy is physically dissipated as waste heat.
/// Returns total curtailed energy in MW.
fn apply_curtailment(region_id: &str, surplus_mw: f64, buildings: &mut [Building]) -> f64 {
    if surplus_mw <= 0.0 {
        return 0.0;
    }

    let mut curtailed = 0.0;
    let mut remaining = surplus_mw;

    // First: curtail renewables (solar, wind).
    for building in buildings.iter_mut() {
        if building.region_id != region_id || building.sector != Sector::Energy {
            continue;
        }
        if remaining <= 0.0 {
            break;
        }
        if let Some(meta) = get_plant_metadata(building) {
            if meta.plant_type.is_renewable() {
                let energy = building
                    .inventory
                    .get(&Commodity::Energy)
                    .copied()
                    .unwrap_or(0.0);
                let curtail_amount = energy.min(remaining);
                if curtail_amount > 0.0 {
                    let new_energy = (energy - curtail_amount).max(0.0);
                    building.inventory.insert(Commodity::Energy, new_energy);
                    curtailed += curtail_amount;
                    remaining -= curtail_amount;
                }
            }
        }
    }

    // Second: throttle thermal plants (reduce fuel consumption — fuel stays in inventory).
    for building in buildings.iter_mut() {
        if building.region_id != region_id || building.sector != Sector::Energy {
            continue;
        }
        if remaining <= 0.0 {
            break;
        }
        if let Some(meta) = get_plant_metadata(building) {
            if meta.plant_type.is_thermal() {
                let energy = building
                    .inventory
                    .get(&Commodity::Energy)
                    .copied()
                    .unwrap_or(0.0);
                let curtail_amount = energy.min(remaining);
                if curtail_amount > 0.0 {
                    let new_energy = (energy - curtail_amount).max(0.0);
                    building.inventory.insert(Commodity::Energy, new_energy);
                    curtailed += curtail_amount;
                    remaining -= curtail_amount;
                }
            }
        }
    }

    curtailed
}

/// Apply grid damage from sustained overfrequency.
///
/// Degrades MV/LV grid condition. If condition drops below 0.5, there is
/// a risk of cascading failure on subsequent turns.
fn apply_grid_damage(region_id: &str, surplus_ratio: f64, grid: &mut PowerGridState) {
    let degradation = 0.005 * surplus_ratio;

    if let Some(lv_cond) = grid.region_lv_condition.get_mut(region_id) {
        *lv_cond = (*lv_cond - degradation).max(0.0);
    }
    if let Some(mv_cond) = grid.region_mv_condition.get_mut(region_id) {
        *mv_cond = (*mv_cond - degradation).max(0.0);
    }
}

/// Phase 81 Wave 2: Build the merit order stack for a region.
///
/// Collects all active plants in the region, computes the marginal cost for each
/// using `compute_marginal_cost()`, applies weather-adjusted available capacity,
/// and sorts by `(marginal_cost, plant_id)` ascending for deterministic dispatch.
///
/// Returns a `Vec` of `(plant_building_id, marginal_cost, available_mw)` sorted
/// cheapest first.
fn build_merit_order_stack(
    buildings: &[Building],
    region_id: &str,
    fuel_prices: &HashMap<Commodity, f64>,
    average_wage: f64,
    weather_state: &WeatherState,
) -> Vec<(String, f64, f64)> {
    let mut stack: Vec<(String, f64, f64)> = Vec::new();

    for building in buildings {
        if building.sector != Sector::Energy {
            continue;
        }
        if building.region_id != region_id {
            continue;
        }

        let metadata = match get_plant_metadata(building) {
            Some(m) => m,
            None => continue,
        };

        // Compute marginal cost for this plant.
        let marginal_cost = compute_marginal_cost(&metadata, fuel_prices, average_wage);

        // Compute weather-adjusted available capacity.
        let weather = get_region_weather_modifier(weather_state, region_id);
        let weather_mult = weather_output_multiplier(
            metadata.plant_type,
            metadata.cooling_type,
            metadata.has_cooling_upgrade,
            &weather,
        );

        // Available MW = nameplate * weather_multiplier, but not more than
        // the energy currently in inventory (actual produced output).
        let energy_in_inventory = building
            .inventory
            .get(&Commodity::Energy)
            .copied()
            .unwrap_or(0.0);
        let nameplate_adjusted = metadata.nameplate_capacity_mw * weather_mult;
        let available_mw = nameplate_adjusted.min(energy_in_inventory.max(0.0));

        if available_mw > 0.0 {
            stack.push((building.id.clone(), marginal_cost, available_mw));
        }
    }

    // Sort by (marginal_cost, plant_id) for deterministic dispatch ordering.
    stack.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    stack
}

/// Phase 81 Wave 2: Clear the spot market for a region using merit-order dispatch.
///
/// Dispatches plants from cheapest to most expensive until demand is met.
/// Clearing price = marginal cost of the last dispatched plant.
///
/// Applies demand elasticity and scarcity ceiling:
/// - If supply > demand (overproduction): price drops toward zero but not below
///   `average_wage * 0.0001` (the zero-marginal-cost floor).
/// - If supply < demand (scarcity): price spikes toward `average_wage * 0.01`
///   (scarcity ceiling, dynamically scaled to wages).
/// - If no plants are dispatched (zero demand): price = `average_wage * 0.0001`.
///
/// Returns `(clearing_price, dispatch_results)` where `dispatch_results` maps
/// `plant_building_id` to dispatched MW.
fn clear_spot_market(
    stack: &[(String, f64, f64)],
    demand_mw: f64,
    average_wage: f64,
) -> (f64, Vec<(String, f64)>) {
    let price_floor = average_wage * 0.0001;
    let scarcity_ceiling = average_wage * 0.01;

    // Zero demand: no dispatch needed, price at floor.
    if demand_mw <= 0.0 {
        return (price_floor, Vec::new());
    }

    let mut dispatch_results: Vec<(String, f64)> = Vec::new();
    let mut remaining_demand = demand_mw;
    let mut total_supply: f64 = 0.0;
    let mut last_marginal_cost = price_floor;

    for (plant_id, marginal_cost, available_mw) in stack {
        if remaining_demand <= 0.0 {
            break;
        }

        let dispatched = remaining_demand.min(*available_mw);
        dispatch_results.push((plant_id.clone(), dispatched));
        remaining_demand -= dispatched;
        total_supply += dispatched;
        last_marginal_cost = *marginal_cost;
    }

    // Compute clearing price based on supply/demand balance.
    let clearing_price = if total_supply >= demand_mw {
        // Supply meets or exceeds demand: clearing price = marginal cost of
        // the last dispatched plant. If there's significant overproduction,
        // the price drops toward the floor.
        let surplus_ratio = if demand_mw > 0.0 {
            (total_supply - demand_mw) / demand_mw
        } else {
            0.0
        };
        if surplus_ratio > 0.10 {
            // Significant overproduction: price drops toward floor.
            let discount = (1.0 - surplus_ratio.min(0.8)).max(0.2);
            (last_marginal_cost * discount).max(price_floor)
        } else {
            last_marginal_cost.max(price_floor)
        }
    } else {
        // Scarcity: supply < demand. Price spikes toward scarcity ceiling.
        // The spike is proportional to the deficit ratio.
        let deficit_ratio = if demand_mw > 0.0 {
            (demand_mw - total_supply) / demand_mw
        } else {
            0.0
        };
        // Blend the marginal cost with the scarcity ceiling based on deficit severity.
        let scarcity_weight = deficit_ratio.min(1.0);
        let blended =
            last_marginal_cost * (1.0 - scarcity_weight) + scarcity_ceiling * scarcity_weight;
        blended.max(last_marginal_cost).min(scarcity_ceiling)
    };

    (clearing_price, dispatch_results)
}

/// Calculate electricity spot price based on supply/demand balance.
///
/// **Deprecated**: Phase 81 Wave 2 replaces this with merit-order spot market
/// clearing via `build_merit_order_stack()` and `clear_spot_market()`.
/// Kept for backward compatibility with any tests that reference it.
#[deprecated(note = "Replaced by merit-order spot market clearing in Phase 81 Wave 2")]
#[allow(deprecated)]
#[allow(dead_code)]
fn calculate_spot_price(
    supply_mw: f64,
    demand_mw: f64,
    overprod_tier: OverproductionTier,
    shed_tier: LoadShedTier,
) -> f64 {
    // Base price: a fraction of average industrial wage per MWh.
    // This is dynamically scaled, not a magic number.
    let base_price = 50.0; // Placeholder — will be replaced with average_wage-based calculation.

    let ratio = if demand_mw > 0.0 {
        supply_mw / demand_mw
    } else {
        1.0
    };

    match (overprod_tier, shed_tier) {
        (OverproductionTier::IndustrialBuff, _) => base_price * 0.7, // 30% discount during glut.
        (OverproductionTier::Curtailment, _) => base_price * 0.5, // 50% discount during heavy glut.
        (OverproductionTier::GridDamage, _) => base_price * 0.3,  // Near-free during extreme glut.
        (_, LoadShedTier::Tier1) => base_price * 1.2,
        (_, LoadShedTier::Tier2) => base_price * 1.5,
        (_, LoadShedTier::Tier3) => base_price * 2.0,
        (_, LoadShedTier::Tier4) => base_price * 3.0,
        (_, LoadShedTier::Blackout) => base_price * 5.0,
        _ => base_price * ratio.max(0.1).min(2.0),
    }
}

/// Degrade grid condition per turn based on load factor.
///
/// Heavily loaded lines degrade faster. This is called every turn after
/// grid distribution.
pub fn degrade_grid_condition(grid: &mut PowerGridState, flows: &HashMap<(String, String), f64>) {
    for line in &mut grid.hv_lines {
        let flow = flows
            .get(&(line.from_region.clone(), line.to_region.clone()))
            .copied()
            .unwrap_or(0.0)
            .abs();
        let load_factor = if line.capacity_mw > 0.0 {
            (flow / line.capacity_mw).min(1.0)
        } else {
            0.0
        };
        let degradation = 0.001 * (1.0 + load_factor);
        line.condition = (line.condition - degradation).max(0.0);
    }
}

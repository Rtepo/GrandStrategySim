//! Utility grid distribution — Phase 8.1.
//!
//! Converts `Commodity::Energy` and `Commodity::Heat` from energy-sector building
//! inventories into regional `CapacityType` supply, then distributes that supply
//! to `UtilityConnections` on housing and commercial buildings.

use crate::entities::Building;
use crate::registries::enums::{Commodity, Sector};
use crate::society::geography::Region;
use crate::society::housing::{CommercialBuilding, HousingBuilding};
use crate::state::Season;
use crate::utilities::config::UtilityConfig;
use crate::utilities::demand::UtilityDemand;
use crate::infrastructure::CapacityType;

use std::collections::HashMap;

/// Result of utility distribution per region.
#[derive(Debug, Clone, Default)]
pub struct UtilityDistributionResult {
    /// Region ID -> total electricity distributed (kWh).
    pub electricity_distributed: HashMap<String, f64>,
    /// Region ID -> total heating distributed (GJ).
    pub heating_distributed: HashMap<String, f64>,
    /// Region ID -> total water distributed (liters).
    pub water_distributed: HashMap<String, f64>,
}

/// Distribute utilities from energy-sector buildings to the grid.
///
/// # Arguments
/// * `regions` - Mutable regions (capacity_pool updated).
/// * `buildings` - Mutable buildings (Commodity::Energy/Heat consumed from inventory).
/// * `housing_buildings` - Mutable housing (UtilityConnections updated).
/// * `commercial_buildings` - Mutable commercial buildings (UtilityConnections updated).
/// * `utility_config` - Conversion factors.
/// * `season` - Current season (affects heating demand distribution).
///
/// # Rules
/// * Only `Sector::Energy` buildings produce grid electricity/heat.
/// * Water treatment buildings draw from grid (their UtilityConnections), not B2B inventory.
/// * `Commodity::Energy` is consumed (removed from Building.inventory) when converted to grid capacity.
/// * Distribution to buildings is proportional to demand share.
pub fn distribute_utilities(
    regions: &mut [Region],
    buildings: &mut [Building],
    housing_buildings: &mut [HousingBuilding],
    commercial_buildings: &mut [CommercialBuilding],
    utility_config: &UtilityConfig,
    season: Season,
) -> UtilityDistributionResult {
    let mut result = UtilityDistributionResult::default();

    for region in regions.iter_mut() {
        let region_id = &region.id;

        // Step 1: Collect energy and heat from Sector::Energy buildings in this region
        let mut total_electricity_kwh: f64 = 0.0;
        let mut total_heating_gj: f64 = 0.0;

        for building in buildings.iter_mut() {
            if building.sector != Sector::Energy {
                continue;
            }
            if building.region_id != *region_id {
                continue;
            }

            // Consume Commodity::Energy from inventory → electricity supply
            let energy_in_inventory = building.inventory.get(&Commodity::Energy).copied().unwrap_or(0.0);
            if energy_in_inventory > 0.0 {
                total_electricity_kwh += energy_in_inventory * utility_config.energy_to_kwh_factor;
                building.inventory.remove(&Commodity::Energy);
            }

            // Consume Commodity::Heat from inventory → district heating supply
            let heat_in_inventory = building.inventory.get(&Commodity::Heat).copied().unwrap_or(0.0);
            if heat_in_inventory > 0.0 {
                total_heating_gj += heat_in_inventory * utility_config.energy_to_gj_heating_factor;
                building.inventory.remove(&Commodity::Heat);
            }
        }

        // Step 2: Write to region capacity_pool
        region.capacity_pool.insert(CapacityType::ElectricitySupply, total_electricity_kwh);
        region.capacity_pool.insert(CapacityType::DistrictHeating, total_heating_gj);

        // Step 3: Calculate total demand per utility type for proportional distribution
        let mut total_elec_demand: f64 = 0.0;
        let mut total_heat_demand: f64 = 0.0;
        let mut total_water_demand: f64 = 0.0;

        for hb in housing_buildings.iter() {
            if !region.micro_regions.contains_key(&hb.micro_region_id) {
                continue;
            }
            let demand = UtilityDemand::for_housing(hb, season);
            total_elec_demand += demand.electricity_demand;
            total_heat_demand += demand.heating_demand;
            total_water_demand += demand.surface_water_demand + demand.groundwater_demand;
        }

        for cb in commercial_buildings.iter() {
            if !region.micro_regions.contains_key(&cb.micro_region_id) {
                continue;
            }
            let demand = UtilityDemand::for_commercial(cb, season);
            total_elec_demand += demand.electricity_demand;
            total_heat_demand += demand.heating_demand;
            total_water_demand += demand.surface_water_demand + demand.groundwater_demand;
        }

        // Step 4: Distribute supply to UtilityConnections proportionally by demand share
        let elec_supply = total_electricity_kwh;
        let heat_supply = total_heating_gj;
        let water_supply = region.capacity_pool.get(&CapacityType::SurfaceWaterSupply).copied().unwrap_or(0.0)
            + region.capacity_pool.get(&CapacityType::GroundwaterSupply).copied().unwrap_or(0.0);

        for hb in housing_buildings.iter_mut() {
            if !region.micro_regions.contains_key(&hb.micro_region_id) {
                continue;
            }
            let demand = UtilityDemand::for_housing(hb, season);

            if total_elec_demand > 0.0 {
                let share = demand.electricity_demand / total_elec_demand;
                hb.utility_connections.electricity_capacity = (elec_supply * share).min(demand.electricity_demand);
            }
            if total_heat_demand > 0.0 {
                let share = demand.heating_demand / total_heat_demand;
                hb.utility_connections.district_heating_capacity = (heat_supply * share).min(demand.heating_demand);
            }
            if total_water_demand > 0.0 {
                let share = (demand.surface_water_demand + demand.groundwater_demand) / total_water_demand;
                let allocated = (water_supply * share).min(demand.surface_water_demand + demand.groundwater_demand);
                hb.utility_connections.surface_water_capacity = allocated * 0.7;
                hb.utility_connections.groundwater_capacity = allocated * 0.3;
            }
        }

        for cb in commercial_buildings.iter_mut() {
            if !region.micro_regions.contains_key(&cb.micro_region_id) {
                continue;
            }
            let demand = UtilityDemand::for_commercial(cb, season);

            if total_elec_demand > 0.0 {
                let share = demand.electricity_demand / total_elec_demand;
                cb.utility_connections.electricity_capacity = (elec_supply * share).min(demand.electricity_demand);
            }
            if total_heat_demand > 0.0 {
                let share = demand.heating_demand / total_heat_demand;
                cb.utility_connections.district_heating_capacity = (heat_supply * share).min(demand.heating_demand);
            }
            if total_water_demand > 0.0 {
                let share = (demand.surface_water_demand + demand.groundwater_demand) / total_water_demand;
                let allocated = (water_supply * share).min(demand.surface_water_demand + demand.groundwater_demand);
                cb.utility_connections.surface_water_capacity = allocated * 0.7;
                cb.utility_connections.groundwater_capacity = allocated * 0.3;
            }
        }

        result.electricity_distributed.insert(region_id.clone(), total_electricity_kwh);
        result.heating_distributed.insert(region_id.clone(), total_heating_gj);
        result.water_distributed.insert(region_id.clone(), water_supply);
    }

    result
}

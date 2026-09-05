//! Utility consumption and penalties — Phase 8.2.
//!
//! Calculates utility deficits per building, applies blackout efficiency penalties
//! for Wave 3, calculates winter mortality, and bills consumers.

use crate::entities::Company;
use crate::registries::enums::Sector;
use crate::society::geography::Region;
use crate::society::housing::{CommercialBuilding, HousingBuilding, HousingType};
use crate::state::Season;
use crate::utilities::config::{UtilityConfig, UtilityPricingConfig};
use crate::utilities::demand::UtilityDemand;

use std::collections::HashMap;

/// Result of utility consumption processing.
#[derive(Debug, Clone, Default)]
pub struct UtilityConsumptionResult {
    /// Maps building_id -> efficiency_penalty (0.0 = no penalty, 0.5 = 50% loss).
    pub building_efficiency_penalties: HashMap<String, f64>,
    /// Total billing collected per region (region_id -> amount).
    pub billing_collected: HashMap<String, f64>,
    /// Total treasury subsidies applied per region.
    pub treasury_subsidies: HashMap<String, f64>,
}

/// Process utility consumption for all buildings, calculate deficits, penalties, and billing.
///
/// # Arguments
/// * `regions` - Mutable regions (winter_mortality_multiplier written here,
///   water_reserves drawn down for standalone wells).
/// * `housing_buildings` - Mutable housing buildings (well extraction recorded).
/// * `commercial_buildings` - Commercial buildings (read for demand).
/// * `companies` - Mutable companies (utility providers credited via available_cash).
/// * `utility_config` - Conversion factors and penalty parameters.
/// * `pricing_config` - Tariff rates for billing.
/// * `season` - Current season.
///
/// # Rules
/// * `building_efficiency_penalties` is returned for Wave 3 — NOT stored on Building struct.
/// * `winter_mortality_multiplier` is written to `Region` for Phase 5 of the NEXT turn.
/// * All utility bill payments credit `Company.available_cash` (NOT brokerage_account).
/// * Treasury subsidizes consumers who cannot afford the full bill.
/// * Blueprint 006: Standalone wells draw from region.water_reserves.groundwater_volume.
///   Sewage is gated by water_extracted. Off-grid sewage routes to standalone_biohazard.
pub fn process_utility_consumption(
    regions: &mut [Region],
    housing_buildings: &mut [HousingBuilding],
    commercial_buildings: &[CommercialBuilding],
    companies: &mut [Company],
    utility_config: &UtilityConfig,
    pricing_config: &UtilityPricingConfig,
    season: Season,
) -> UtilityConsumptionResult {
    let mut result = UtilityConsumptionResult::default();

    // Collect energy provider company IDs for billing credit
    let energy_company_ids: Vec<String> = companies
        .iter()
        .filter(|c| c.sector == Sector::Energy)
        .map(|c| c.id.clone())
        .collect();

    for region in regions.iter_mut() {
        let region_id = region.id.clone();
        let mut total_mortality_weighted: f64 = 0.0;
        let mut total_occupied_slots: f64 = 0.0;
        let mut region_billing: f64 = 0.0;
        let mut region_subsidy: f64 = 0.0;

        // Process housing buildings
        for hb in housing_buildings.iter_mut() {
            if !region.micro_regions.contains_key(&hb.micro_region_id) {
                continue;
            }

            let demand = UtilityDemand::for_housing(hb, season);
            let connections = &hb.utility_connections;

            // Electricity deficit
            let elec_demand = demand.electricity_demand.max(1.0);
            let elec_supply = connections.electricity_capacity;
            let _elec_deficit = 1.0 - (elec_supply / elec_demand).min(1.0);

            // Heating deficit (winter only)
            let heat_deficit = if season == Season::Winter {
                (demand.heating_demand - connections.district_heating_capacity).max(0.0)
            } else {
                0.0
            };

            // Winter mortality
            let housing_quality = housing_quality_for_type(&hb.housing_type);
            let mortality_multiplier =
                UtilityDemand::calculate_winter_mortality(heat_deficit, housing_quality);
            let occupied = hb.primary_slots.occupied_slots as f64
                + hb.sublet_slots
                    .as_ref()
                    .map(|s| s.occupied_slots as f64)
                    .unwrap_or(0.0);
            total_mortality_weighted += mortality_multiplier * occupied;
            total_occupied_slots += occupied;

            // Billing: electricity + heating + water
            let elec_consumed = elec_supply.min(demand.electricity_demand);
            let heat_consumed = connections
                .district_heating_capacity
                .min(demand.heating_demand);

            // Blueprint 006: Water consumption gated by well for standalone.
            // Off-grid buildings MUST have a constructed WaterWell to draw water.
            // Without a well, standalone water yields zero — no water from thin air.
            let water_from_connections = connections.surface_water_capacity
                + connections.groundwater_capacity;
            let water_demand_total = demand.surface_water_demand + demand.groundwater_demand;

            let (water_consumed, is_off_grid) = if crate::utilities::consumption_bom::is_centralized_water_method(
                &hb.active_water_supply,
            ) {
                // Centralized supply — water from municipal mains
                (water_from_connections.min(water_demand_total), false)
            } else if hb.can_draw_standalone_water() {
                // Standalone supply with constructed well — draw from aquifer.
                // Rule 1: Water must be physically extracted from groundwater_volume.
                let well_yield = hb.standalone_water_yield();
                let well_demand = well_yield.min(water_demand_total);
                // Physical extraction from region's aquifer (Rule 1, Rule 20).
                let (drawn_liters, _water_quality) =
                    region.water_reserves.draw_groundwater(well_demand);
                // Record extraction for well lifecycle (Rule 15: maintenance scaling).
                if let Some(well) = hb.water_well.as_mut() {
                    well.record_extraction(drawn_liters);
                }
                (drawn_liters, true)
            } else {
                // No well constructed = no standalone water (Rule 1: conservation)
                (0.0, true)
            };

            // Blueprint 006: Gate sewage by water_extracted (Rule 1: mass conservation).
            // sewage_volume = MIN(water_consumed, demand.sewage_generation).
            // A building with zero water produces zero sewage.
            let sewage_volume = water_consumed.min(demand.sewage_generation);

            // Blueprint 006: Route off-grid sewage to LocalPollutionState.
            // Off-grid buildings (no sewer connection) convert sewage to
            // standalone_biohazard mass (Rule 1: no mass vanishes).
            if is_off_grid && sewage_volume > 0.0 {
                crate::environment::smog::off_grid_waste_emission(
                    &mut region.local_pollution,
                    sewage_volume,
                    demand.waste_generation,
                    &hb.id,
                    &region_id,
                );
            }

            let mut bill = elec_consumed * pricing_config.price_per_kwh
                + heat_consumed * pricing_config.price_per_gj_heating
                + water_consumed * pricing_config.price_per_liter_water;

            // R8: Housing Cooperative utility economies of scale.
            // If the building's owner is a HousingCooperative, apply the
            // utility discount (e.g., 0.10 = 10% discount on utility bills).
            // This makes HousingCooperative operational rather than inert.
            if !hb.owner.is_empty() {
                if let Some(owner_company) = companies.iter().find(|c| c.id == hb.owner) {
                    let discount = owner_company.legal_form.calculate_utility_discount();
                    if discount > 0.0 {
                        bill *= (1.0 - discount).max(0.0);
                    }
                }
            }

            region_billing += bill;
            // Subsidy for housing (low-income support)
            region_subsidy += bill * pricing_config.treasury_subsidy_ratio;
        }

        // Process commercial buildings
        for cb in commercial_buildings.iter() {
            if !region.micro_regions.contains_key(&cb.micro_region_id) {
                continue;
            }

            let demand = UtilityDemand::for_commercial(cb, season);
            let connections = &cb.utility_connections;

            // Electricity deficit
            let elec_demand = demand.electricity_demand.max(1.0);
            let elec_supply = connections.electricity_capacity;
            let elec_deficit = 1.0 - (elec_supply / elec_demand).min(1.0);

            // Efficiency penalty for Wave 3
            let penalty = elec_deficit * utility_config.blackout_efficiency_penalty;
            result
                .building_efficiency_penalties
                .insert(cb.id.clone(), penalty);

            // Billing
            let elec_consumed = elec_supply.min(demand.electricity_demand);
            let heat_consumed = connections
                .district_heating_capacity
                .min(demand.heating_demand);
            let water_consumed = (connections.surface_water_capacity
                + connections.groundwater_capacity)
                .min(demand.surface_water_demand + demand.groundwater_demand);

            let mut bill = elec_consumed * pricing_config.price_per_kwh
                + heat_consumed * pricing_config.price_per_gj_heating
                + water_consumed * pricing_config.price_per_liter_water;

            // R8: Housing Cooperative utility economies of scale for commercial
            // buildings managed by a housing cooperative.
            if !cb.owner_id.is_empty() {
                if let Some(owner_company) = companies.iter().find(|c| c.id == cb.owner_id) {
                    let discount = owner_company.legal_form.calculate_utility_discount();
                    if discount > 0.0 {
                        bill *= (1.0 - discount).max(0.0);
                    }
                }
            }

            region_billing += bill;

            // Commercial buildings pay from owning company's available_cash
            // (The actual deduction happens elsewhere — here we just track totals)
        }

        // Write winter mortality multiplier to region
        if total_occupied_slots > 0.0 {
            region.winter_mortality_multiplier = total_mortality_weighted / total_occupied_slots;
        } else {
            region.winter_mortality_multiplier = 1.0;
        }

        // Credit utility providers: distribute billing to energy companies proportionally
        if !energy_company_ids.is_empty() && region_billing > 0.0 {
            let per_company = region_billing / energy_company_ids.len() as f64;
            for company in companies.iter_mut() {
                if energy_company_ids.contains(&company.id) {
                    company.available_cash += per_company;
                }
            }
        }

        result
            .billing_collected
            .insert(region_id.clone(), region_billing);
        result.treasury_subsidies.insert(region_id, region_subsidy);
    }

    result
}

/// Map HousingType to a quality score (0.0 - 1.0).
fn housing_quality_for_type(ht: &HousingType) -> f64 {
    match ht {
        HousingType::Hut => 0.10,
        HousingType::Slum => 0.15,
        HousingType::WorkersHousing => 0.25,
        HousingType::EstateHousing => 0.40,
        HousingType::SocialHousing => 0.35,
        HousingType::Tenement => 0.45,
        HousingType::SkilledHousing => 0.55,
        HousingType::Rectory => 0.60,
        HousingType::Monastery => 0.65,
        HousingType::Palace => 0.90,
        HousingType::CityPalace => 1.00,
    }
}

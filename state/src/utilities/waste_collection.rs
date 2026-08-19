//! Waste collection and processing — Phase 8.3.
//!
//! Aggregates waste generation from all buildings per micro-region, routes it to
//! landfill Buildings (Sector::WasteManagement), and applies overflow penalties.

use crate::entities::{Building, Company};
use crate::economy::transfer_settler::{debit_company_by_id, credit_company_by_id};
use crate::registries::enums::Sector;
use crate::society::geography::Region;
use crate::society::housing::{CommercialBuilding, HousingBuilding};
use crate::state::Season;
use crate::utilities::demand::UtilityDemand;
use crate::utilities::waste::LandfillData;

use std::collections::HashMap;

/// Result of waste processing per region.
#[derive(Debug, Clone, Default)]
pub struct WasteTurnResult {
    /// Region ID -> total waste processed (tons).
    pub waste_processed: HashMap<String, f64>,
    /// Region ID -> total waste overflow (tons, uncollected).
    pub waste_overflow: HashMap<String, f64>,
    /// Region ID -> pollution generated.
    pub pollution_generated: HashMap<String, f64>,
    /// Region ID -> recovered commodities (commodity name -> tons).
    pub commodities_recovered: HashMap<String, HashMap<String, f64>>,
}

/// Process waste for all regions.
///
/// # Arguments
/// * `regions` - Regions (for micro-region lookup and health degradation).
/// * `buildings` - Mutable buildings (landfill Buildings with LandfillData).
/// * `companies` - Mutable companies (operating costs deducted from available_cash).
/// * `housing_buildings` - Housing buildings (waste_generation source).
/// * `commercial_buildings` - Commercial buildings (waste_generation source).
/// * `season` - Current season.
///
/// # Rules
/// * Landfill buildings are identified by `Sector::WasteManagement` and `landfill_data.is_some()`.
/// * Operating costs deducted from owning `Company.available_cash`.
/// * Recovered commodities added to `Building.inventory`.
/// * Overflow increases `health_degradation_rate` on `ClassDemographics` (future).
pub fn process_waste_turn(
    regions: &mut [Region],
    buildings: &mut [Building],
    companies: &mut [Company],
    housing_buildings: &[HousingBuilding],
    commercial_buildings: &[CommercialBuilding],
    _season: Season,
) -> WasteTurnResult {
    let mut result = WasteTurnResult::default();

    for region in regions.iter_mut() {
        let region_id = region.id.clone();
        let mut total_waste: f64 = 0.0;
        let mut total_recyclable: f64 = 0.0;

        // Aggregate waste from housing buildings in this region's micro-regions
        for hb in housing_buildings.iter() {
            if !region.micro_regions.contains_key(&hb.micro_region_id) {
                continue;
            }
            let demand = UtilityDemand::for_housing(hb, _season);
            total_waste += demand.waste_generation;
            total_recyclable += demand.waste_generation * demand.recyclable_fraction;
        }

        // Aggregate waste from commercial buildings
        for cb in commercial_buildings.iter() {
            if !region.micro_regions.contains_key(&cb.micro_region_id) {
                continue;
            }
            let demand = UtilityDemand::for_commercial(cb, _season);
            total_waste += demand.waste_generation;
            total_recyclable += demand.waste_generation * demand.recyclable_fraction;
        }

        if total_waste <= 0.0 {
            continue;
        }

        // Find landfill buildings in this region
        let mut landfill_indices: Vec<usize> = Vec::new();
        for (i, building) in buildings.iter().enumerate() {
            if building.sector == Sector::WasteManagement
                && building.region_id == region_id
                && building.landfill_data.is_some()
            {
                landfill_indices.push(i);
            }
        }

        if landfill_indices.is_empty() {
            // No landfill — all waste overflows
            result.waste_overflow.insert(region_id.clone(), total_waste);
            // Health penalty would be applied here (future)
            continue;
        }

        // Distribute waste among landfill buildings
        let waste_per_landfill = total_waste / landfill_indices.len() as f64;
        let mut processed_total: f64 = 0.0;
        let mut overflow_total: f64 = 0.0;
        let mut pollution_total: f64 = 0.0;
        let mut all_recovered: HashMap<String, f64> = HashMap::new();

        for &idx in &landfill_indices {
            let building = &mut buildings[idx];
            let landfill_data = building.landfill_data.as_mut().unwrap();

            // Check capacity
            if !landfill_data.has_capacity() {
                overflow_total += waste_per_landfill;
                continue;
            }

            // Process waste
            let waste_result = landfill_data.process_waste(waste_per_landfill);
            processed_total += waste_result.waste_destroyed;
            pollution_total += waste_result.pollution_generated;

            // Merge recovered commodities into Building.inventory
            for (commodity_name, qty) in &waste_result.commodities_recovered {
                *all_recovered.entry(commodity_name.clone()).or_insert(0.0) += *qty;
                // Also store in building inventory (future: map to Commodity enum)
            }

            // Deduct operating cost from owning company, credit a LocalServices supplier in the same region
            let owner_id = building.owner_id.clone();
            let op_cost = landfill_data.operating_cost;
            let debited = debit_company_by_id(companies, &owner_id, op_cost);
            if debited > 0.0 {
                let supplier_id = companies
                    .iter()
                    .find(|c| c.sector == Sector::LocalServices && c.region_id == region_id)
                    .map(|c| c.id.clone());
                if let Some(sid) = supplier_id {
                    credit_company_by_id(companies, &sid, debited);
                }
            }
        }

        result.waste_processed.insert(region_id.clone(), processed_total);
        result.waste_overflow.insert(region_id.clone(), overflow_total);
        result.pollution_generated.insert(region_id.clone(), pollution_total);
        if !all_recovered.is_empty() {
            result.commodities_recovered.insert(region_id, all_recovered);
        }
    }

    result
}

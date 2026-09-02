//! Phase 7: Infrastructure funding and production logic.
//!
//! This module implements the universal ownership model for infrastructure
//! buildings (education, healthcare, municipal), where funding comes from
//! the owner_id's treasury (State, Local Gov, or Private Company).

use crate::economy::infrastructure_config::InfrastructureConfig;
use crate::entities::Building;
use crate::registries::enums::Commodity;
use crate::state::treasury::Treasury;
use std::collections::BTreeMap;

/// Allocates infrastructure funding from owner to buildings.
///
/// # Arguments
/// * `buildings` - Slice of buildings requiring funding
/// * `treasury` - Central State treasury (for State-owned buildings)
/// * `local_governments` - Map of local government treasuries (for locally-owned buildings)
/// * `companies` - Map of company treasuries (for privately-owned buildings)
///
/// # Returns
/// Updated treasuries with funding allocations deducted
///
/// # Rules
/// * Universal Ownership: Funding comes from owner_id's treasury
/// * Double-Entry: owner.available_cash decreases, building.reserve increases
/// * Insolvency Guard: If owner lacks cash, building receives no funding
pub fn allocate_owner_infrastructure_funding(
    buildings: &mut [Building],
    treasury: &mut Treasury,
    local_governments: &mut BTreeMap<String, f64>,
    companies: &mut BTreeMap<String, f64>,
    config: &InfrastructureConfig,
    average_wage: f64,
) {
    for building in buildings.iter_mut() {
        let owner_id = &building.owner_id;
        let funding_amount = calculate_funding_requirement(building, config, average_wage);

        // Determine funding source based on owner_id
        if owner_id.starts_with("STATE_") {
            // Central State owned
            if treasury.liquid_reserves >= funding_amount {
                treasury.liquid_reserves -= funding_amount;
                building.reserve += funding_amount;
            }
        } else if owner_id.starts_with("LOCAL_") {
            // Local Government owned
            if let Some(local_cash) = local_governments.get_mut(owner_id) {
                if *local_cash >= funding_amount {
                    *local_cash -= funding_amount;
                    building.reserve += funding_amount;
                }
            }
        } else {
            // Private Company owned
            if let Some(company_cash) = companies.get_mut(owner_id) {
                if *company_cash >= funding_amount {
                    *company_cash -= funding_amount;
                    building.reserve += funding_amount;
                }
            }
        }
    }
}

/// Calculates funding requirement for a building based on its operating costs.
///
/// # Arguments
/// * `building` - The building to calculate funding for
/// * `config` - Infrastructure config with wage multipliers
/// * `average_wage` - Current national average wage (Rule 2: dynamic scaling)
///
/// # Returns
/// Required funding amount for this turn
///
/// # Rules
/// * Phase C.1: Cost per worker = `average_wage * sector_multiplier`
/// * Total = `worker_capacity * cost_per_worker` (Rule 15: physical scaling)
fn calculate_funding_requirement(
    building: &Building,
    config: &InfrastructureConfig,
    average_wage: f64,
) -> f64 {
    let cost_per_worker = config.cost_per_worker(building.sector, average_wage);
    building.worker_capacity as f64 * cost_per_worker
}

/// Submits B2B procurement orders for infrastructure buildings.
///
/// # Arguments
/// * `buildings` - Slice of buildings needing inputs
/// * `order_book` - B2B order book to submit orders to
///
/// # Rules
/// * Buildings submit buy orders for their required inputs
/// * Orders funded from building.reserve
/// * Physical Limits: No reserve = no orders submitted
pub fn submit_infrastructure_procurement_orders(
    buildings: &[Building],
    order_book: &mut BTreeMap<Commodity, Vec<(String, f64, f64)>>,
) {
    for building in buildings {
        if building.reserve <= 0.0 {
            continue; // No funding, skip
        }

        // Get input requirements from active method
        let inputs = &building.active_method.inputs;

        for (commodity, quantity_per_1000) in inputs.iter() {
            let required_quantity = quantity_per_1000 * (building.worker_capacity as f64 / 1000.0);
            let max_price = building.reserve / required_quantity;

            if max_price > 0.0 {
                order_book.entry(*commodity).or_default().push((
                    building.id.clone(),
                    required_quantity,
                    max_price,
                ));
            }
        }
    }
}

/// Executes infrastructure production using physical inputs.
///
/// # Arguments
/// * `buildings` - Slice of buildings to operate
/// * `inventories` - Building inventories (inputs consumed, outputs produced)
///
/// # Rules
/// * Physical Limits: Production limited by available inputs
/// * Outputs deposited in building inventory
/// * Innovation Points, Health Capacity, Education Slots are physical commodities
pub fn execute_infrastructure_production(
    buildings: &mut [Building],
    inventories: &mut BTreeMap<String, BTreeMap<Commodity, f64>>,
) {
    for building in buildings.iter_mut() {
        let building_inventory = inventories.entry(building.id.clone()).or_default();

        // Check input availability
        let inputs = &building.active_method.inputs;
        let mut fulfillment_ratio: f64 = 1.0_f64;

        for (commodity, required_quantity) in inputs.iter() {
            let available = building_inventory.get(commodity).copied().unwrap_or(0.0);
            let ratio: f64 = if *required_quantity > 0.0 {
                (available / required_quantity).min(1.0_f64)
            } else {
                1.0_f64
            };
            fulfillment_ratio = fulfillment_ratio.min(ratio);
        }

        // Consume inputs proportionally
        // Rule 20: Clamp to zero — inventory cannot go negative.
        for (commodity, required_quantity) in inputs.iter() {
            let consumed = required_quantity * fulfillment_ratio;
            let current = building_inventory.entry(*commodity).or_insert(0.0);
            *current = (*current - consumed).max(0.0);
        }

        // Produce outputs proportionally
        let outputs = &building.active_method.outputs;
        for (commodity, base_quantity) in outputs.iter() {
            let produced = base_quantity * fulfillment_ratio * building.active_method.efficiency;
            *building_inventory.entry(*commodity).or_insert(0.0) += produced;
        }

        // Update last production tracking
        building.last_production = outputs
            .iter()
            .map(|(c, q)| {
                (
                    *c,
                    q * fulfillment_ratio * building.active_method.efficiency,
                )
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::enums::Sector;

    #[test]
    fn funding_requirement_calculation() {
        let mut building = Building::default();
        building.sector = Sector::EducationalServices;
        building.worker_capacity = 100;

        let requirement =
            calculate_funding_requirement(&building, &InfrastructureConfig::default(), 1000.0);
        assert_eq!(requirement, 1_000_000.0); // 100 workers * 1000 wage * 10 multiplier
    }

    #[test]
    fn state_owned_building_funding() {
        let mut building = Building::default();
        building.owner_id = "STATE_CENTRAL".to_string();
        building.worker_capacity = 10;
        building.sector = Sector::EducationalServices;

        // Phase C.1: cost = 10 workers * 1000 wage * 10 multiplier = 100,000
        let mut treasury = Treasury {
            liquid_reserves: 200_000.0,
            ..Default::default()
        };

        let mut local_governments = BTreeMap::new();
        let mut companies = BTreeMap::new();

        let mut buildings = vec![building];
        allocate_owner_infrastructure_funding(
            &mut buildings,
            &mut treasury,
            &mut local_governments,
            &mut companies,
            &InfrastructureConfig::default(),
            1000.0, // average_wage for test
        );

        // 10 workers * 1000 wage * 10 edu_multiplier = 100,000 deducted
        assert_eq!(treasury.liquid_reserves, 100_000.0);
    }

    #[test]
    fn insolvency_guard_no_funding() {
        let mut building = Building::default();
        building.owner_id = "STATE_CENTRAL".to_string();
        building.worker_capacity = 100;
        building.sector = Sector::EducationalServices;

        let mut treasury = Treasury {
            liquid_reserves: 500.0, // Insufficient for 10000 requirement
            ..Default::default()
        };

        let mut local_governments = BTreeMap::new();
        let mut companies = BTreeMap::new();

        let mut buildings = vec![building];
        allocate_owner_infrastructure_funding(
            &mut buildings,
            &mut treasury,
            &mut local_governments,
            &mut companies,
            &InfrastructureConfig::default(),
            1000.0, // average_wage for test
        );

        assert_eq!(treasury.liquid_reserves, 500.0); // No deduction
        assert_eq!(buildings[0].reserve, 0.0); // No funding received
    }
}

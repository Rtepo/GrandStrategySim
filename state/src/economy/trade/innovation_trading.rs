//! Phase 7: Innovation Points B2B trading.
//!
//! This module implements the physical commodity trading of Innovation Points
//! between universities (producers) and the State (consumer).

use crate::economy::innovation_config::InnovationConfig;
use crate::entities::Building;
use crate::registries::enums::Commodity;
use crate::state::treasury::Treasury;
use std::collections::BTreeMap;

/// Trades Innovation Points via B2B market.
///
/// # Arguments
/// * `buildings` - Slice of buildings (universities) with Innovation Points in inventory
/// * `treasury` - Central State treasury
/// * `building_inventories` - Building inventories containing Innovation Points
///
/// # Returns
/// Updated treasury and building inventories after trading
///
/// # Rules
/// * Physical Limits: Innovation Points are physical commodities in inventory
/// * State must buy via B2B if not owned directly
/// * If State owns university, can transfer directly without B2B
/// * Double-Entry: Treasury cash decreases, building reserve increases
pub fn trade_innovation_points_b2b(
    buildings: &mut [Building],
    treasury: &mut Treasury,
    building_inventories: &mut BTreeMap<String, BTreeMap<Commodity, f64>>,
    config: &InnovationConfig,
) {
    for building in buildings.iter_mut() {
        let building_inventory = building_inventories.entry(building.id.clone()).or_insert_with(BTreeMap::new);
        let available_points = building_inventory.get(&Commodity::InnovationPoints).copied().unwrap_or(0.0);
        
        if available_points <= 0.0 {
            continue;
        }
        
        // Check if State owns this building
        if building.owner_id.starts_with("STATE_") {
            // Direct transfer: State owns the university
            // Transfer Innovation Points to Treasury.science.innovation_points
            treasury.science.innovation_points += available_points;
            *building_inventory.entry(Commodity::InnovationPoints).or_insert(0.0) = 0.0;
        } else {
            // B2B purchase: State must buy from Local Gov or Private owner
            let price_per_point = config.innovation_point_price;
            let total_cost = available_points * price_per_point;
            
            if treasury.liquid_reserves >= total_cost {
                // State can afford purchase
                treasury.liquid_reserves -= total_cost;
                treasury.science.innovation_points += available_points;
                building.reserve += total_cost;
                *building_inventory.entry(Commodity::InnovationPoints).or_insert(0.0) = 0.0;
            }
            // If State cannot afford, points remain in building inventory (unsold)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::enums::Sector;

    #[test]
    fn state_owned_university_direct_transfer() {
        let mut building = Building::default();
        building.id = "UNI_001".to_string();
        building.owner_id = "STATE_CENTRAL".to_string();
        building.sector = Sector::EducationalServices;
        
        let mut treasury = Treasury {
            liquid_reserves: 10000.0,
            science: crate::state::treasury::ScienceState {
                innovation_points: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };
        
        let mut building_inventories = BTreeMap::new();
        building_inventories.insert(
            "UNI_001".to_string(),
            BTreeMap::from([(Commodity::InnovationPoints, 50.0)]),
        );
        
        trade_innovation_points_b2b(
            &mut [building],
            &mut treasury,
            &mut building_inventories,
            &InnovationConfig::default(),
        );
        
        assert_eq!(treasury.science.innovation_points, 50.0);
        assert_eq!(treasury.liquid_reserves, 10000.0); // No cost for direct transfer
        assert_eq!(
            building_inventories["UNI_001"].get(&Commodity::InnovationPoints).copied().unwrap_or(0.0),
            0.0
        );
    }

    #[test]
    fn private_university_b2b_purchase() {
        let mut building = Building::default();
        building.id = "UNI_002".to_string();
        building.owner_id = "COMPANY_PHARMA".to_string();
        building.sector = Sector::EducationalServices;
        
        let mut treasury = Treasury {
            liquid_reserves: 10000.0,
            science: crate::state::treasury::ScienceState {
                innovation_points: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };
        
        let mut building_inventories = BTreeMap::new();
        building_inventories.insert(
            "UNI_002".to_string(),
            BTreeMap::from([(Commodity::InnovationPoints, 50.0)]),
        );
        
        let mut buildings = vec![building];
        trade_innovation_points_b2b(
            &mut buildings,
            &mut treasury,
            &mut building_inventories,
            &InnovationConfig::default(),
        );

        assert_eq!(treasury.science.innovation_points, 50.0);
        assert_eq!(treasury.liquid_reserves, 5000.0); // 50 * 100 deducted
        assert_eq!(buildings[0].reserve, 5000.0); // Building receives payment
    }

    #[test]
    fn insufficient_cash_no_purchase() {
        let mut building = Building::default();
        building.id = "UNI_003".to_string();
        building.owner_id = "COMPANY_PHARMA".to_string();
        building.sector = Sector::EducationalServices;
        
        let mut treasury = Treasury {
            liquid_reserves: 1000.0, // Insufficient for 50 * 100 = 5000
            science: crate::state::treasury::ScienceState {
                innovation_points: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };
        
        let mut building_inventories = BTreeMap::new();
        building_inventories.insert(
            "UNI_003".to_string(),
            BTreeMap::from([(Commodity::InnovationPoints, 50.0)]),
        );
        
        let mut buildings = vec![building];
        trade_innovation_points_b2b(
            &mut buildings,
            &mut treasury,
            &mut building_inventories,
            &InnovationConfig::default(),
        );

        assert_eq!(treasury.science.innovation_points, 0.0); // No purchase
        assert_eq!(treasury.liquid_reserves, 1000.0); // Cash unchanged
        assert_eq!(
            building_inventories["UNI_003"].get(&Commodity::InnovationPoints).copied().unwrap_or(0.0),
            50.0
        ); // Points remain unsold
    }
}

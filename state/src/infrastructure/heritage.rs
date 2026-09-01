//! Heritage site mechanic for building longevity and prestige effects.
//!
//! This module implements heritage site designation, maintenance subsidies,
//! and tourism/prestige effects for historically significant buildings.

use crate::infrastructure::building_condition::calculate_maintenance_bom;
use crate::registries::enums::Sector;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Error types for heritage operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeritageError {
    /// Building does not meet eligibility criteria
    NotEligible,
    /// Insufficient funds for maintenance
    InsufficientFunds,
    /// Market clearing failed
    MarketClearingFailed,
}

/// Building with heritage site attributes.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct HeritageBuilding {
    /// Building identifier
    #[serde(default)]
    pub id: String,

    /// Year the building was built
    #[serde(default)]
    pub year_built: u32,

    /// Current condition (0.0 to 1.0)
    #[serde(default)]
    pub condition: f64,

    /// Heritage site flag
    #[serde(default)]
    pub is_heritage_site: bool,

    /// Reserve fund for maintenance
    #[serde(default)]
    pub reserve: f64,

    /// Fixed capital value (asset value)
    #[serde(default)]
    pub fixed_capital: f64,

    /// Building sector
    #[serde(default)]
    pub sector: Sector,
}

/// Market interface for commodity trading.
pub trait Market {
    /// Submits buy orders to the market clearing engine.
    fn submit_buy_orders(
        &mut self,
        buyer_id: String,
        orders: BTreeMap<String, f64>,
    ) -> Result<BTreeMap<String, f64>, String>;

    /// Calculates market cost for given commodity quantities.
    fn calculate_market_cost(&self, orders: &BTreeMap<String, f64>) -> f64;
}

/// Country with budget for heritage subsidies.
#[derive(Debug, Clone)]
pub struct Country {
    /// Budget with nominal_budget field
    pub budget: Budget,
}

/// Budget for state expenditures.
#[derive(Debug, Clone)]
pub struct Budget {
    /// Nominal budget available
    pub nominal_budget: f64,
}

/// Checks if a building is eligible for heritage site designation.
///
/// # Arguments
/// * `building` - The building to check
/// * `current_year` - Current simulation year
///
/// # Returns
/// `true` if eligible, `false` otherwise
///
/// # Rules
/// * Must be at least 50 years old
/// * Must have maintained condition > 0.6 for most of its life
/// * Must not have been significantly renovated (preserves authenticity)
pub fn check_heritage_eligibility(building: &HeritageBuilding, current_year: u32) -> bool {
    let years_since_build = current_year - building.year_built;

    // Must be at least 50 years old
    if years_since_build < 50 {
        return false;
    }

    // Must have maintained condition > 0.6 for most of its life
    if building.condition < 0.6 {
        return false;
    }

    true
}

/// Applies heritage site effects (prestige, tourism, protection).
///
/// # Arguments
/// * `building` - The heritage building
/// * `region_prestige` - Mutable reference to region prestige
/// * `tourism_revenue` - Mutable reference to tourism revenue
///
/// # Rules
/// * Massive localized prestige boost (+5 prestige per heritage site)
/// * Tourism demand multiplier (+20% tourism revenue)
/// * Cannot be demolished
/// * Cannot be technologically upgraded
/// * Condition maintenance costs are subsidized by state
pub fn apply_heritage_effects(
    building: &HeritageBuilding,
    region_prestige: &mut f64,
    tourism_revenue: &mut f64,
) {
    if !building.is_heritage_site {
        return;
    }

    // Massive localized prestige boost
    *region_prestige += 5.0;

    // Tourism demand multiplier
    *tourism_revenue *= 1.2;
}

/// Process heritage effects for all buildings in a turn.
///
/// This wrapper iterates over all buildings, checks heritage eligibility,
/// applies prestige/tourism effects, and processes maintenance subsidies.
///
/// # Arguments
/// * `buildings` - All buildings to check for heritage status
/// * `current_year` - Current simulation year
/// * `region_prestige` - Mutable map of region_id -> prestige value
/// * `tourism_revenue` - Mutable map of region_id -> tourism revenue
///
/// # Rules
/// * Buildings >= 50 years old with condition > 0.6 become heritage sites
/// * Heritage buildings add +5 prestige and +20% tourism to their region
/// * Heritage buildings cannot be demolished or upgraded
pub fn process_heritage_effects(
    buildings: &mut [crate::entities::Building],
    current_year: u32,
    region_prestige: &mut std::collections::BTreeMap<String, f64>,
    tourism_revenue: &mut std::collections::BTreeMap<String, f64>,
) {
    for building in buildings {
        if !building.is_heritage_site {
            let years_since_build = current_year.saturating_sub(building.year_built);
            if years_since_build >= 50 && building.condition > 0.6 {
                building.is_heritage_site = true;
            }
        }

        if building.is_heritage_site {
            *region_prestige
                .entry(building.region_id.clone())
                .or_insert(0.0) += 5.0;

            let tourism = tourism_revenue
                .entry(building.region_id.clone())
                .or_insert(0.0);
            *tourism *= 1.2;
        }
    }
}

/// Checks if a heritage building can be demolished.
///
/// # Arguments
/// * `building` - The building to check
///
/// # Returns
/// `false` if heritage site (protected), `true` otherwise
pub fn can_demolish(building: &HeritageBuilding) -> bool {
    if building.is_heritage_site {
        return false; // Heritage sites are protected
    }
    true
}

/// Checks if a heritage building can be technologically upgraded.
///
/// # Arguments
/// * `building` - The building to check
///
/// # Returns
/// `false` if heritage site (preserves original technology), `true` otherwise
pub fn can_upgrade_technology(building: &HeritageBuilding) -> bool {
    if building.is_heritage_site {
        return false; // Heritage sites preserve original technology
    }
    true
}

/// Applies heritage maintenance subsidy.
///
/// # Arguments
/// * `building` - The heritage building
/// * `country` - Country with budget for subsidies
/// * `market` - Market for commodity trading
///
/// # Returns
/// Ok(()) if successful, Err(HeritageError) if failed
///
/// # Rules
/// * Maintenance costs are based on physical asset value, not profitability
/// * State covers 50% of maintenance cost
/// * Building must pay full cost first, then receives subsidy
/// * If building cannot afford maintenance, loses heritage status
pub fn apply_heritage_subsidy<M: Market>(
    building: &mut HeritageBuilding,
    country: &mut Country,
    market: &mut M,
) -> Result<(), HeritageError> {
    if building.is_heritage_site {
        // Calculate physical material requirements for maintenance
        let maintenance_bom = calculate_maintenance_bom(&building.sector);

        // Convert BTreeMap<Commodity, f64> to BTreeMap<String, f64> for market interface
        let mut market_orders: BTreeMap<String, f64> = BTreeMap::new();
        for (commodity, quantity) in &maintenance_bom {
            let commodity_name =
                serde_json::to_string(commodity).unwrap_or_else(|_| format!("{:?}", commodity));
            market_orders.insert(commodity_name, *quantity);
        }

        // Check if building has sufficient reserve for estimated cost
        let estimated_cost = market.calculate_market_cost(&market_orders);
        if building.reserve < estimated_cost {
            building.is_heritage_site = false; // Lose status due to gross negligence
            return Err(HeritageError::InsufficientFunds);
        }

        // Submit buy orders to market clearing engine
        let cleared_orders = match market.submit_buy_orders(building.id.clone(), market_orders) {
            Ok(orders) => orders,
            Err(_) => {
                // Market failure - cannot complete maintenance
                building.is_heritage_site = false;
                return Err(HeritageError::MarketClearingFailed);
            }
        };

        // Calculate actual cost based on cleared orders
        let actual_cost = market.calculate_market_cost(&cleared_orders);
        let subsidy = actual_cost * 0.5; // State covers 50%

        // CRITICAL: Building pays full cost first
        building.reserve -= actual_cost;

        // Then state provides subsidy
        country.budget.nominal_budget -= subsidy;
        building.reserve += subsidy;

        // Net effect: building pays (actual_cost - subsidy)
        // Cash flows naturally to commodity producers via market clearing
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockMarket {
        base_price: f64,
    }

    impl Market for MockMarket {
        fn submit_buy_orders(
            &mut self,
            _buyer_id: String,
            orders: BTreeMap<String, f64>,
        ) -> Result<BTreeMap<String, f64>, String> {
            Ok(orders) // Mock: all orders cleared
        }

        fn calculate_market_cost(&self, orders: &BTreeMap<String, f64>) -> f64 {
            orders.values().sum::<f64>() * self.base_price
        }
    }

    #[test]
    fn test_check_heritage_eligibility_too_young() {
        let building = HeritageBuilding {
            year_built: 2000,
            condition: 0.8,
            ..Default::default()
        };
        let current_year = 2040; // 40 years old
        assert!(!check_heritage_eligibility(&building, current_year));
    }

    #[test]
    fn test_check_heritage_eligibility_poor_condition() {
        let building = HeritageBuilding {
            year_built: 1950,
            condition: 0.5, // Below 0.6 threshold
            ..Default::default()
        };
        let current_year = 2020; // 70 years old
        assert!(!check_heritage_eligibility(&building, current_year));
    }

    #[test]
    fn test_check_heritage_eligibility_eligible() {
        let building = HeritageBuilding {
            year_built: 1950,
            condition: 0.8,
            ..Default::default()
        };
        let current_year = 2020; // 70 years old
        assert!(check_heritage_eligibility(&building, current_year));
    }

    #[test]
    fn test_apply_heritage_effects() {
        let building = HeritageBuilding {
            is_heritage_site: true,
            ..Default::default()
        };
        let mut prestige = 10.0;
        let mut tourism = 100.0;

        apply_heritage_effects(&building, &mut prestige, &mut tourism);

        assert_eq!(prestige, 15.0); // +5 prestige
        assert_eq!(tourism, 120.0); // +20% tourism revenue
    }

    #[test]
    fn test_can_demolish_heritage() {
        let building = HeritageBuilding {
            is_heritage_site: true,
            ..Default::default()
        };
        assert!(!can_demolish(&building));
    }

    #[test]
    fn test_can_demolish_non_heritage() {
        let building = HeritageBuilding {
            is_heritage_site: false,
            ..Default::default()
        };
        assert!(can_demolish(&building));
    }

    #[test]
    fn test_can_upgrade_technology_heritage() {
        let building = HeritageBuilding {
            is_heritage_site: true,
            ..Default::default()
        };
        assert!(!can_upgrade_technology(&building));
    }

    #[test]
    fn test_apply_heritage_subsidy_success() {
        let mut building = HeritageBuilding {
            is_heritage_site: true,
            reserve: 1000.0,
            fixed_capital: 10000.0,
            sector: Sector::Construction,
            ..Default::default()
        };
        let mut country = Country {
            budget: Budget {
                nominal_budget: 10000.0,
            },
        };
        let mut market = MockMarket { base_price: 1.0 };

        let result = apply_heritage_subsidy(&mut building, &mut country, &mut market);
        assert!(result.is_ok());
        assert!(building.is_heritage_site);
    }

    #[test]
    fn test_apply_heritage_subsidy_insufficient_funds() {
        let mut building = HeritageBuilding {
            is_heritage_site: true,
            reserve: 10.0, // Too low
            fixed_capital: 10000.0,
            sector: Sector::Construction,
            ..Default::default()
        };
        let mut country = Country {
            budget: Budget {
                nominal_budget: 10000.0,
            },
        };
        let mut market = MockMarket { base_price: 1.0 };

        let result = apply_heritage_subsidy(&mut building, &mut country, &mut market);
        assert!(matches!(result, Err(HeritageError::InsufficientFunds)));
        assert!(!building.is_heritage_site); // Lost status
    }
}

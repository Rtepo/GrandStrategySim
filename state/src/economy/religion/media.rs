//! Phase 18C: Media sector B2C service clearing.
//!
//! This module implements the B2C clearing for `Commodity::Information`,
//! produced by `MediaAndEntertainment`-sector companies. Citizens consume
//! information like education/health, paying from their savings.
//!
//! When a `PropagandaConfig` is active, the state can subsidize the B2C shelf
//! price to near-zero, ensuring maximum consumption. The propaganda effects
//! fire proportionally to the actual consumption ratio.

use crate::economy::service_config::ServicePricingConfig;
use crate::economy::transfer_settler::{credit_company_by_id, debit_citizen_savings_region};
use crate::entities::{Building, Company};
use crate::registries::enums::{Commodity, Sector};
use crate::state::Country;
use std::collections::BTreeMap;

/// Result of information B2C clearing.
#[derive(Debug, Clone, Default)]
pub struct InformationB2cResult {
    /// Total units of information consumed across all regions.
    pub total_consumed: f64,
    /// Total information needed across all regions.
    pub total_needed: f64,
    /// Consumption ratio (consumed / needed), clamped to [0, 1].
    pub consumption_ratio: f64,
    /// Total citizen payments for information.
    pub citizen_payments: f64,
    /// Total government subsidy paid to media companies.
    pub government_subsidy: f64,
    /// Per-region consumption map.
    pub region_consumption: BTreeMap<String, f64>,
}

/// Populates information service needs from regional demographics.
///
/// # Arguments
/// * `country` - Country with regions and class demographics
///
/// # Returns
/// Map of region_id → information units needed.
///
/// # Rules
/// * Each person generates base information demand of 0.05 units.
/// * Urban classes consume 1.5x more information (city media access).
/// * Low savings_per_capita (< 50) reduces demand by 30% (can't afford media).
pub fn populate_information_service_needs(country: &Country) -> BTreeMap<String, f64> {
    let mut needs = BTreeMap::new();
    for region in &country.regions {
        let mut info_need = 0.0_f64;
        for class in region.class_demographics.rural_classes.values() {
            info_need += calculate_information_need_for_class(class);
        }
        for class in region.class_demographics.urban_classes.values() {
            info_need += calculate_information_need_for_class(class) * 1.5;
        }
        needs.insert(region.id.clone(), info_need);
    }
    needs
}

/// Calculates information need for a single demographic class.
fn calculate_information_need_for_class(
    class: &crate::society::geography::ClassDemographics,
) -> f64 {
    let base = class.population as f64 * 0.05;
    let poverty_mult = if class.savings_per_capita < 50.0 {
        0.7
    } else {
        1.0
    };
    base * poverty_mult
}

/// Pending information service transaction.
struct PendingInfoTxn {
    building_id: String,
    owner_id: String,
    region_id: String,
    citizen_payment: f64,
    government_subsidy: f64,
    units_consumed: f64,
    is_public: bool,
}

/// Executes B2C clearing for Commodity::Information.
///
/// # Arguments
/// * `buildings` - Slice of media buildings with Information inventory
/// * `companies` - Mutable companies (for crediting private building revenue)
/// * `country` - Mutable country (for citizen savings and government subsidy)
/// * `service_needs` - Information service needs by region
/// * `building_inventories` - Mutable building inventories
/// * `config` - Service pricing configuration
/// * `propaganda_subsidy_rate` - If > 0, state subsidizes this fraction of price (0.0 = no subsidy, 0.95 = near-free)
///
/// # Rules
/// * MediaAndEntertainment-sector buildings produce Information.
/// * State-owned media buildings (owner_id starts with STATE_ or LOCAL_) get full subsidy.
/// * Private media buildings charge market price unless propaganda subsidy is active.
/// * Insolvency Guard: If Treasury runs out of cash, subsidy fails gracefully.
/// * Returns consumption ratio for propaganda effect scaling.
pub fn clear_information_b2c(
    buildings: &mut [Building],
    companies: &mut [Company],
    country: &mut Country,
    service_needs: &BTreeMap<String, f64>,
    building_inventories: &mut BTreeMap<String, BTreeMap<Commodity, f64>>,
    config: &ServicePricingConfig,
    propaganda_subsidy_rate: f64,
) -> InformationB2cResult {
    let commodity = Commodity::Information;
    // Phase C.2: Dynamic cost-plus pricing (Rule 2/21).
    let average_wage = country.macro_indicators.average_wage.max(1.0);
    let price_per_unit = config.information_price_per_unit(average_wage);
    let mut txns = Vec::new();

    for building in buildings.iter() {
        // Only process MediaAndEntertainment sector buildings
        if building.sector != Sector::MediaAndEntertainment {
            continue;
        }

        let available = building_inventories
            .get(&building.id)
            .and_then(|inv| inv.get(&commodity).copied())
            .unwrap_or(0.0);

        if available <= 0.0 || price_per_unit <= 0.0 {
            continue;
        }

        let region_id = &building.region_id;
        let service_need = service_needs.get(region_id).copied().unwrap_or(0.0);
        let is_state_media =
            building.owner_id.starts_with("STATE_") || building.owner_id.starts_with("LOCAL_");

        let total_citizen_savings = country
            .regions
            .iter()
            .find(|r| &r.id == region_id)
            .map(|r| {
                r.class_demographics
                    .rural_classes
                    .values()
                    .map(|d| d.savings)
                    .sum::<f64>()
                    + r.class_demographics
                        .urban_classes
                        .values()
                        .map(|d| d.savings)
                        .sum::<f64>()
            })
            .unwrap_or(0.0);

        // Determine subsidy: state media always subsidized; private media subsidized only when propaganda is active
        let subsidy_rate = if is_state_media {
            1.0 // State media is always free to citizens
        } else if propaganda_subsidy_rate > 0.0 {
            propaganda_subsidy_rate
        } else {
            0.0
        };

        let subsidized_price = price_per_unit * (1.0 - subsidy_rate);
        let subsidy_per_unit = price_per_unit - subsidized_price;

        if subsidy_per_unit > 0.0 {
            // Subsidized path: government pays the difference
            let total_subsidy_needed = available * subsidy_per_unit;
            let gov_cash = country.budget.liquid_reserves;

            if gov_cash >= total_subsidy_needed {
                // Full subsidy — citizens pay near-zero
                let citizen_price = subsidized_price;
                let affordable = if citizen_price > 0.0 {
                    (total_citizen_savings / citizen_price).floor()
                } else {
                    available // Free to citizens
                };
                let consumed = affordable.min(available).min(service_need);
                txns.push(PendingInfoTxn {
                    building_id: building.id.clone(),
                    owner_id: building.owner_id.clone(),
                    region_id: building.region_id.clone(),
                    citizen_payment: consumed * citizen_price,
                    government_subsidy: consumed * subsidy_per_unit,
                    units_consumed: consumed,
                    is_public: is_state_media,
                });
            } else {
                // Insolvency guard: partial subsidy — government pays what it can
                let affordable_subsidy_units = (gov_cash / subsidy_per_unit).floor();
                let subsidized_units = affordable_subsidy_units.min(available).min(service_need);
                let remaining_units = available.min(service_need) - subsidized_units;

                if subsidized_units > 0.0 {
                    txns.push(PendingInfoTxn {
                        building_id: building.id.clone(),
                        owner_id: building.owner_id.clone(),
                        region_id: building.region_id.clone(),
                        citizen_payment: subsidized_units * subsidized_price,
                        government_subsidy: subsidized_units * subsidy_per_unit,
                        units_consumed: subsidized_units,
                        is_public: is_state_media,
                    });
                }

                // Remaining units at full price
                if remaining_units > 0.0 {
                    let affordable = (total_citizen_savings / price_per_unit).floor();
                    let consumed = affordable.min(remaining_units);
                    txns.push(PendingInfoTxn {
                        building_id: building.id.clone(),
                        owner_id: building.owner_id.clone(),
                        region_id: building.region_id.clone(),
                        citizen_payment: consumed * price_per_unit,
                        government_subsidy: 0.0,
                        units_consumed: consumed,
                        is_public: is_state_media,
                    });
                }
            }
        } else {
            // No subsidy — citizens pay full price
            let affordable = (total_citizen_savings / price_per_unit).floor();
            let consumed = affordable.min(available).min(service_need);
            txns.push(PendingInfoTxn {
                building_id: building.id.clone(),
                owner_id: building.owner_id.clone(),
                region_id: building.region_id.clone(),
                citizen_payment: consumed * price_per_unit,
                government_subsidy: 0.0,
                units_consumed: consumed,
                is_public: is_state_media,
            });
        }
    }

    // Aggregate consumption per region
    let mut region_consumption: BTreeMap<String, f64> = BTreeMap::new();
    let mut total_consumed = 0.0_f64;
    let mut total_citizen_payments = 0.0_f64;
    let mut total_gov_subsidy = 0.0_f64;

    for txn in &txns {
        if txn.units_consumed > 0.0 {
            *region_consumption
                .entry(txn.region_id.clone())
                .or_insert(0.0) += txn.units_consumed;
            total_consumed += txn.units_consumed;
            total_citizen_payments += txn.citizen_payment;
            total_gov_subsidy += txn.government_subsidy;
        }
    }

    // Apply transactions (write phase)
    for txn in &txns {
        if txn.units_consumed <= 0.0 {
            continue;
        }

        // Update inventory
        if let Some(inv) = building_inventories.get_mut(&txn.building_id) {
            *inv.entry(commodity).or_insert(0.0) -= txn.units_consumed;
        }

        // Debit citizen savings
        if txn.citizen_payment > 0.0 {
            if let Some(region) = country.regions.iter_mut().find(|r| r.id == txn.region_id) {
                debit_citizen_savings_region(region, txn.citizen_payment);
            }
        }

        // Debit government treasury
        if txn.government_subsidy > 0.0 {
            country.budget.liquid_reserves -= txn.government_subsidy;
        }

        // Credit revenue
        let total_revenue = txn.citizen_payment + txn.government_subsidy;
        if total_revenue <= 0.0 {
            continue;
        }

        if txn.is_public {
            if let Some(building) = buildings.iter_mut().find(|b| b.id == txn.building_id) {
                building.reserve += total_revenue;
            }
        } else {
            credit_company_by_id(companies, &txn.owner_id, total_revenue);
        }
    }

    let total_needed: f64 = service_needs.values().sum();
    let consumption_ratio = if total_needed > 0.0 {
        (total_consumed / total_needed).min(1.0)
    } else {
        0.0
    };

    InformationB2cResult {
        total_consumed,
        total_needed,
        consumption_ratio,
        citizen_payments: total_citizen_payments,
        government_subsidy: total_gov_subsidy,
        region_consumption,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::{ClassDemographics, Region};

    fn make_test_country(region_id: &str, citizen_savings: f64, gov_reserves: f64) -> Country {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = gov_reserves;
        // Phase C.2: Set average_wage for dynamic cost-plus pricing (Rule 2).
        country.macro_indicators.average_wage = 1000.0;
        country.regions.clear();
        let mut region = Region::default();
        region.id = region_id.to_string();
        let mut demo = ClassDemographics::default();
        demo.savings = citizen_savings;
        demo.savings_per_capita = citizen_savings;
        demo.population = 100;
        region
            .class_demographics
            .urban_classes
            .insert("workers".to_string(), demo);
        country.regions.push(region);
        country
    }

    #[test]
    fn test_private_media_revenue_no_subsidy() {
        let mut building = Building::default();
        building.id = "MEDIA_001".to_string();
        building.owner_id = "COMPANY_MEDIA".to_string();
        building.sector = Sector::MediaAndEntertainment;
        building.region_id = "REGION_001".to_string();

        let mut country = make_test_country("REGION_001", 1000.0, 0.0);
        let needs = BTreeMap::from([("REGION_001".to_string(), 100.0)]);
        let mut inventories = BTreeMap::from([(
            "MEDIA_001".to_string(),
            BTreeMap::from([(Commodity::Information, 50.0)]),
        )]);

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let result = clear_information_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &needs,
            &mut inventories,
            &ServicePricingConfig::default(),
            0.0,
        );

        // No subsidy: citizens pay 30.0 per unit. Affordable = 1000/30 = 33 units
        assert!(result.total_consumed > 0.0);
        assert_eq!(result.government_subsidy, 0.0);
        assert!(result.citizen_payments > 0.0);
    }

    #[test]
    fn test_state_media_subsidy_success() {
        let mut building = Building::default();
        building.id = "STATE_MEDIA_001".to_string();
        building.owner_id = "STATE_MEDIA".to_string();
        building.sector = Sector::MediaAndEntertainment;
        building.region_id = "REGION_001".to_string();

        let mut country = make_test_country("REGION_001", 100.0, 10000.0);
        let needs = BTreeMap::from([("REGION_001".to_string(), 100.0)]);
        let mut inventories = BTreeMap::from([(
            "STATE_MEDIA_001".to_string(),
            BTreeMap::from([(Commodity::Information, 50.0)]),
        )]);

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let result = clear_information_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &needs,
            &mut inventories,
            &ServicePricingConfig::default(),
            0.0, // State media is always fully subsidized regardless of propaganda config
        );

        // State media: 100% subsidy, citizens pay 0
        assert_eq!(result.citizen_payments, 0.0);
        assert!(result.government_subsidy > 0.0);
        assert_eq!(result.total_consumed, 50.0); // All consumed, need is 100, supply is 50
    }

    #[test]
    fn test_propaganda_subsidy_near_free() {
        let mut building = Building::default();
        building.id = "MEDIA_002".to_string();
        building.owner_id = "COMPANY_MEDIA".to_string();
        building.sector = Sector::MediaAndEntertainment;
        building.region_id = "REGION_001".to_string();

        let mut country = make_test_country("REGION_001", 10.0, 100000.0);
        let needs = BTreeMap::from([("REGION_001".to_string(), 100.0)]);
        let mut inventories = BTreeMap::from([(
            "MEDIA_002".to_string(),
            BTreeMap::from([(Commodity::Information, 50.0)]),
        )]);

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let result = clear_information_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &needs,
            &mut inventories,
            &ServicePricingConfig::default(),
            0.95, // 95% subsidy — near free
        );

        // Phase C.2: Dynamic price = (1000*0.02 + 1000*0.1/24) * 1.10 = 26.583...
        // With 95% subsidy: 26.583 * 0.05 = 1.329 per unit
        // Affordable = 10 / 1.329 = 7 units
        // consumption_ratio = 7/100 = 0.07 < 0.2
        assert!(result.total_consumed > 0.0);
        assert!(result.government_subsidy > 0.0);
        assert!(result.consumption_ratio < 0.2); // Low consumption due to low savings
    }

    #[test]
    fn test_insolvency_guard_subsidy_fails() {
        let mut building = Building::default();
        building.id = "MEDIA_003".to_string();
        building.owner_id = "COMPANY_MEDIA".to_string();
        building.sector = Sector::MediaAndEntertainment;
        building.region_id = "REGION_001".to_string();

        let mut country = make_test_country("REGION_001", 1000.0, 10.0); // Very low gov reserves
        let needs = BTreeMap::from([("REGION_001".to_string(), 100.0)]);
        let mut inventories = BTreeMap::from([(
            "MEDIA_003".to_string(),
            BTreeMap::from([(Commodity::Information, 50.0)]),
        )]);

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let result = clear_information_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &needs,
            &mut inventories,
            &ServicePricingConfig::default(),
            0.95, // 95% subsidy requested but Treasury is nearly empty
        );

        // Subsidy: 50 * 30 * 0.95 = 1425, but gov only has 10
        // Subsidized units = 10 / 28.5 = 0 units
        // Remaining at full price: affordable = 1000/30 = 33
        assert!(result.government_subsidy <= 10.0); // Can't spend more than it has
        assert!(result.citizen_payments > 0.0); // Citizens pay full price for most
    }
}

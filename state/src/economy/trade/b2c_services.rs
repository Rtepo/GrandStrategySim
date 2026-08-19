//! Phase 7: B2C service trading with insolvency guard.
//!
//! This module implements B2C trading of EducationSlots and HealthCapacity
//! with PriceIntervention for public services and insolvency guard for governments.

use crate::economy::service_config::ServicePricingConfig;
use crate::economy::transfer_settler::{credit_company_by_id, debit_citizen_savings_region};
use crate::entities::{Building, Company};
use crate::registries::enums::Commodity;
use crate::society::geography::{ClassDemographics, HealthStatus};
use crate::state::Country;
use std::collections::BTreeMap;

/// Populates education service needs from regional demographics.
///
/// # Arguments
/// * `country` - Country with regions and class demographics
///
/// # Returns
/// Map of region_id → education slots needed.
///
/// # Rules
/// * Each child/young adult in a demographic class generates 1.0 education need.
/// * Population is used as proxy: 20% of population needs education slots.
/// * Classes with poor health or low savings get bonus education need (retraining).
pub fn populate_education_service_needs(country: &Country) -> BTreeMap<String, f64> {
    let mut needs = BTreeMap::new();
    for region in &country.regions {
        let mut edu_need = 0.0_f64;
        for (_, class) in &region.class_demographics.rural_classes {
            edu_need += calculate_education_need_for_class(class);
        }
        for (_, class) in &region.class_demographics.urban_classes {
            edu_need += calculate_education_need_for_class(class) * 1.2;
        }
        needs.insert(region.id.clone(), edu_need);
    }
    needs
}

/// Populates health service needs from regional demographics.
///
/// # Arguments
/// * `country` - Country with regions and class demographics
///
/// # Returns
/// Map of region_id → health capacity needed.
///
/// # Rules
/// * Each person generates base health need of 0.1 capacity units.
/// * Health status multiplier: Critical=3.0, Poor=2.0, Fair=1.5, Good=0.8, Excellent=0.5.
/// * Low savings_per_capita (< 50) increases need by 50% (can't afford private care).
pub fn populate_health_service_needs(country: &Country) -> BTreeMap<String, f64> {
    let mut needs = BTreeMap::new();
    for region in &country.regions {
        let mut health_need = 0.0_f64;
        for (_, class) in &region.class_demographics.rural_classes {
            health_need += calculate_health_need_for_class(class);
        }
        for (_, class) in &region.class_demographics.urban_classes {
            health_need += calculate_health_need_for_class(class) * 1.3;
        }
        needs.insert(region.id.clone(), health_need);
    }
    needs
}

/// Calculates education need for a single demographic class.
fn calculate_education_need_for_class(class: &ClassDemographics) -> f64 {
    let base = class.population as f64 * 0.20;
    let health_bonus = match class.health_status {
        HealthStatus::Critical | HealthStatus::Poor => 1.3,
        _ => 1.0,
    };
    let poverty_bonus = if class.savings_per_capita < 50.0 { 1.5 } else { 1.0 };
    base * health_bonus * poverty_bonus
}

/// Calculates health need for a single demographic class.
fn calculate_health_need_for_class(class: &ClassDemographics) -> f64 {
    let base = class.population as f64 * 0.10;
    let health_mult = match class.health_status {
        HealthStatus::Critical => 3.0,
        HealthStatus::Poor => 2.0,
        HealthStatus::Fair => 1.5,
        HealthStatus::Good => 0.8,
        HealthStatus::Excellent => 0.5,
    };
    let poverty_mult = if class.savings_per_capita < 50.0 { 1.5 } else { 1.0 };
    base * health_mult * poverty_mult
}

/// Pending service transaction (computed in read-only phase, applied in write phase).
struct PendingServiceTxn {
    building_id: String,
    owner_id: String,
    region_id: String,
    citizen_payment: f64,
    government_subsidy: f64,
    units_consumed: f64,
    is_public: bool,
}

/// Executes B2C clearing for EducationSlots with insolvency guard.
///
/// # Arguments
/// * `buildings` - Slice of schools/universities with EducationSlots
/// * `companies` - Mutable companies (for crediting private building revenue)
/// * `country` - Mutable country (for citizen savings and government subsidy)
/// * `service_needs` - Education service needs by region
/// * `building_inventories` - Mutable building inventories
/// * `config` - Service pricing configuration
///
/// # Rules
/// * Public institutions use PriceIntervention with 100% buyer_subsidy
/// * Private institutions charge market price
/// * Insolvency Guard: If government runs out of cash, subsidy fails gracefully
/// * Citizens pay full price if subsidy fails
/// * Citizen savings are debited directly from ClassDemographics (Phase 16A)
/// * Private building revenue credits owner company's brokerage_account.cash (Phase 16A)
/// * Government subsidy debits from country.budget.liquid_reserves (Phase 16A)
/// * Returns map of region_id → total units consumed (Phase 17B: for assimilation coverage).
pub fn clear_education_slots_b2c(
    buildings: &mut [Building],
    companies: &mut [Company],
    country: &mut Country,
    service_needs: &BTreeMap<String, f64>,
    building_inventories: &mut BTreeMap<String, BTreeMap<Commodity, f64>>,
    config: &ServicePricingConfig,
) -> BTreeMap<String, f64> {
    let commodity = Commodity::EducationSlots;
    let txns = compute_service_transactions(
        buildings,
        country,
        service_needs,
        building_inventories,
        config,
        commodity,
    );
    // Aggregate units consumed per region before applying.
    let mut region_consumption: BTreeMap<String, f64> = BTreeMap::new();
    for txn in &txns {
        if txn.units_consumed > 0.0 {
            *region_consumption.entry(txn.region_id.clone()).or_insert(0.0) += txn.units_consumed;
        }
    }
    apply_service_transactions(txns, buildings, companies, country, building_inventories, commodity);
    region_consumption
}

/// Executes B2C clearing for HealthCapacity with insolvency guard.
///
/// # Arguments
/// * `buildings` - Slice of hospitals/clinics with HealthCapacity
/// * `companies` - Mutable companies (for crediting private building revenue)
/// * `country` - Mutable country (for citizen savings and government subsidy)
/// * `service_needs` - Health service needs by region
/// * `building_inventories` - Mutable building inventories
/// * `config` - Service pricing configuration
///
/// # Rules
/// * Public institutions use PriceIntervention with 100% buyer_subsidy
/// * Private institutions charge market price
/// * Insolvency Guard: If government runs out of cash, subsidy fails gracefully
/// * Citizens pay full price if subsidy fails
/// * Citizen savings are debited directly from ClassDemographics (Phase 16A)
/// * Private building revenue credits owner company's brokerage_account.cash (Phase 16A)
/// * Government subsidy debits from country.budget.liquid_reserves (Phase 16A)
pub fn clear_health_capacity_b2c(
    buildings: &mut [Building],
    companies: &mut [Company],
    country: &mut Country,
    service_needs: &BTreeMap<String, f64>,
    building_inventories: &mut BTreeMap<String, BTreeMap<Commodity, f64>>,
    config: &ServicePricingConfig,
) {
    let commodity = Commodity::HealthCapacity;
    let txns = compute_service_transactions(
        buildings,
        country,
        service_needs,
        building_inventories,
        config,
        commodity,
    );
    apply_service_transactions(txns, buildings, companies, country, building_inventories, commodity);
}

/// Compute service transactions (read-only phase).
///
/// Iterates buildings, reads citizen savings from regions, and determines
/// how much each citizen pays and how much the government subsidizes.
/// Does NOT mutate any state.
fn compute_service_transactions(
    buildings: &[Building],
    country: &Country,
    service_needs: &BTreeMap<String, f64>,
    building_inventories: &BTreeMap<String, BTreeMap<Commodity, f64>>,
    config: &ServicePricingConfig,
    commodity: Commodity,
) -> Vec<PendingServiceTxn> {
    let mut txns = Vec::new();

    for building in buildings.iter() {
        let available = building_inventories
            .get(&building.id)
            .and_then(|inv| inv.get(&commodity).copied())
            .unwrap_or(0.0);

        if available <= 0.0 {
            continue;
        }

        let region_id = &building.region_id;
        let service_need = service_needs.get(region_id).copied().unwrap_or(0.0);
        let price_per_unit = calculate_service_price(building, config);
        if price_per_unit <= 0.0 {
            continue;
        }

        let is_public = building.owner_id.starts_with("STATE_") || building.owner_id.starts_with("LOCAL_");

        let total_citizen_savings = country
            .regions
            .iter()
            .find(|r| &r.id == region_id)
            .map(|r| {
                r.class_demographics.rural_classes.values().map(|d| d.savings).sum::<f64>()
                    + r.class_demographics.urban_classes.values().map(|d| d.savings).sum::<f64>()
            })
            .unwrap_or(0.0);

        if is_public {
            let subsidy_amount = available * price_per_unit;
            let gov_cash = country.budget.liquid_reserves;

            if gov_cash >= subsidy_amount {
                txns.push(PendingServiceTxn {
                    building_id: building.id.clone(),
                    owner_id: building.owner_id.clone(),
                    region_id: building.region_id.clone(),
                    citizen_payment: 0.0,
                    government_subsidy: subsidy_amount,
                    units_consumed: available.min(service_need),
                    is_public: true,
                });
            } else {
                let affordable = (total_citizen_savings / price_per_unit).floor();
                let consumed = affordable.min(available).min(service_need);
                txns.push(PendingServiceTxn {
                    building_id: building.id.clone(),
                    owner_id: building.owner_id.clone(),
                    region_id: building.region_id.clone(),
                    citizen_payment: consumed * price_per_unit,
                    government_subsidy: 0.0,
                    units_consumed: consumed,
                    is_public: true,
                });
            }
        } else {
            let affordable = (total_citizen_savings / price_per_unit).floor();
            let consumed = affordable.min(available).min(service_need);
            txns.push(PendingServiceTxn {
                building_id: building.id.clone(),
                owner_id: building.owner_id.clone(),
                region_id: building.region_id.clone(),
                citizen_payment: consumed * price_per_unit,
                government_subsidy: 0.0,
                units_consumed: consumed,
                is_public: false,
            });
        }
    }

    txns
}

/// Apply service transactions (write phase).
///
/// Mutates citizen savings, government treasury, building reserves,
/// and company accounts based on computed transactions.
fn apply_service_transactions(
    txns: Vec<PendingServiceTxn>,
    buildings: &mut [Building],
    companies: &mut [Company],
    country: &mut Country,
    building_inventories: &mut BTreeMap<String, BTreeMap<Commodity, f64>>,
    commodity: Commodity,
) {
    for txn in &txns {
        if txn.units_consumed <= 0.0 {
            continue;
        }

        // Update inventory
        if let Some(inv) = building_inventories.get_mut(&txn.building_id) {
            *inv.entry(commodity).or_insert(0.0) -= txn.units_consumed;
        }

        // Debit citizen savings from region
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
}

/// Calculates service price based on operating costs.
///
/// # Arguments
/// * `building` - Building to calculate price for
///
/// # Returns
/// Price per unit of service
///
/// # Rules
/// * Based on worker capacity and sector-specific cost multipliers
/// * Education: 50 currency units per slot
/// * Healthcare: 75 currency units per capacity unit
fn calculate_service_price(building: &Building, config: &ServicePricingConfig) -> f64 {
    match building.sector {
        crate::registries::enums::Sector::EducationalServices => config.education_price_per_slot,
        crate::registries::enums::Sector::MedicalServices => config.health_price_per_capacity,
        _ => config.default_service_price,
    }
}

// ── Phase 23C: Passenger Transport B2C ──

/// Phase 23C: Populate commute service needs (PassengerTransport demand)
/// from regional demographics.
///
/// Computes the total PassengerTransport units needed per region based on
/// the working population and a per-capita commute demand factor.
///
/// # Returns
/// Map of `region_id → PassengerTransport units needed`.
pub fn populate_commute_service_needs(
    country: &Country,
    commute_demand_factor: f64,
) -> BTreeMap<String, f64> {
    let mut needs = BTreeMap::new();
    for region in &country.regions {
        let mut demand = 0.0_f64;
        for (_, class) in &region.class_demographics.rural_classes {
            demand += class.population as f64 * class.labor_participation * commute_demand_factor;
        }
        for (_, class) in &region.class_demographics.urban_classes {
            demand += class.population as f64 * class.labor_participation * commute_demand_factor;
        }
        needs.insert(region.id.clone(), demand);
    }
    needs
}

/// Phase 23C: Clear the PassengerTransport B2C market.
///
/// Transport buildings (`Sector::TransportLogistics`) with `PassengerTransport`
/// in their inventory sell to commuters. Public operators (owner starts with
/// `LOCAL_` or `STATE_`) are subsidized by the Treasury; private operators
/// charge market price.
///
/// # Arguments
/// * `buildings` - Mutable buildings (PassengerTransport consumed from inventory).
/// * `companies` - Mutable companies (private operator revenue credited).
/// * `country` - Mutable country (citizen savings debited, Treasury subsidy).
/// * `service_needs` - PassengerTransport demand per region.
/// * `config` - Service pricing configuration.
///
/// # Returns
/// Map of `region_id → commute coverage ratio` (0.0–1.0).
pub fn clear_passenger_transport_b2c(
    buildings: &mut [Building],
    companies: &mut [Company],
    country: &mut Country,
    service_needs: &BTreeMap<String, f64>,
    config: &ServicePricingConfig,
) -> BTreeMap<String, f64> {
    let commodity = Commodity::PassengerTransport;
    let mut coverage: BTreeMap<String, f64> = BTreeMap::new();

    // Group transport buildings by region and compute supply.
    let mut supply_by_region: BTreeMap<String, f64> = BTreeMap::new();
    for building in buildings.iter() {
        if building.sector != crate::registries::enums::Sector::TransportLogistics {
            continue;
        }
        let available = building.inventory.get(&commodity).copied().unwrap_or(0.0);
        if available > 0.0 {
            *supply_by_region.entry(building.region_id.clone()).or_insert(0.0) += available;
        }
    }

    // Collect region IDs and demands first (to avoid borrow conflicts).
    let region_demands: Vec<(String, f64, f64)> = country.regions.iter()
        .map(|r| {
            let demand = service_needs.get(&r.id).copied().unwrap_or(0.0);
            let supply = supply_by_region.get(&r.id).copied().unwrap_or(0.0);
            (r.id.clone(), demand, supply)
        })
        .collect();

    // Clear per region (mutable phase).
    for (region_id, demand, supply) in &region_demands {
        let demand = *demand;
        let supply = *supply;
        if demand <= 0.0 || supply <= 0.0 {
            coverage.insert(region_id.clone(), 0.0);
            continue;
        }

        let consumed = demand.min(supply);
        let coverage_ratio = consumed / demand;

        // Find transport buildings in this region and consume PassengerTransport.
        let mut remaining_to_consume = consumed;
        for building in buildings.iter_mut() {
            if remaining_to_consume <= 0.0 {
                break;
            }
            if building.sector != crate::registries::enums::Sector::TransportLogistics {
                continue;
            }
            if building.region_id != *region_id {
                continue;
            }
            let available = building.inventory.get(&commodity).copied().unwrap_or(0.0);
            if available <= 0.0 {
                continue;
            }
            let to_consume = remaining_to_consume.min(available);
            let new_qty = (available - to_consume).max(0.0);
            if new_qty > 0.0 {
                building.inventory.insert(commodity, new_qty);
            } else {
                building.inventory.remove(&commodity);
            }

            let is_public = building.owner_id.starts_with("STATE_") || building.owner_id.starts_with("LOCAL_");
            let price_per_unit = if is_public {
                config.default_service_price * 0.2 // 80% subsidized
            } else {
                config.default_service_price
            };

            let revenue = to_consume * price_per_unit;

            if is_public {
                // Public: Treasury pays the subsidy portion.
                let subsidy = to_consume * config.default_service_price * 0.8;
                let citizen_payment = revenue;
                if country.budget.liquid_reserves >= subsidy {
                    country.budget.liquid_reserves -= subsidy;
                }
                // Debit citizen savings proportionally.
                if let Some(region) = country.regions.iter_mut().find(|r| r.id == *region_id) {
                    debit_citizen_savings_region(region, citizen_payment);
                }
                // Credit the public building's reserve.
                building.reserve += subsidy + citizen_payment;
            } else {
                // Private: citizens pay full price.
                if let Some(region) = country.regions.iter_mut().find(|r| r.id == *region_id) {
                    debit_citizen_savings_region(region, revenue);
                }
                // Credit the private operator company.
                credit_company_by_id(companies, &building.owner_id, revenue);
            }

            remaining_to_consume -= to_consume;
        }

        coverage.insert(region_id.clone(), coverage_ratio);
    }

    coverage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::enums::Sector;
    use crate::state::{Country, Treasury, MacroData, TaxRates};
    use crate::society::geography::{Region, RegionalClassDemographics, ClassDemographics};

    fn make_test_country(region_id: &str, citizen_savings: f64, gov_reserves: f64) -> Country {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = gov_reserves;
        country.regions.clear();
        let mut region = Region::default();
        region.id = region_id.to_string();
        let mut demo = ClassDemographics::default();
        demo.savings = citizen_savings;
        demo.savings_per_capita = citizen_savings;
        demo.population = 100;
        region.class_demographics.rural_classes.insert("peasants".to_string(), demo);
        country.regions.push(region);
        country
    }

    #[test]
    fn public_institution_subsidy_success() {
        let mut building = Building::default();
        building.id = "SCHOOL_001".to_string();
        building.owner_id = "LOCAL_CITY".to_string();
        building.sector = Sector::EducationalServices;
        building.region_id = "REGION_001".to_string();
        
        let mut country = make_test_country("REGION_001", 1000.0, 10000.0);
        let service_needs = BTreeMap::from([("REGION_001".to_string(), 50.0)]);
        let mut building_inventories = BTreeMap::from([(
            "SCHOOL_001".to_string(),
            BTreeMap::from([(Commodity::EducationSlots, 100.0)]),
        )]);
        
        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        clear_education_slots_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &service_needs,
            &mut building_inventories,
            &ServicePricingConfig::default(),
        );

        // Subsidy: 100 * 50 = 5000, consumed = min(100, 50) = 50
        assert_eq!(country.budget.liquid_reserves, 5000.0); // 10000 - 5000
        assert_eq!(buildings[0].reserve, 5000.0);
        // Citizen savings unchanged (subsidized)
        let region = &country.regions[0];
        let demo = &region.class_demographics.rural_classes["peasants"];
        assert_eq!(demo.savings, 1000.0);
        assert_eq!(
            building_inventories["SCHOOL_001"].get(&Commodity::EducationSlots).copied().unwrap_or(0.0),
            50.0
        ); // 100 - 50 consumed
    }

    #[test]
    fn public_institution_insolvency_guard() {
        let mut building = Building::default();
        building.id = "SCHOOL_002".to_string();
        building.owner_id = "LOCAL_CITY".to_string();
        building.sector = Sector::EducationalServices;
        building.region_id = "REGION_001".to_string();
        
        let mut country = make_test_country("REGION_001", 1000.0, 100.0); // Insufficient gov
        let service_needs = BTreeMap::from([("REGION_001".to_string(), 50.0)]);
        let mut building_inventories = BTreeMap::from([(
            "SCHOOL_002".to_string(),
            BTreeMap::from([(Commodity::EducationSlots, 100.0)]),
        )]);
        
        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        clear_education_slots_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &service_needs,
            &mut building_inventories,
            &ServicePricingConfig::default(),
        );

        // Subsidy fails, citizens pay full price
        // Affordable: 1000 / 50 = 20 slots
        assert_eq!(country.budget.liquid_reserves, 100.0); // Unchanged (insolvency)
        let region = &country.regions[0];
        let demo = &region.class_demographics.rural_classes["peasants"];
        assert_eq!(demo.savings, 0.0); // 1000 - (20 * 50)
        assert_eq!(buildings[0].reserve, 1000.0); // 20 * 50
        assert_eq!(
            building_inventories["SCHOOL_002"].get(&Commodity::EducationSlots).copied().unwrap_or(0.0),
            80.0
        ); // 100 - 20 consumed
    }

    #[test]
    fn private_institution_no_subsidy() {
        let mut building = Building::default();
        building.id = "SCHOOL_003".to_string();
        building.owner_id = "COMPANY_EDU".to_string();
        building.sector = Sector::EducationalServices;
        building.region_id = "REGION_001".to_string();

        let mut country = make_test_country("REGION_001", 1000.0, 0.0);
        let service_needs = BTreeMap::from([("REGION_001".to_string(), 50.0)]);
        let mut building_inventories = BTreeMap::from([(
            "SCHOOL_003".to_string(),
            BTreeMap::from([(Commodity::EducationSlots, 100.0)]),
        )]);

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        clear_education_slots_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &service_needs,
            &mut building_inventories,
            &ServicePricingConfig::default(),
        );

        // No subsidy, citizens pay full price
        // Affordable: 1000 / 50 = 20 slots
        let region = &country.regions[0];
        let demo = &region.class_demographics.rural_classes["peasants"];
        assert_eq!(demo.savings, 0.0); // 1000 - (20 * 50)
        // Private building: revenue goes to company (not building.reserve)
        assert_eq!(buildings[0].reserve, 0.0);
        assert_eq!(
            building_inventories["SCHOOL_003"].get(&Commodity::EducationSlots).copied().unwrap_or(0.0),
            80.0
        ); // 100 - 20 consumed
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 23C: Passenger transport B2C tests
    // ═══════════════════════════════════════════════════════════

    fn make_transport_building(id: &str, owner: &str, region: &str, supply: f64) -> Building {
        let mut b = Building::default();
        b.id = id.to_string();
        b.owner_id = owner.to_string();
        b.sector = Sector::TransportLogistics;
        b.region_id = region.to_string();
        b.inventory.insert(Commodity::PassengerTransport, supply);
        b
    }

    #[test]
    fn passenger_transport_public_subsidy_consumes_supply() {
        let building = make_transport_building("STATE_BUS_1", "STATE_TRANSPORT", "R1", 100.0);
        let mut country = make_test_country("R1", 5000.0, 10000.0);
        let needs = BTreeMap::from([("R1".to_string(), 50.0)]);
        let config = ServicePricingConfig::default();

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let coverage = clear_passenger_transport_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &needs,
            &config,
        );

        // Demand 50, supply 100 → consumed 50, coverage 1.0
        assert_eq!(coverage.get("R1").copied().unwrap_or(0.0), 1.0);
        // Inventory consumed
        let remaining = buildings[0].inventory.get(&Commodity::PassengerTransport).copied().unwrap_or(0.0);
        assert_eq!(remaining, 50.0);
        // Public: Treasury pays 80% subsidy
        // subsidy = 50 * default_service_price * 0.8
        let expected_subsidy = 50.0 * config.default_service_price * 0.8;
        assert_eq!(country.budget.liquid_reserves, 10000.0 - expected_subsidy);
    }

    #[test]
    fn passenger_transport_private_no_subsidy() {
        let building = make_transport_building("PVT_BUS_1", "COMPANY_BUS", "R1", 100.0);
        let mut country = make_test_country("R1", 5000.0, 10000.0);
        let needs = BTreeMap::from([("R1".to_string(), 50.0)]);
        let config = ServicePricingConfig::default();

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let coverage = clear_passenger_transport_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &needs,
            &config,
        );

        assert_eq!(coverage.get("R1").copied().unwrap_or(0.0), 1.0);
        // Private: no Treasury subsidy
        assert_eq!(country.budget.liquid_reserves, 10000.0);
        // Citizens pay full price
        let region = &country.regions[0];
        let demo = &region.class_demographics.rural_classes["peasants"];
        // citizen_payment = 50 * default_service_price
        let expected_payment = 50.0 * config.default_service_price;
        assert_eq!(demo.savings, 5000.0 - expected_payment);
    }

    #[test]
    fn passenger_transport_zero_supply_zero_coverage() {
        let building = make_transport_building("STATE_BUS_2", "STATE_TRANSPORT", "R1", 0.0);
        let mut country = make_test_country("R1", 5000.0, 10000.0);
        let needs = BTreeMap::from([("R1".to_string(), 50.0)]);
        let config = ServicePricingConfig::default();

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let coverage = clear_passenger_transport_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &needs,
            &config,
        );

        // No supply → coverage 0, no consumption
        assert_eq!(coverage.get("R1").copied().unwrap_or(0.0), 0.0);
        assert_eq!(country.budget.liquid_reserves, 10000.0);
    }

    #[test]
    fn passenger_transport_partial_coverage_when_supply_limited() {
        let building = make_transport_building("STATE_BUS_3", "STATE_TRANSPORT", "R1", 20.0);
        let mut country = make_test_country("R1", 50000.0, 100000.0);
        let needs = BTreeMap::from([("R1".to_string(), 100.0)]);
        let config = ServicePricingConfig::default();

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let coverage = clear_passenger_transport_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &needs,
            &config,
        );

        // Demand 100, supply 20 → consumed 20, coverage 0.2
        assert!((coverage.get("R1").copied().unwrap_or(0.0) - 0.2).abs() < 1e-9);
        // All supply consumed
        let remaining = buildings[0].inventory.get(&Commodity::PassengerTransport).copied().unwrap_or(0.0);
        assert_eq!(remaining, 0.0);
    }
}

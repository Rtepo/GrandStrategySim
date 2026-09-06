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
        for class in region.class_demographics.rural_classes.values() {
            edu_need += calculate_education_need_for_class(class);
        }
        for class in region.class_demographics.urban_classes.values() {
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
        for class in region.class_demographics.rural_classes.values() {
            health_need += calculate_health_need_for_class(class);
        }
        for class in region.class_demographics.urban_classes.values() {
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
    let poverty_bonus = if class.savings_per_capita < 50.0 {
        1.5
    } else {
        1.0
    };
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
    let poverty_mult = if class.savings_per_capita < 50.0 {
        1.5
    } else {
        1.0
    };
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
            *region_consumption
                .entry(txn.region_id.clone())
                .or_insert(0.0) += txn.units_consumed;
        }
    }
    apply_service_transactions(
        txns,
        buildings,
        companies,
        country,
        building_inventories,
        commodity,
    );
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
    apply_service_transactions(
        txns,
        buildings,
        companies,
        country,
        building_inventories,
        commodity,
    );
}

/// Phase 18S: Populate sports/recreation service needs from regional demographics.
///
/// # Rules
/// * Each person generates base sports need of 0.05 visitor-slots.
/// * Urban populations have 1.5x higher demand (more sedentary lifestyles).
/// * Demand is NOT filtered by savings — public facilities are accessible to
///   all citizens via 100% buyer_subsidy (blueprint v2 correction).
/// * Private facilities are naturally gated by B2C clearing (insufficient
///   savings → unmet demand).
pub fn populate_sports_service_needs(country: &Country) -> BTreeMap<String, f64> {
    let mut needs = BTreeMap::new();
    for region in &country.regions {
        let mut sports_need = 0.0_f64;
        for class in region.class_demographics.rural_classes.values() {
            sports_need += class.population as f64 * 0.05;
        }
        for class in region.class_demographics.urban_classes.values() {
            sports_need += class.population as f64 * 0.05 * 1.5;
        }
        needs.insert(region.id.clone(), sports_need);
    }
    needs
}

/// Phase 18S: Executes B2C clearing for SportsCapacity with seasonality and
/// insolvency guard.
///
/// # Arguments
/// * `buildings` - Slice of sports facilities with SportsCapacity inventory
/// * `companies` - Mutable companies (for crediting private building revenue)
/// * `country` - Mutable country (for citizen savings and government subsidy)
/// * `service_needs` - Sports service needs by region
/// * `building_inventories` - Mutable building inventories
/// * `config` - Service pricing configuration
/// * `weather_state` - Weather state for seasonality modifier computation
/// * `season` - Current season enum
///
/// # Rules
/// * Public institutions use PriceIntervention with 100% buyer_subsidy
///   (citizens with ZERO savings can access public facilities for free)
/// * Private institutions charge market price (insufficient savings → unmet)
/// * Insolvency Guard: If government runs out of cash, subsidy fails gracefully
/// * Open-air facilities lose efficiency in winter based on ACTUAL weather data
/// * Indoor facilities operate year-round
/// * Citizen savings are debited directly from ClassDemographics (Phase 16A)
/// * Government subsidy debits from country.budget.liquid_reserves
///
/// # Returns
/// Map of region_id → sports capacity consumed (for health impact calculation)
pub fn clear_sports_capacity_b2c(
    buildings: &mut [Building],
    companies: &mut [Company],
    country: &mut Country,
    service_needs: &BTreeMap<String, f64>,
    building_inventories: &mut BTreeMap<String, BTreeMap<Commodity, f64>>,
    config: &ServicePricingConfig,
    weather_state: &crate::economy::weather::WeatherState,
    season: crate::state::Season,
) -> BTreeMap<String, f64> {
    let commodity = Commodity::SportsCapacity;

    // Phase 18S: Apply seasonality modifiers to building inventories before
    // clearing. Open-air facilities lose capacity in winter/extreme heat based
    // on actual regional weather data. Indoor facilities operate year-round.
    // We apply the modifier by scaling the available inventory.
    let mut seasonality_factors: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    for building in buildings.iter() {
        let factor = compute_sports_seasonality_factor(
            building,
            weather_state,
            season,
        );
        seasonality_factors.insert(building.id.clone(), factor);
    }

    // Apply seasonality by scaling down inventory for this turn's clearing.
    // We temporarily reduce the inventory, then restore the unsold portion.
    let mut original_inventory: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    for (bid, factor) in &seasonality_factors {
        if let Some(inv) = building_inventories.get_mut(bid) {
            if let Some(qty) = inv.get_mut(&commodity) {
                let original = *qty;
                original_inventory.insert(bid.clone(), original);
                *qty = original * factor;
            }
        }
    }

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
            *region_consumption
                .entry(txn.region_id.clone())
                .or_insert(0.0) += txn.units_consumed;
        }
    }

    apply_service_transactions(
        txns,
        buildings,
        companies,
        country,
        building_inventories,
        commodity,
    );

    // Restore unsold inventory that was held back by seasonality.
    // The difference between original and current (after clearing) is what
    // was consumed. We restore the seasonality-scaled portion that wasn't sold.
    for (bid, original) in &original_inventory {
        if let Some(inv) = building_inventories.get_mut(bid) {
            if let Some(qty) = inv.get_mut(&commodity) {
                let factor = seasonality_factors.get(bid).copied().unwrap_or(1.0);
                let scaled = original * factor;
                let consumed = scaled - *qty;
                // Restore: original - consumed (but not negative)
                *qty = (original - consumed).max(0.0);
            }
        }
    }

    region_consumption
}

/// Phase 18S: Compute the seasonality factor for a sports facility based on
/// its type and the regional weather state.
///
/// # Rules
/// * Open-air facilities (OpenAirField): close in winter (factor = 0.0 if
///   EarlyFrost or temperature below 0°C), reduced in extreme heat (0.3)
/// * Indoor facilities (IndoorHall, Stadium): operate year-round (factor = 1.0)
/// * The modifier is computed FROM the weather state, not a global counter
fn compute_sports_seasonality_factor(
    building: &Building,
    weather_state: &crate::economy::weather::WeatherState,
    season: crate::state::Season,
) -> f64 {
    // Determine facility class from the active production method name.
    // Sports facilities are identified by their sector and production method.
    let is_open_air = building
        .active_method
        .active_methods
        .production
        .contains("Open Air")
        || building
            .active_method
            .active_methods
            .production
            .contains("open_air");

    if !is_open_air {
        // IndoorHall and Stadium operate year-round
        return 1.0;
    }

    // Open-air: check weather events for the building's region
    let region_id = &building.region_id;
    let has_early_frost = weather_state
        .active_events
        .iter()
        .any(|e| {
            e.event_type == crate::economy::weather::WeatherEventType::EarlyFrost
                && e.affected_regions.iter().any(|r| r == region_id)
        });
    let has_heatwave = weather_state
        .active_events
        .iter()
        .any(|e| {
            e.event_type == crate::economy::weather::WeatherEventType::Heatwave
                && e.affected_regions.iter().any(|r| r == region_id)
        });

    // Winter + EarlyFrost → closed
    if season == crate::state::Season::Winter || has_early_frost {
        return 0.0;
    }

    // Extreme heat summer → reduced to 30%
    if season == crate::state::Season::Summer && has_heatwave {
        return 0.3;
    }

    1.0
}
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
    // Phase C.2: Get average_wage for dynamic cost-plus pricing (Rule 2).
    let average_wage = country.macro_indicators.average_wage.max(1.0);

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
        let price_per_unit = calculate_service_price(building, config, average_wage);
        if price_per_unit <= 0.0 {
            continue;
        }

        let is_public =
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
            // Rule 20: Clamp to zero — inventory cannot go negative.
            let current = inv.entry(commodity).or_insert(0.0);
            *current = (*current - txn.units_consumed).max(0.0);
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
/// Phase C.2: Cost-Plus pricing with CAPEX amortization (Rule 21).
/// All prices scale with `average_wage` (Rule 2: no magic nominal constants).
fn calculate_service_price(
    building: &Building,
    config: &ServicePricingConfig,
    average_wage: f64,
) -> f64 {
    match building.sector {
        crate::registries::enums::Sector::EducationalServices => {
            config.education_price_per_slot(average_wage)
        }
        crate::registries::enums::Sector::MedicalServices => {
            config.health_price_per_capacity(average_wage)
        }
        crate::registries::enums::Sector::SportsRecreation => {
            config.sports_price_per_capacity(average_wage)
        }
        _ => config.default_service_price(average_wage),
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
        for class in region.class_demographics.rural_classes.values() {
            demand += class.population as f64 * class.labor_participation * commute_demand_factor;
        }
        for class in region.class_demographics.urban_classes.values() {
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
    // Phase C.2: Dynamic cost-plus pricing (Rule 2/21).
    let average_wage = country.macro_indicators.average_wage.max(1.0);

    // Group transport buildings by region and compute supply.
    let mut supply_by_region: BTreeMap<String, f64> = BTreeMap::new();
    for building in buildings.iter() {
        if building.sector != crate::registries::enums::Sector::TransportLogistics {
            continue;
        }
        let available = building.inventory.get(&commodity).copied().unwrap_or(0.0);
        if available > 0.0 {
            *supply_by_region
                .entry(building.region_id.clone())
                .or_insert(0.0) += available;
        }
    }

    // Collect region IDs and demands first (to avoid borrow conflicts).
    let region_demands: Vec<(String, f64, f64)> = country
        .regions
        .iter()
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

            let is_public =
                building.owner_id.starts_with("STATE_") || building.owner_id.starts_with("LOCAL_");
            let price_per_unit = if is_public {
                config.default_service_price(average_wage) * 0.2 // 80% subsidized
            } else {
                config.default_service_price(average_wage)
            };

            let revenue = to_consume * price_per_unit;

            if is_public {
                // Public: Treasury pays the subsidy portion.
                let subsidy = to_consume * config.default_service_price(average_wage) * 0.8;
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
    use crate::society::geography::{ClassDemographics, Region, RuralClass};
    use crate::state::Country;

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
            .rural_classes
            .insert(RuralClass::FreePeasant, demo);
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

        // Phase C.2: Dynamic price = (1000*0.05 + 1000*0.5/24) * 1.10 = 77.916...
        let edu_price = ServicePricingConfig::default().education_price_per_slot(1000.0);
        // B2C clearing: treasury subsidizes full supply (100 units),
        // but only 50 units are consumed from inventory (demand = 50).
        let expected_subsidy = 100.0 * edu_price;
        assert_eq!(country.budget.liquid_reserves, 10000.0 - expected_subsidy);
        assert_eq!(buildings[0].reserve, expected_subsidy);
        // Citizen savings unchanged (subsidized)
        let region = &country.regions[0];
        let demo = &region.class_demographics.rural_classes[&RuralClass::FreePeasant];
        assert_eq!(demo.savings, 1000.0);
        assert_eq!(
            building_inventories["SCHOOL_001"]
                .get(&Commodity::EducationSlots)
                .copied()
                .unwrap_or(0.0),
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

        // Phase C.2: Subsidy fails, citizens pay full dynamic price.
        let edu_price = ServicePricingConfig::default().education_price_per_slot(1000.0);
        // Affordable: 1000 / edu_price slots
        let affordable = (1000.0 / edu_price).floor();
        let citizen_payment = affordable * edu_price;
        assert_eq!(country.budget.liquid_reserves, 100.0); // Unchanged (insolvency)
        let region = &country.regions[0];
        let demo = &region.class_demographics.rural_classes[&RuralClass::FreePeasant];
        assert!((demo.savings - (1000.0 - citizen_payment)).abs() < 0.01);
        assert!((buildings[0].reserve - citizen_payment).abs() < 0.01);
        assert_eq!(
            building_inventories["SCHOOL_002"]
                .get(&Commodity::EducationSlots)
                .copied()
                .unwrap_or(0.0),
            100.0 - affordable
        );
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

        // Phase C.2: No subsidy, citizens pay full dynamic price.
        let edu_price = ServicePricingConfig::default().education_price_per_slot(1000.0);
        let affordable = (1000.0 / edu_price).floor();
        let citizen_payment = affordable * edu_price;
        let region = &country.regions[0];
        let demo = &region.class_demographics.rural_classes[&RuralClass::FreePeasant];
        assert!((demo.savings - (1000.0 - citizen_payment)).abs() < 0.01);
        // Private building: revenue goes to company (not building.reserve)
        assert_eq!(buildings[0].reserve, 0.0);
        assert_eq!(
            building_inventories["SCHOOL_003"]
                .get(&Commodity::EducationSlots)
                .copied()
                .unwrap_or(0.0),
            100.0 - affordable
        );
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
        let remaining = buildings[0]
            .inventory
            .get(&Commodity::PassengerTransport)
            .copied()
            .unwrap_or(0.0);
        assert_eq!(remaining, 50.0);
        // Public: Treasury pays 80% subsidy
        // subsidy = 50 * default_service_price(avg_wage) * 0.8
        let expected_subsidy = 50.0 * config.default_service_price(1000.0) * 0.8;
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
        let demo = &region.class_demographics.rural_classes[&RuralClass::FreePeasant];
        // citizen_payment = 50 * default_service_price(avg_wage)
        let expected_payment = 50.0 * config.default_service_price(1000.0);
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
        let remaining = buildings[0]
            .inventory
            .get(&Commodity::PassengerTransport)
            .copied()
            .unwrap_or(0.0);
        assert_eq!(remaining, 0.0);
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 18S: Sports & Recreation B2C Clearing Tests
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn sports_public_facility_accessible_to_zero_savings_citizen() {
        // Blueprint v2 correction: Public sports facilities must be accessible
        // to citizens with ZERO savings through 100% buyer_subsidy.
        let mut building = Building::default();
        building.id = "SPORTS_PUB_001".to_string();
        building.owner_id = "LOCAL_CITY".to_string();
        building.sector = Sector::SportsRecreation;
        building.region_id = "REGION_SPORTS".to_string();

        // Citizen has ZERO savings, government has plenty of cash
        let mut country = make_test_country("REGION_SPORTS", 0.0, 100000.0);
        let service_needs = BTreeMap::from([("REGION_SPORTS".to_string(), 50.0)]);
        let mut building_inventories = BTreeMap::from([(
            "SPORTS_PUB_001".to_string(),
            BTreeMap::from([(Commodity::SportsCapacity, 100.0)]),
        )]);

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let weather = crate::economy::weather::WeatherState::default();
        let consumption = clear_sports_capacity_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &service_needs,
            &mut building_inventories,
            &ServicePricingConfig::default(),
            &weather,
            crate::state::Season::Summer,
        );

        // Public facility with 100% subsidy: citizens with zero savings
        // can access it. Consumption should be > 0.
        let consumed = consumption.get("REGION_SPORTS").copied().unwrap_or(0.0);
        assert!(
            consumed > 0.0,
            "Public sports facility must be accessible to zero-savings citizens via subsidy"
        );

        // Citizen savings should remain 0 (fully subsidized)
        let region = &country.regions[0];
        let demo = &region.class_demographics.rural_classes[&RuralClass::FreePeasant];
        assert_eq!(
            demo.savings, 0.0,
            "Zero-savings citizen should not be charged for public sports facility"
        );

        // Government treasury should be debited
        assert!(
            country.budget.liquid_reserves < 100000.0,
            "Government treasury must be debited for sports subsidy"
        );
    }

    #[test]
    fn sports_private_facility_rejects_zero_savings_citizen() {
        // Private sports facilities are gated by B2C affordability.
        // Citizens with zero savings should have unmet demand.
        let mut building = Building::default();
        building.id = "SPORTS_PRIV_001".to_string();
        building.owner_id = "PRIVATE_CORP".to_string();
        building.sector = Sector::SportsRecreation;
        building.region_id = "REGION_SPORTS".to_string();

        // Citizen has ZERO savings
        let mut country = make_test_country("REGION_SPORTS", 0.0, 100000.0);
        let service_needs = BTreeMap::from([("REGION_SPORTS".to_string(), 50.0)]);
        let mut building_inventories = BTreeMap::from([(
            "SPORTS_PRIV_001".to_string(),
            BTreeMap::from([(Commodity::SportsCapacity, 100.0)]),
        )]);

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let weather = crate::economy::weather::WeatherState::default();
        let consumption = clear_sports_capacity_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &service_needs,
            &mut building_inventories,
            &ServicePricingConfig::default(),
            &weather,
            crate::state::Season::Summer,
        );

        // Private facility: zero-savings citizen cannot afford it.
        let consumed = consumption.get("REGION_SPORTS").copied().unwrap_or(0.0);
        assert_eq!(
            consumed, 0.0,
            "Private sports facility must reject zero-savings citizens (unmet demand)"
        );

        // Inventory should remain full (nothing consumed)
        let remaining = building_inventories["SPORTS_PRIV_001"]
            .get(&Commodity::SportsCapacity)
            .copied()
            .unwrap_or(0.0);
        assert_eq!(
            remaining, 100.0,
            "Private facility inventory should be untouched when citizen has no savings"
        );
    }

    #[test]
    fn sports_open_air_closes_in_winter() {
        // Open-air facilities must close when weather indicates winter.
        let mut building = Building::default();
        building.id = "SPORTS_OPEN_001".to_string();
        building.owner_id = "LOCAL_CITY".to_string();
        building.sector = Sector::SportsRecreation;
        building.region_id = "REGION_SPORTS".to_string();
        // Mark as open-air via production method name
        building.active_method.active_methods.production = "Open Air Field".to_string();

        let mut country = make_test_country("REGION_SPORTS", 1000.0, 100000.0);
        let service_needs = BTreeMap::from([("REGION_SPORTS".to_string(), 50.0)]);
        let mut building_inventories = BTreeMap::from([(
            "SPORTS_OPEN_001".to_string(),
            BTreeMap::from([(Commodity::SportsCapacity, 100.0)]),
        )]);

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let weather = crate::economy::weather::WeatherState::default();

        // Winter → open-air facility closes (factor = 0.0)
        let consumption = clear_sports_capacity_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &service_needs,
            &mut building_inventories,
            &ServicePricingConfig::default(),
            &weather,
            crate::state::Season::Winter,
        );

        let consumed = consumption.get("REGION_SPORTS").copied().unwrap_or(0.0);
        assert_eq!(
            consumed, 0.0,
            "Open-air sports facility must close in winter (zero consumption)"
        );

        // Inventory should remain full (seasonality factor = 0.0)
        let remaining = building_inventories["SPORTS_OPEN_001"]
            .get(&Commodity::SportsCapacity)
            .copied()
            .unwrap_or(0.0);
        assert_eq!(
            remaining, 100.0,
            "Open-air facility inventory should be untouched in winter"
        );
    }

    #[test]
    fn sports_indoor_hall_operates_in_winter() {
        // Indoor facilities operate at full capacity year-round.
        let mut building = Building::default();
        building.id = "SPORTS_INDOOR_001".to_string();
        building.owner_id = "LOCAL_CITY".to_string();
        building.sector = Sector::SportsRecreation;
        building.region_id = "REGION_SPORTS".to_string();
        // Mark as indoor (not open-air) via production method name
        building.active_method.active_methods.production = "Indoor Hall".to_string();

        let mut country = make_test_country("REGION_SPORTS", 1000.0, 100000.0);
        let service_needs = BTreeMap::from([("REGION_SPORTS".to_string(), 50.0)]);
        let mut building_inventories = BTreeMap::from([(
            "SPORTS_INDOOR_001".to_string(),
            BTreeMap::from([(Commodity::SportsCapacity, 100.0)]),
        )]);

        let mut buildings = vec![building];
        let mut companies: Vec<Company> = Vec::new();
        let weather = crate::economy::weather::WeatherState::default();

        // Winter → indoor facility still operates at full capacity
        let consumption = clear_sports_capacity_b2c(
            &mut buildings,
            &mut companies,
            &mut country,
            &service_needs,
            &mut building_inventories,
            &ServicePricingConfig::default(),
            &weather,
            crate::state::Season::Winter,
        );

        let consumed = consumption.get("REGION_SPORTS").copied().unwrap_or(0.0);
        assert!(
            consumed > 0.0,
            "Indoor sports facility must operate at full capacity in winter"
        );
    }

    #[test]
    fn sports_commodity_serialization() {
        // Verify SportsCapacity serializes and deserializes correctly.
        let commodity = Commodity::SportsCapacity;
        let json = serde_json::to_string(&commodity).unwrap();
        let deserialized: Commodity = serde_json::from_str(&json).unwrap();
        assert_eq!(commodity, deserialized);
    }

    #[test]
    fn sports_capacity_type_serialization() {
        // Verify CapacityType::SportsCapacity serializes correctly.
        use crate::infrastructure::CapacityType;
        let cap = CapacityType::SportsCapacity;
        let json = serde_json::to_string(&cap).unwrap();
        let deserialized: CapacityType = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, deserialized);
    }

    #[test]
    fn sports_production_methods_registered() {
        // Verify sports production methods are registered.
        let methods = crate::registries::production_methods_data::default_production_methods();
        let sports = methods.get("sports_recreation");
        assert!(
            sports.is_some(),
            "Sports recreation methods must be registered"
        );
    }

    #[test]
    fn sports_service_price_scales_with_wage() {
        // Rule 2: Price must scale with average_wage (no magic constants).
        let config = ServicePricingConfig::default();
        let price_low = config.sports_price_per_capacity(1000.0);
        let price_high = config.sports_price_per_capacity(10_000.0);
        assert!(
            price_high > price_low,
            "Sports price must scale with wage"
        );
        assert!(price_low > 0.0, "Sports price must be positive");
    }
}

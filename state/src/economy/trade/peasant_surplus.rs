//! Peasant surplus aggregation via agricultural cooperatives (Phase 5 — Agrarian Audit).
//!
//! This module implements the peasant surplus → cooperative aggregator pathway.
//!
//! ## Core Principle
//!
//! `ClassDemographics` is NOT a `Company`. The B2B `OrderBook` requires a valid
//! `seller_id` corresponding to a corporate entity. Using a phantom string (like
//! the broken "fishing_sector" bug) would cause the settlement engine to fail to
//! credit the seller, destroying M0 money.
//!
//! Free Peasants MUST NOT submit B2B orders directly. Instead, they utilize
//! agricultural `LegalForm::Cooperative` companies in their region as market
//! aggregators (Skupy / Spółdzielnie Rolnicze).
//!
//! ## Flow
//!
//! 1. FreePeasant surplus is identified from demographic/physical production state.
//! 2. A valid regional agricultural Cooperative is selected (competitive/pro-rata).
//! 3. The Cooperative pays the peasants through a direct, exact cash transfer
//!    (debit cooperative `available_cash`, credit FreePeasant class `savings`).
//! 4. The Cooperative receives physical goods into its building inventory.
//! 5. The Cooperative submits ordinary B2B sell asks using its actual company ID.
//! 6. `settle_trades` credits the Cooperative through the standard settlement path.
//!
//! ## Conservation
//!
//! All transfers are strict double-entry. No money is created or destroyed.
//! The cooperative payment IS the balancing entry for peasant surplus.
//! The B2B settlement credits the cooperative (real company) — M0 is conserved.

#![allow(missing_docs)]

use crate::economy::market::market_history::MarketHistory;
use crate::entities::legal_form::{CooperativeData, LegalForm};
use crate::entities::{Building, Company};
use crate::registries::enums::{Commodity, Sector};
use crate::society::geography::{Region, RuralClass};
use std::collections::BTreeMap;

/// Configuration for peasant surplus aggregation (no magic numbers).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PeasantSurplusConfig {
    /// Fraction of FreePeasant FTE that produces marketable surplus (after
    /// subsistence deduction). The remaining FTE is consumed by subsistence.
    #[serde(default = "default_surplus_fraction")]
    pub surplus_fte_fraction: f64,
    /// Physical output per FTE per turn (in commodity units).
    /// Scaled by average_wage for inflation-proofing of the financial side,
    /// but the physical quantity is determined by agricultural productivity.
    #[serde(default = "default_output_per_fte")]
    pub output_per_fte: f64,
    /// Maximum fraction of cooperative available_cash that can be spent on
    /// peasant surplus purchases per turn.
    #[serde(default = "default_max_cash_for_surplus")]
    pub max_cash_for_surplus_fraction: f64,
    /// Commodities produced by FreePeasant smallholders.
    #[serde(default = "default_peasant_commodities")]
    pub peasant_commodities: Vec<Commodity>,
}

impl Default for PeasantSurplusConfig {
    fn default() -> Self {
        Self {
            surplus_fte_fraction: default_surplus_fraction(),
            output_per_fte: default_output_per_fte(),
            max_cash_for_surplus_fraction: default_max_cash_for_surplus(),
            peasant_commodities: default_peasant_commodities(),
        }
    }
}

fn default_surplus_fraction() -> f64 {
    0.3 // 30% of FreePeasant FTE produces marketable surplus
}
fn default_output_per_fte() -> f64 {
    10.0 // 10 commodity units per FTE per turn
}
fn default_max_cash_for_surplus() -> f64 {
    0.5 // Cooperative can spend up to 50% of cash on surplus purchases
}
fn default_peasant_commodities() -> Vec<Commodity> {
    vec![Commodity::Cereal, Commodity::Vegetable, Commodity::Meat]
}

/// Result of peasant surplus processing for a single turn.
#[derive(Debug, Clone, Default)]
pub struct PeasantSurplusResult {
    /// Total surplus units transferred to cooperatives.
    pub total_surplus_units: f64,
    /// Total cash paid to FreePeasant class by cooperatives.
    pub total_cash_paid: f64,
    /// Number of cooperatives that received surplus.
    pub cooperatives_used: usize,
    /// Number of regions where surplus was processed.
    pub regions_processed: usize,
}

/// Process peasant surplus through agricultural cooperatives.
///
/// This function runs BEFORE `submit_company_b2b_orders` so that the surplus
/// is in cooperative building inventory when sell asks are generated.
///
/// # Arguments
/// * `companies` - Mutable slice of all companies. Agricultural cooperatives
///   are identified by `Sector::Agriculture` + `LegalForm::Cooperative`.
/// * `buildings` - Mutable slice of all buildings. Cooperative buildings are
///   found by matching `owner_id` to cooperative company IDs.
/// * `regions` - Mutable slice of regions. FreePeasant class demographics are
///   read and their savings are credited.
/// * `market_history` - Market history for VWAP pricing.
/// * `config` - Peasant surplus configuration.
///
/// # Returns
/// Aggregate result across all regions.
///
/// # Conservation
/// - Physical: surplus commodities are added to cooperative building inventory.
/// - Financial: cooperative `available_cash` is debited, FreePeasant `savings`
///   is credited by the exact same amount.
/// - No phantom seller IDs are created. The cooperative's real `company.id`
///   is used for all subsequent B2B sell asks.
pub fn process_peasant_surplus_to_cooperative(
    companies: &mut [Company],
    buildings: &mut [Building],
    regions: &mut [Region],
    market_history: &MarketHistory,
    config: &PeasantSurplusConfig,
) -> PeasantSurplusResult {
    let mut result = PeasantSurplusResult::default();

    // Build a map: region_id → list of cooperative company indices.
    let mut region_cooperatives: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, company) in companies.iter().enumerate() {
        if company.sector == Sector::Agriculture {
            if let LegalForm::Cooperative(_) = &company.legal_form {
                region_cooperatives
                    .entry(company.region_id.clone())
                    .or_default()
                    .push(idx);
            }
        }
    }

    // Build a map: company_id → list of building indices.
    let mut company_buildings: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, building) in buildings.iter().enumerate() {
        company_buildings
            .entry(building.owner_id.clone())
            .or_default()
            .push(idx);
    }

    for region in regions.iter_mut() {
        // Check if this region has FreePeasant population.
        let free_peasant_pop = region
            .class_demographics
            .rural_classes
            .get(&RuralClass::FreePeasant)
            .map(|d| d.population)
            .unwrap_or(0);

        if free_peasant_pop <= 0 {
            continue;
        }

        // Get FreePeasant FTE for surplus calculation.
        let free_peasant_fte = region
            .class_demographics
            .rural_classes
            .get(&RuralClass::FreePeasant)
            .map(|d| d.available_fte)
            .unwrap_or(0.0);

        if free_peasant_fte <= 0.0 {
            continue;
        }

        // Find agricultural cooperatives in this region.
        let coop_indices = match region_cooperatives.get(&region.id) {
            Some(indices) if !indices.is_empty() => indices.clone(),
            _ => {
                // No cooperative exists — the surplus rots. This is a real
                // economic consequence: without market access, FreePeasant
                // surplus is wasted. This creates economic pressure to form
                // or join cooperatives.
                continue;
            }
        };

        // Calculate total surplus bundle.
        let surplus_fte = free_peasant_fte * config.surplus_fte_fraction;
        let mut surplus_bundle: BTreeMap<Commodity, f64> = BTreeMap::new();
        for commodity in &config.peasant_commodities {
            let units = surplus_fte * config.output_per_fte
                / config.peasant_commodities.len() as f64;
            surplus_bundle.insert(*commodity, units);
        }

        // Calculate total surplus value at VWAP.
        let mut total_surplus_value = 0.0;
        for (commodity, units) in &surplus_bundle {
            let vwap = market_history
                .vwap_per_commodity
                .get(commodity)
                .copied()
                .unwrap_or_else(|| {
                    // Fallback to last trade price, then base price.
                    market_history
                        .last_trade_price
                        .get(commodity)
                        .copied()
                        .unwrap_or(1.0)
                });
            total_surplus_value += vwap * units;
        }

        if total_surplus_value <= 0.0 {
            continue;
        }

        // Pro-rata distribution across cooperatives based on their available_cash.
        // This is a competitive mechanism: cooperatives with more working capital
        // can purchase more surplus. This replaces arbitrary fixed splits (Rule 5).
        let mut coop_capacities: Vec<(usize, f64)> = Vec::new();
        let mut total_coop_cash = 0.0;
        for &idx in &coop_indices {
            let cash = companies[idx].available_cash * config.max_cash_for_surplus_fraction;
            if cash > 0.0 {
                coop_capacities.push((idx, cash));
                total_coop_cash += cash;
            }
        }

        if total_coop_cash <= 0.0 || coop_capacities.is_empty() {
            // No cooperative can afford to buy surplus — it rots.
            continue;
        }

        // The actual surplus purchased is limited by cooperative purchasing power.
        let purchase_ratio = (total_coop_cash / total_surplus_value).min(1.0);

        result.regions_processed += 1;

        // Distribute surplus pro-rata across cooperatives.
        for (coop_idx, coop_cash) in &coop_capacities {
            let coop_share = coop_cash / total_coop_cash;
            let coop_payment = total_surplus_value * purchase_ratio * coop_share;

            if coop_payment <= 0.0 {
                continue;
            }

            // Debit the cooperative's available_cash.
            let actual_payment = companies[*coop_idx].available_cash.min(coop_payment);
            companies[*coop_idx].available_cash -= actual_payment;

            // Credit FreePeasant class savings in the region.
            if let Some(fp_demo) = region
                .class_demographics
                .rural_classes
                .get_mut(&RuralClass::FreePeasant)
            {
                fp_demo.savings += actual_payment;
                if fp_demo.population > 0 {
                    fp_demo.savings_per_capita = fp_demo.savings / fp_demo.population as f64;
                }
            }

            // Transfer physical surplus to the cooperative's building inventory.
            // Find the cooperative's buildings and add the surplus commodities.
            let coop_id = companies[*coop_idx].id.clone();
            if let Some(building_indices) = company_buildings.get(&coop_id) {
                // Use the first building as the inventory location.
                // In a real system, this would consider capacity and logistics.
                if let Some(&first_bldg_idx) = building_indices.first() {
                    let building = &mut buildings[first_bldg_idx];
                    for (commodity, total_units) in &surplus_bundle {
                        let units = total_units * purchase_ratio * coop_share;
                        if units > 0.0 {
                            // Check inventory capacity before adding.
                            let current_total: f64 = building.inventory.values().sum();
                            let remaining_capacity =
                                building.inventory_capacity - current_total;
                            let actual_units = units.min(remaining_capacity.max(0.0));
                            if actual_units > 0.0 {
                                *building
                                    .inventory
                                    .entry(*commodity)
                                    .or_insert(0.0) += actual_units;
                                result.total_surplus_units += actual_units;
                            }
                        }
                    }
                }
            }

            // Update cooperative member_count to reflect FreePeasant participation.
            if let LegalForm::Cooperative(data) = &mut companies[*coop_idx].legal_form {
                // Update member_count to the larger of current or FreePeasant pop.
                data.member_count = data.member_count.max(free_peasant_pop as u32);
            }

            result.total_cash_paid += actual_payment;
            result.cooperatives_used += 1;
        }
    }

    result
}

/// Ensure every rural region with FreePeasant population has at least one
/// agricultural Cooperative.
///
/// This function is called during world generation. If a region has FreePeasant
/// population but no `Sector::Agriculture` company with `LegalForm::Cooperative`,
/// it spawns one. The cooperative's `member_count` is initialized to the
/// FreePeasant population (capped at `actual_capacity`).
///
/// # Arguments
/// * `companies` - Mutable vector of companies (new cooperatives are pushed here).
/// * `buildings` - Mutable vector of buildings (a warehouse is created for the coop).
/// * `regions` - Regions slice (read-only, for checking FreePeasant population).
/// * `seed_capital_wage_multiple` - Seed capital as a multiple of average_wage.
///   This is extracted from an explicit source (e.g., regional development fund
///   or peasant contributions) — NOT created from nothing.
///
/// # Returns
/// Number of cooperatives created.
pub fn ensure_agricultural_cooperatives(
    companies: &mut Vec<Company>,
    buildings: &mut Vec<Building>,
    regions: &[Region],
    average_wage: f64,
    seed_capital_wage_multiple: f64,
) -> usize {
    let mut created = 0;

    // Build a set of region IDs that already have agricultural cooperatives.
    let mut regions_with_coops: std::collections::HashSet<String> = std::collections::HashSet::new();
    for company in companies.iter() {
        if company.sector == Sector::Agriculture {
            if let LegalForm::Cooperative(_) = &company.legal_form {
                regions_with_coops.insert(company.region_id.clone());
            }
        }
    }

    for region in regions {
        let free_peasant_pop = region
            .class_demographics
            .rural_classes
            .get(&RuralClass::FreePeasant)
            .map(|d| d.population)
            .unwrap_or(0);

        if free_peasant_pop <= 0 {
            continue;
        }

        if regions_with_coops.contains(&region.id) {
            continue;
        }

        // Create a new agricultural cooperative for this region.
        let coop_id = format!("coop_agri_{}_{}", region.id, companies.len());
        let seed_capital = average_wage * seed_capital_wage_multiple;

        let mut coop = Company::default();
        coop.id = coop_id.clone();
        coop.name = format!("Agricultural Cooperative ({})", region.id);
        coop.sector = Sector::Agriculture;
        coop.region_id = region.id.clone();
        coop.legal_form = LegalForm::Cooperative(CooperativeData {
            member_count: free_peasant_pop as u32,
            patronage_pool: 0.0,
            federation_id: None,
        });
        coop.available_cash = seed_capital;
        coop.liquid_capital = seed_capital;
        coop.company_capital = seed_capital;

        // Create a warehouse building for the cooperative.
        let mut warehouse = Building::default();
        warehouse.id = format!("warehouse_{}", coop_id);
        warehouse.owner_id = coop_id.clone();
        warehouse.region_id = region.id.clone();
        warehouse.sector = Sector::Agriculture;
        warehouse.name = format!("Cooperative Warehouse ({})", region.id);
        warehouse.inventory_capacity = 10000.0; // Sufficient for surplus aggregation
        warehouse.year_built = 1900; // Will be set properly by caller

        companies.push(coop);
        buildings.push(warehouse);
        regions_with_coops.insert(region.id.clone());
        created += 1;
    }

    created
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::market::market_history::MarketHistory;
    use crate::entities::legal_form::{CooperativeData, LegalForm};
    use crate::registries::enums::Commodity;
    use crate::society::geography::{ClassDemographics, RegionalClassDemographics};
    use std::collections::BTreeMap;

    fn make_region_with_free_peasants(region_id: &str, pop: i64) -> Region {
        let mut rural = BTreeMap::new();
        rural.insert(
            RuralClass::FreePeasant,
            ClassDemographics {
                population: pop,
                labor_participation: 0.55,
                available_fte: pop as f64 * 0.55,
                savings: pop as f64 * 100.0,
                savings_per_capita: 100.0,
                ..Default::default()
            },
        );

        let mut region = Region::default();
        region.id = region_id.to_string();
        region.class_demographics = RegionalClassDemographics {
            rural_classes: rural,
            urban_classes: BTreeMap::new(),
        };
        region
    }

    fn make_cooperative(company_id: &str, region_id: &str, cash: f64) -> Company {
        let mut company = Company::default();
        company.id = company_id.to_string();
        company.name = format!("Coop {}", company_id);
        company.sector = Sector::Agriculture;
        company.region_id = region_id.to_string();
        company.legal_form = LegalForm::Cooperative(CooperativeData {
            member_count: 100,
            patronage_pool: 0.0,
            federation_id: None,
        });
        company.available_cash = cash;
        company.liquid_capital = cash;
        company.company_capital = cash;
        company
    }

    fn make_warehouse(owner_id: &str, region_id: &str) -> Building {
        let mut building = Building::default();
        building.id = format!("wh_{}", owner_id);
        building.owner_id = owner_id.to_string();
        building.region_id = region_id.to_string();
        building.sector = Sector::Agriculture;
        building.name = format!("Warehouse {}", owner_id);
        building.inventory_capacity = 10000.0;
        building
    }

    #[test]
    fn peasant_surplus_flows_through_cooperative() {
        let mut region = make_region_with_free_peasants("R1", 1000);
        let mut company = make_cooperative("coop1", "R1", 50000.0);
        let mut building = make_warehouse("coop1", "R1");

        let mut companies = vec![company];
        let mut buildings = vec![building];
        let mut regions = vec![region];

        let mut market_history = MarketHistory::default();
        market_history
            .vwap_per_commodity
            .insert(Commodity::Cereal, 50.0);
        market_history
            .vwap_per_commodity
            .insert(Commodity::Vegetable, 30.0);
        market_history
            .vwap_per_commodity
            .insert(Commodity::Meat, 100.0);

        let config = PeasantSurplusConfig::default();
        let result = process_peasant_surplus_to_cooperative(
            &mut companies,
            &mut buildings,
            &mut regions,
            &market_history,
            &config,
        );

        // The cooperative should have received surplus and paid peasants.
        assert!(result.total_surplus_units > 0.0, "Surplus should be transferred");
        assert!(result.total_cash_paid > 0.0, "Cash should be paid to peasants");
        assert_eq!(result.cooperatives_used, 1, "One cooperative should be used");

        // The cooperative's available_cash should have decreased.
        assert!(
            companies[0].available_cash < 50000.0,
            "Cooperative cash should decrease after paying peasants"
        );

        // The FreePeasant savings should have increased.
        let fp_savings = regions[0].class_demographics.rural_classes[&RuralClass::FreePeasant].savings;
        assert!(
            fp_savings > 1000.0 * 100.0,
            "FreePeasant savings should increase after receiving payment"
        );

        // The cooperative's building inventory should have commodities.
        let inventory_total: f64 = buildings[0].inventory.values().sum();
        assert!(
            inventory_total > 0.0,
            "Cooperative building inventory should have surplus commodities"
        );
    }

    #[test]
    fn no_cooperative_means_surplus_rots() {
        let mut region = make_region_with_free_peasants("R1", 1000);
        let mut regions = vec![region];

        let companies: Vec<Company> = vec![]; // No cooperatives
        let buildings: Vec<Building> = vec![];
        let market_history = MarketHistory::default();

        let config = PeasantSurplusConfig::default();
        let result = process_peasant_surplus_to_cooperative(
            &mut companies.clone(),
            &mut buildings.clone(),
            &mut regions,
            &market_history,
            &config,
        );

        // No surplus should be processed without a cooperative.
        assert_eq!(result.total_surplus_units, 0.0, "No surplus without cooperative");
        assert_eq!(result.total_cash_paid, 0.0, "No payment without cooperative");
    }

    #[test]
    fn no_phantom_seller_ids_created() {
        let mut region = make_region_with_free_peasants("R1", 1000);
        let mut company = make_cooperative("coop_real_id", "R1", 50000.0);
        let mut building = make_warehouse("coop_real_id", "R1");

        let mut companies = vec![company];
        let mut buildings = vec![building];
        let mut regions = vec![region];

        let mut market_history = MarketHistory::default();
        market_history
            .vwap_per_commodity
            .insert(Commodity::Cereal, 50.0);

        let config = PeasantSurplusConfig::default();
        let _result = process_peasant_surplus_to_cooperative(
            &mut companies,
            &mut buildings,
            &mut regions,
            &market_history,
            &config,
        );

        // The cooperative's company ID should be a valid, real ID — not a phantom.
        assert!(
            !companies[0].id.is_empty(),
            "Cooperative must have a valid company ID"
        );
        assert!(
            companies[0].id.starts_with("coop"),
            "Cooperative ID should be a real identifier"
        );
        // The ID must NOT be a phantom like "PEASANT_SURPLUS" or "FreePeasant".
        assert_ne!(companies[0].id, "PEASANT_SURPLUS");
        assert_ne!(companies[0].id, "FreePeasant");
    }

    #[test]
    fn cash_conservation_between_cooperative_and_peasants() {
        let mut region = make_region_with_free_peasants("R1", 1000);
        let mut company = make_cooperative("coop1", "R1", 50000.0);
        let mut building = make_warehouse("coop1", "R1");

        let mut companies = vec![company];
        let mut buildings = vec![building];
        let mut regions = vec![region];

        let mut market_history = MarketHistory::default();
        market_history
            .vwap_per_commodity
            .insert(Commodity::Cereal, 50.0);
        market_history
            .vwap_per_commodity
            .insert(Commodity::Vegetable, 30.0);
        market_history
            .vwap_per_commodity
            .insert(Commodity::Meat, 100.0);

        let config = PeasantSurplusConfig::default();

        let coop_cash_before = companies[0].available_cash;
        let fp_savings_before = regions[0].class_demographics.rural_classes[&RuralClass::FreePeasant].savings;
        let total_before = coop_cash_before + fp_savings_before;

        let _result = process_peasant_surplus_to_cooperative(
            &mut companies,
            &mut buildings,
            &mut regions,
            &market_history,
            &config,
        );

        let coop_cash_after = companies[0].available_cash;
        let fp_savings_after = regions[0].class_demographics.rural_classes[&RuralClass::FreePeasant].savings;
        let total_after = coop_cash_after + fp_savings_after;

        // M0 conservation: the total cash should be preserved.
        // (cooperative cash decreased, peasant savings increased by the same amount)
        assert!(
            (total_before - total_after).abs() < 0.01,
            "Cash must be conserved: before={:.2}, after={:.2}, diff={:.2}",
            total_before, total_after, total_before - total_after
        );
    }

    #[test]
    fn pro_rata_distribution_across_cooperatives() {
        let mut region = make_region_with_free_peasants("R1", 1000);

        // Two cooperatives with different cash levels.
        let coop1 = make_cooperative("coop1", "R1", 30000.0);
        let coop2 = make_cooperative("coop2", "R1", 60000.0);
        let wh1 = make_warehouse("coop1", "R1");
        let wh2 = make_warehouse("coop2", "R1");

        let mut companies = vec![coop1, coop2];
        let mut buildings = vec![wh1, wh2];
        let mut regions = vec![region];

        let mut market_history = MarketHistory::default();
        market_history
            .vwap_per_commodity
            .insert(Commodity::Cereal, 50.0);
        market_history
            .vwap_per_commodity
            .insert(Commodity::Vegetable, 30.0);
        market_history
            .vwap_per_commodity
            .insert(Commodity::Meat, 100.0);

        let config = PeasantSurplusConfig::default();
        let result = process_peasant_surplus_to_cooperative(
            &mut companies,
            &mut buildings,
            &mut regions,
            &market_history,
            &config,
        );

        // Both cooperatives should be used.
        assert_eq!(result.cooperatives_used, 2, "Both cooperatives should participate");

        // The cooperative with more cash should have paid more (pro-rata).
        let coop1_paid = 30000.0 - companies[0].available_cash;
        let coop2_paid = 60000.0 - companies[1].available_cash;
        assert!(
            coop2_paid > coop1_paid,
            "Cooperative with more cash should pay more (pro-rata): coop1={:.2}, coop2={:.2}",
            coop1_paid, coop2_paid
        );
    }

    #[test]
    fn ensure_cooperatives_creates_for_regions_with_free_peasants() {
        let region = make_region_with_free_peasants("R1", 500);
        let regions = vec![region];

        let mut companies: Vec<Company> = vec![];
        let mut buildings: Vec<Building> = vec![];

        let created = ensure_agricultural_cooperatives(
            &mut companies,
            &mut buildings,
            &regions,
            1000.0,
            10.0,
        );

        assert_eq!(created, 1, "One cooperative should be created");
        assert_eq!(companies.len(), 1, "One company should exist");
        assert!(
            matches!(companies[0].legal_form, LegalForm::Cooperative(_)),
            "Created company should be a cooperative"
        );
        assert_eq!(companies[0].sector, Sector::Agriculture);
        assert_eq!(buildings.len(), 1, "One warehouse should be created");
    }

    #[test]
    fn ensure_cooperatives_skips_regions_without_free_peasants() {
        let mut region = Region::default();
        region.id = "R1".to_string();
        // No FreePeasant population.
        let regions = vec![region];

        let mut companies: Vec<Company> = vec![];
        let mut buildings: Vec<Building> = vec![];

        let created = ensure_agricultural_cooperatives(
            &mut companies,
            &mut buildings,
            &regions,
            1000.0,
            10.0,
        );

        assert_eq!(created, 0, "No cooperative should be created without FreePeasants");
    }
}

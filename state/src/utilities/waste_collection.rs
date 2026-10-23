//! Waste collection and processing — Phase 8.3.
//!
//! Aggregates waste generation from all buildings per micro-region, routes it to
//! landfill Buildings (Sector::WasteManagement), and applies overflow penalties.

use crate::economy::transfer_settler::{credit_company_by_id, debit_company_by_id};
use crate::entities::legal_form::{
    LegalForm, MunicipalCompanyData, MunicipalServiceType,
};
use crate::entities::{Building, Company};
use crate::registries::enums::Sector;
use crate::society::geography::Region;
use crate::society::housing::{CommercialBuilding, HousingBuilding};
use crate::state::Season;
use crate::utilities::demand::UtilityDemand;

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

// ============================================================================
// v4.20 WS4: MUNICIPAL SERVICES EMPLOYMENT
// ============================================================================
//
// State-owned municipal-service buildings (`Sector::WasteManagement` and
// `Sector::PublicServices` with `owner_id == "State"`) used to be staffed by
// nobody: the State Employer pseudo-company posts demand only in the FIRST
// region and never adopts the buildings, so `current_employment` stayed at 0
// in every region and `waste_management_total_fte` read 0 economy-wide.
//
// This system gives every governed region a persistent municipal employer —
// a real `Company` with `LegalForm::MunicipalCompany` — per municipal sector:
//
//  * ADOPTION: the company takes ownership of the region's "State"-owned (or
//    previously-municipal, owner-orphaned) buildings of its sector, so the
//    Phase-25 employment sync attributes its hires to real buildings and the
//    `sector_employment` tab gains WasteManagement/PublicServices rows.
//  * LABOR DEMAND: `target_fte_demand` = Σ building worker_capacity ×
//    scale_factor — the same seat formula the employment sync uses.
//  * WAGES: `offered_wage_per_fte` = regional median offered wage × 1.05
//    (fallback: national median), satisfying "competitive wages" while
//    staying honest — the labor market still liquidity-clamps demand.
//  * FUNDING (strict double-entry, both sources are M0-counted):
//      1. a JST services contract — `governance.budget.liquid_reserves`
//         → `local_expenditures`, capped at 60% of reserves so JST
//         procurement and mandate payments still have cash;
//      2. a citizen sanitation/administration fee debited pro-rata from
//         class savings (same pattern as `jst_spending::collect_local_fees`),
//         only to cover the residual after the JST contract.
//    Credited to `available_cash` (spendable payroll pocket) and booked to
//    `liquid_capital` (service revenue → equity), so the utility's capital
//    ledger absorbs the wage expense `process_company` deducts and a
//    payroll-only entity is not liquidated for running the deficit it was
//    funded to run.
//
// All iteration is over slices/BTreeMaps — no HashMap-order effects; no RNG.

/// Deterministic company id for a region's municipal employer of a sector.
fn municipal_employer_id(sector: Sector, region_id: &str) -> String {
    let tag = match sector {
        Sector::WasteManagement => "WASTE",
        _ => "PUBSVC",
    };
    format!("MUN-{}-{}", tag, region_id)
}

fn municipal_display_name(sector: Sector, region_name: &str) -> String {
    let label = match sector {
        Sector::WasteManagement => "Municipal Waste Utility",
        _ => "Municipal Public Services",
    };
    format!("{} ({})", label, region_name)
}

/// Ensure every governed region has a staffed municipal employer for each
/// municipal sector that owns service buildings in the region.
///
/// Called at the top of `process_waste_turn` so the employers exist and post
/// demand before the regional labor market clears later in the same turn.
/// Also safe to call standalone (e.g., from a dedicated turn phase).
///
/// # Arguments
/// * `regions` — regions (JST reserves + citizen savings fund the payroll).
/// * `buildings` — buildings; "State"/orphaned-municipal buildings of the
///   municipal sectors are adopted by the regional employer.
/// * `companies` — the country's company vec; the municipal employer is
///   created once and persists, then updates its demand/wage each turn.
pub fn ensure_municipal_services_employers(
    regions: &mut [Region],
    buildings: &mut [Building],
    companies: &mut Vec<Company>,
) {
    const MUNICIPAL_SECTORS: [Sector; 2] = [Sector::WasteManagement, Sector::PublicServices];
    /// Maximum share of JST reserves the municipal-services contract may
    /// draw per turn — leaves headroom for JST procurement and mandates.
    const JST_RESERVE_CAP: f64 = 0.60;
    /// Sanitation/administration fee per capita, in wage units (0.2% of the
    /// offered wage — same order as `jst_spending`'s 0.1% admin fee).
    const FEE_PER_CAPITA_WAGE_SHARE: f64 = 0.002;

    // Median offered wage — computed per region, with a national fallback.
    // Municipal employers ("MUN-" ids) are excluded so they never ratchet
    // their own reference upward turn over turn.
    let mut national_wages: Vec<f64> = companies
        .iter()
        .filter(|c| c.offered_wage_per_fte > 0.0 && !c.id.starts_with("MUN-"))
        .map(|c| c.offered_wage_per_fte)
        .collect();
    national_wages.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let national_median = if national_wages.is_empty() {
        0.0
    } else {
        national_wages[national_wages.len() / 2]
    };

    for region in regions.iter_mut() {
        let mut regional_wages: Vec<f64> = companies
            .iter()
            .filter(|c| {
                c.region_id == region.id
                    && c.offered_wage_per_fte > 0.0
                    && !c.id.starts_with("MUN-")
            })
            .map(|c| c.offered_wage_per_fte)
            .collect();
        regional_wages.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let regional_median = if regional_wages.is_empty() {
            national_median
        } else {
            regional_wages[regional_wages.len() / 2]
        };
        // Offered wage: >= regional median by construction (median × 1.05).
        let offered_wage = (regional_median * 1.05).max(1.0);

        let region_id = region.id.clone();
        let region_name = if region.display_name.is_empty() {
            region.id.clone()
        } else {
            region.display_name.clone()
        };

        for sector in MUNICIPAL_SECTORS {
            let employer_id = municipal_employer_id(sector, &region_id);

            // ── ADOPTION ── adopt buildings of this sector in this region
            // owned by "State", already owned by this employer (counted so
            // demand/wage/funding refresh every turn), or orphaned by a
            // defunct municipal employer. Private-sector buildings keep
            // their owner.
            let mut capacity: u32 = 0;
            let mut building_ids: Vec<String> = Vec::new();
            for b in buildings.iter_mut() {
                if b.sector != sector || b.region_id != region_id {
                    continue;
                }
                let adoptable = b.owner_id == "State"
                    || b.owner_id == employer_id
                    || (b.owner_id.starts_with("MUN-")
                        && !companies.iter().any(|c| c.id == b.owner_id));
                if !adoptable {
                    continue;
                }
                b.owner_id = employer_id.clone();
                // Seat formula mirrors the Phase-25 employment sync:
                // per-plant seats = worker_capacity × scale_factor.
                capacity = capacity
                    .saturating_add(b.worker_capacity.saturating_mul(b.scale_factor.max(1)));
                building_ids.push(b.id.clone());
            }
            if building_ids.is_empty() {
                continue;
            }

            // ── CREATE / UPDATE EMPLOYER ──
            let idx = match companies.iter().position(|c| c.id == employer_id) {
                Some(i) => i,
                None => {
                    // The municipal concession's fixed capital is valued at
                    // the going wage per staffed seat — a real, market-linked
                    // asset figure, not a synthetic constant.
                    let fixed_capital = capacity as f64 * offered_wage;
                    let gov_id = region
                        .governance
                        .as_ref()
                        .map(|g| g.id.clone())
                        .unwrap_or_else(|| region_id.clone());
                    let service_type = if sector == Sector::WasteManagement {
                        MunicipalServiceType::WasteUtility
                    } else {
                        // Generic municipal services — no dedicated variant
                        // exists in MunicipalServiceType; the default is the
                        // honest "legacy combined utility" bucket.
                        MunicipalServiceType::default()
                    };
                    let mut company = Company::new(
                        employer_id.clone(),
                        municipal_display_name(sector, &region_name),
                        sector,
                        LegalForm::MunicipalCompany(MunicipalCompanyData {
                            owner_municipality: gov_id,
                            service_type,
                            service_coverage: region.population.max(0) as f64,
                            municipal_subsidy: 0.0,
                            privatizable: false,
                            regulatory_oversight: 1.0,
                        }),
                        fixed_capital,
                        0.0, // liquid capital accrues from service revenue below
                        capacity,
                    );
                    company.region_id = region_id.clone();
                    company.state_share = 1.0;
                    // Best turn proxy available inside the region — the
                    // regional tax ledger; 0 when unfilled.
                    company.founded_turn = region
                        .treasury
                        .tax_history
                        .back()
                        .map(|e| e.turn + 1)
                        .unwrap_or(0);
                    companies.push(company);
                    companies.len() - 1
                }
            };
            let company = &mut companies[idx];
            company.worker_capacity = capacity;
            company.building_ids = building_ids;
            company.target_fte_demand = capacity;
            company.physical_fte_demand = capacity;
            // Track the offered-wage history for downward-rigidity logic.
            if company.offered_wage_per_fte > 0.0 {
                company.prev_offered_wage_per_fte = company.offered_wage_per_fte;
            }
            company.offered_wage_per_fte = offered_wage;
            company.target_wage = offered_wage;
            company.state_share = 1.0;
            if let LegalForm::MunicipalCompany(ref mut data) = company.legal_form {
                data.service_coverage = region.population.max(0) as f64;
            }

            // ── FUNDING (strict double-entry) ──
            let payroll_need = capacity as f64 * offered_wage;
            let mut shortfall = (payroll_need - company.operational_cash()).max(0.0);
            let mut funded = 0.0_f64;

            // 1) JST municipal-services contract — the JST pays the utility
            //    for the services its buildings deliver this turn.
            if shortfall > 0.0 {
                if let Some(gov) = region.governance.as_mut() {
                    let pay = shortfall.min(gov.budget.liquid_reserves * JST_RESERVE_CAP);
                    if pay > 0.0 {
                        gov.budget.liquid_reserves -= pay;
                        gov.budget.local_expenditures += pay;
                        gov.budget.budget_balance -= pay;
                        shortfall -= pay;
                        funded += pay;
                    }
                }
            }

            // 2) Citizen sanitation/administration fee for the residual —
            //    per-capita billing debited pro-rata from class savings
            //    (BTreeMap iteration — deterministic), clamped per class.
            if shortfall > 0.0 {
                let total_class_pop: i64 = region
                    .class_demographics
                    .rural_classes
                    .values()
                    .chain(region.class_demographics.urban_classes.values())
                    .map(|d| d.population)
                    .sum();
                if total_class_pop > 0 {
                    let fee_total =
                        (total_class_pop as f64 * offered_wage * FEE_PER_CAPITA_WAGE_SHARE)
                            .min(shortfall);
                    let fee_per_person = fee_total / total_class_pop as f64;
                    for demo in region
                        .class_demographics
                        .rural_classes
                        .values_mut()
                        .chain(region.class_demographics.urban_classes.values_mut())
                    {
                        let class_fee = fee_per_person * demo.population.max(0) as f64;
                        let actual = class_fee.min(demo.savings.max(0.0));
                        demo.savings -= actual;
                        if demo.population > 0 {
                            demo.savings_per_capita = demo.savings / demo.population as f64;
                        }
                        funded += actual;
                    }
                }
            }

            // Credit the payroll pocket AND book the revenue to equity so
            // `process_company`'s wage-expense deduction does not liquidate
            // a utility that is solvent-by-funding.
            if funded > 0.0 {
                company.available_cash += funded;
                company.liquid_capital += funded;
                if let LegalForm::MunicipalCompany(ref mut data) = company.legal_form {
                    data.municipal_subsidy = funded;
                }
            }
        }
    }
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
/// * Landfill buildings are identified by `Sector::WasteManagement` and `landfill_state.is_some()`.
/// * Operating costs deducted from owning `Company.available_cash`.
/// * Recovered commodities added to `Building.inventory`.
/// * Overflow increases `health_degradation_rate` on `ClassDemographics` (future).
/// * v4.20 WS4: `companies` is a `&mut Vec` (was `&mut []`) because
///   `ensure_municipal_services_employers` may instantiate a region's
///   municipal employer on first run.
pub fn process_waste_turn(
    regions: &mut [Region],
    buildings: &mut [Building],
    companies: &mut Vec<Company>,
    housing_buildings: &[HousingBuilding],
    commercial_buildings: &[CommercialBuilding],
    _season: Season,
) -> WasteTurnResult {
    let mut result = WasteTurnResult::default();

    // v4.20 WS4: Municipal service employers (waste + public services) must
    // exist, own their buildings, and post funded labor demand BEFORE the
    // regional labor market clears later this turn.
    ensure_municipal_services_employers(regions, buildings, companies);

    for region in regions.iter_mut() {
        let region_id = region.id.clone();
        let mut total_waste: f64 = 0.0;

        // Aggregate waste from housing buildings in this region's micro-regions
        for hb in housing_buildings.iter() {
            if !region.micro_regions.contains_key(&hb.micro_region_id) {
                continue;
            }
            let demand = UtilityDemand::for_housing(hb, _season);
            total_waste += demand.waste_generation;
        }

        // Aggregate waste from commercial buildings
        for cb in commercial_buildings.iter() {
            if !region.micro_regions.contains_key(&cb.micro_region_id) {
                continue;
            }
            let demand = UtilityDemand::for_commercial(cb, _season);
            total_waste += demand.waste_generation;
        }

        if total_waste <= 0.0 {
            continue;
        }

        // Find landfill buildings in this region
        let mut landfill_indices: Vec<usize> = Vec::new();
        for (i, building) in buildings.iter().enumerate() {
            if building.sector == Sector::WasteManagement
                && building.region_id == region_id
                && building.landfill_state.is_some()
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
        let all_recovered: HashMap<String, f64> = HashMap::new();

        for &idx in &landfill_indices {
            let building = &mut buildings[idx];
            let landfill_state = building.landfill_state.as_mut().unwrap();

            // LOGISTICAL BOUND 2: Hard capacity stop — accept_waste returns 0 if full
            let mut waste_input = std::collections::HashMap::new();
            waste_input.insert(
                crate::registries::enums::Commodity::MixedWaste,
                waste_per_landfill,
            );
            let accepted = landfill_state.accept_waste(&waste_input);
            if accepted <= 0.0 {
                overflow_total += waste_per_landfill;
                continue;
            }
            processed_total += accepted;
            // Pollution from landfill (leachate leakage)
            pollution_total += landfill_state.leachate_leakage();

            // Deduct operating cost from owning company, credit a LocalServices supplier in the same region
            let owner_id = building.owner_id.clone();
            let op_cost = 2000.0; // Base operating cost per turn
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

        result
            .waste_processed
            .insert(region_id.clone(), processed_total);
        result
            .waste_overflow
            .insert(region_id.clone(), overflow_total);
        result
            .pollution_generated
            .insert(region_id.clone(), pollution_total);
        if !all_recovered.is_empty() {
            result
                .commodities_recovered
                .insert(region_id, all_recovered);
        }
    }

    result
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::local_government::{RegionalBudget, RegionalGovernance};

    fn region_with_gov(id: &str, reserves: f64) -> Region {
        let mut r = Region::default();
        r.id = id.to_string();
        r.display_name = format!("Region {id}");
        r.population = 10_000;
        let mut gov = RegionalGovernance::default();
        gov.id = format!("JST-{id}");
        gov.budget = RegionalBudget {
            liquid_reserves: reserves,
            ..Default::default()
        };
        r.governance = Some(gov);
        r
    }

    fn state_service_building(id: &str, region_id: &str, sector: Sector, cap: u32) -> Building {
        Building {
            id: id.to_string(),
            sector,
            region_id: region_id.to_string(),
            owner_id: "State".to_string(),
            worker_capacity: cap,
            scale_factor: 1,
            ..Default::default()
        }
    }

    fn reference_company(id: &str, region_id: &str, wage: f64) -> Company {
        let mut c = Company::default();
        c.id = id.to_string();
        c.region_id = region_id.to_string();
        c.sector = Sector::Agriculture;
        c.offered_wage_per_fte = wage;
        c
    }

    #[test]
    fn test_municipal_employer_created_funded_and_posts_demand() {
        let mut regions = vec![region_with_gov("REG1", 50_000.0)];
        let mut buildings = vec![
            state_service_building("BLD-WM-1", "REG1", Sector::WasteManagement, 40),
            state_service_building("BLD-PS-1", "REG1", Sector::PublicServices, 20),
        ];
        let mut companies = vec![reference_company("KRS-1", "REG1", 200.0)];

        ensure_municipal_services_employers(&mut regions, &mut buildings, &mut companies);

        let waste = companies
            .iter()
            .find(|c| c.id == "MUN-WASTE-REG1")
            .expect("waste employer must exist");
        let pubsvc = companies
            .iter()
            .find(|c| c.id == "MUN-PUBSVC-REG1")
            .expect("public-services employer must exist");

        // Labor demand = Σ building worker_capacity × scale_factor.
        assert_eq!(waste.target_fte_demand, 40);
        assert_eq!(waste.physical_fte_demand, 40);
        assert_eq!(pubsvc.target_fte_demand, 20);

        // Competitive wage: regional median (200) × 1.05 = 210 ≥ median.
        assert_eq!(waste.offered_wage_per_fte, 210.0);
        assert!(waste.offered_wage_per_fte >= 200.0);

        // Municipal ownership + adoption.
        assert_eq!(waste.state_share, 1.0);
        assert!(matches!(
            waste.legal_form,
            LegalForm::MunicipalCompany(ref d)
                if d.service_type == MunicipalServiceType::WasteUtility
                    && d.owner_municipality == "JST-REG1"
        ));
        assert_eq!(buildings[0].owner_id, "MUN-WASTE-REG1");
        assert_eq!(buildings[1].owner_id, "MUN-PUBSVC-REG1");
        assert!(waste.building_ids.contains(&"BLD-WM-1".to_string()));

        // Funding: payroll needs 40×210 + 20×210 = 12_600 from JST reserves
        // (cap 60% of 50_000 — well above the need).
        assert_eq!(waste.available_cash, 8_400.0);
        assert_eq!(pubsvc.available_cash, 4_200.0);
        let gov = regions[0].governance.as_ref().unwrap();
        assert_eq!(gov.budget.liquid_reserves, 50_000.0 - 12_600.0);
        assert_eq!(gov.budget.local_expenditures, 12_600.0);
    }

    #[test]
    fn test_municipal_employer_is_idempotent() {
        let mut regions = vec![region_with_gov("REG1", 50_000.0)];
        let mut buildings = vec![state_service_building(
            "BLD-WM-1",
            "REG1",
            Sector::WasteManagement,
            40,
        )];
        let mut companies = vec![reference_company("KRS-1", "REG1", 200.0)];

        ensure_municipal_services_employers(&mut regions, &mut buildings, &mut companies);
        ensure_municipal_services_employers(&mut regions, &mut buildings, &mut companies);

        let mun_count = companies
            .iter()
            .filter(|c| c.id.starts_with("MUN-WASTE-REG1"))
            .count();
        assert_eq!(mun_count, 1, "employer must persist, not duplicate");
        let waste = companies
            .iter()
            .find(|c| c.id == "MUN-WASTE-REG1")
            .unwrap();
        assert_eq!(waste.target_fte_demand, 40);
        assert_eq!(waste.building_ids.len(), 1);
        // Prior offered wage is tracked for downward-rigidity logic.
        assert_eq!(waste.prev_offered_wage_per_fte, 210.0);
    }

    #[test]
    fn test_private_buildings_are_not_adopted() {
        let mut regions = vec![region_with_gov("REG1", 50_000.0)];
        let mut b = state_service_building("BLD-WM-1", "REG1", Sector::WasteManagement, 40);
        b.owner_id = "KRS-9".to_string(); // privately owned
        let mut buildings = vec![b];
        let mut companies = vec![reference_company("KRS-1", "REG1", 200.0)];

        ensure_municipal_services_employers(&mut regions, &mut buildings, &mut companies);

        assert!(
            !companies.iter().any(|c| c.id.starts_with("MUN-")),
            "no municipal employer when nothing is state-owned"
        );
        assert_eq!(buildings[0].owner_id, "KRS-9");
    }

    #[test]
    fn test_employer_posts_demand_even_unfunded() {
        // Broke JST, no class savings — employer still exists and posts
        // demand; the labor market liquidity-clamps actual hires.
        let mut regions = vec![region_with_gov("REG1", 0.0)];
        let mut buildings = vec![state_service_building(
            "BLD-WM-1",
            "REG1",
            Sector::WasteManagement,
            40,
        )];
        let mut companies: Vec<Company> = Vec::new();

        ensure_municipal_services_employers(&mut regions, &mut buildings, &mut companies);

        let waste = companies
            .iter()
            .find(|c| c.id == "MUN-WASTE-REG1")
            .expect("employer exists even with zero funding");
        assert_eq!(waste.target_fte_demand, 40);
        assert!(waste.offered_wage_per_fte >= 1.0, "wage floor holds");
        assert_eq!(waste.available_cash, 0.0);
    }
}

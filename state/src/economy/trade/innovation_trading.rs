//! Phase 7/E.9: Innovation Points B2B trading.
//!
//! This module implements the physical commodity trading of domain-specific
//! Innovation Points between universities (producers) and the State (consumer).
//! Also handles ResearchOutput from research institutes.

use crate::economy::innovation_config::InnovationConfig;
use crate::economy::trade::transfer_settler::{
    settle_transfer_mapped, settle_treasury_to_company, TransferRecipient,
};
use crate::entities::{Building, Company};
use crate::registries::enums::{Commodity, Sector};
use crate::registries::tech_tree::ResearchDomain;
use crate::state::Country;
use std::collections::BTreeMap;

/// All innovation-point commodity variants (Phase E.9).
const INNOVATION_COMMODITIES: [Commodity; 8] = [
    Commodity::InnovationEngineering,
    Commodity::InnovationMetallurgy,
    Commodity::InnovationChemistry,
    Commodity::InnovationElectronics,
    Commodity::InnovationComputing,
    Commodity::InnovationMedicine,
    Commodity::InnovationPhysics,
    Commodity::InnovationAgronomy,
];

/// Maps an innovation commodity to its ResearchDomain.
fn commodity_to_domain(commodity: &Commodity) -> Option<ResearchDomain> {
    match commodity {
        Commodity::InnovationEngineering => Some(ResearchDomain::Engineering),
        Commodity::InnovationMetallurgy => Some(ResearchDomain::Metallurgy),
        Commodity::InnovationChemistry => Some(ResearchDomain::Chemistry),
        Commodity::InnovationElectronics => Some(ResearchDomain::Electronics),
        Commodity::InnovationComputing => Some(ResearchDomain::Computing),
        Commodity::InnovationMedicine => Some(ResearchDomain::Medicine),
        Commodity::InnovationPhysics => Some(ResearchDomain::Physics),
        Commodity::InnovationAgronomy => Some(ResearchDomain::Agronomy),
        _ => None,
    }
}

/// Trades domain-specific Innovation Points and ResearchOutput via B2B market.
///
/// # Arguments
/// * `buildings` - Slice of buildings (universities/research institutes) with outputs in inventory
/// * `companies` - Slice of all companies (for finding building owner companies)
/// * `country` - Mutable country state (Treasury debited, bank synced)
/// * `building_inventories` - Building inventories containing innovation commodities
/// * `average_wage` - Country average wage for dynamic price computation (Phase E.8)
/// * `_config` - Innovation config (retained for API compat; price is now dynamic)
///
/// # Rules
/// * Physical Limits: Innovation Points are physical commodities in inventory
/// * State-owned universities: direct transfer (no payment)
/// * Private/Local-Gov universities: State pays via `settle_treasury_to_company`
///   (Phase E.2 — fixes fiat leak where money went to `building.reserve` instead
///   of the owner company's cash account)
/// * Double-Entry: Treasury cash decreases, owner company cash increases
/// * Domain-specific: each commodity variant maps to the matching innovation_pool entry
/// * ResearchOutput: transferred to treasury.science.research_output
pub fn trade_innovation_points_b2b(
    buildings: &mut [Building],
    companies: &mut [Company],
    country: &mut Country,
    building_inventories: &mut BTreeMap<String, BTreeMap<Commodity, f64>>,
    average_wage: f64,
    _config: &InnovationConfig,
) {
    // Build owner_id → company_idx map for settle_treasury_to_company.
    let id_to_idx: std::collections::HashMap<String, usize> = companies
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), i))
        .collect();

    for building in buildings.iter_mut() {
        let building_inventory = building_inventories.entry(building.id.clone()).or_default();

        // Process all innovation commodity variants + ResearchOutput
        let commodities_to_process: Vec<Commodity> = INNOVATION_COMMODITIES
            .iter()
            .cloned()
            .chain(std::iter::once(Commodity::ResearchOutput))
            .collect();

        for commodity in &commodities_to_process {
            let available_points = building_inventory
                .get(commodity)
                .copied()
                .unwrap_or(0.0);

            if available_points <= 0.0 {
                continue;
            }

            // Check if State owns this building
            if building.owner_id.starts_with("STATE_") {
                // Direct transfer: State owns the university/institute
                if let Some(domain) = commodity_to_domain(commodity) {
                    *country.budget.science.innovation_pool.entry(domain).or_insert(0.0) +=
                        available_points;
                } else if *commodity == Commodity::ResearchOutput {
                    country.budget.science.research_output += available_points;
                }
                building_inventory.insert(*commodity, 0.0);
            } else {
                // B2B purchase: State must buy from Local Gov or Private owner.
                // Phase E.2: Use settle_treasury_to_company to credit the owner
                // company's cash account (fixes fiat leak where money went to
                // building.reserve with no real counterparty).
                // Phase E.8: Dynamic price — 10× average_wage (inflation-proof, Rule 2).
                // Replaces the hardcoded 100.0 magic number.
                let price_per_point = (average_wage * 10.0).max(1.0);
                let total_cost = available_points * price_per_point;

                // Find the owner company index.
                let owner_idx = id_to_idx.get(&building.owner_id).copied();

                if let Some(idx) = owner_idx {
                    // Settle via proper double-entry transfer.
                    if settle_treasury_to_company(companies, idx, total_cost, country).is_ok() {
                        // Success — credit innovation points to treasury.
                        if let Some(domain) = commodity_to_domain(commodity) {
                            *country.budget.science.innovation_pool.entry(domain).or_insert(0.0) +=
                                available_points;
                        } else if *commodity == Commodity::ResearchOutput {
                            country.budget.science.research_output += available_points;
                        }
                        building_inventory.insert(*commodity, 0.0);
                    }
                    // If transfer fails (insufficient Treasury), points remain unsold.
                } else {
                    // Owner company not found — fallback to old behavior for
                    // buildings without a company owner (e.g. local gov buildings).
                    // This preserves existing behavior for edge cases.
                    if country.budget.liquid_reserves >= total_cost {
                        country.budget.liquid_reserves -= total_cost;
                        if let Some(domain) = commodity_to_domain(commodity) {
                            *country.budget.science.innovation_pool.entry(domain).or_insert(0.0) +=
                                available_points;
                        } else if *commodity == Commodity::ResearchOutput {
                            country.budget.science.research_output += available_points;
                        }
                        building.reserve += total_cost;
                        building_inventory.insert(*commodity, 0.0);
                    }
                }
            }
        }
    }
}

/// Phase 95/E.9: Purchase domain-specific Innovation Points from universities
/// for a corporate R&D budget.
///
/// Scans `buildings` for university-sector buildings with the matching domain's
/// innovation commodity in inventory and purchases pro-rata across all available
/// suppliers (Rule 5). Each purchase is settled via `settle_transfer` (Rule 1 —
/// strict double-entry: company pays, university owner receives).
///
/// # Arguments
/// * `companies` - Mutable slice of all companies (payer + university owners).
/// * `buildings` - Mutable slice of buildings (universities with Innovation Points).
/// * `building_inventories` - Building inventories (mutated to deduct points).
/// * `payer_idx` - Index of the paying company in `companies`.
/// * `domain` - Research domain (determines which innovation commodity to purchase).
/// * `points_needed` - Total Innovation Points the company wants to acquire.
/// * `price_per_point` - Dynamic price per Innovation Point (computed from `average_wage`).
/// * `country` - Mutable country state (for `settle_transfer` bank sync).
///
/// # Returns
/// The total number of Innovation Points actually acquired (may be less than
/// `points_needed` if domestic supply is insufficient).
///
/// # Rules
/// * Domain-specific: only purchases the commodity matching `domain.innovation_commodity()`.
/// * Pro-rata distribution: each university supplies points proportional to its
///   available inventory relative to total domestic supply.
/// * Double-entry: `settle_transfer` debits the payer and credits the university
///   owner company via `TransferRecipient::OtherCompany`.
/// * State-owned universities (`owner_id.starts_with("STATE_")`): the company
///   pays the State Treasury directly via `TransferRecipient::Treasury`.
/// * If the payer cannot afford the full amount, partial purchase is made
///   (graceful degradation — buy what you can).
/// * Points are deducted from `building_inventories` immediately.
pub fn purchase_innovation_points_for_company(
    companies: &mut [Company],
    buildings: &mut [Building],
    building_inventories: &mut BTreeMap<String, BTreeMap<Commodity, f64>>,
    payer_idx: usize,
    domain: ResearchDomain,
    points_needed: f64,
    price_per_point: f64,
    country: &mut Country,
) -> f64 {
    if points_needed <= 0.0 || price_per_point <= 0.0 {
        return 0.0;
    }

    let target_commodity = domain.innovation_commodity();

    // Build id → idx map for settle_transfer.
    let id_to_idx: std::collections::HashMap<String, usize> = companies
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), i))
        .collect();

    // Collect available points per university building (pro-rata supply).
    let mut suppliers: Vec<(usize, String, f64, bool)> = Vec::new(); // (building_idx, building_id, available, is_state_owned)
    let mut total_available: f64 = 0.0;
    for (b_idx, building) in buildings.iter().enumerate() {
        if building.sector != Sector::EducationalServices {
            continue;
        }
        let inv = building_inventories.entry(building.id.clone()).or_default();
        let available = inv
            .get(&target_commodity)
            .copied()
            .unwrap_or(0.0);
        if available <= 0.0 {
            continue;
        }
        let is_state = building.owner_id.starts_with("STATE_");
        suppliers.push((b_idx, building.id.clone(), available, is_state));
        total_available += available;
    }

    if total_available <= 0.0 {
        return 0.0; // No domestic supply.
    }

    let points_to_buy = points_needed.min(total_available);

    // Check payer affordability — buy what we can.
    let payer_cash = companies
        .get(payer_idx)
        .map(|c| {
            c.brokerage_account
                .as_ref()
                .map(|ba| ba.cash.max(0.0))
                .unwrap_or(c.available_cash.max(0.0))
        })
        .unwrap_or(0.0);

    if payer_cash <= 0.0 {
        return 0.0;
    }

    let affordable_points = (payer_cash / price_per_point).min(points_to_buy);
    if affordable_points <= 0.0 {
        return 0.0;
    }

    // Purchase pro-rata from each supplier.
    let mut points_acquired: f64 = 0.0;
    for (b_idx, building_id, available, is_state) in &suppliers {
        let share = available / total_available;
        let points_from_this = affordable_points * share;
        if points_from_this <= 0.0 {
            continue;
        }
        let cost = points_from_this * price_per_point;
        if cost <= 0.0 {
            continue;
        }

        // Deduct points from building inventory.
        let inv = building_inventories.entry(building_id.clone()).or_default();
        let current = inv
            .get(&target_commodity)
            .copied()
            .unwrap_or(0.0);
        let new_val = (current - points_from_this).max(0.0);
        if new_val > 0.0 {
            inv.insert(target_commodity, new_val);
        } else {
            inv.remove(&target_commodity);
        }

        // Settle the payment via double-entry transfer.
        let recipient = if *is_state {
            TransferRecipient::Treasury
        } else {
            // Find the university owner company index.
            let owner_id = &buildings[*b_idx].owner_id;
            match id_to_idx.get(owner_id) {
                Some(&owner_idx) => TransferRecipient::OtherCompany {
                    recipient_idx: owner_idx,
                },
                None => TransferRecipient::Treasury, // Fallback: pay Treasury if owner not found.
            }
        };

        let _ = settle_transfer_mapped_safe(
            companies, &id_to_idx, payer_idx, cost, &recipient, country,
        );

        // Credit the building's reserve for non-state owners (revenue).
        if !is_state {
            buildings[*b_idx].reserve += cost;
        }

        points_acquired += points_from_this;
    }

    points_acquired
}

/// Wrapper around `settle_transfer_mapped` that accepts a `HashMap<String, usize>`.
fn settle_transfer_mapped_safe(
    companies: &mut [Company],
    id_to_idx: &std::collections::HashMap<String, usize>,
    payer_idx: usize,
    amount: f64,
    recipient: &TransferRecipient,
    country: &mut Country,
) -> Result<(), String> {
    match settle_transfer_mapped(companies, id_to_idx, payer_idx, amount, recipient, country) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Transfer failed: {:?}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::enums::Sector;

    fn make_country_with_treasury(liquid_reserves: f64) -> Country {
        let mut country = Country::default();
        country.budget.liquid_reserves = liquid_reserves;
        country
    }

    /// Test average wage: 10.0 → price_per_point = 100.0 (matches old hardcoded value).
    const TEST_AVG_WAGE: f64 = 10.0;

    #[test]
    fn state_owned_university_direct_transfer() {
        let mut building = Building::default();
        building.id = "UNI_001".to_string();
        building.owner_id = "STATE_CENTRAL".to_string();
        building.sector = Sector::EducationalServices;

        let mut country = make_country_with_treasury(10000.0);

        let mut building_inventories = BTreeMap::new();
        building_inventories.insert(
            "UNI_001".to_string(),
            BTreeMap::from([(Commodity::InnovationEngineering, 50.0)]),
        );

        let mut companies: Vec<Company> = Vec::new();
        trade_innovation_points_b2b(
            &mut [building],
            &mut companies,
            &mut country,
            &mut building_inventories,
            TEST_AVG_WAGE,
            &InnovationConfig::default(),
        );

        assert_eq!(
            country.budget.science.innovation_pool[&ResearchDomain::Engineering],
            50.0
        );
        assert_eq!(country.budget.liquid_reserves, 10000.0); // No cost for direct transfer
        assert_eq!(
            building_inventories["UNI_001"]
                .get(&Commodity::InnovationEngineering)
                .copied()
                .unwrap_or(0.0),
            0.0
        );
    }

    #[test]
    fn private_university_b2b_purchase_credits_owner() {
        // Phase E.2: Private university purchase must credit the owner company,
        // not just building.reserve (fiat leak fix).
        let mut building = Building::default();
        building.id = "UNI_002".to_string();
        building.owner_id = "COMPANY_PHARMA".to_string();
        building.sector = Sector::EducationalServices;

        let mut owner = Company::default();
        owner.id = "COMPANY_PHARMA".to_string();
        owner.available_cash = 0.0;
        // Phase 94: Give the owner a primary_bank_id so settle_treasury_to_company
        // can sync bank reserves (M0 conservation).
        owner.primary_bank_id = Some("BANK_TEST".to_string());

        // Create a test bank so the bank sync in settle_treasury_to_company works.
        let mut bank = Company::default();
        bank.id = "BANK_TEST".to_string();
        bank.sector = Sector::Banking;
        bank.bank_type = Some(crate::state::banking::BankType::Universal);
        bank.balance_sheet = Some(crate::state::banking::BankBalanceSheet {
            reserves_at_central_bank: 1_000_000.0,
            deposits: 1_000_000.0,
            tier_1_capital: 0.0,
            ..Default::default()
        });

        let mut country = make_country_with_treasury(10000.0);
        let mut companies = vec![owner, bank];

        let mut building_inventories = BTreeMap::new();
        building_inventories.insert(
            "UNI_002".to_string(),
            BTreeMap::from([(Commodity::InnovationMedicine, 50.0)]),
        );

        let mut buildings = vec![building];
        trade_innovation_points_b2b(
            &mut buildings,
            &mut companies,
            &mut country,
            &mut building_inventories,
            TEST_AVG_WAGE,
            &InnovationConfig::default(),
        );

        assert_eq!(
            country.budget.science.innovation_pool[&ResearchDomain::Medicine],
            50.0
        );
        // Price = 10 * 10 = 100, cost = 50 * 100 = 5000
        assert_eq!(country.budget.liquid_reserves, 5000.0);
        // Phase E.2: Owner company receives the payment, not building.reserve.
        let owner_cash = companies[0]
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash)
            .unwrap_or(companies[0].available_cash);
        assert_eq!(owner_cash, 5000.0); // Owner company credited
    }

    #[test]
    fn insufficient_cash_no_purchase() {
        let mut building = Building::default();
        building.id = "UNI_003".to_string();
        building.owner_id = "COMPANY_PHARMA".to_string();
        building.sector = Sector::EducationalServices;

        let mut owner = Company::default();
        owner.id = "COMPANY_PHARMA".to_string();

        let mut country = make_country_with_treasury(1000.0); // Insufficient for 50 * 100 = 5000

        let mut building_inventories = BTreeMap::new();
        building_inventories.insert(
            "UNI_003".to_string(),
            BTreeMap::from([(Commodity::InnovationChemistry, 50.0)]),
        );

        let mut buildings = vec![building];
        let mut companies = vec![owner];
        trade_innovation_points_b2b(
            &mut buildings,
            &mut companies,
            &mut country,
            &mut building_inventories,
            TEST_AVG_WAGE,
            &InnovationConfig::default(),
        );

        assert_eq!(
            country.budget.science.innovation_pool[&ResearchDomain::Chemistry],
            0.0
        ); // No purchase
        assert_eq!(country.budget.liquid_reserves, 1000.0); // Cash unchanged
        assert_eq!(
            building_inventories["UNI_003"]
                .get(&Commodity::InnovationChemistry)
                .copied()
                .unwrap_or(0.0),
            50.0
        ); // Points remain unsold
    }

    #[test]
    fn research_output_transferred_to_treasury() {
        let mut building = Building::default();
        building.id = "RI_001".to_string();
        building.owner_id = "STATE_CENTRAL".to_string();
        building.sector = Sector::EducationalServices;

        let mut country = make_country_with_treasury(10000.0);

        let mut building_inventories = BTreeMap::new();
        building_inventories.insert(
            "RI_001".to_string(),
            BTreeMap::from([(Commodity::ResearchOutput, 30.0)]),
        );

        let mut companies: Vec<Company> = Vec::new();
        trade_innovation_points_b2b(
            &mut [building],
            &mut companies,
            &mut country,
            &mut building_inventories,
            TEST_AVG_WAGE,
            &InnovationConfig::default(),
        );

        assert_eq!(country.budget.science.research_output, 30.0);
    }
}

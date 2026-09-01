//! Phase 7: Innovation Points B2B trading.
//!
//! This module implements the physical commodity trading of Innovation Points
//! between universities (producers) and the State (consumer).

use crate::economy::innovation_config::InnovationConfig;
use crate::economy::trade::transfer_settler::{settle_transfer_mapped, TransferRecipient};
use crate::entities::{Building, Company};
use crate::registries::enums::{Commodity, Sector};
use crate::state::treasury::Treasury;
use crate::state::Country;
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
        let building_inventory = building_inventories.entry(building.id.clone()).or_default();
        let available_points = building_inventory
            .get(&Commodity::InnovationPoints)
            .copied()
            .unwrap_or(0.0);

        if available_points <= 0.0 {
            continue;
        }

        // Check if State owns this building
        if building.owner_id.starts_with("STATE_") {
            // Direct transfer: State owns the university
            // Transfer Innovation Points to Treasury.science.innovation_points
            treasury.science.innovation_points += available_points;
            *building_inventory
                .entry(Commodity::InnovationPoints)
                .or_insert(0.0) = 0.0;
        } else {
            // B2B purchase: State must buy from Local Gov or Private owner
            let price_per_point = config.innovation_point_price;
            let total_cost = available_points * price_per_point;

            if treasury.liquid_reserves >= total_cost {
                // State can afford purchase
                treasury.liquid_reserves -= total_cost;
                treasury.science.innovation_points += available_points;
                building.reserve += total_cost;
                *building_inventory
                    .entry(Commodity::InnovationPoints)
                    .or_insert(0.0) = 0.0;
            }
            // If State cannot afford, points remain in building inventory (unsold)
        }
    }
}

/// Phase 95: Purchase Innovation Points from universities for a corporate R&D budget.
///
/// Scans `buildings` for university-sector buildings with `InnovationPoints` in
/// inventory and purchases pro-rata across all available suppliers (Rule 5).
/// Each purchase is settled via `settle_transfer` (Rule 1 — strict double-entry:
/// company pays, university owner receives).
///
/// # Arguments
/// * `companies` - Mutable slice of all companies (payer + university owners).
/// * `buildings` - Mutable slice of buildings (universities with Innovation Points).
/// * `building_inventories` - Building inventories (mutated to deduct points).
/// * `payer_idx` - Index of the paying company in `companies`.
/// * `points_needed` - Total Innovation Points the company wants to acquire.
/// * `price_per_point` - Dynamic price per Innovation Point (computed from `average_wage`).
/// * `country` - Mutable country state (for `settle_transfer` bank sync).
///
/// # Returns
/// The total number of Innovation Points actually acquired (may be less than
/// `points_needed` if domestic supply is insufficient).
///
/// # Rules
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
    points_needed: f64,
    price_per_point: f64,
    country: &mut Country,
) -> f64 {
    if points_needed <= 0.0 || price_per_point <= 0.0 {
        return 0.0;
    }

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
            .get(&Commodity::InnovationPoints)
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
            .get(&Commodity::InnovationPoints)
            .copied()
            .unwrap_or(0.0);
        let new_val = (current - points_from_this).max(0.0);
        if new_val > 0.0 {
            inv.insert(Commodity::InnovationPoints, new_val);
        } else {
            inv.remove(&Commodity::InnovationPoints);
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
            building_inventories["UNI_001"]
                .get(&Commodity::InnovationPoints)
                .copied()
                .unwrap_or(0.0),
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
            building_inventories["UNI_003"]
                .get(&Commodity::InnovationPoints)
                .copied()
                .unwrap_or(0.0),
            50.0
        ); // Points remain unsold
    }
}

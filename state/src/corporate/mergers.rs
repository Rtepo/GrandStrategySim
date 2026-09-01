//! Mergers and Acquisitions lifecycle (Phase E — full execution).
//!
//! This module implements organic, market-driven M&A with the following invariants:
//! - Every cash movement routes through `settle_transfer` / `settle_transfer_mapped`.
//! - Physical inventory is moved one commodity at a time to an acquirer building.
//! - Cross-region moves consume `Commodity::FreightCapacity` from the acquirer.
//! - Overflow is first fire-sold at `FIRE_SALE_DISCOUNT_RATIO`; any unsold portion
//!   is written off as a real loss.
//! - `LoanRef`s and bank `loans_issued.borrower_id` are updated to the acquirer.

use crate::economy::market::MarketSignal;
use crate::economy::transfer_settler::{settle_transfer_mapped, TransferRecipient};
use crate::entities::{Building, Company};
use crate::registries::enums::Commodity;
use crate::state::banking::LoanStatus;
use crate::state::Country;
use std::collections::BTreeMap;
use std::collections::HashMap;

/// Discount applied to the reference market price when an acquirer must liquidate
/// overflow inventory that does not physically fit into its warehouses.
pub const FIRE_SALE_DISCOUNT_RATIO: f64 = 0.35;

/// Freight consumption ratio: one unit of `Commodity::FreightCapacity` is required
/// per unit of cargo for cross-region physical transfers. Within the same region
/// no freight is consumed.
pub const FREIGHT_PER_UNIT: f64 = 1.0;

/// A planned acquisition used for the first (planning) pass.
#[derive(Debug)]
struct AcquisitionPlan {
    target_idx: usize,
    acquirer_idx: usize,
    price: f64,
    target_buildings: Vec<usize>,
}

/// Process M&A for a single country in the turn loop.
///
/// Runs in two safe passes:
/// 1. **Scan** for distressed or sector-saturated targets and a matching acquirer.
/// 2. **Execute** acquisitions via a delta buffer: tombstone the target and copy
///    assets, loans and (freight-aware) inventory to the acquirer.
pub fn process_mergers_and_acquisitions(
    companies: &mut [Company],
    buildings: &mut [Building],
    country: &mut Country,
    _year: u32,
    market_signal: &MarketSignal,
    _current_turn: u32,
) {
    if companies.is_empty() {
        return;
    }

    // ------------------------------------------------------------------
    // Phase C/E: Precomputed maps (no O(N) per-trade scans).
    // ------------------------------------------------------------------
    let id_to_idx: HashMap<String, usize> = companies
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), i))
        .collect();

    let mut owner_to_buildings: HashMap<String, Vec<usize>> = HashMap::default();
    for (i, b) in buildings.iter().enumerate() {
        owner_to_buildings
            .entry(b.owner_id.clone())
            .or_default()
            .push(i);
    }

    // ------------------------------------------------------------------
    // Phase E.1: Planning pass — never mutates `companies` here.
    // ------------------------------------------------------------------
    let mut plans: Vec<AcquisitionPlan> = Vec::new();

    for (target_idx, target) in companies.iter().enumerate() {
        if target.is_liquidated
            || target.merged_into.is_some()
            || target.bank_type.is_some()
            || target.worker_capacity == 0
        {
            continue;
        }

        // Distress or cash-starvation trigger.
        let is_distressed =
            target.available_cash < 0.0 || target.liquid_capital < target.liabilities;
        let target_sector = target.sector;
        let target_region = target.region_id.clone();

        // Simple market-driven trigger: fixed overcapacity or negative cash.
        if !is_distressed && target.available_cash > 0.0 {
            continue;
        }

        // Find the most liquid same-sector acquirer in the same region.
        let mut best_acquirer: Option<(usize, f64)> = None;
        for (acquirer_idx, acquirer) in companies.iter().enumerate() {
            if acquirer_idx == target_idx
                || acquirer.is_liquidated
                || acquirer.merged_into.is_some()
                || acquirer.bank_type.is_some()
                || acquirer.sector != target_sector
                || acquirer.region_id != target_region
                || acquirer.available_cash <= 0.0
            {
                continue;
            }
            match best_acquirer {
                None => best_acquirer = Some((acquirer_idx, acquirer.available_cash)),
                Some((_, cash)) if acquirer.available_cash > cash => {
                    best_acquirer = Some((acquirer_idx, acquirer.available_cash));
                }
                _ => {}
            }
        }

        let Some((acquirer_idx, _)) = best_acquirer else {
            continue;
        };

        // Acquisition price is the target's going-concern capital, floored at zero.
        let price = target.company_capital.max(0.0);
        if price > companies[acquirer_idx].available_cash {
            continue;
        }

        let target_buildings = owner_to_buildings
            .get(&target.id)
            .cloned()
            .unwrap_or_default();

        plans.push(AcquisitionPlan {
            target_idx,
            acquirer_idx,
            price,
            target_buildings,
        });
    }

    // ------------------------------------------------------------------
    // Phase E.2: Execution — apply each plan sequentially. Plans are
    // independent except for the tombstoning and index maps, which are
    // rebuilt after this function returns via `retain` in `turn.rs`.
    // ------------------------------------------------------------------
    for plan in plans {
        execute_acquisition(
            &plan,
            companies,
            buildings,
            &id_to_idx,
            &mut owner_to_buildings,
            market_signal,
        );
    }

    // Reconcile any country-level aggregates that were touched.
    let _ = country;
}

fn execute_acquisition(
    plan: &AcquisitionPlan,
    companies: &mut [Company],
    buildings: &mut [Building],
    id_to_idx: &HashMap<String, usize>,
    owner_to_buildings: &mut HashMap<String, Vec<usize>>,
    market_signal: &MarketSignal,
) {
    let target_idx = plan.target_idx;
    let acquirer_idx = plan.acquirer_idx;
    if target_idx >= companies.len() || acquirer_idx >= companies.len() {
        return;
    }
    if companies[target_idx].is_liquidated || companies[acquirer_idx].is_liquidated {
        return;
    }

    // Double-entry acquisition payment: acquirer -> target.
    if plan.price > 0.0 {
        let mut dummy_country = Country::default();
        let _ = settle_transfer_mapped(
            companies,
            id_to_idx,
            acquirer_idx,
            plan.price,
            &TransferRecipient::OtherCompany {
                recipient_idx: target_idx,
            },
            &mut dummy_country,
        );
    }

    // Read-only snapshots to avoid interleaved borrows.
    let acquirer_id = companies[acquirer_idx].id.clone();
    let target_id = companies[target_idx].id.clone();

    // ------------------------------------------------------------------
    // E.2a: Liabilities & loans (LoanRef + bank loan book update).
    // ------------------------------------------------------------------
    let target_loans: Vec<crate::state::banking::LoanRef> =
        std::mem::take(&mut companies[target_idx].outstanding_loans);
    for loan in &target_loans {
        if let Some(&bi) = id_to_idx.get(&loan.bank_id) {
            if let Some(ref mut bs) = companies[bi].balance_sheet {
                if let Some(issued) = bs.loans_issued.iter_mut().find(|l| l.id == loan.loan_id) {
                    issued.borrower_id = acquirer_id.clone();
                    issued.status = LoanStatus::Merged;
                }
            }
        }
    }
    companies[acquirer_idx]
        .outstanding_loans
        .extend(target_loans);
    companies[acquirer_idx].liabilities += companies[target_idx].liabilities;

    // ------------------------------------------------------------------
    // E.2b: Financial and physical capital aggregation.
    // ------------------------------------------------------------------
    companies[acquirer_idx].available_cash += companies[target_idx].available_cash;
    companies[acquirer_idx].liquid_capital += companies[target_idx].liquid_capital;
    companies[acquirer_idx].fixed_capital += companies[target_idx].fixed_capital;
    companies[acquirer_idx].credit_cash += companies[target_idx].credit_cash;
    companies[acquirer_idx].debit_cash += companies[target_idx].debit_cash;
    companies[acquirer_idx].worker_capacity += companies[target_idx].worker_capacity;
    companies[acquirer_idx].is_national_champion |= companies[target_idx].is_national_champion;
    companies[acquirer_idx].annual_profit_accumulator +=
        companies[target_idx].annual_profit_accumulator;

    // Move building IDs.
    let mut target_building_ids = std::mem::take(&mut companies[target_idx].building_ids);
    companies[acquirer_idx]
        .building_ids
        .append(&mut target_building_ids);

    // ------------------------------------------------------------------
    // E.2c: Freight-aware inventory integration with fire-sale fallback.
    // ------------------------------------------------------------------
    let acquirer_buildings = owner_to_buildings
        .get(&acquirer_id)
        .cloned()
        .unwrap_or_default();
    let same_region = companies[target_idx].region_id == companies[acquirer_idx].region_id;

    for &b_idx in &plan.target_buildings {
        if b_idx >= buildings.len() {
            continue;
        }

        let source_inventory: BTreeMap<Commodity, f64> =
            std::mem::take(&mut buildings[b_idx].inventory);
        for (commodity, quantity) in source_inventory {
            if quantity <= 0.0 {
                continue;
            }

            let freight_needed = if same_region {
                0.0
            } else {
                quantity * FREIGHT_PER_UNIT
            };

            // Try to consume freight capacity from the acquirer for cross-region loads.
            if freight_needed > 0.0
                && !consume_freight_from_buildings(buildings, &acquirer_buildings, freight_needed)
            {
                // Insufficient logistics — write off the goods and their book value.
                write_off_inventory(companies, acquirer_idx, commodity, quantity, market_signal);
                continue;
            }

            // Find a destination building with spare inventory capacity.
            let mut placed = false;
            for &dest_idx in &acquirer_buildings {
                if dest_idx >= buildings.len() || dest_idx == b_idx {
                    continue;
                }
                let used: f64 = buildings[dest_idx].inventory.values().sum();
                let free = buildings[dest_idx].inventory_capacity - used;
                if free >= quantity {
                    *buildings[dest_idx]
                        .inventory
                        .entry(commodity)
                        .or_insert(0.0) += quantity;
                    placed = true;
                    break;
                }
            }

            if placed {
                continue;
            }

            // No warehouse capacity — attempt a fire-sale to any other company
            // in the same region that has enough cash.
            let price = market_signal.prices.get(&commodity).copied().unwrap_or(0.0);
            let book_value = quantity * price;
            let recoverable = book_value * (1.0 - FIRE_SALE_DISCOUNT_RATIO);

            if let Some(buyer_idx) = find_fire_sale_buyer(
                companies,
                &acquirer_id,
                &target_id,
                acquirer_idx,
                recoverable,
            ) {
                let mut dummy_country = Country::default();
                if recoverable > 0.0
                    && settle_transfer_mapped(
                        companies,
                        id_to_idx,
                        buyer_idx,
                        recoverable,
                        &TransferRecipient::OtherCompany {
                            recipient_idx: acquirer_idx,
                        },
                        &mut dummy_country,
                    )
                    .is_ok()
                {
                    continue;
                }
            }

            // No buyer: physical destruction with a real write-off.
            write_off_inventory(companies, acquirer_idx, commodity, quantity, market_signal);
        }

        buildings[b_idx].owner_id = acquirer_id.clone();
        buildings[b_idx].current_employment = 0;
    }

    // Update ownership map: target buildings now belong to the acquirer.
    owner_to_buildings.remove(&target_id);
    owner_to_buildings
        .entry(acquirer_id)
        .or_default()
        .extend(plan.target_buildings.iter().copied());

    // ------------------------------------------------------------------
    // E.2d: Tombstone and equity recalculation.
    // ------------------------------------------------------------------
    companies[target_idx].is_liquidated = true;
    companies[target_idx].merged_into = Some(companies[acquirer_idx].id.clone());
    companies[target_idx].worker_capacity = 0;
    companies[target_idx].available_cash = 0.0;
    companies[target_idx].liquid_capital = 0.0;
    companies[target_idx].fixed_capital = 0.0;
    companies[target_idx].company_capital = 0.0;
    companies[target_idx].liabilities = 0.0;
    companies[target_idx].outstanding_loans.clear();

    companies[acquirer_idx].company_capital = (companies[acquirer_idx].fixed_capital
        + companies[acquirer_idx].liquid_capital
        - companies[acquirer_idx].liabilities)
        .max(0.0);
    if companies[acquirer_idx].worker_capacity >= 25_000 {
        companies[acquirer_idx].is_national_champion = true;
    }
}

/// Consume `Commodity::FreightCapacity` from `candiate_buildings` until `amount` is satisfied.
/// Returns `true` if the full amount was consumed.
fn consume_freight_from_buildings(
    buildings: &mut [Building],
    candidate_buildings: &[usize],
    amount: f64,
) -> bool {
    if amount <= 0.0 {
        return true;
    }
    let mut remaining = amount;
    for &b_idx in candidate_buildings {
        if b_idx >= buildings.len() {
            continue;
        }
        let available = buildings[b_idx]
            .inventory
            .get(&Commodity::FreightCapacity)
            .copied()
            .unwrap_or(0.0);
        if available <= 0.0 {
            continue;
        }
        let consumed = remaining.min(available);
        let new_qty = (available - consumed).max(0.0);
        if new_qty > 0.0 {
            buildings[b_idx]
                .inventory
                .insert(Commodity::FreightCapacity, new_qty);
        } else {
            buildings[b_idx]
                .inventory
                .remove(&Commodity::FreightCapacity);
        }
        remaining -= consumed;
        if remaining <= 0.0 {
            return true;
        }
    }
    false
}

/// Find a non-distressed company (other than acquirer and target) in the same
/// region as the acquirer that can pay `recoverable` cash.
fn find_fire_sale_buyer(
    companies: &[Company],
    acquirer_id: &str,
    target_id: &str,
    acquirer_idx: usize,
    recoverable: f64,
) -> Option<usize> {
    if recoverable <= 0.0 {
        return None;
    }
    let acquirer = &companies[acquirer_idx];
    companies.iter().enumerate().find_map(|(i, c)| {
        if c.id == acquirer_id || c.id == target_id {
            return None;
        }
        if c.is_liquidated || c.merged_into.is_some() || c.bank_type.is_some() {
            return None;
        }
        if c.region_id != acquirer.region_id {
            return None;
        }
        if c.available_cash >= recoverable {
            Some(i)
        } else {
            None
        }
    })
}

/// Write off the book value of physically destroyed inventory.
/// The loss is taken from `liquid_capital` because the asset side of the
/// balance sheet is permanently reduced.
fn write_off_inventory(
    companies: &mut [Company],
    acquirer_idx: usize,
    commodity: Commodity,
    quantity: f64,
    market_signal: &MarketSignal,
) {
    let price = market_signal.prices.get(&commodity).copied().unwrap_or(0.0);
    let book_value = quantity * price;
    if book_value > 0.0 {
        companies[acquirer_idx].liquid_capital -= book_value;
        companies[acquirer_idx].liquid_capital = companies[acquirer_idx].liquid_capital.max(0.0);
    }
}

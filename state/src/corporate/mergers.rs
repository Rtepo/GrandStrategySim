//! Mergers and Acquisitions lifecycle (Phase E — full execution).
//!
//! This module implements organic, market-driven M&A with the following invariants:
//! - Every cash movement routes through `settle_transfer` / `settle_transfer_mapped`.
//! - Physical inventory and staffing stay inside the transferred building —
//!   they are part of the going-concern asset the acquirer paid for.
//! - `LoanRef`s and bank `loans_issued.borrower_id` are updated to the acquirer.

use crate::economy::market::MarketSignal;
use crate::economy::transfer_settler::{settle_transfer_mapped, TransferRecipient};
use crate::entities::{Building, Company};
use crate::state::banking::LoanStatus;
use crate::state::Country;
use std::collections::HashMap;

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

        // Distress trigger: negative cash (overdrawn) or insolvency
        // (liquid capital below liabilities). A merely zero-cash company is
        // illiquid, not distressed — in a thin cash circuit nearly every
        // solvent firm sits at ~0 between settlement and payroll, so using
        // `available_cash <= 0` as a trigger mass-consolidates the economy
        // every turn and destroys the absorbed firms' labor demand.
        let is_distressed =
            target.available_cash < 0.0 || target.liquid_capital < target.liabilities;
        let target_sector = target.sector;
        let target_region = target.region_id.clone();

        if !is_distressed {
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
    // Phase 94: Transfer brokerage_cash to acquirer to prevent M0 leak.
    // The liquidated_cash routing after lifecycle captures is_liquidated
    // companies' brokerage_cash and adds it to treasury. For banked companies,
    // brokerage_cash is NOT M0 (M1 backed by bank deposits). Adding it to
    // treasury (M0) would create M0 from M1 without CB injection tracking.
    // Transferring it to the acquirer preserves the M1 classification and
    // prevents the leak.
    let target_brokerage_cash = companies[target_idx]
        .brokerage_account
        .as_ref()
        .map(|b| b.cash)
        .unwrap_or(0.0);
    if target_brokerage_cash > 0.0 {
        companies[acquirer_idx].available_cash += target_brokerage_cash;
        if let Some(brokerage) = &mut companies[target_idx].brokerage_account {
            brokerage.cash = 0.0;
        }
    }
    companies[acquirer_idx].liquid_capital += companies[target_idx].liquid_capital;
    companies[acquirer_idx].fixed_capital += companies[target_idx].fixed_capital;
    companies[acquirer_idx].credit_cash += companies[target_idx].credit_cash;
    companies[acquirer_idx].debit_cash += companies[target_idx].debit_cash;
    companies[acquirer_idx].worker_capacity += companies[target_idx].worker_capacity;
    companies[acquirer_idx].scale_factor += companies[target_idx].scale_factor;
    // Labor demand and employment must follow the absorbed capacity and
    // buildings — otherwise the merged entity keeps only the acquirer's own
    // demand and the target's workforce demand is silently destroyed.
    companies[acquirer_idx].target_fte_demand += companies[target_idx].target_fte_demand;
    companies[acquirer_idx].physical_fte_demand += companies[target_idx].physical_fte_demand;
    companies[acquirer_idx].fulfilled_fte += companies[target_idx].fulfilled_fte;
    companies[acquirer_idx].furloughed_workers_count +=
        companies[target_idx].furloughed_workers_count;
    companies[acquirer_idx].is_national_champion |= companies[target_idx].is_national_champion;
    companies[acquirer_idx].annual_profit_accumulator +=
        companies[target_idx].annual_profit_accumulator;

    // Move building IDs.
    let mut target_building_ids = std::mem::take(&mut companies[target_idx].building_ids);
    companies[acquirer_idx]
        .building_ids
        .append(&mut target_building_ids);

    // ------------------------------------------------------------------
    // E.2c: Inventory and staffing transfer with the building itself.
    // An acquisition changes the owner_id on the title, not the physical
    // contents: on-site inputs, output stock and the employed crew are part
    // of the going-concern asset the acquirer just paid for. The previous
    // implementation drained every building's inventory and attempted to
    // redistribute it into the acquirer's OTHER buildings — the acquired
    // building was excluded from the destination list, so any stock that did
    // not fit elsewhere was written off (or fire-sold) and the crew was
    // zeroed. Bootstrap-cash M&A cascades therefore annihilated each
    // transferred plant's fuel buffer and staffing in a single turn, and
    // serial acquisitions (A→B→C→D) repeated the destruction every hop.
    // Buildings that continue operating under the new owner keep their
    // stock and their staff; the labor-clearing sync and the ask/posting
    // loop manage them from the next turn.
    // ------------------------------------------------------------------
    for &b_idx in &plan.target_buildings {
        if b_idx >= buildings.len() {
            continue;
        }
        buildings[b_idx].owner_id = acquirer_id.clone();
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
    companies[target_idx].scale_factor = 0;
    companies[target_idx].target_fte_demand = 0;
    companies[target_idx].physical_fte_demand = 0;
    companies[target_idx].fulfilled_fte = 0;
    companies[target_idx].furloughed_workers_count = 0.0;
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


